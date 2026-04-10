mod inode;
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
use crate::path::split_path;

pub(crate) use inode::{Inode, InodeKind};

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

/// An in-memory virtual filesystem backed by an inode table.
///
/// Paths are `/`-separated strings. Every stored path is absolute; the root
/// directory `/` always exists.
///
/// Every filesystem object gets an inode number. Directories store
/// `BTreeMap<String, u64>` mapping names to inode numbers, enabling
/// hard links (multiple names pointing to the same inode).
#[derive(Clone)]
pub struct MemFs {
    inodes: BTreeMap<u64, Inode>,
    next_ino: u64,
    root_ino: u64,
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
        let mut inodes = BTreeMap::new();
        inodes.insert(
            2,
            Inode {
                kind: InodeKind::Dir {
                    children: BTreeMap::new(),
                },
                mode: 0o755,
                uid: 0,
                gid: 0,
                mtime: 0,
                ctime: 0,
                atime: 0,
                nlink: 2,
            },
        );
        MemFs {
            inodes,
            next_ino: 3,
            root_ino: 2,
            current_uid: 0,
            current_gid: 0,
            supplementary_gids: Vec::new(),
            time: 0,
            umask: 0o022,
        }
    }

    /// Allocate a new inode number.
    fn alloc_ino(&mut self) -> u64 {
        let ino = self.next_ino;
        self.next_ino += 1;
        ino
    }

    #[allow(dead_code)]
    fn inode(&self, ino: u64) -> Option<&Inode> {
        self.inodes.get(&ino)
    }

    #[allow(dead_code)]
    fn inode_mut(&mut self, ino: u64) -> Option<&mut Inode> {
        self.inodes.get_mut(&ino)
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

    /// Change the owner and/or group of a file or directory.
    /// Follows symlinks (changes ownership of the target, not the symlink).
    ///
    /// Matches Linux semantics:
    /// - Only root (uid 0) can change the owner (uid).
    /// - The file owner can change the group to any group they belong to
    ///   (current gid or supplementary gids).
    /// - Root can change both owner and group to any value.
    pub fn chown(&mut self, path: &str, uid: u32, gid: u32) -> Result<(), VfsError> {
        let (ino, _) = self.traverse(path)?;
        let inode = self
            .inodes
            .get(&ino)
            .ok_or(VfsError::from(VfsErrorKind::NotFound))?;

        if self.current_uid != 0 {
            // Non-root: cannot change owner
            if uid != inode.uid {
                return Err(VfsErrorKind::PermissionDenied.into());
            }
            // Must be the file owner
            if self.current_uid != inode.uid {
                return Err(VfsErrorKind::PermissionDenied.into());
            }
            // New gid must be in caller's groups
            if gid != self.current_gid && !self.supplementary_gids.contains(&gid) {
                return Err(VfsErrorKind::PermissionDenied.into());
            }
        }

        let now = self.tick();
        let inode = self.inodes.get_mut(&ino).unwrap();
        inode.uid = uid;
        inode.gid = gid;
        inode.ctime = now;
        Ok(())
    }

    // -- Queries ------------------------------------------------------------

    /// Get a borrowed view of the entry at `path`, following symlinks.
    pub fn get(&self, path: &str) -> Option<EntryRef<'_>> {
        self.traverse(path).ok().map(|(_, n)| n.as_entry_ref())
    }

    /// Returns `true` if `path` exists (follows symlinks).
    pub fn exists(&self, path: &str) -> bool {
        self.traverse(path).is_ok()
    }

    /// Returns `true` if `path` is a file (follows symlinks).
    pub fn is_file(&self, path: &str) -> bool {
        self.traverse(path).is_ok_and(|(_, n)| n.is_file())
    }

    /// Returns `true` if `path` is a directory (follows symlinks).
    pub fn is_dir(&self, path: &str) -> bool {
        self.traverse(path).is_ok_and(|(_, n)| n.is_dir())
    }

    /// Returns `true` if `path` itself is a symlink (does **not** follow it).
    pub fn is_symlink(&self, path: &str) -> bool {
        self.traverse_nofollow(path)
            .is_ok_and(|(_, n)| n.is_symlink())
    }

    /// Read the raw bytes of a file. Checks read permission.
    pub fn read(&self, path: &str) -> Result<&[u8], VfsError> {
        let (_, inode) = self.traverse(path)?;
        match &inode.kind {
            InodeKind::File { content } => {
                if !self.check_permission(inode, 4) {
                    return Err(VfsError::from(VfsErrorKind::PermissionDenied));
                }
                Ok(content)
            }
            InodeKind::Dir { .. } => Err(VfsError::from(VfsErrorKind::IsADirectory)),
            InodeKind::Symlink { .. } => Err(VfsError::from(VfsErrorKind::NotFound)),
        }
    }

    /// Read file content as a UTF-8 string. Checks read permission.
    pub fn read_to_string(&self, path: &str) -> Result<&str, VfsError> {
        let bytes = self.read(path)?;
        core::str::from_utf8(bytes).map_err(|_| VfsError::from(VfsErrorKind::InvalidUtf8))
    }

    /// Return metadata for the entry at `path`, following symlinks.
    pub fn metadata(&self, path: &str) -> Result<Metadata, VfsError> {
        let (ino, inode) = self.traverse(path)?;
        Ok(inode.to_metadata(ino))
    }

    /// Return metadata without following the final symlink.
    pub fn symlink_metadata(&self, path: &str) -> Result<Metadata, VfsError> {
        let (ino, inode) = self.traverse_nofollow(path)?;
        Ok(inode.to_metadata(ino))
    }

    /// Read the target of a symlink without following it.
    pub fn read_link(&self, path: &str) -> Result<String, VfsError> {
        let (_, inode) = self.traverse_nofollow(path)?;
        match &inode.kind {
            InodeKind::Symlink { target } => Ok(target.clone()),
            _ => Err(VfsError::from(VfsErrorKind::NotASymlink)),
        }
    }

    /// List the direct children of a directory, sorted by name.
    /// Follows symlinks to resolve the directory.
    /// Requires read permission on the directory.
    pub fn read_dir(&self, dir: &str) -> Result<Vec<DirEntry>, VfsError> {
        let (_, inode) = self.traverse(dir)?;
        match &inode.kind {
            InodeKind::Dir { children } => {
                if !self.check_permission(inode, 4) {
                    return Err(VfsErrorKind::PermissionDenied.into());
                }
                let mut entries = Vec::new();
                for (name, child_ino) in children {
                    if let Some(child) = self.inodes.get(child_ino) {
                        entries.push(DirEntry {
                            name: name.clone(),
                            is_dir: child.is_dir(),
                            is_symlink: child.is_symlink(),
                            mode: child.mode(),
                            mtime: child.mtime(),
                            size: match &child.kind {
                                InodeKind::File { content } => content.len(),
                                _ => 0,
                            },
                            ino: *child_ino,
                        });
                    }
                }
                Ok(entries)
            }
            _ => Err(VfsError::from(VfsErrorKind::NotADirectory)),
        }
    }

    // -- Iteration ----------------------------------------------------------

    /// Collect all stored paths via depth-first traversal.
    pub fn paths(&self) -> Vec<String> {
        let mut out = Vec::new();
        inode::collect_paths(&self.inodes, self.root_ino, "/", &mut out);
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
            Some((ino, _)) => {
                let mut out = Vec::new();
                inode::collect_paths(&self.inodes, ino, &normalized, &mut out);
                out
            }
            None => Vec::new(),
        }
    }

    /// Collect all `(path, entry_ref)` pairs under `path` via DFS.
    pub fn iter_prefix(&self, path: &str) -> Vec<(String, EntryRef<'_>)> {
        let normalized = crate::normalize(path);
        match self.traverse(&normalized).ok() {
            Some((ino, _)) => {
                let mut out = Vec::new();
                inode::collect_entries(&self.inodes, ino, &normalized, &mut out);
                out
            }
            None => Vec::new(),
        }
    }

    /// Return a lazy depth-first iterator over all entries.
    pub fn walk(&self) -> Walk<'_> {
        Walk::new(&self.inodes, self.root_ino, "/".into())
    }

    /// Return a lazy depth-first iterator over entries under `path`.
    ///
    /// Returns an empty iterator if `path` does not exist.
    pub fn walk_prefix(&self, path: &str) -> Walk<'_> {
        let normalized = crate::normalize(path);
        match self.traverse(&normalized) {
            Ok((ino, _)) => Walk::new(&self.inodes, ino, normalized),
            Err(_) => Walk::empty(),
        }
    }

    /// Return a lazy iterator over directory entries at `path`.
    /// Requires read permission on the directory.
    pub fn read_dir_iter(&self, path: &str) -> Result<ReadDirIter<'_>, VfsError> {
        let (_, inode) = self.traverse(path)?;
        match &inode.kind {
            InodeKind::Dir { children } => {
                if !self.check_permission(inode, 4) {
                    return Err(VfsErrorKind::PermissionDenied.into());
                }
                Ok(ReadDirIter::new(&self.inodes, children.iter()))
            }
            _ => Err(VfsErrorKind::NotADirectory.into()),
        }
    }

    // -- Mutations ----------------------------------------------------------

    /// Insert an entry at the given path.
    ///
    /// This is a low-level method; it does **not** create parent directories.
    /// Timestamps from the entry are **ignored**; the current clock value is
    /// used instead. Use [`insert_raw`](Self::insert_raw) to preserve original
    /// timestamps.
    pub fn insert(&mut self, path: String, entry: Entry) {
        let now = self.tick();
        let ino = self.alloc_ino();
        let inode = match entry {
            Entry::File {
                content,
                mode,
                uid,
                gid,
                ..
            } => Inode {
                kind: InodeKind::File { content },
                mode,
                uid,
                gid,
                mtime: now,
                ctime: now,
                atime: now,
                nlink: 1,
            },
            Entry::Dir { mode, uid, gid, .. } => Inode {
                kind: InodeKind::Dir {
                    children: BTreeMap::new(),
                },
                mode,
                uid,
                gid,
                mtime: now,
                ctime: now,
                atime: now,
                nlink: 2,
            },
            Entry::Symlink {
                target, uid, gid, ..
            } => Inode {
                kind: InodeKind::Symlink { target },
                mode: 0o777,
                uid,
                gid,
                mtime: now,
                ctime: now,
                atime: now,
                nlink: 1,
            },
        };
        self.inodes.insert(ino, inode);
        if path == "/" {
            // Replace root: transfer children concept not applicable, just replace
            // Keep root_ino pointing to old slot, swap inodes
            let old_root = self.inodes.remove(&self.root_ino);
            let new_root = self.inodes.remove(&ino).unwrap();
            self.inodes.insert(self.root_ino, new_root);
            // Clean up old root inode if needed
            drop(old_root);
            return;
        }
        if let Some((children, name)) = self.traverse_parent_mut(&path) {
            if let Some(old_ino) = children.insert(name, ino) {
                self.dec_nlink(old_ino);
            }
        } else {
            // Parent doesn't exist, clean up the allocated inode
            self.inodes.remove(&ino);
        }
    }

    /// Insert an entry at the given path, preserving its exact timestamp values.
    ///
    /// Unlike [`insert`](Self::insert), this does NOT advance the internal clock and
    /// stores the timestamps from the `Entry` as-is. Used during deserialization.
    #[cfg(feature = "serde")]
    #[allow(dead_code)]
    pub(crate) fn insert_raw(&mut self, path: String, entry: Entry) {
        let ino = self.alloc_ino();
        let inode = match entry {
            Entry::File {
                content,
                mode,
                uid,
                gid,
                mtime,
                ctime,
                atime,
            } => Inode {
                kind: InodeKind::File { content },
                mode,
                uid,
                gid,
                mtime,
                ctime,
                atime,
                nlink: 1,
            },
            Entry::Dir {
                mode,
                uid,
                gid,
                mtime,
                ctime,
                atime,
            } => Inode {
                kind: InodeKind::Dir {
                    children: BTreeMap::new(),
                },
                mode,
                uid,
                gid,
                mtime,
                ctime,
                atime,
                nlink: 2,
            },
            Entry::Symlink {
                target,
                uid,
                gid,
                mtime,
                ctime,
                atime,
            } => Inode {
                kind: InodeKind::Symlink { target },
                mode: 0o777,
                uid,
                gid,
                mtime,
                ctime,
                atime,
                nlink: 1,
            },
        };
        self.inodes.insert(ino, inode);
        if path == "/" {
            let old_root = self.inodes.remove(&self.root_ino);
            let new_root = self.inodes.remove(&ino).unwrap();
            self.inodes.insert(self.root_ino, new_root);
            drop(old_root);
            return;
        }
        if let Some((children, name)) = self.traverse_parent_mut(&path) {
            if let Some(old_ino) = children.insert(name, ino) {
                self.dec_nlink(old_ino);
            }
        } else {
            self.inodes.remove(&ino);
        }
    }

    /// Decrement nlink of an inode and remove it if nlink reaches 0.
    /// For directories, recursively decrements children.
    fn dec_nlink(&mut self, ino: u64) {
        if let Some(inode) = self.inodes.get_mut(&ino) {
            inode.nlink = inode.nlink.saturating_sub(1);
            if inode.nlink == 0 {
                let removed = self.inodes.remove(&ino);
                // If it was a directory, recursively dec_nlink children
                if let Some(Inode {
                    kind: InodeKind::Dir { children },
                    ..
                }) = removed
                {
                    for (_, child_ino) in children {
                        self.dec_nlink(child_ino);
                    }
                }
            }
        }
    }

    /// Remove the entry at `path` and return it as an [`Entry`], if it existed.
    pub fn remove(&mut self, path: &str) -> Option<Entry> {
        if path == "/" {
            return None;
        }
        // Check write permission on parent directory
        if !self.check_parent_write(path) {
            return None;
        }
        let path_str = path.to_string();
        let (children, name) = self.traverse_parent_mut(&path_str)?;
        let ino = children.remove(&name)?;
        let inode = self.inodes.get(&ino)?;
        let entry = inode.to_entry();
        self.dec_nlink(ino);
        Some(entry)
    }

    /// Create a symbolic link at `link_path` pointing to `target`.
    ///
    /// Returns [`VfsErrorKind::AlreadyExists`] if `link_path` already exists.
    pub fn symlink(&mut self, target: &str, link_path: &str) -> Result<(), VfsError> {
        let parent = crate::parent(link_path).unwrap_or("/");
        if !self.is_dir(parent) {
            return Err(VfsError::from(VfsErrorKind::NotFound));
        }
        // Check write permission on the parent directory
        if !self.check_parent_write(link_path) {
            return Err(VfsErrorKind::PermissionDenied.into());
        }
        // Check if the name already exists (using nofollow so we detect symlinks too)
        if self.traverse_nofollow(link_path).is_ok() {
            return Err(VfsErrorKind::AlreadyExists.into());
        }
        let now = self.tick();
        let uid = self.current_uid;
        let gid = self.current_gid;
        let ino = self.alloc_ino();
        self.inodes.insert(
            ino,
            Inode {
                kind: InodeKind::Symlink {
                    target: target.to_string(),
                },
                mode: 0o777,
                uid,
                gid,
                mtime: now,
                ctime: now,
                atime: now,
                nlink: 1,
            },
        );
        let link_path_owned = link_path.to_string();
        let (children, name) = self
            .traverse_parent_mut(&link_path_owned)
            .ok_or(VfsError::from(VfsErrorKind::NotFound))?;
        children.insert(name, ino);
        Ok(())
    }

    /// Check whether the current user has write permission on the parent
    /// directory of `path`. Returns `false` if the parent does not exist.
    fn check_parent_write(&self, path: &str) -> bool {
        let normalized = crate::normalize(path);
        let components = crate::path::split_path(&normalized);
        if components.is_empty() {
            return false; // root has no parent
        }
        let parent_path = if components.len() == 1 {
            "/".to_string()
        } else {
            alloc::format!("/{}", components[..components.len() - 1].join("/"))
        };
        match self.traverse(&parent_path) {
            Ok((_, inode)) => self.check_permission(inode, 2),
            Err(_) => false,
        }
    }

    /// Write a file with default permissions (`0o644`), masked by umask.
    ///
    /// If the path already points to an existing file inode, the content is
    /// updated in-place so that hard links to the same inode see the change.
    pub fn write(&mut self, path: &str, content: impl Into<Vec<u8>>) {
        let now = self.tick();
        let content = content.into();
        let mode = self.effective_mode(0o644);

        // Try to update existing file inode in-place (preserves hard links)
        if let Ok((ino, _)) = self.traverse(path) {
            if let Some(inode) = self.inodes.get(&ino) {
                if matches!(inode.kind, InodeKind::File { .. }) {
                    // Check write permission on the existing inode
                    if !self.check_permission(inode, 2) {
                        return;
                    }
                }
            }
            if let Some(inode) = self.inodes.get_mut(&ino) {
                if let InodeKind::File { content: ref mut c } = inode.kind {
                    *c = content;
                    inode.mtime = now;
                    inode.ctime = now;
                    inode.atime = now;
                    return;
                }
            }
        }

        // File doesn't exist or path points to dir/symlink — create new inode
        // Check parent directory write permission first
        if !self.check_parent_write(path) {
            return;
        }
        let ino = self.alloc_ino();
        let uid = self.current_uid;
        let gid = self.current_gid;
        self.inodes.insert(
            ino,
            Inode {
                kind: InodeKind::File { content },
                mode,
                uid,
                gid,
                mtime: now,
                ctime: now,
                atime: now,
                nlink: 1,
            },
        );
        if let Some((children, name)) = self.traverse_parent_mut(path) {
            if let Some(old_ino) = children.insert(name, ino) {
                self.dec_nlink(old_ino);
            }
        } else {
            // Parent doesn't exist, clean up
            self.inodes.remove(&ino);
        }
    }

    /// Write a file with explicit permissions (masked by umask).
    ///
    /// If the path already points to an existing file inode, the content and
    /// mode are updated in-place so that hard links to the same inode see the
    /// change.
    pub fn write_with_mode(&mut self, path: &str, content: impl Into<Vec<u8>>, mode: u16) {
        let now = self.tick();
        let content = content.into();
        let effective = self.effective_mode(mode);

        // Try to update existing file inode in-place (preserves hard links)
        if let Ok((ino, _)) = self.traverse(path) {
            if let Some(inode) = self.inodes.get(&ino) {
                if matches!(inode.kind, InodeKind::File { .. }) {
                    // Check write permission on the existing inode
                    if !self.check_permission(inode, 2) {
                        return;
                    }
                }
            }
            if let Some(inode) = self.inodes.get_mut(&ino) {
                if let InodeKind::File { content: ref mut c } = inode.kind {
                    *c = content;
                    inode.mode = effective;
                    inode.mtime = now;
                    inode.ctime = now;
                    inode.atime = now;
                    return;
                }
            }
        }

        // File doesn't exist or path points to dir/symlink — create new inode
        // Check parent directory write permission first
        if !self.check_parent_write(path) {
            return;
        }
        let ino = self.alloc_ino();
        let uid = self.current_uid;
        let gid = self.current_gid;
        self.inodes.insert(
            ino,
            Inode {
                kind: InodeKind::File { content },
                mode: effective,
                uid,
                gid,
                mtime: now,
                ctime: now,
                atime: now,
                nlink: 1,
            },
        );
        if let Some((children, name)) = self.traverse_parent_mut(path) {
            if let Some(old_ino) = children.insert(name, ino) {
                self.dec_nlink(old_ino);
            }
        } else {
            self.inodes.remove(&ino);
        }
    }

    /// Append data to an existing file. Checks write permission.
    pub fn append(&mut self, path: &str, data: &[u8]) -> Result<(), VfsError> {
        let (ino, _) = self.traverse(path)?;
        {
            let inode = self
                .inodes
                .get(&ino)
                .ok_or(VfsError::from(VfsErrorKind::NotFound))?;
            match &inode.kind {
                InodeKind::File { .. } => {
                    if !self.check_permission(inode, 2) {
                        return Err(VfsError::from(VfsErrorKind::PermissionDenied));
                    }
                }
                InodeKind::Dir { .. } => return Err(VfsError::from(VfsErrorKind::IsADirectory)),
                InodeKind::Symlink { .. } => return Err(VfsError::from(VfsErrorKind::NotFound)),
            }
        }
        let now = self.tick();
        let inode = self
            .inodes
            .get_mut(&ino)
            .ok_or(VfsError::from(VfsErrorKind::NotFound))?;
        if let InodeKind::File { content } = &mut inode.kind {
            content.extend_from_slice(data);
            inode.mtime = now;
            inode.ctime = now;
            inode.atime = now;
            Ok(())
        } else {
            Err(VfsError::from(VfsErrorKind::NotFound))
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
        // Check write permission on parent directory
        if !self.check_parent_write(path) {
            return Err(VfsErrorKind::PermissionDenied.into());
        }
        let now = self.tick();
        let dir_mode = self.effective_mode(0o755);
        let uid = self.current_uid;
        let gid = self.current_gid;
        let ino = self.alloc_ino();
        self.inodes.insert(
            ino,
            Inode {
                kind: InodeKind::Dir {
                    children: BTreeMap::new(),
                },
                mode: dir_mode,
                uid,
                gid,
                mtime: now,
                ctime: now,
                atime: now,
                nlink: 2,
            },
        );
        let (children, name) = self
            .traverse_parent_mut(path)
            .ok_or(VfsError::from(VfsErrorKind::NotFound))?;
        children.insert(name, ino);
        Ok(())
    }

    /// Create a directory and all missing ancestors.
    /// Follows symlinks in intermediate path components.
    pub fn create_dir_all(&mut self, path: &str) {
        let now = self.tick();
        let dir_mode = self.effective_mode(0o755);
        let uid = self.current_uid;
        let gid = self.current_gid;
        let normalized = crate::normalize(path);
        let components = split_path(&normalized);

        for i in 1..=components.len() {
            let prefix = alloc::format!("/{}", components[..i].join("/"));
            match self.traverse(&prefix) {
                Ok((_, inode)) if inode.is_dir() => continue,
                Ok(_) => return, // exists but not a dir
                Err(_) => {}     // doesn't exist, create it
            }
            let new_ino = self.alloc_ino();
            self.inodes.insert(
                new_ino,
                Inode {
                    kind: InodeKind::Dir {
                        children: BTreeMap::new(),
                    },
                    mode: dir_mode,
                    uid,
                    gid,
                    mtime: now,
                    ctime: now,
                    atime: now,
                    nlink: 2,
                },
            );
            if let Some((children, name)) = self.traverse_parent_mut(&prefix) {
                children.insert(name, new_ino);
            } else {
                self.inodes.remove(&new_ino);
                return;
            }
        }
    }

    /// Create an empty file if `path` does not already exist.
    /// If it already exists, updates `mtime` (like Unix `touch`).
    pub fn touch(&mut self, path: &str) {
        let now = self.tick();
        let file_mode = self.effective_mode(0o644);
        // Try to update existing
        if let Ok((ino, _)) = self.traverse(path) {
            if let Some(inode) = self.inodes.get_mut(&ino) {
                inode.mtime = now;
                inode.atime = now;
            }
            return;
        }
        // Check parent directory write permission before creating new file
        if !self.check_parent_write(path) {
            return;
        }
        let uid = self.current_uid;
        let gid = self.current_gid;
        // Check if parent exists before allocating
        let normalized = crate::normalize(path);
        let components = split_path(&normalized);
        if components.is_empty() {
            return;
        }
        // Try to insert into parent
        let ino = self.alloc_ino();
        self.inodes.insert(
            ino,
            Inode {
                kind: InodeKind::File {
                    content: Vec::new(),
                },
                mode: file_mode,
                uid,
                gid,
                mtime: now,
                ctime: now,
                atime: now,
                nlink: 1,
            },
        );
        if let Some((children, name)) = self.traverse_parent_mut(path) {
            // Only insert if not already there (or_insert semantics)
            children.entry(name).or_insert(ino);
            // If we didn't actually use our ino (entry existed), clean up
            // But since we checked traverse above and it failed, the entry shouldn't exist.
        } else {
            self.inodes.remove(&ino);
        }
    }

    /// Remove a directory and everything beneath it.
    pub fn remove_dir_all(&mut self, path: &str) -> Result<(), VfsError> {
        if path == "/" {
            // Clear root's children
            let root_ino = self.root_ino;
            let children = if let Some(Inode {
                kind: InodeKind::Dir { children },
                ..
            }) = self.inodes.get(&root_ino)
            {
                children.clone()
            } else {
                return Ok(());
            };
            // Remove all children from root
            if let Some(Inode {
                kind:
                    InodeKind::Dir {
                        children: root_children,
                    },
                ..
            }) = self.inodes.get_mut(&root_ino)
            {
                root_children.clear();
            }
            // Dec nlink on all former children
            for (_, child_ino) in children {
                self.dec_nlink(child_ino);
            }
            return Ok(());
        }
        // Check it exists and is a directory
        match self.traverse(path) {
            Ok((_, n)) if n.is_dir() => {}
            Ok(_) => return Err(VfsError::from(VfsErrorKind::NotADirectory)),
            Err(e) => return Err(e),
        }
        let path_str = path.to_string();
        if let Some((children, name)) = self.traverse_parent_mut(&path_str) {
            if let Some(ino) = children.remove(&name) {
                self.dec_nlink(ino);
            }
        }
        Ok(())
    }

    /// Set the permission mode on an existing entry (follows symlinks).
    /// Requires the caller to be root (uid == 0) or the file owner.
    pub fn set_mode(&mut self, path: &str, mode: u16) -> Result<(), VfsError> {
        let now = self.tick();
        let (ino, _) = self.traverse(path)?;
        let inode = self
            .inodes
            .get(&ino)
            .ok_or(VfsError::from(VfsErrorKind::NotFound))?;
        if self.current_uid != 0 && self.current_uid != inode.uid {
            return Err(VfsErrorKind::PermissionDenied.into());
        }
        let inode = self.inodes.get_mut(&ino).unwrap();
        if let Some(m) = inode.mode_mut() {
            *m = mode;
        }
        inode.ctime = now;
        Ok(())
    }

    /// Explicitly set the access time of the entry at `path`.
    pub fn set_atime(&mut self, path: &str, time: u64) -> Result<(), VfsError> {
        let (ino, _) = self.traverse(path)?;
        let inode = self
            .inodes
            .get_mut(&ino)
            .ok_or(VfsError::from(VfsErrorKind::NotFound))?;
        inode.atime = time;
        Ok(())
    }

    /// Copy a file. Checks read permission on the source.
    pub fn copy(&mut self, src: &str, dst: &str) -> Result<(), VfsError> {
        let content = {
            let (_, inode) = self.traverse(src)?;
            match &inode.kind {
                InodeKind::File { content } => {
                    if !self.check_permission(inode, 4) {
                        return Err(VfsError::from(VfsErrorKind::PermissionDenied));
                    }
                    content.clone()
                }
                InodeKind::Dir { .. } => return Err(VfsError::from(VfsErrorKind::IsADirectory)),
                InodeKind::Symlink { .. } => return Err(VfsError::from(VfsErrorKind::NotFound)),
            }
        };

        // Verify destination parent exists before allocating a new inode
        let dst_parent = crate::parent(dst).unwrap_or("/");
        if !self.is_dir(dst_parent) {
            return Err(VfsErrorKind::NotFound.into());
        }
        // Check write permission on destination parent directory
        if !self.check_parent_write(dst) {
            return Err(VfsErrorKind::PermissionDenied.into());
        }

        let now = self.tick();
        let ino = self.alloc_ino();
        self.inodes.insert(
            ino,
            Inode {
                kind: InodeKind::File { content },
                mode: self.effective_mode(0o644),
                uid: self.current_uid,
                gid: self.current_gid,
                mtime: now,
                ctime: now,
                atime: now,
                nlink: 1,
            },
        );
        if let Some((children, name)) = self.traverse_parent_mut(dst) {
            if let Some(old_ino) = children.insert(name, ino) {
                self.dec_nlink(old_ino);
            }
        } else {
            // Shouldn't happen since we checked parent above, but clean up just in case
            self.inodes.remove(&ino);
        }
        Ok(())
    }

    /// Truncate or extend a file to `len` bytes. Checks write permission.
    pub fn truncate(&mut self, path: &str, len: usize) -> Result<(), VfsError> {
        let (ino, _) = self.traverse(path)?;
        {
            let inode = self
                .inodes
                .get(&ino)
                .ok_or(VfsError::from(VfsErrorKind::NotFound))?;
            match &inode.kind {
                InodeKind::File { .. } => {
                    if !self.check_permission(inode, 2) {
                        return Err(VfsError::from(VfsErrorKind::PermissionDenied));
                    }
                }
                InodeKind::Dir { .. } => return Err(VfsError::from(VfsErrorKind::IsADirectory)),
                InodeKind::Symlink { .. } => return Err(VfsError::from(VfsErrorKind::NotFound)),
            }
        }
        let now = self.tick();
        let inode = self
            .inodes
            .get_mut(&ino)
            .ok_or(VfsError::from(VfsErrorKind::NotFound))?;
        if let InodeKind::File { content } = &mut inode.kind {
            content.resize(len, 0u8);
            inode.mtime = now;
            inode.ctime = now;
            inode.atime = now;
            Ok(())
        } else {
            Err(VfsError::from(VfsErrorKind::NotFound))
        }
    }

    /// Returns `true` if `path` is a directory with no children.
    pub fn is_empty_dir(&self, path: &str) -> bool {
        match self.traverse(path).ok() {
            Some((_, inode)) => {
                matches!(&inode.kind, InodeKind::Dir { children } if children.is_empty())
            }
            _ => false,
        }
    }

    /// Move (rename) an entry from `src` to `dst`.
    pub fn rename(&mut self, src: &str, dst: &str) -> Result<(), VfsError> {
        if src == "/" || dst == "/" {
            return Err(VfsError::from(VfsErrorKind::PermissionDenied));
        }

        // Check write permission on source parent directory
        if !self.check_parent_write(src) {
            return Err(VfsErrorKind::PermissionDenied.into());
        }
        // Check write permission on destination parent directory
        if !self.check_parent_write(dst) {
            return Err(VfsErrorKind::PermissionDenied.into());
        }

        // Verify destination parent exists BEFORE removing from source
        let dst_normalized = crate::normalize(dst);
        let dst_components = split_path(&dst_normalized);
        if dst_components.is_empty() {
            return Err(VfsErrorKind::NotFound.into());
        }
        let dst_parent = if dst_components.len() == 1 {
            "/".to_string()
        } else {
            alloc::format!("/{}", dst_components[..dst_components.len() - 1].join("/"))
        };
        if !self.is_dir(&dst_parent) {
            return Err(VfsErrorKind::NotFound.into());
        }

        // Now safe to remove source and insert at destination
        let src_str = src.to_string();
        let ino = {
            let (children, name) = self
                .traverse_parent_mut(&src_str)
                .ok_or(VfsError::from(VfsErrorKind::NotFound))?;
            children
                .remove(&name)
                .ok_or(VfsError::from(VfsErrorKind::NotFound))?
        };
        // Insert into destination parent
        if let Some((children, name)) = self.traverse_parent_mut(dst) {
            if let Some(old_ino) = children.insert(name, ino) {
                self.dec_nlink(old_ino);
            }
        }
        Ok(())
    }

    /// Create a hard link. `dst` becomes a new name for the inode behind `src`.
    /// Only files can be hard-linked (not directories or symlinks).
    pub fn hard_link(&mut self, src: &str, dst: &str) -> Result<(), VfsError> {
        let (src_ino, src_inode) = self.traverse(src)?;
        if !src_inode.is_file() {
            return Err(VfsErrorKind::PermissionDenied.into());
        }
        if self.exists(dst) {
            return Err(VfsErrorKind::AlreadyExists.into());
        }
        // Check write permission on destination parent directory
        if !self.check_parent_write(dst) {
            return Err(VfsErrorKind::PermissionDenied.into());
        }
        let now = self.tick();
        // Insert into parent directory
        let dst_str = dst.to_string();
        let (children, name) = self
            .traverse_parent_mut(&dst_str)
            .ok_or(VfsError::from(VfsErrorKind::NotFound))?;
        children.insert(name, src_ino);
        // Increment nlink and update ctime
        let inode = self.inodes.get_mut(&src_ino).unwrap();
        inode.nlink += 1;
        inode.ctime = now;
        Ok(())
    }

    /// Check whether the current user can access the file at `path` with
    /// the given mode. Returns `Ok(())` on success.
    pub fn access(&self, path: &str, mode: AccessMode) -> Result<(), VfsError> {
        let (_, inode) = self.traverse(path)?;
        if mode == AccessMode::F_OK {
            return Ok(());
        }
        if mode.0 & AccessMode::R_OK.0 != 0 && !self.check_permission(inode, 4) {
            return Err(VfsError::from(VfsErrorKind::PermissionDenied));
        }
        if mode.0 & AccessMode::W_OK.0 != 0 && !self.check_permission(inode, 2) {
            return Err(VfsError::from(VfsErrorKind::PermissionDenied));
        }
        if mode.0 & AccessMode::X_OK.0 != 0 && !self.check_permission(inode, 1) {
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

    /// Construct a `MemFs` from raw internal parts. Used by the serde
    /// implementation to rebuild a filesystem from a snapshot.
    #[cfg(feature = "serde")]
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn from_raw_parts(
        inodes: BTreeMap<u64, Inode>,
        next_ino: u64,
        root_ino: u64,
        current_uid: u32,
        current_gid: u32,
        supplementary_gids: Vec<u32>,
        time: u64,
        umask: u16,
    ) -> Self {
        MemFs {
            inodes,
            next_ino,
            root_ino,
            current_uid,
            current_gid,
            supplementary_gids,
            time,
            umask,
        }
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
        if let Some(Inode {
            kind: InodeKind::Dir { children },
            ..
        }) = self.inodes.get(&self.root_ino)
        {
            let count = children.len();
            for (i, (name, child_ino)) in children.iter().enumerate() {
                inode::fmt_tree(&self.inodes, *child_ino, f, name, "", i == count - 1)?;
            }
        }
        Ok(())
    }
}
