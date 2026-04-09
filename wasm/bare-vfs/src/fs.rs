use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::fmt;

use crate::dir::DirEntry;
use crate::entry::{Entry, EntryRef};
use crate::error::VfsError;
use crate::metadata::Metadata;

// ---------------------------------------------------------------------------
// Internal trie node
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub(crate) enum TreeNode {
    File {
        content: Vec<u8>,
        mode: u16,
    },
    Dir {
        mode: u16,
        children: BTreeMap<String, TreeNode>,
    },
}

impl TreeNode {
    fn is_dir(&self) -> bool {
        matches!(self, TreeNode::Dir { .. })
    }

    fn is_file(&self) -> bool {
        matches!(self, TreeNode::File { .. })
    }

    fn mode(&self) -> u16 {
        match self {
            TreeNode::File { mode, .. } | TreeNode::Dir { mode, .. } => *mode,
        }
    }

    fn mode_mut(&mut self) -> &mut u16 {
        match self {
            TreeNode::File { mode, .. } | TreeNode::Dir { mode, .. } => mode,
        }
    }

    fn as_entry_ref(&self) -> EntryRef<'_> {
        match self {
            TreeNode::File { content, mode } => EntryRef::File {
                content,
                mode: *mode,
            },
            TreeNode::Dir { mode, .. } => EntryRef::Dir { mode: *mode },
        }
    }

    fn to_metadata(&self) -> Metadata {
        Metadata::new(
            self.is_file(),
            match self {
                TreeNode::File { content, .. } => content.len(),
                TreeNode::Dir { .. } => 0,
            },
            self.mode(),
        )
    }

    /// Collect all paths via DFS.
    fn collect_paths(&self, prefix: &str, out: &mut Vec<String>) {
        out.push(prefix.to_string());
        if let TreeNode::Dir { children, .. } = self {
            for (name, child) in children {
                let child_path = if prefix == "/" {
                    alloc::format!("/{}", name)
                } else {
                    alloc::format!("{}/{}", prefix, name)
                };
                child.collect_paths(&child_path, out);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Path splitting helper
// ---------------------------------------------------------------------------

/// Split an absolute path into components. "/" → empty vec, "/a/b" → ["a", "b"].
fn split_path(path: &str) -> Vec<&str> {
    path.split('/').filter(|s| !s.is_empty()).collect()
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
}

impl MemFs {
    /// Create a new filesystem containing only the root directory `/`.
    pub fn new() -> Self {
        MemFs {
            root: TreeNode::Dir {
                mode: 0o755,
                children: BTreeMap::new(),
            },
        }
    }

    // -- Internal traversal -------------------------------------------------

    fn traverse(&self, path: &str) -> Option<&TreeNode> {
        if path.is_empty() {
            return None;
        }
        if path == "/" {
            return Some(&self.root);
        }
        let mut node = &self.root;
        for component in split_path(path) {
            match node {
                TreeNode::Dir { children, .. } => {
                    node = children.get(component)?;
                }
                TreeNode::File { .. } => return None,
            }
        }
        Some(node)
    }

    fn traverse_mut(&mut self, path: &str) -> Option<&mut TreeNode> {
        if path.is_empty() {
            return None;
        }
        if path == "/" {
            return Some(&mut self.root);
        }
        let mut node = &mut self.root;
        for component in split_path(path) {
            match node {
                TreeNode::Dir { children, .. } => {
                    node = children.get_mut(component)?;
                }
                TreeNode::File { .. } => return None,
            }
        }
        Some(node)
    }

    /// Get mutable access to the parent directory's children map and the leaf name.
    ///
    /// Uses a raw-pointer trick to work around Rust's inability to return
    /// a mutable reference produced by chained reborrows.
    fn traverse_parent_mut<'a>(
        &'a mut self,
        path: &'a str,
    ) -> Option<(&'a mut BTreeMap<String, TreeNode>, &'a str)> {
        let components = split_path(path);
        if components.is_empty() {
            return None; // root has no parent
        }
        let (parents, leaf) = components.split_at(components.len() - 1);
        let leaf_name = leaf[0];

        let mut node: *mut TreeNode = &mut self.root;
        for component in parents {
            // SAFETY: we hold &mut self, so exclusive access is guaranteed.
            // The pointer chain follows the same path a safe &mut traversal would,
            // but avoids the reborrow-lifetime problem.
            unsafe {
                match &mut *node {
                    TreeNode::Dir { children, .. } => {
                        node = children.get_mut(*component)? as *mut TreeNode;
                    }
                    TreeNode::File { .. } => return None,
                }
            }
        }
        unsafe {
            match &mut *node {
                TreeNode::Dir { children, .. } => Some((children, leaf_name)),
                TreeNode::File { .. } => None,
            }
        }
    }

    // -- Queries ------------------------------------------------------------

    /// Get a borrowed view of the entry at `path`, if it exists.
    pub fn get(&self, path: &str) -> Option<EntryRef<'_>> {
        self.traverse(path).map(|n| n.as_entry_ref())
    }

    /// Returns `true` if an entry exists at `path`.
    pub fn exists(&self, path: &str) -> bool {
        self.traverse(path).is_some()
    }

    /// Returns `true` if `path` is a file.
    pub fn is_file(&self, path: &str) -> bool {
        self.traverse(path).is_some_and(|n| n.is_file())
    }

    /// Returns `true` if `path` is a directory.
    pub fn is_dir(&self, path: &str) -> bool {
        self.traverse(path).is_some_and(|n| n.is_dir())
    }

    /// Read the raw byte content of a file, checking read permission.
    pub fn read(&self, path: &str) -> Result<&[u8], VfsError> {
        match self.traverse(path) {
            Some(TreeNode::File { content, mode }) => {
                if mode & 0o400 == 0 {
                    Err(VfsError::PermissionDenied)
                } else {
                    Ok(content)
                }
            }
            Some(TreeNode::Dir { .. }) => Err(VfsError::IsADirectory),
            None => Err(VfsError::NotFound),
        }
    }

    /// Read the content of a file as a UTF-8 string, checking read permission.
    ///
    /// Returns [`VfsError::InvalidUtf8`] if the content is not valid UTF-8.
    pub fn read_to_string(&self, path: &str) -> Result<&str, VfsError> {
        let bytes = self.read(path)?;
        core::str::from_utf8(bytes).map_err(|_| VfsError::InvalidUtf8)
    }

    /// Get metadata for an entry.
    pub fn metadata(&self, path: &str) -> Result<Metadata, VfsError> {
        match self.traverse(path) {
            Some(node) => Ok(node.to_metadata()),
            None => Err(VfsError::NotFound),
        }
    }

    // -- Directory listing --------------------------------------------------

    /// List the direct children of a directory, sorted by name.
    pub fn read_dir(&self, dir: &str) -> Result<Vec<DirEntry>, VfsError> {
        match self.traverse(dir) {
            Some(TreeNode::Dir { children, .. }) => {
                // BTreeMap is already sorted by key
                Ok(children
                    .iter()
                    .map(|(name, node)| DirEntry {
                        name: name.clone(),
                        is_dir: node.is_dir(),
                        mode: node.mode(),
                    })
                    .collect())
            }
            Some(TreeNode::File { .. }) => Err(VfsError::NotADirectory),
            None => Err(VfsError::NotFound),
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

    // -- Mutations ----------------------------------------------------------

    /// Insert an entry at the given path.
    ///
    /// This is a low-level method; it does **not** create parent directories.
    /// Panics if the parent directory does not exist.
    pub fn insert(&mut self, path: String, entry: Entry) {
        let node = match entry {
            Entry::File { content, mode } => TreeNode::File { content, mode },
            Entry::Dir { mode } => TreeNode::Dir {
                mode,
                children: BTreeMap::new(),
            },
        };
        if path == "/" {
            self.root = node;
            return;
        }
        if let Some((children, name)) = self.traverse_parent_mut(&path) {
            children.insert(name.to_string(), node);
        }
    }

    /// Remove the entry at `path` and return it as an [`Entry`], if it existed.
    ///
    /// For directories, this removes the entire subtree.
    pub fn remove(&mut self, path: &str) -> Option<Entry> {
        if path == "/" {
            return None;
        }
        let (children, name) = self.traverse_parent_mut(path)?;
        let node = children.remove(name)?;
        Some(match node {
            TreeNode::File { content, mode } => Entry::File { content, mode },
            TreeNode::Dir { mode, .. } => Entry::Dir { mode },
        })
    }

    /// Write a file with default permissions (`0o644`). Overwrites if it exists.
    pub fn write(&mut self, path: &str, content: impl Into<Vec<u8>>) {
        let node = TreeNode::File {
            content: content.into(),
            mode: 0o644,
        };
        if let Some((children, name)) = self.traverse_parent_mut(path) {
            children.insert(name.to_string(), node);
        }
    }

    /// Write a file with explicit permissions. Overwrites if it exists.
    pub fn write_with_mode(&mut self, path: &str, content: impl Into<Vec<u8>>, mode: u16) {
        let node = TreeNode::File {
            content: content.into(),
            mode,
        };
        if let Some((children, name)) = self.traverse_parent_mut(path) {
            children.insert(name.to_string(), node);
        }
    }

    /// Append data to an existing file. Checks write permission.
    pub fn append(&mut self, path: &str, data: &[u8]) -> Result<(), VfsError> {
        match self.traverse_mut(path) {
            Some(TreeNode::File { content, mode }) => {
                if *mode & 0o200 == 0 {
                    return Err(VfsError::PermissionDenied);
                }
                content.extend_from_slice(data);
                Ok(())
            }
            Some(TreeNode::Dir { .. }) => Err(VfsError::IsADirectory),
            None => Err(VfsError::NotFound),
        }
    }

    /// Create a single directory. Fails if the parent does not exist.
    pub fn create_dir(&mut self, path: &str) -> Result<(), VfsError> {
        if self.exists(path) {
            return Err(VfsError::AlreadyExists);
        }
        let parent = crate::parent(path).unwrap_or("/");
        if !self.is_dir(parent) {
            return Err(VfsError::NotFound);
        }
        let (children, name) = self.traverse_parent_mut(path).ok_or(VfsError::NotFound)?;
        children.insert(
            name.to_string(),
            TreeNode::Dir {
                mode: 0o755,
                children: BTreeMap::new(),
            },
        );
        Ok(())
    }

    /// Create a directory and all missing ancestors.
    pub fn create_dir_all(&mut self, path: &str) {
        let components = split_path(path);
        let mut node = &mut self.root;
        for component in components {
            // Ensure current node is a directory, then get/create child
            let children = match node {
                TreeNode::Dir { children, .. } => children,
                TreeNode::File { .. } => return, // can't descend into a file
            };
            node = children
                .entry(component.to_string())
                .or_insert_with(|| TreeNode::Dir {
                    mode: 0o755,
                    children: BTreeMap::new(),
                });
        }
    }

    /// Create an empty file if `path` does not already exist.
    pub fn touch(&mut self, path: &str) {
        if self.exists(path) {
            return;
        }
        if let Some((children, name)) = self.traverse_parent_mut(path) {
            children
                .entry(name.to_string())
                .or_insert_with(|| TreeNode::File {
                    content: Vec::new(),
                    mode: 0o644,
                });
        }
    }

    /// Remove a directory and everything beneath it.
    pub fn remove_dir_all(&mut self, path: &str) -> Result<(), VfsError> {
        if path == "/" {
            // Clear root children but keep root itself
            if let TreeNode::Dir { children, .. } = &mut self.root {
                children.clear();
            }
            return Ok(());
        }
        match self.traverse(path) {
            Some(n) if n.is_dir() => {}
            Some(_) => return Err(VfsError::NotADirectory),
            None => return Err(VfsError::NotFound),
        }
        // Now remove — traverse_parent_mut borrows mutably
        if let Some((children, name)) = self.traverse_parent_mut(path) {
            children.remove(name);
        }
        Ok(())
    }

    /// Set the permission mode on an existing entry.
    pub fn set_mode(&mut self, path: &str, mode: u16) -> Result<(), VfsError> {
        match self.traverse_mut(path) {
            Some(node) => {
                *node.mode_mut() = mode;
                Ok(())
            }
            None => Err(VfsError::NotFound),
        }
    }

    /// Copy a file. Checks read permission on the source.
    pub fn copy(&mut self, src: &str, dst: &str) -> Result<(), VfsError> {
        let (content, _mode) = match self.traverse(src) {
            Some(TreeNode::File { content, mode }) => {
                if mode & 0o400 == 0 {
                    return Err(VfsError::PermissionDenied);
                }
                (content.clone(), *mode)
            }
            Some(TreeNode::Dir { .. }) => return Err(VfsError::IsADirectory),
            None => return Err(VfsError::NotFound),
        };
        let node = TreeNode::File {
            content,
            mode: 0o644,
        };
        if let Some((children, name)) = self.traverse_parent_mut(dst) {
            children.insert(name.to_string(), node);
        }
        Ok(())
    }

    /// Move (rename) an entry from `src` to `dst`.
    pub fn rename(&mut self, src: &str, dst: &str) -> Result<(), VfsError> {
        if src == "/" || dst == "/" {
            return Err(VfsError::PermissionDenied);
        }
        // Remove source
        let src_components = split_path(src);
        let src_name = *src_components.last().ok_or(VfsError::NotFound)?;
        let node = {
            let (children, _) = self.traverse_parent_mut(src).ok_or(VfsError::NotFound)?;
            children.remove(src_name).ok_or(VfsError::NotFound)?
        };
        // Insert at destination
        if let Some((children, name)) = self.traverse_parent_mut(dst) {
            children.insert(name.to_string(), node);
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

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    #[test]
    fn new_has_root() {
        let fs = MemFs::new();
        assert!(fs.is_dir("/"));
    }

    #[test]
    fn default_has_root() {
        let fs = MemFs::default();
        assert!(fs.is_dir("/"));
    }

    // -- read / read_to_string / write --------------------------------------

    #[test]
    fn write_and_read_string() {
        let mut fs = MemFs::new();
        fs.write("/hello.txt", "world".to_string());
        assert_eq!(fs.read_to_string("/hello.txt"), Ok("world"));
        assert_eq!(fs.read("/hello.txt"), Ok(b"world".as_slice()));
    }

    #[test]
    fn write_and_read_bytes() {
        let mut fs = MemFs::new();
        fs.write("/bin", vec![0u8, 1, 0xFF]);
        assert_eq!(fs.read("/bin"), Ok([0u8, 1, 0xFF].as_slice()));
        assert_eq!(fs.read_to_string("/bin"), Err(VfsError::InvalidUtf8));
    }

    #[test]
    fn write_overwrites_existing() {
        let mut fs = MemFs::new();
        fs.write("/f.txt", "old".to_string());
        fs.write("/f.txt", "new".to_string());
        assert_eq!(fs.read_to_string("/f.txt"), Ok("new"));
    }

    #[test]
    fn write_with_mode_sets_permissions() {
        let mut fs = MemFs::new();
        fs.write_with_mode("/secret", "x".to_string(), 0o000);
        assert_eq!(fs.read("/secret"), Err(VfsError::PermissionDenied));
    }

    #[test]
    fn read_missing() {
        let fs = MemFs::new();
        assert_eq!(fs.read_to_string("/nope"), Err(VfsError::NotFound));
    }

    #[test]
    fn read_directory() {
        let mut fs = MemFs::new();
        fs.create_dir_all("/a");
        assert_eq!(fs.read("/a"), Err(VfsError::IsADirectory));
    }

    // -- exists / is_file / is_dir / get ------------------------------------

    #[test]
    fn exists_and_type_checks() {
        let mut fs = MemFs::new();
        fs.write("/f.txt", "".to_string());
        fs.create_dir_all("/d");

        assert!(fs.exists("/f.txt"));
        assert!(fs.exists("/d"));
        assert!(!fs.exists("/nope"));
        assert!(fs.is_file("/f.txt"));
        assert!(!fs.is_file("/d"));
        assert!(fs.is_dir("/d"));
        assert!(!fs.is_dir("/f.txt"));
    }

    #[test]
    fn get_returns_entry_ref() {
        let mut fs = MemFs::new();
        fs.write("/f.txt", "data".to_string());
        let e = fs.get("/f.txt").unwrap();
        assert!(e.is_file());
        assert_eq!(e.content_str(), Some("data"));
        assert!(fs.get("/missing").is_none());
    }

    // -- metadata -----------------------------------------------------------

    #[test]
    fn metadata_file() {
        let mut fs = MemFs::new();
        fs.write_with_mode("/f.txt", "hello", 0o755);
        let m = fs.metadata("/f.txt").unwrap();
        assert!(m.is_file());
        assert_eq!(m.len(), 5);
        assert_eq!(m.mode(), 0o755);
    }

    #[test]
    fn metadata_dir() {
        let fs = MemFs::new();
        let m = fs.metadata("/").unwrap();
        assert!(m.is_dir());
        assert_eq!(m.len(), 0);
    }

    #[test]
    fn metadata_not_found() {
        let fs = MemFs::new();
        assert_eq!(fs.metadata("/nope"), Err(VfsError::NotFound));
    }

    // -- append -------------------------------------------------------------

    #[test]
    fn append_to_file() {
        let mut fs = MemFs::new();
        fs.write("/log", "line1\n".to_string());
        fs.append("/log", b"line2\n").unwrap();
        assert_eq!(fs.read_to_string("/log"), Ok("line1\nline2\n"));
    }

    #[test]
    fn append_not_found() {
        let mut fs = MemFs::new();
        assert_eq!(fs.append("/nope", b"x"), Err(VfsError::NotFound));
    }

    #[test]
    fn append_to_directory() {
        let mut fs = MemFs::new();
        fs.create_dir_all("/d");
        assert_eq!(fs.append("/d", b"x"), Err(VfsError::IsADirectory));
    }

    #[test]
    fn append_permission_denied() {
        let mut fs = MemFs::new();
        fs.write_with_mode("/ro", "x", 0o444);
        assert_eq!(fs.append("/ro", b"y"), Err(VfsError::PermissionDenied));
    }

    // -- create_dir ---------------------------------------------------------

    #[test]
    fn create_dir_single() {
        let mut fs = MemFs::new();
        assert!(fs.create_dir("/sub").is_ok());
        assert!(fs.is_dir("/sub"));
    }

    #[test]
    fn create_dir_already_exists() {
        let mut fs = MemFs::new();
        fs.create_dir_all("/sub");
        assert_eq!(fs.create_dir("/sub"), Err(VfsError::AlreadyExists));
    }

    #[test]
    fn create_dir_parent_missing() {
        let mut fs = MemFs::new();
        assert_eq!(fs.create_dir("/a/b"), Err(VfsError::NotFound));
    }

    #[test]
    fn create_dir_all_and_list() {
        let mut fs = MemFs::new();
        fs.create_dir_all("/a/b/c");
        assert!(fs.is_dir("/a"));
        assert!(fs.is_dir("/a/b"));
        assert!(fs.is_dir("/a/b/c"));

        let children = fs.read_dir("/a").unwrap();
        assert_eq!(children.len(), 1);
        assert_eq!(children[0].name, "b");
        assert!(children[0].is_dir);
    }

    #[test]
    fn create_dir_all_idempotent() {
        let mut fs = MemFs::new();
        fs.create_dir_all("/a/b/c");
        fs.create_dir_all("/a/b/c");
        assert!(fs.is_dir("/a/b/c"));
    }

    // -- touch --------------------------------------------------------------

    #[test]
    fn touch_creates_empty_file() {
        let mut fs = MemFs::new();
        fs.touch("/new.txt");
        assert_eq!(fs.read("/new.txt"), Ok(b"".as_slice()));
    }

    #[test]
    fn touch_does_not_overwrite() {
        let mut fs = MemFs::new();
        fs.write("/f.txt", "data".to_string());
        fs.touch("/f.txt");
        assert_eq!(fs.read_to_string("/f.txt"), Ok("data"));
    }

    // -- remove / remove_dir_all --------------------------------------------

    #[test]
    fn remove_file() {
        let mut fs = MemFs::new();
        fs.write("/f.txt", "x".to_string());
        assert!(fs.remove("/f.txt").is_some());
        assert!(!fs.exists("/f.txt"));
    }

    #[test]
    fn remove_nonexistent() {
        let mut fs = MemFs::new();
        assert!(fs.remove("/nope").is_none());
    }

    #[test]
    fn remove_dir_all_recursive() {
        let mut fs = MemFs::new();
        fs.create_dir_all("/a/b");
        fs.write("/a/b/f.txt", "x".to_string());
        fs.write("/a/g.txt", "y".to_string());
        assert!(fs.remove_dir_all("/a").is_ok());
        assert!(!fs.exists("/a"));
        assert!(!fs.exists("/a/b"));
        assert!(!fs.exists("/a/b/f.txt"));
    }

    #[test]
    fn remove_dir_all_preserves_siblings() {
        let mut fs = MemFs::new();
        fs.create_dir_all("/a/target");
        fs.write("/a/target/f.txt", "x".to_string());
        fs.write("/a/sibling.txt", "keep".to_string());
        fs.remove_dir_all("/a/target").unwrap();
        assert!(!fs.exists("/a/target"));
        assert_eq!(fs.read_to_string("/a/sibling.txt"), Ok("keep"));
    }

    #[test]
    fn remove_dir_all_not_found() {
        let mut fs = MemFs::new();
        assert_eq!(fs.remove_dir_all("/nope"), Err(VfsError::NotFound));
    }

    #[test]
    fn remove_dir_all_on_file() {
        let mut fs = MemFs::new();
        fs.write("/f.txt", "x".to_string());
        assert_eq!(fs.remove_dir_all("/f.txt"), Err(VfsError::NotADirectory));
    }

    // -- set_mode -----------------------------------------------------------

    #[test]
    fn set_mode_file() {
        let mut fs = MemFs::new();
        fs.write("/f.txt", "x".to_string());
        fs.set_mode("/f.txt", 0o000).unwrap();
        assert_eq!(fs.read("/f.txt"), Err(VfsError::PermissionDenied));
        fs.set_mode("/f.txt", 0o644).unwrap();
        assert_eq!(fs.read_to_string("/f.txt"), Ok("x"));
    }

    #[test]
    fn set_mode_dir() {
        let mut fs = MemFs::new();
        fs.create_dir_all("/d");
        fs.set_mode("/d", 0o500).unwrap();
        assert_eq!(fs.get("/d").unwrap().mode(), 0o500);
    }

    #[test]
    fn set_mode_not_found() {
        let mut fs = MemFs::new();
        assert_eq!(fs.set_mode("/nope", 0o644), Err(VfsError::NotFound));
    }

    // -- copy ---------------------------------------------------------------

    #[test]
    fn copy_file() {
        let mut fs = MemFs::new();
        fs.write("/a.txt", "hello".to_string());
        fs.copy("/a.txt", "/b.txt").unwrap();
        assert_eq!(fs.read_to_string("/b.txt"), Ok("hello"));
        assert_eq!(fs.read_to_string("/a.txt"), Ok("hello"));
    }

    #[test]
    fn copy_not_found() {
        let mut fs = MemFs::new();
        assert_eq!(fs.copy("/nope", "/dst"), Err(VfsError::NotFound));
    }

    #[test]
    fn copy_directory() {
        let mut fs = MemFs::new();
        fs.create_dir_all("/d");
        assert_eq!(fs.copy("/d", "/d2"), Err(VfsError::IsADirectory));
    }

    #[test]
    fn copy_permission_denied() {
        let mut fs = MemFs::new();
        fs.write_with_mode("/secret", "x", 0o000);
        assert_eq!(fs.copy("/secret", "/dst"), Err(VfsError::PermissionDenied));
    }

    #[test]
    fn copy_overwrites_destination() {
        let mut fs = MemFs::new();
        fs.write("/src", "new".to_string());
        fs.write("/dst", "old".to_string());
        fs.copy("/src", "/dst").unwrap();
        assert_eq!(fs.read_to_string("/dst"), Ok("new"));
    }

    // -- rename -------------------------------------------------------------

    #[test]
    fn rename_file() {
        let mut fs = MemFs::new();
        fs.write("/old.txt", "data".to_string());
        fs.rename("/old.txt", "/new.txt").unwrap();
        assert!(!fs.exists("/old.txt"));
        assert_eq!(fs.read_to_string("/new.txt"), Ok("data"));
    }

    #[test]
    fn rename_not_found() {
        let mut fs = MemFs::new();
        assert_eq!(fs.rename("/nope", "/dst"), Err(VfsError::NotFound));
    }

    // -- read_dir -----------------------------------------------------------

    #[test]
    fn read_dir_sorted() {
        let mut fs = MemFs::new();
        fs.write("/c.txt", "".to_string());
        fs.write("/a.txt", "".to_string());
        fs.write("/b.txt", "".to_string());
        let entries = fs.read_dir("/").unwrap();
        let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, &["a.txt", "b.txt", "c.txt"]);
    }

    #[test]
    fn read_dir_not_found() {
        let fs = MemFs::new();
        assert_eq!(fs.read_dir("/nope"), Err(VfsError::NotFound));
    }

    #[test]
    fn read_dir_on_file() {
        let mut fs = MemFs::new();
        fs.write("/f.txt", "x".to_string());
        assert_eq!(fs.read_dir("/f.txt"), Err(VfsError::NotADirectory));
    }

    #[test]
    fn read_dir_empty() {
        let mut fs = MemFs::new();
        fs.create_dir_all("/empty");
        assert!(fs.read_dir("/empty").unwrap().is_empty());
    }

    #[test]
    fn read_dir_skips_nested() {
        let mut fs = MemFs::new();
        fs.create_dir_all("/a/b/c");
        fs.write("/a/b/c/f.txt", "x".to_string());
        let entries = fs.read_dir("/a").unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "b");
    }

    #[test]
    fn read_dir_mixed_entries() {
        let mut fs = MemFs::new();
        fs.write("/file.txt", "".to_string());
        fs.create_dir_all("/dir");
        let entries = fs.read_dir("/").unwrap();
        let file_e = entries.iter().find(|e| e.name == "file.txt").unwrap();
        let dir_e = entries.iter().find(|e| e.name == "dir").unwrap();
        assert!(!file_e.is_dir);
        assert!(dir_e.is_dir);
    }

    // -- iter / paths -------------------------------------------------------

    #[test]
    fn iter_all_entries() {
        let mut fs = MemFs::new();
        fs.create_dir_all("/a");
        fs.write("/a/f.txt", "x".to_string());
        let paths = fs.paths();
        assert!(paths.contains(&"/".to_string()));
        assert!(paths.contains(&"/a".to_string()));
        assert!(paths.contains(&"/a/f.txt".to_string()));
    }

    // -- insert (low-level) -------------------------------------------------

    #[test]
    fn insert_raw_entry() {
        let mut fs = MemFs::new();
        fs.insert("/custom".into(), Entry::file_with_mode("data", 0o755));
        let e = fs.get("/custom").unwrap();
        assert_eq!(e.content_str(), Some("data"));
        assert!(e.is_executable());
    }

    // -- Trie-specific behavior ---------------------------------------------

    #[test]
    fn deep_nesting() {
        let mut fs = MemFs::new();
        fs.create_dir_all("/a/b/c/d/e/f");
        fs.write("/a/b/c/d/e/f/deep.txt", "bottom".to_string());
        assert_eq!(fs.read_to_string("/a/b/c/d/e/f/deep.txt"), Ok("bottom"));
        assert!(fs.is_dir("/a/b/c/d/e/f"));
        assert!(fs.is_dir("/a/b/c"));
    }

    #[test]
    fn remove_dir_drops_entire_subtree() {
        let mut fs = MemFs::new();
        fs.create_dir_all("/a/b/c");
        fs.write("/a/b/c/f1.txt", "1".to_string());
        fs.write("/a/b/f2.txt", "2".to_string());
        // remove() on a dir now drops the whole subtree
        let removed = fs.remove("/a/b");
        assert!(removed.is_some());
        assert!(!fs.exists("/a/b"));
        assert!(!fs.exists("/a/b/c"));
        assert!(!fs.exists("/a/b/c/f1.txt"));
        assert!(!fs.exists("/a/b/f2.txt"));
        // parent still exists
        assert!(fs.is_dir("/a"));
    }

    #[test]
    fn remove_file_does_not_affect_siblings() {
        let mut fs = MemFs::new();
        fs.write("/a.txt", "a".to_string());
        fs.write("/b.txt", "b".to_string());
        fs.remove("/a.txt");
        assert!(!fs.exists("/a.txt"));
        assert_eq!(fs.read_to_string("/b.txt"), Ok("b"));
    }

    #[test]
    fn write_to_missing_parent_is_noop() {
        let mut fs = MemFs::new();
        fs.write("/nonexistent/file.txt", "data".to_string());
        assert!(!fs.exists("/nonexistent"));
        assert!(!fs.exists("/nonexistent/file.txt"));
    }

    #[test]
    fn touch_missing_parent_is_noop() {
        let mut fs = MemFs::new();
        fs.touch("/nonexistent/file.txt");
        assert!(!fs.exists("/nonexistent/file.txt"));
    }

    #[test]
    fn paths_dfs_order() {
        let mut fs = MemFs::new();
        fs.create_dir_all("/a/b");
        fs.write("/a/b/f.txt", "".to_string());
        fs.write("/a/g.txt", "".to_string());
        fs.create_dir_all("/z");
        let paths = fs.paths();
        // DFS: / → /a → /a/b → /a/b/f.txt → /a/g.txt → /z
        let a_idx = paths.iter().position(|p| p == "/a").unwrap();
        let ab_idx = paths.iter().position(|p| p == "/a/b").unwrap();
        let abf_idx = paths.iter().position(|p| p == "/a/b/f.txt").unwrap();
        let ag_idx = paths.iter().position(|p| p == "/a/g.txt").unwrap();
        let z_idx = paths.iter().position(|p| p == "/z").unwrap();
        // /a comes before its children
        assert!(a_idx < ab_idx);
        assert!(ab_idx < abf_idx);
        // /a subtree before /z
        assert!(ag_idx < z_idx);
    }

    #[test]
    fn iter_returns_correct_entry_types() {
        let mut fs = MemFs::new();
        fs.create_dir_all("/d");
        fs.write("/d/f.txt", "data".to_string());
        let entries = fs.iter();
        let root = entries.iter().find(|(p, _)| p == "/").unwrap();
        assert!(root.1.is_dir());
        let dir = entries.iter().find(|(p, _)| p == "/d").unwrap();
        assert!(dir.1.is_dir());
        let file = entries.iter().find(|(p, _)| p == "/d/f.txt").unwrap();
        assert!(file.1.is_file());
        assert_eq!(file.1.content_str(), Some("data"));
    }

    #[test]
    fn create_dir_all_does_not_overwrite_file() {
        let mut fs = MemFs::new();
        fs.write("/a", "file".to_string());
        // Trying to create_dir_all through a file stops
        fs.create_dir_all("/a/b/c");
        assert!(fs.is_file("/a")); // still a file
        assert!(!fs.exists("/a/b"));
    }

    #[test]
    fn remove_root_returns_none() {
        let mut fs = MemFs::new();
        assert!(fs.remove("/").is_none());
        assert!(fs.is_dir("/")); // root survives
    }

    #[test]
    fn rename_moves_subtree() {
        let mut fs = MemFs::new();
        fs.create_dir_all("/src/sub");
        fs.write("/src/sub/f.txt", "data".to_string());
        fs.create_dir_all("/dst");
        fs.rename("/src", "/dst/moved").unwrap();
        assert!(!fs.exists("/src"));
        assert!(fs.is_dir("/dst/moved"));
        assert!(fs.is_dir("/dst/moved/sub"));
        assert_eq!(fs.read_to_string("/dst/moved/sub/f.txt"), Ok("data"));
    }

    #[test]
    fn many_siblings() {
        let mut fs = MemFs::new();
        for i in 0..100 {
            fs.write(&alloc::format!("/f{:03}.txt", i), alloc::format!("{}", i));
        }
        let entries = fs.read_dir("/").unwrap();
        assert_eq!(entries.len(), 100);
        // Sorted by name
        assert_eq!(entries[0].name, "f000.txt");
        assert_eq!(entries[99].name, "f099.txt");
    }

    #[test]
    fn metadata_via_get() {
        let mut fs = MemFs::new();
        fs.write_with_mode("/f.txt", "hi", 0o444);
        let e = fs.get("/f.txt").unwrap();
        assert!(e.is_file());
        assert!(e.is_readable());
        assert!(!e.is_writable());
        assert_eq!(e.len(), 2);
    }

    #[test]
    fn get_root() {
        let fs = MemFs::new();
        let e = fs.get("/").unwrap();
        assert!(e.is_dir());
        assert_eq!(e.mode(), 0o755);
    }

    #[test]
    fn get_nested_missing() {
        let mut fs = MemFs::new();
        fs.create_dir_all("/a/b");
        assert!(fs.get("/a/b/c").is_none());
        assert!(fs.get("/a/b/c/d").is_none());
        assert!(fs.get("/x").is_none());
    }

    // -- Path edge cases ----------------------------------------------------

    #[test]
    fn empty_path_is_not_found() {
        let fs = MemFs::new();
        assert!(!fs.exists(""));
        assert!(fs.get("").is_none());
        assert_eq!(fs.read(""), Err(VfsError::NotFound));
        assert_eq!(fs.metadata(""), Err(VfsError::NotFound));
    }

    #[test]
    fn trailing_slash_treated_as_component() {
        let mut fs = MemFs::new();
        // "/a/" splits into ["a", ""] — the empty component is filtered out by split_path
        fs.create_dir_all("/a");
        fs.write("/a/f.txt", "x".to_string());
        // These should still work because split_path filters empty segments
        assert!(fs.is_dir("/a"));
    }

    #[test]
    fn path_with_special_chars() {
        let mut fs = MemFs::new();
        fs.write("/hello world.txt", "spaces".to_string());
        fs.write("/café.txt", "unicode".to_string());
        fs.write("/.hidden", "dot".to_string());
        assert_eq!(fs.read_to_string("/hello world.txt"), Ok("spaces"));
        assert_eq!(fs.read_to_string("/café.txt"), Ok("unicode"));
        assert_eq!(fs.read_to_string("/.hidden"), Ok("dot"));
    }

    // -- Mutation conflict edge cases ---------------------------------------

    #[test]
    fn write_overwrites_dir_with_file() {
        let mut fs = MemFs::new();
        fs.create_dir_all("/a/b");
        fs.write("/a/b/f.txt", "x".to_string());
        // Overwrite dir /a with a file
        fs.write("/a", "now a file".to_string());
        assert!(fs.is_file("/a"));
        // Children are gone (the dir node was replaced)
        assert!(!fs.exists("/a/b"));
    }

    #[test]
    fn insert_dir_replaces_file() {
        let mut fs = MemFs::new();
        fs.write("/x", "file".to_string());
        fs.insert("/x".into(), Entry::dir());
        assert!(fs.is_dir("/x"));
    }

    #[test]
    fn touch_on_existing_dir_is_noop() {
        let mut fs = MemFs::new();
        fs.create_dir_all("/d");
        fs.touch("/d");
        // Should still be a directory, not converted to file
        assert!(fs.is_dir("/d"));
    }

    #[test]
    fn create_dir_where_file_exists() {
        let mut fs = MemFs::new();
        fs.write("/x", "file".to_string());
        assert_eq!(fs.create_dir("/x"), Err(VfsError::AlreadyExists));
        assert!(fs.is_file("/x")); // unchanged
    }

    #[test]
    fn rename_to_self() {
        let mut fs = MemFs::new();
        fs.write("/f.txt", "data".to_string());
        // This removes source then inserts at dst — same path means it works
        assert!(fs.rename("/f.txt", "/f.txt").is_ok());
        assert_eq!(fs.read_to_string("/f.txt"), Ok("data"));
    }

    #[test]
    fn rename_overwrites_destination() {
        let mut fs = MemFs::new();
        fs.write("/src", "new".to_string());
        fs.write("/dst", "old".to_string());
        fs.rename("/src", "/dst").unwrap();
        assert!(!fs.exists("/src"));
        assert_eq!(fs.read_to_string("/dst"), Ok("new"));
    }

    #[test]
    fn rename_root_fails() {
        let mut fs = MemFs::new();
        fs.create_dir_all("/dst");
        assert!(fs.rename("/", "/dst/root").is_err());
    }

    // -- Boundary conditions ------------------------------------------------

    #[test]
    fn empty_fs_paths() {
        let fs = MemFs::new();
        let paths = fs.paths();
        assert_eq!(paths, vec!["/".to_string()]);
    }

    #[test]
    fn empty_fs_iter() {
        let fs = MemFs::new();
        let entries = fs.iter();
        assert_eq!(entries.len(), 1);
        assert!(entries[0].1.is_dir());
    }

    #[test]
    fn append_to_empty_file() {
        let mut fs = MemFs::new();
        fs.touch("/f.txt");
        fs.append("/f.txt", b"hello").unwrap();
        assert_eq!(fs.read_to_string("/f.txt"), Ok("hello"));
    }

    #[test]
    fn append_multiple_times() {
        let mut fs = MemFs::new();
        fs.write("/f.txt", "a".to_string());
        fs.append("/f.txt", b"b").unwrap();
        fs.append("/f.txt", b"c").unwrap();
        assert_eq!(fs.read_to_string("/f.txt"), Ok("abc"));
    }

    #[test]
    fn remove_dir_all_root_clears_children() {
        let mut fs = MemFs::new();
        fs.create_dir_all("/a/b");
        fs.write("/a/b/f.txt", "x".to_string());
        fs.write("/c.txt", "y".to_string());
        fs.remove_dir_all("/").unwrap();
        assert!(fs.is_dir("/")); // root still exists
        assert!(!fs.exists("/a")); // children gone
        assert!(!fs.exists("/c.txt"));
        assert!(fs.read_dir("/").unwrap().is_empty());
    }

    #[test]
    fn remove_dir_all_empty_dir() {
        let mut fs = MemFs::new();
        fs.create_dir_all("/empty");
        assert!(fs.remove_dir_all("/empty").is_ok());
        assert!(!fs.exists("/empty"));
    }

    #[test]
    fn set_mode_on_root() {
        let mut fs = MemFs::new();
        fs.set_mode("/", 0o500).unwrap();
        assert_eq!(fs.get("/").unwrap().mode(), 0o500);
    }

    #[test]
    fn read_dir_preserves_child_modes() {
        let mut fs = MemFs::new();
        fs.write_with_mode("/a.txt", "x", 0o444);
        fs.create_dir_all("/d");
        fs.set_mode("/d", 0o700).unwrap();
        let entries = fs.read_dir("/").unwrap();
        let a = entries.iter().find(|e| e.name == "a.txt").unwrap();
        let d = entries.iter().find(|e| e.name == "d").unwrap();
        assert_eq!(a.mode, 0o444);
        assert_eq!(d.mode, 0o700);
    }

    // -- Binary content edge cases ------------------------------------------

    #[test]
    fn file_with_null_bytes() {
        let mut fs = MemFs::new();
        fs.write("/bin", vec![0u8, 0, 0]);
        assert_eq!(fs.read("/bin"), Ok([0u8, 0, 0].as_slice()));
        // Null bytes are valid UTF-8
        assert_eq!(fs.read_to_string("/bin"), Ok("\0\0\0"));
    }

    #[test]
    fn read_returns_exact_bytes() {
        let mut fs = MemFs::new();
        let data: Vec<u8> = (0..=255).collect();
        fs.write("/all_bytes", data.clone());
        assert_eq!(fs.read("/all_bytes").unwrap().len(), 256);
        assert_eq!(fs.read("/all_bytes"), Ok(data.as_slice()));
    }

    // -- Post-deletion consistency ------------------------------------------

    #[test]
    fn paths_after_deletion() {
        let mut fs = MemFs::new();
        fs.create_dir_all("/a/b");
        fs.write("/a/b/f.txt", "x".to_string());
        fs.write("/c.txt", "y".to_string());
        fs.remove_dir_all("/a").unwrap();
        let paths = fs.paths();
        assert!(paths.contains(&"/".to_string()));
        assert!(paths.contains(&"/c.txt".to_string()));
        assert!(!paths.contains(&"/a".to_string()));
        assert!(!paths.contains(&"/a/b".to_string()));
    }

    #[test]
    fn operations_after_remove_middle() {
        let mut fs = MemFs::new();
        fs.create_dir_all("/a/b/c");
        fs.write("/a/b/c/deep.txt", "deep".to_string());
        // Remove middle node
        fs.remove_dir_all("/a/b").unwrap();
        assert!(fs.is_dir("/a"));
        assert!(!fs.exists("/a/b"));
        // Can recreate
        fs.create_dir_all("/a/b/new");
        fs.write("/a/b/new/f.txt", "fresh".to_string());
        assert_eq!(fs.read_to_string("/a/b/new/f.txt"), Ok("fresh"));
        // Old deep path is still gone
        assert!(!fs.exists("/a/b/c"));
    }
}
