mod node;
mod readdir;
mod traverse;
mod walk;

#[cfg(test)]
mod tests;

#[cfg(feature = "serde")]
mod serde_impl;

pub use readdir::ReadDirIter;
pub use walk::Walk;

use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::fmt;

use crate::dir::DirEntry;
use crate::entry::{Entry, EntryRef};
use crate::error::{VfsError, VfsErrorKind};
use crate::metadata::Metadata;

use node::split_path;
pub(crate) use node::TreeNode;

// ---------------------------------------------------------------------------
// AccessMode
// ---------------------------------------------------------------------------

/// Bitflags for `access()` permission testing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct AccessMode(pub(crate) u8);

impl AccessMode {
    /// Test for existence only.
    pub const F_OK: Self = Self(0);
    /// Test read permission.
    pub const R_OK: Self = Self(4);
    /// Test write permission.
    pub const W_OK: Self = Self(2);
    /// Test execute permission.
    pub const X_OK: Self = Self(1);
}

impl core::ops::BitOr for AccessMode {
    type Output = Self;
    fn bitor(self, rhs: Self) -> Self {
        Self(self.0 | rhs.0)
    }
}

// ---------------------------------------------------------------------------
// MemFs
// ---------------------------------------------------------------------------

/// An in-memory virtual filesystem backed by a trie (prefix tree).
///
/// Paths are `/`-separated strings. Every stored path is absolute; the root
/// directory `/` always exists.
///
/// Directory listings are O(children), not O(total entries).
/// Recursive deletion is O(1) — just drops the subtree.
pub struct MemFs {
    root: TreeNode,
    current_uid: u32,
    current_gid: u32,
    supplementary_gids: Vec<u32>,
    time: u64,
    umask: u16,
}

impl MemFs {
    /// Create a new filesystem containing only the root directory `/`.
    /// Defaults to uid=0, gid=0 (root user).
    pub fn new() -> Self {
        MemFs {
            root: TreeNode::Dir {
                mode: 0o755,
                children: BTreeMap::new(),
                uid: 0,
                gid: 0,
                mtime: 0,
                ctime: 0,
                atime: 0,
            },
            current_uid: 0,
            current_gid: 0,
            supplementary_gids: Vec::new(),
            time: 0,
            umask: 0o022,
        }
    }

    /// Advance the internal clock and return the new timestamp.
    fn tick(&mut self) -> u64 {
        self.time += 1;
        self.time
    }

    /// Override the internal clock value.
    pub fn set_time(&mut self, t: u64) {
        self.time = t;
    }

    /// Returns the current internal clock value.
    pub fn time(&self) -> u64 {
        self.time
    }

    /// Set the file-creation mask. Returns the previous umask value.
    pub fn set_umask(&mut self, mask: u16) -> u16 {
        let old = self.umask;
        self.umask = mask;
        old
    }

    /// Returns the current file-creation mask.
    pub fn umask(&self) -> u16 {
        self.umask
    }

    /// Apply the current umask to a requested permission mode.
    fn effective_mode(&self, requested: u16) -> u16 {
        requested & !self.umask
    }

    /// Set the current user identity for permission checks.
    pub fn set_current_user(&mut self, uid: u32, gid: u32) {
        self.current_uid = uid;
        self.current_gid = gid;
    }

    /// Add a supplementary group ID for the current user.
    pub fn add_supplementary_gid(&mut self, gid: u32) {
        self.supplementary_gids.push(gid);
    }

    /// Returns the current user ID.
    pub fn current_uid(&self) -> u32 {
        self.current_uid
    }

    /// Returns the current group ID.
    pub fn current_gid(&self) -> u32 {
        self.current_gid
    }

    /// Returns the supplementary group IDs.
    pub fn supplementary_gids(&self) -> &[u32] {
        &self.supplementary_gids
    }

    /// Change the owner of a file or directory.
    /// Follows symlinks (changes ownership of the target, not the symlink).
    pub fn chown(&mut self, path: &str, uid: u32, gid: u32) -> Result<(), VfsError> {
        let now = self.tick();
        match self.traverse_mut(path)? {
            TreeNode::File {
                uid: node_uid,
                gid: node_gid,
                ctime,
                ..
            } => {
                *node_uid = uid;
                *node_gid = gid;
                *ctime = now;
                Ok(())
            }
            TreeNode::Dir {
                uid: node_uid,
                gid: node_gid,
                ctime,
                ..
            } => {
                *node_uid = uid;
                *node_gid = gid;
                *ctime = now;
                Ok(())
            }
            TreeNode::Symlink {
                uid: node_uid,
                gid: node_gid,
                ctime,
                ..
            } => {
                *node_uid = uid;
                *node_gid = gid;
                *ctime = now;
                Ok(())
            }
        }
    }

    // -- Queries ------------------------------------------------------------

    /// Get a borrowed view of the entry at `path`, following symlinks.
    pub fn get(&self, path: &str) -> Option<EntryRef<'_>> {
        self.traverse(path).ok().map(|n| n.as_entry_ref())
    }

    /// Returns `true` if `path` exists (follows symlinks).
    pub fn exists(&self, path: &str) -> bool {
        self.traverse(path).is_ok()
    }

    /// Returns `true` if `path` is a file (follows symlinks).
    pub fn is_file(&self, path: &str) -> bool {
        self.traverse(path).is_ok_and(|n| n.is_file())
    }

    /// Returns `true` if `path` is a directory (follows symlinks).
    pub fn is_dir(&self, path: &str) -> bool {
        self.traverse(path).is_ok_and(|n| n.is_dir())
    }

    /// Returns `true` if `path` itself is a symlink (does **not** follow it).
    pub fn is_symlink(&self, path: &str) -> bool {
        self.traverse_nofollow(path).is_ok_and(|n| n.is_symlink())
    }

    /// Read the raw bytes of a file. Checks read permission.
    pub fn read(&self, path: &str) -> Result<&[u8], VfsError> {
        match self.traverse(path)? {
            node @ TreeNode::File { content, .. } => {
                if !self.check_permission(node, 4) {
                    return Err(VfsError::from(VfsErrorKind::PermissionDenied));
                }
                Ok(content)
            }
            TreeNode::Dir { .. } => Err(VfsError::from(VfsErrorKind::IsADirectory)),
            TreeNode::Symlink { .. } => Err(VfsError::from(VfsErrorKind::NotFound)),
        }
    }

    /// Read file content as a UTF-8 string. Checks read permission.
    pub fn read_to_string(&self, path: &str) -> Result<&str, VfsError> {
        let bytes = self.read(path)?;
        core::str::from_utf8(bytes).map_err(|_| VfsError::from(VfsErrorKind::InvalidUtf8))
    }

    /// Return metadata for the entry at `path`, following symlinks.
    pub fn metadata(&self, path: &str) -> Result<Metadata, VfsError> {
        Ok(self.traverse(path)?.to_metadata())
    }

    /// Return metadata without following the final symlink.
    pub fn symlink_metadata(&self, path: &str) -> Result<Metadata, VfsError> {
        Ok(self.traverse_nofollow(path)?.to_metadata())
    }

    /// Read the target of a symlink without following it.
    pub fn read_link(&self, path: &str) -> Result<String, VfsError> {
        match self.traverse_nofollow(path)? {
            TreeNode::Symlink { target, .. } => Ok(target.clone()),
            _ => Err(VfsError::from(VfsErrorKind::NotASymlink)),
        }
    }

    /// List the direct children of a directory, sorted by name.
    /// Follows symlinks to resolve the directory.
    pub fn read_dir(&self, dir: &str) -> Result<Vec<DirEntry>, VfsError> {
        match self.traverse(dir)? {
            TreeNode::Dir { children, .. } => Ok(children
                .iter()
                .map(|(name, node)| DirEntry {
                    name: name.clone(),
                    is_dir: node.is_dir(),
                    is_symlink: node.is_symlink(),
                    mode: node.mode(),
                    mtime: node.mtime(),
                    size: match node {
                        TreeNode::File { content, .. } => content.len(),
                        _ => 0,
                    },
                })
                .collect()),
            TreeNode::File { .. } | TreeNode::Symlink { .. } => {
                Err(VfsError::from(VfsErrorKind::NotADirectory))
            }
        }
    }

    // -- Iteration ----------------------------------------------------------

    /// Collect all stored paths via depth-first traversal.
    pub fn paths(&self) -> Vec<String> {
        let mut out = Vec::new();
        self.root.collect_paths("/", &mut out);
        out
    }

    /// Collect all `(path, entry_ref)` pairs via depth-first traversal.
    pub fn iter(&self) -> Vec<(String, EntryRef<'_>)> {
        self.paths()
            .into_iter()
            .filter_map(|p| {
                let entry = self.get(&p)?;
                Some((p, entry))
            })
            .collect()
    }

    /// Collect all paths under `path` via DFS.
    pub fn paths_prefix(&self, path: &str) -> Vec<String> {
        let normalized = crate::normalize(path);
        match self.traverse(&normalized).ok() {
            Some(node) => {
                let mut out = Vec::new();
                node.collect_paths(&normalized, &mut out);
                out
            }
            None => Vec::new(),
        }
    }

    /// Collect all `(path, entry_ref)` pairs under `path` via DFS.
    pub fn iter_prefix(&self, path: &str) -> Vec<(String, EntryRef<'_>)> {
        let normalized = crate::normalize(path);
        match self.traverse(&normalized).ok() {
            Some(node) => {
                let mut out = Vec::new();
                node.collect_entries(&normalized, &mut out);
                out
            }
            None => Vec::new(),
        }
    }

    /// Return a lazy depth-first iterator over all entries.
    pub fn walk(&self) -> Walk<'_> {
        Walk::new(&self.root, "/".into())
    }

    /// Return a lazy depth-first iterator over entries under `path`.
    ///
    /// Returns an empty iterator if `path` does not exist.
    pub fn walk_prefix(&self, path: &str) -> Walk<'_> {
        let normalized = crate::normalize(path);
        match self.traverse(&normalized) {
            Ok(node) => Walk::new(node, normalized),
            Err(_) => Walk::empty(),
        }
    }

    /// Return a lazy iterator over directory entries at `path`.
    pub fn read_dir_iter(&self, path: &str) -> Result<ReadDirIter<'_>, VfsError> {
        match self.traverse(path)? {
            TreeNode::Dir { children, .. } => Ok(ReadDirIter::new(children.iter())),
            _ => Err(VfsErrorKind::NotADirectory.into()),
        }
    }

    // -- Mutations ----------------------------------------------------------

    /// Insert an entry at the given path.
    ///
    /// This is a low-level method; it does **not** create parent directories.
    pub fn insert(&mut self, path: String, entry: Entry) {
        let now = self.tick();
        let node = match entry {
            Entry::File {
                content,
                mode,
                uid,
                gid,
                ..
            } => TreeNode::File {
                content,
                mode,
                uid,
                gid,
                mtime: now,
                ctime: now,
                atime: now,
            },
            Entry::Dir { mode, uid, gid, .. } => TreeNode::Dir {
                mode,
                children: BTreeMap::new(),
                uid,
                gid,
                mtime: now,
                ctime: now,
                atime: now,
            },
            Entry::Symlink {
                target, uid, gid, ..
            } => TreeNode::Symlink {
                target,
                uid,
                gid,
                mtime: now,
                ctime: now,
                atime: now,
            },
        };
        if path == "/" {
            self.root = node;
            return;
        }
        if let Some((children, name)) = self.traverse_parent_mut(&path) {
            children.insert(name, node);
        }
    }

    /// Insert an entry at the given path, preserving its exact timestamp values.
    ///
    /// Unlike [`insert`](Self::insert), this does NOT advance the internal clock and
    /// stores the timestamps from the `Entry` as-is. Used during deserialization.
    #[cfg(feature = "serde")]
    pub(crate) fn insert_raw(&mut self, path: String, entry: Entry) {
        let node = match entry {
            Entry::File {
                content,
                mode,
                uid,
                gid,
                mtime,
                ctime,
                atime,
            } => TreeNode::File {
                content,
                mode,
                uid,
                gid,
                mtime,
                ctime,
                atime,
            },
            Entry::Dir {
                mode,
                uid,
                gid,
                mtime,
                ctime,
                atime,
            } => TreeNode::Dir {
                mode,
                children: BTreeMap::new(),
                uid,
                gid,
                mtime,
                ctime,
                atime,
            },
            Entry::Symlink {
                target,
                uid,
                gid,
                mtime,
                ctime,
                atime,
            } => TreeNode::Symlink {
                target,
                uid,
                gid,
                mtime,
                ctime,
                atime,
            },
        };
        if path == "/" {
            self.root = node;
            return;
        }
        if let Some((children, name)) = self.traverse_parent_mut(&path) {
            children.insert(name, node);
        }
    }

    /// Remove the entry at `path` and return it as an [`Entry`], if it existed.
    pub fn remove(&mut self, path: &str) -> Option<Entry> {
        if path == "/" {
            return None;
        }
        let path_str = path.to_string();
        let (children, name) = self.traverse_parent_mut(&path_str)?;
        let node = children.remove(&name)?;
        Some(match node {
            TreeNode::File {
                content,
                mode,
                uid,
                gid,
                mtime,
                ctime,
                atime,
            } => Entry::File {
                content,
                mode,
                uid,
                gid,
                mtime,
                ctime,
                atime,
            },
            TreeNode::Dir {
                mode,
                uid,
                gid,
                mtime,
                ctime,
                atime,
                ..
            } => Entry::Dir {
                mode,
                uid,
                gid,
                mtime,
                ctime,
                atime,
            },
            TreeNode::Symlink {
                target,
                uid,
                gid,
                mtime,
                ctime,
                atime,
            } => Entry::Symlink {
                target,
                uid,
                gid,
                mtime,
                ctime,
                atime,
            },
        })
    }

    /// Create a symbolic link at `link_path` pointing to `target`.
    pub fn symlink(&mut self, target: &str, link_path: &str) -> Result<(), VfsError> {
        let parent = crate::parent(link_path).unwrap_or("/");
        if !self.is_dir(parent) {
            return Err(VfsError::from(VfsErrorKind::NotFound));
        }
        let now = self.tick();
        let link_path_owned = link_path.to_string();
        let uid = self.current_uid;
        let gid = self.current_gid;
        let (children, name) = self
            .traverse_parent_mut(&link_path_owned)
            .ok_or(VfsError::from(VfsErrorKind::NotFound))?;
        children.insert(
            name,
            TreeNode::Symlink {
                target: target.to_string(),
                uid,
                gid,
                mtime: now,
                ctime: now,
                atime: now,
            },
        );
        Ok(())
    }

    /// Write a file with default permissions (`0o644`), masked by umask.
    pub fn write(&mut self, path: &str, content: impl Into<Vec<u8>>) {
        let now = self.tick();
        let node = TreeNode::File {
            content: content.into(),
            mode: self.effective_mode(0o644),
            uid: self.current_uid,
            gid: self.current_gid,
            mtime: now,
            ctime: now,
            atime: now,
        };
        if let Some((children, name)) = self.traverse_parent_mut(path) {
            children.insert(name, node);
        }
    }

    /// Write a file with explicit permissions (masked by umask).
    pub fn write_with_mode(&mut self, path: &str, content: impl Into<Vec<u8>>, mode: u16) {
        let now = self.tick();
        let node = TreeNode::File {
            content: content.into(),
            mode: self.effective_mode(mode),
            uid: self.current_uid,
            gid: self.current_gid,
            mtime: now,
            ctime: now,
            atime: now,
        };
        if let Some((children, name)) = self.traverse_parent_mut(path) {
            children.insert(name, node);
        }
    }

    /// Append data to an existing file. Checks write permission.
    pub fn append(&mut self, path: &str, data: &[u8]) -> Result<(), VfsError> {
        let allowed: Result<bool, VfsError> = match self.traverse(path)? {
            node @ TreeNode::File { .. } => {
                if self.check_permission(node, 2) {
                    Ok(true)
                } else {
                    Err(VfsError::from(VfsErrorKind::PermissionDenied))
                }
            }
            TreeNode::Dir { .. } => Err(VfsError::from(VfsErrorKind::IsADirectory)),
            TreeNode::Symlink { .. } => Err(VfsError::from(VfsErrorKind::NotFound)),
        };
        allowed?;
        let now = self.tick();
        match self.traverse_mut(path)? {
            TreeNode::File {
                content,
                mtime,
                ctime,
                atime,
                ..
            } => {
                content.extend_from_slice(data);
                *mtime = now;
                *ctime = now;
                *atime = now;
                Ok(())
            }
            _ => Err(VfsError::from(VfsErrorKind::NotFound)),
        }
    }

    /// Create a single directory. Fails if the parent does not exist.
    pub fn create_dir(&mut self, path: &str) -> Result<(), VfsError> {
        if self.exists(path) {
            return Err(VfsError::from(VfsErrorKind::AlreadyExists));
        }
        let parent = crate::parent(path).unwrap_or("/");
        if !self.is_dir(parent) {
            return Err(VfsError::from(VfsErrorKind::NotFound));
        }
        let now = self.tick();
        let dir_mode = self.effective_mode(0o755);
        let uid = self.current_uid;
        let gid = self.current_gid;
        let (children, name) = self
            .traverse_parent_mut(path)
            .ok_or(VfsError::from(VfsErrorKind::NotFound))?;
        children.insert(
            name,
            TreeNode::Dir {
                mode: dir_mode,
                children: BTreeMap::new(),
                uid,
                gid,
                mtime: now,
                ctime: now,
                atime: now,
            },
        );
        Ok(())
    }

    /// Create a directory and all missing ancestors.
    pub fn create_dir_all(&mut self, path: &str) {
        let now = self.tick();
        let dir_mode = self.effective_mode(0o755);
        let uid = self.current_uid;
        let gid = self.current_gid;
        let components = split_path(path);
        let mut node = &mut self.root;
        for component in components {
            let children = match node {
                TreeNode::Dir { children, .. } => children,
                TreeNode::File { .. } | TreeNode::Symlink { .. } => return,
            };
            node = children
                .entry(component.to_string())
                .or_insert_with(|| TreeNode::Dir {
                    mode: dir_mode,
                    children: BTreeMap::new(),
                    uid,
                    gid,
                    mtime: now,
                    ctime: now,
                    atime: now,
                });
        }
    }

    /// Create an empty file if `path` does not already exist.
    /// If it already exists, updates `mtime` (like Unix `touch`).
    pub fn touch(&mut self, path: &str) {
        let now = self.tick();
        let file_mode = self.effective_mode(0o644);
        if let Ok(node) = self.traverse_mut(path) {
            match node {
                TreeNode::File { mtime, atime, .. }
                | TreeNode::Dir { mtime, atime, .. }
                | TreeNode::Symlink { mtime, atime, .. } => {
                    *mtime = now;
                    *atime = now;
                }
            }
            return;
        }
        let uid = self.current_uid;
        let gid = self.current_gid;
        if let Some((children, name)) = self.traverse_parent_mut(path) {
            children.entry(name).or_insert_with(|| TreeNode::File {
                content: Vec::new(),
                mode: file_mode,
                uid,
                gid,
                mtime: now,
                ctime: now,
                atime: now,
            });
        }
    }

    /// Remove a directory and everything beneath it.
    pub fn remove_dir_all(&mut self, path: &str) -> Result<(), VfsError> {
        if path == "/" {
            if let TreeNode::Dir { children, .. } = &mut self.root {
                children.clear();
            }
            return Ok(());
        }
        match self.traverse(path) {
            Ok(n) if n.is_dir() => {}
            Ok(_) => return Err(VfsError::from(VfsErrorKind::NotADirectory)),
            Err(e) => return Err(e),
        }
        if let Some((children, name)) = self.traverse_parent_mut(path) {
            children.remove(&name);
        }
        Ok(())
    }

    /// Set the permission mode on an existing entry (follows symlinks).
    pub fn set_mode(&mut self, path: &str, mode: u16) -> Result<(), VfsError> {
        let now = self.tick();
        let node = self.traverse_mut(path)?;
        if let Some(m) = node.mode_mut() {
            *m = mode;
        }
        match node {
            TreeNode::File { ctime, .. }
            | TreeNode::Dir { ctime, .. }
            | TreeNode::Symlink { ctime, .. } => {
                *ctime = now;
            }
        }
        Ok(())
    }

    /// Explicitly set the access time of the entry at `path`.
    pub fn set_atime(&mut self, path: &str, time: u64) -> Result<(), VfsError> {
        match self.traverse_mut(path)? {
            TreeNode::File { atime, .. }
            | TreeNode::Dir { atime, .. }
            | TreeNode::Symlink { atime, .. } => {
                *atime = time;
                Ok(())
            }
        }
    }

    /// Copy a file. Checks read permission on the source.
    pub fn copy(&mut self, src: &str, dst: &str) -> Result<(), VfsError> {
        let content = match self.traverse(src)? {
            node @ TreeNode::File { content, .. } => {
                if !self.check_permission(node, 4) {
                    return Err(VfsError::from(VfsErrorKind::PermissionDenied));
                }
                content.clone()
            }
            TreeNode::Dir { .. } => return Err(VfsError::from(VfsErrorKind::IsADirectory)),
            TreeNode::Symlink { .. } => return Err(VfsError::from(VfsErrorKind::NotFound)),
        };
        let now = self.tick();
        let node = TreeNode::File {
            content,
            mode: self.effective_mode(0o644),
            uid: self.current_uid,
            gid: self.current_gid,
            mtime: now,
            ctime: now,
            atime: now,
        };
        if let Some((children, name)) = self.traverse_parent_mut(dst) {
            children.insert(name, node);
        }
        Ok(())
    }

    /// Truncate or extend a file to `len` bytes. Checks write permission.
    pub fn truncate(&mut self, path: &str, len: usize) -> Result<(), VfsError> {
        let allowed: Result<(), VfsError> = match self.traverse(path)? {
            node @ TreeNode::File { .. } => {
                if self.check_permission(node, 2) {
                    Ok(())
                } else {
                    Err(VfsError::from(VfsErrorKind::PermissionDenied))
                }
            }
            TreeNode::Dir { .. } => Err(VfsError::from(VfsErrorKind::IsADirectory)),
            TreeNode::Symlink { .. } => Err(VfsError::from(VfsErrorKind::NotFound)),
        };
        allowed?;
        let now = self.tick();
        match self.traverse_mut(path)? {
            TreeNode::File {
                content,
                mtime,
                ctime,
                atime,
                ..
            } => {
                content.resize(len, 0u8);
                *mtime = now;
                *ctime = now;
                *atime = now;
                Ok(())
            }
            _ => Err(VfsError::from(VfsErrorKind::NotFound)),
        }
    }

    /// Returns `true` if `path` is a directory with no children.
    pub fn is_empty_dir(&self, path: &str) -> bool {
        match self.traverse(path).ok() {
            Some(TreeNode::Dir { children, .. }) => children.is_empty(),
            _ => false,
        }
    }

    /// Move (rename) an entry from `src` to `dst`.
    pub fn rename(&mut self, src: &str, dst: &str) -> Result<(), VfsError> {
        if src == "/" || dst == "/" {
            return Err(VfsError::from(VfsErrorKind::PermissionDenied));
        }
        let src_components = split_path(src);
        let src_name = *src_components
            .last()
            .ok_or(VfsError::from(VfsErrorKind::NotFound))?;
        let node = {
            let (children, _) = self
                .traverse_parent_mut(src)
                .ok_or(VfsError::from(VfsErrorKind::NotFound))?;
            children
                .remove(src_name)
                .ok_or(VfsError::from(VfsErrorKind::NotFound))?
        };
        if let Some((children, name)) = self.traverse_parent_mut(dst) {
            children.insert(name, node);
        }
        Ok(())
    }

    /// Check whether the current user can access the file at `path` with
    /// the given mode. Returns `Ok(())` on success.
    pub fn access(&self, path: &str, mode: AccessMode) -> Result<(), VfsError> {
        let node = self.traverse(path)?;
        if mode == AccessMode::F_OK {
            return Ok(());
        }
        if mode.0 & AccessMode::R_OK.0 != 0 && !self.check_permission(node, 4) {
            return Err(VfsError::from(VfsErrorKind::PermissionDenied));
        }
        if mode.0 & AccessMode::W_OK.0 != 0 && !self.check_permission(node, 2) {
            return Err(VfsError::from(VfsErrorKind::PermissionDenied));
        }
        if mode.0 & AccessMode::X_OK.0 != 0 && !self.check_permission(node, 1) {
            return Err(VfsError::from(VfsErrorKind::PermissionDenied));
        }
        Ok(())
    }

    // -- Delegated path utilities -------------------------------------------

    /// Normalize an absolute path. Alias for [`crate::normalize`].
    pub fn normalize(path: &str) -> String {
        crate::normalize(path)
    }

    /// Return the parent path. Alias for [`crate::parent`].
    pub fn parent(path: &str) -> Option<&str> {
        crate::parent(path)
    }

    /// Resolve all symlinks and `.`/`..` segments, returning the canonical path.
    /// Returns an error if any component does not exist.
    pub fn canonical_path(&self, path: &str) -> Result<String, VfsError> {
        let normalized = crate::normalize(path);
        self.resolve_path_following_symlinks(&normalized, 0)
    }
}

impl Default for MemFs {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Debug for MemFs {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MemFs").finish_non_exhaustive()
    }
}

impl fmt::Display for MemFs {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "/")?;
        if let TreeNode::Dir { children, .. } = &self.root {
            let count = children.len();
            for (i, (name, child)) in children.iter().enumerate() {
                child.fmt_tree(f, name, "", i == count - 1)?;
            }
        }
        Ok(())
    }
}
