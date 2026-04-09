//! A minimal, synchronous, in-memory virtual filesystem.
//!
//! `bare-vfs` provides a [`MemFs`] backed by a `BTreeMap<String, Entry>` that works
//! everywhere Rust compiles — including `wasm32-unknown-unknown` and `no_std` targets.
//!
//! # Quick start
//!
//! ```
//! use bare_vfs::MemFs;
//!
//! let mut fs = MemFs::new();
//! fs.create_dir_all("/src/bin");
//! fs.write("/src/main.rs", "fn main() {}".into());
//!
//! assert!(fs.is_file("/src/main.rs"));
//! assert_eq!(fs.read_to_string("/src/main.rs").unwrap(), "fn main() {}");
//! ```

#![no_std]
extern crate alloc;

use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::fmt;

// ---------------------------------------------------------------------------
// Error
// ---------------------------------------------------------------------------

/// Errors returned by filesystem operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VfsError {
    /// The path does not exist.
    NotFound,
    /// Expected a file but found a directory.
    IsADirectory,
    /// Expected a directory but found a file.
    NotADirectory,
    /// Insufficient permissions for the operation.
    PermissionDenied,
    /// An entry already exists at the path.
    AlreadyExists,
}

impl fmt::Display for VfsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            VfsError::NotFound => f.write_str("No such file or directory"),
            VfsError::IsADirectory => f.write_str("Is a directory"),
            VfsError::NotADirectory => f.write_str("Not a directory"),
            VfsError::PermissionDenied => f.write_str("Permission denied"),
            VfsError::AlreadyExists => f.write_str("File exists"),
        }
    }
}

// ---------------------------------------------------------------------------
// Entry
// ---------------------------------------------------------------------------

/// A node in the virtual filesystem — either a file or a directory.
#[derive(Debug, Clone)]
pub enum Entry {
    /// A regular file with UTF-8 content and a Unix permission mode.
    File { content: String, mode: u16 },
    /// A directory with a Unix permission mode.
    Dir { mode: u16 },
}

impl Entry {
    /// Create a file with default permissions (`0o644`).
    pub fn file(content: String) -> Self {
        Entry::File {
            content,
            mode: 0o644,
        }
    }

    /// Create a file with explicit permissions.
    pub fn file_with_mode(content: String, mode: u16) -> Self {
        Entry::File { content, mode }
    }

    /// Create a directory with default permissions (`0o755`).
    pub fn dir() -> Self {
        Entry::Dir { mode: 0o755 }
    }

    /// Create a directory with explicit permissions.
    pub fn dir_with_mode(mode: u16) -> Self {
        Entry::Dir { mode }
    }

    /// Returns `true` if this entry is a directory.
    pub fn is_dir(&self) -> bool {
        matches!(self, Entry::Dir { .. })
    }

    /// Returns `true` if this entry is a file.
    pub fn is_file(&self) -> bool {
        matches!(self, Entry::File { .. })
    }

    /// Returns the file content, or `None` for directories.
    pub fn content(&self) -> Option<&str> {
        match self {
            Entry::File { content, .. } => Some(content),
            Entry::Dir { .. } => None,
        }
    }

    /// Returns the Unix permission mode.
    pub fn mode(&self) -> u16 {
        match self {
            Entry::File { mode, .. } | Entry::Dir { mode, .. } => *mode,
        }
    }

    /// Returns `true` if the owner read bit is set.
    pub fn is_readable(&self) -> bool {
        self.mode() & 0o400 != 0
    }

    /// Returns `true` if the owner write bit is set.
    pub fn is_writable(&self) -> bool {
        self.mode() & 0o200 != 0
    }

    /// Returns `true` if any execute bit is set.
    pub fn is_executable(&self) -> bool {
        self.mode() & 0o111 != 0
    }

    /// Format a mode as a Unix permission string (e.g., `"rwxr-xr-x"`).
    pub fn format_mode(mode: u16) -> String {
        let mut s = String::with_capacity(9);
        for shift in [6, 3, 0] {
            let bits = (mode >> shift) & 0o7;
            s.push(if bits & 4 != 0 { 'r' } else { '-' });
            s.push(if bits & 2 != 0 { 'w' } else { '-' });
            s.push(if bits & 1 != 0 { 'x' } else { '-' });
        }
        s
    }
}

// ---------------------------------------------------------------------------
// DirEntry
// ---------------------------------------------------------------------------

/// A single entry returned by [`MemFs::read_dir`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirEntry {
    /// File or directory name (no leading slash).
    pub name: String,
    /// `true` when the entry is a directory.
    pub is_dir: bool,
    /// Unix permission mode of the entry.
    pub mode: u16,
}

// ---------------------------------------------------------------------------
// MemFs
// ---------------------------------------------------------------------------

/// An in-memory virtual filesystem backed by a sorted `BTreeMap`.
///
/// Paths are `/`-separated strings. Every stored path is absolute; the root
/// directory `/` always exists.
pub struct MemFs {
    entries: BTreeMap<String, Entry>,
}

impl MemFs {
    /// Create a new filesystem containing only the root directory `/`.
    pub fn new() -> Self {
        let mut entries = BTreeMap::new();
        entries.insert("/".to_string(), Entry::dir());
        MemFs { entries }
    }

    // -- Queries ------------------------------------------------------------

    /// Get a reference to the entry at `path`, if it exists.
    pub fn get(&self, path: &str) -> Option<&Entry> {
        self.entries.get(path)
    }

    /// Returns `true` if an entry exists at `path`.
    pub fn exists(&self, path: &str) -> bool {
        self.entries.contains_key(path)
    }

    /// Returns `true` if `path` is a file.
    pub fn is_file(&self, path: &str) -> bool {
        self.entries.get(path).is_some_and(|e| e.is_file())
    }

    /// Returns `true` if `path` is a directory.
    pub fn is_dir(&self, path: &str) -> bool {
        self.entries.get(path).is_some_and(|e| e.is_dir())
    }

    /// Read the content of a file, checking read permission.
    ///
    /// Returns [`VfsError::NotFound`] if the path does not exist,
    /// [`VfsError::IsADirectory`] if it is a directory, or
    /// [`VfsError::PermissionDenied`] if the owner read bit is not set.
    pub fn read_to_string(&self, path: &str) -> Result<&str, VfsError> {
        match self.entries.get(path) {
            Some(Entry::File { content, mode }) => {
                if mode & 0o400 == 0 {
                    Err(VfsError::PermissionDenied)
                } else {
                    Ok(content)
                }
            }
            Some(Entry::Dir { .. }) => Err(VfsError::IsADirectory),
            None => Err(VfsError::NotFound),
        }
    }

    // -- Directory listing --------------------------------------------------

    /// List the direct children of a directory, sorted by name.
    pub fn read_dir(&self, dir: &str) -> Result<Vec<DirEntry>, VfsError> {
        match self.entries.get(dir) {
            Some(e) if e.is_dir() => {}
            Some(_) => return Err(VfsError::NotADirectory),
            None => return Err(VfsError::NotFound),
        }

        let prefix = if dir == "/" {
            "/".into()
        } else {
            format!("{}/", dir)
        };

        let mut result = Vec::new();
        for (p, entry) in &self.entries {
            if p == dir || !p.starts_with(&prefix) {
                continue;
            }
            let rel = &p[prefix.len()..];
            if rel.is_empty() || rel.contains('/') {
                continue;
            }
            result.push(DirEntry {
                name: rel.to_string(),
                is_dir: entry.is_dir(),
                mode: entry.mode(),
            });
        }
        result.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(result)
    }

    // -- Iteration ----------------------------------------------------------

    /// Iterate over all `(path, entry)` pairs.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &Entry)> {
        self.entries.iter().map(|(k, v)| (k.as_str(), v))
    }

    /// Iterate over all stored paths.
    pub fn paths(&self) -> impl Iterator<Item = &str> {
        self.entries.keys().map(|s| s.as_str())
    }

    // -- Mutations ----------------------------------------------------------

    /// Insert an entry at the given path.
    ///
    /// This is a low-level method; it does **not** create parent directories.
    pub fn insert(&mut self, path: String, entry: Entry) {
        self.entries.insert(path, entry);
    }

    /// Remove the entry at `path` and return it, if it existed.
    pub fn remove(&mut self, path: &str) -> Option<Entry> {
        self.entries.remove(path)
    }

    /// Write a file with default permissions (`0o644`). Overwrites if it exists.
    pub fn write(&mut self, path: &str, content: String) {
        self.entries.insert(path.to_string(), Entry::file(content));
    }

    /// Write a file with explicit permissions. Overwrites if it exists.
    pub fn write_with_mode(&mut self, path: &str, content: String, mode: u16) {
        self.entries
            .insert(path.to_string(), Entry::file_with_mode(content, mode));
    }

    /// Create a single directory. Fails if the parent does not exist.
    pub fn create_dir(&mut self, path: &str) -> Result<(), VfsError> {
        if self.entries.contains_key(path) {
            return Err(VfsError::AlreadyExists);
        }
        let parent = Self::parent(path).unwrap_or("/");
        if !self.is_dir(parent) {
            return Err(VfsError::NotFound);
        }
        self.entries.insert(path.to_string(), Entry::dir());
        Ok(())
    }

    /// Create a directory and all missing ancestors.
    pub fn create_dir_all(&mut self, path: &str) {
        let mut current = String::new();
        for part in path.split('/').filter(|s| !s.is_empty()) {
            current.push('/');
            current.push_str(part);
            self.entries
                .entry(current.clone())
                .or_insert_with(Entry::dir);
        }
    }

    /// Create an empty file if `path` does not already exist.
    pub fn touch(&mut self, path: &str) {
        self.entries
            .entry(path.to_string())
            .or_insert_with(|| Entry::file(String::new()));
    }

    /// Remove a directory and everything beneath it.
    pub fn remove_dir_all(&mut self, path: &str) -> Result<(), VfsError> {
        match self.entries.get(path) {
            Some(e) if e.is_dir() => {}
            Some(_) => return Err(VfsError::NotADirectory),
            None => return Err(VfsError::NotFound),
        }
        let prefix = format!("{}/", path);
        let to_remove: Vec<String> = self
            .entries
            .keys()
            .filter(|k| *k == path || k.starts_with(&prefix))
            .cloned()
            .collect();
        for k in to_remove {
            self.entries.remove(&k);
        }
        Ok(())
    }

    /// Set the permission mode on an existing entry.
    pub fn set_mode(&mut self, path: &str, mode: u16) -> Result<(), VfsError> {
        match self.entries.get(path).cloned() {
            Some(Entry::File { content, .. }) => {
                self.entries
                    .insert(path.to_string(), Entry::file_with_mode(content, mode));
                Ok(())
            }
            Some(Entry::Dir { .. }) => {
                self.entries
                    .insert(path.to_string(), Entry::dir_with_mode(mode));
                Ok(())
            }
            None => Err(VfsError::NotFound),
        }
    }

    /// Copy a file. Checks read permission on the source.
    pub fn copy(&mut self, src: &str, dst: &str) -> Result<(), VfsError> {
        let entry = match self.entries.get(src) {
            Some(Entry::File { content, mode }) => {
                if mode & 0o400 == 0 {
                    return Err(VfsError::PermissionDenied);
                }
                Entry::file(content.clone())
            }
            Some(Entry::Dir { .. }) => return Err(VfsError::IsADirectory),
            None => return Err(VfsError::NotFound),
        };
        self.entries.insert(dst.to_string(), entry);
        Ok(())
    }

    /// Move (rename) an entry from `src` to `dst`.
    pub fn rename(&mut self, src: &str, dst: &str) -> Result<(), VfsError> {
        match self.entries.remove(src) {
            Some(entry) => {
                self.entries.insert(dst.to_string(), entry);
                Ok(())
            }
            None => Err(VfsError::NotFound),
        }
    }

    // -- Path utilities (pure functions) ------------------------------------

    /// Normalize an absolute path: resolve `.` and `..`, collapse separators.
    ///
    /// ```
    /// use bare_vfs::MemFs;
    /// assert_eq!(MemFs::normalize("/a/b/../c/./d"), "/a/c/d");
    /// assert_eq!(MemFs::normalize("/"), "/");
    /// ```
    pub fn normalize(path: &str) -> String {
        let mut parts: Vec<&str> = Vec::new();
        for seg in path.split('/') {
            match seg {
                "" | "." => {}
                ".." => {
                    parts.pop();
                }
                _ => parts.push(seg),
            }
        }
        if parts.is_empty() {
            "/".to_string()
        } else {
            let mut result = String::new();
            for part in &parts {
                result.push('/');
                result.push_str(part);
            }
            result
        }
    }

    /// Return the parent directory path, or `None` for the root.
    ///
    /// ```
    /// use bare_vfs::MemFs;
    /// assert_eq!(MemFs::parent("/a/b"), Some("/a"));
    /// assert_eq!(MemFs::parent("/a"), Some("/"));
    /// assert_eq!(MemFs::parent("/"), None);
    /// ```
    pub fn parent(path: &str) -> Option<&str> {
        if path == "/" {
            return None;
        }
        path.rsplit_once('/')
            .map(|(p, _)| if p.is_empty() { "/" } else { p })
    }
}

impl Default for MemFs {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Debug for MemFs {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MemFs")
            .field("entries", &self.entries.len())
            .finish()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_has_root() {
        let fs = MemFs::new();
        assert!(fs.is_dir("/"));
    }

    #[test]
    fn write_and_read() {
        let mut fs = MemFs::new();
        fs.write("/hello.txt", "world".into());
        assert_eq!(fs.read_to_string("/hello.txt"), Ok("world"));
    }

    #[test]
    fn read_missing() {
        let fs = MemFs::new();
        assert_eq!(fs.read_to_string("/nope"), Err(VfsError::NotFound));
    }

    #[test]
    fn read_dir_entry() {
        let mut fs = MemFs::new();
        fs.create_dir_all("/a");
        assert_eq!(fs.read_to_string("/a"), Err(VfsError::IsADirectory));
    }

    #[test]
    fn read_permission_denied() {
        let mut fs = MemFs::new();
        fs.write_with_mode("/secret", "x".into(), 0o000);
        assert_eq!(
            fs.read_to_string("/secret"),
            Err(VfsError::PermissionDenied)
        );
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
    fn touch_creates_empty_file() {
        let mut fs = MemFs::new();
        fs.touch("/new.txt");
        assert_eq!(fs.read_to_string("/new.txt"), Ok(""));
    }

    #[test]
    fn touch_does_not_overwrite() {
        let mut fs = MemFs::new();
        fs.write("/f.txt", "data".into());
        fs.touch("/f.txt");
        assert_eq!(fs.read_to_string("/f.txt"), Ok("data"));
    }

    #[test]
    fn remove_file() {
        let mut fs = MemFs::new();
        fs.write("/f.txt", "x".into());
        assert!(fs.remove("/f.txt").is_some());
        assert!(!fs.exists("/f.txt"));
    }

    #[test]
    fn remove_dir_all_recursive() {
        let mut fs = MemFs::new();
        fs.create_dir_all("/a/b");
        fs.write("/a/b/f.txt", "x".into());
        fs.write("/a/g.txt", "y".into());
        assert!(fs.remove_dir_all("/a").is_ok());
        assert!(!fs.exists("/a"));
        assert!(!fs.exists("/a/b"));
        assert!(!fs.exists("/a/b/f.txt"));
    }

    #[test]
    fn set_mode() {
        let mut fs = MemFs::new();
        fs.write("/f.txt", "x".into());
        assert!(fs.set_mode("/f.txt", 0o000).is_ok());
        assert_eq!(fs.read_to_string("/f.txt"), Err(VfsError::PermissionDenied));
        assert!(fs.set_mode("/f.txt", 0o644).is_ok());
        assert_eq!(fs.read_to_string("/f.txt"), Ok("x"));
    }

    #[test]
    fn copy_file() {
        let mut fs = MemFs::new();
        fs.write("/a.txt", "hello".into());
        assert!(fs.copy("/a.txt", "/b.txt").is_ok());
        assert_eq!(fs.read_to_string("/b.txt"), Ok("hello"));
        // source still exists
        assert_eq!(fs.read_to_string("/a.txt"), Ok("hello"));
    }

    #[test]
    fn rename_file() {
        let mut fs = MemFs::new();
        fs.write("/old.txt", "data".into());
        assert!(fs.rename("/old.txt", "/new.txt").is_ok());
        assert!(!fs.exists("/old.txt"));
        assert_eq!(fs.read_to_string("/new.txt"), Ok("data"));
    }

    #[test]
    fn normalize_paths() {
        assert_eq!(MemFs::normalize("/a/b/../c/./d"), "/a/c/d");
        assert_eq!(MemFs::normalize("/a/b/../../"), "/");
        assert_eq!(MemFs::normalize("/"), "/");
        assert_eq!(MemFs::normalize("///a///b///"), "/a/b");
    }

    #[test]
    fn parent_paths() {
        assert_eq!(MemFs::parent("/a/b"), Some("/a"));
        assert_eq!(MemFs::parent("/a"), Some("/"));
        assert_eq!(MemFs::parent("/"), None);
    }

    #[test]
    fn format_mode_string() {
        assert_eq!(Entry::format_mode(0o755), "rwxr-xr-x");
        assert_eq!(Entry::format_mode(0o644), "rw-r--r--");
        assert_eq!(Entry::format_mode(0o000), "---------");
    }

    #[test]
    fn read_dir_sorted() {
        let mut fs = MemFs::new();
        fs.write("/c.txt", "".into());
        fs.write("/a.txt", "".into());
        fs.write("/b.txt", "".into());
        let entries = fs.read_dir("/").unwrap();
        let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, &["a.txt", "b.txt", "c.txt"]);
    }

    #[test]
    fn iter_all_entries() {
        let mut fs = MemFs::new();
        fs.create_dir_all("/a");
        fs.write("/a/f.txt", "x".into());
        let paths: Vec<&str> = fs.paths().collect();
        assert!(paths.contains(&"/"));
        assert!(paths.contains(&"/a"));
        assert!(paths.contains(&"/a/f.txt"));
    }

    // -- Entry method tests -------------------------------------------------

    #[test]
    fn entry_file_defaults() {
        let e = Entry::file("hello".into());
        assert!(e.is_file());
        assert!(!e.is_dir());
        assert_eq!(e.content(), Some("hello"));
        assert_eq!(e.mode(), 0o644);
        assert!(e.is_readable());
        assert!(e.is_writable());
        assert!(!e.is_executable());
    }

    #[test]
    fn entry_dir_defaults() {
        let e = Entry::dir();
        assert!(e.is_dir());
        assert!(!e.is_file());
        assert_eq!(e.content(), None);
        assert_eq!(e.mode(), 0o755);
        assert!(e.is_readable());
        assert!(e.is_writable());
        assert!(e.is_executable());
    }

    #[test]
    fn entry_file_with_mode() {
        let e = Entry::file_with_mode("x".into(), 0o400);
        assert!(e.is_readable());
        assert!(!e.is_writable());
        assert!(!e.is_executable());
    }

    #[test]
    fn entry_dir_with_mode() {
        let e = Entry::dir_with_mode(0o500);
        assert_eq!(e.mode(), 0o500);
        assert!(e.is_readable());
        assert!(!e.is_writable());
        assert!(e.is_executable());
    }

    #[test]
    fn format_mode_all_combos() {
        assert_eq!(Entry::format_mode(0o777), "rwxrwxrwx");
        assert_eq!(Entry::format_mode(0o100), "--x------");
        assert_eq!(Entry::format_mode(0o421), "r---w---x");
    }

    // -- Query edge cases ---------------------------------------------------

    #[test]
    fn exists_and_type_checks() {
        let mut fs = MemFs::new();
        fs.write("/f.txt", "".into());
        fs.create_dir_all("/d");

        assert!(fs.exists("/f.txt"));
        assert!(fs.exists("/d"));
        assert!(!fs.exists("/nope"));

        assert!(fs.is_file("/f.txt"));
        assert!(!fs.is_file("/d"));
        assert!(!fs.is_file("/nope"));

        assert!(fs.is_dir("/d"));
        assert!(!fs.is_dir("/f.txt"));
        assert!(!fs.is_dir("/nope"));
    }

    #[test]
    fn get_returns_entry() {
        let mut fs = MemFs::new();
        fs.write("/f.txt", "data".into());
        let e = fs.get("/f.txt").unwrap();
        assert!(e.is_file());
        assert_eq!(e.content(), Some("data"));
        assert!(fs.get("/missing").is_none());
    }

    // -- write / write_with_mode edge cases ---------------------------------

    #[test]
    fn write_overwrites_existing() {
        let mut fs = MemFs::new();
        fs.write("/f.txt", "old".into());
        fs.write("/f.txt", "new".into());
        assert_eq!(fs.read_to_string("/f.txt"), Ok("new"));
    }

    #[test]
    fn write_with_mode_sets_permissions() {
        let mut fs = MemFs::new();
        fs.write_with_mode("/f.txt", "secret".into(), 0o000);
        assert_eq!(
            fs.read_to_string("/f.txt"),
            Err(VfsError::PermissionDenied)
        );
        assert_eq!(fs.get("/f.txt").unwrap().mode(), 0o000);
    }

    // -- create_dir edge cases ----------------------------------------------

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
        let fs = &mut MemFs::new();
        assert_eq!(fs.create_dir("/a/b"), Err(VfsError::NotFound));
    }

    #[test]
    fn create_dir_all_idempotent() {
        let mut fs = MemFs::new();
        fs.create_dir_all("/a/b/c");
        fs.create_dir_all("/a/b/c"); // should not panic or error
        assert!(fs.is_dir("/a/b/c"));
    }

    // -- remove edge cases --------------------------------------------------

    #[test]
    fn remove_nonexistent_returns_none() {
        let mut fs = MemFs::new();
        assert!(fs.remove("/nope").is_none());
    }

    #[test]
    fn remove_dir_all_not_found() {
        let mut fs = MemFs::new();
        assert_eq!(fs.remove_dir_all("/nope"), Err(VfsError::NotFound));
    }

    #[test]
    fn remove_dir_all_on_file() {
        let mut fs = MemFs::new();
        fs.write("/f.txt", "x".into());
        assert_eq!(fs.remove_dir_all("/f.txt"), Err(VfsError::NotADirectory));
    }

    #[test]
    fn remove_dir_all_preserves_siblings() {
        let mut fs = MemFs::new();
        fs.create_dir_all("/a/target");
        fs.write("/a/target/f.txt", "x".into());
        fs.write("/a/sibling.txt", "keep".into());
        fs.remove_dir_all("/a/target").unwrap();
        assert!(!fs.exists("/a/target"));
        assert_eq!(fs.read_to_string("/a/sibling.txt"), Ok("keep"));
    }

    // -- set_mode edge cases ------------------------------------------------

    #[test]
    fn set_mode_on_dir() {
        let mut fs = MemFs::new();
        fs.create_dir_all("/d");
        assert!(fs.set_mode("/d", 0o500).is_ok());
        assert_eq!(fs.get("/d").unwrap().mode(), 0o500);
    }

    #[test]
    fn set_mode_not_found() {
        let mut fs = MemFs::new();
        assert_eq!(fs.set_mode("/nope", 0o644), Err(VfsError::NotFound));
    }

    // -- copy edge cases ----------------------------------------------------

    #[test]
    fn copy_not_found() {
        let mut fs = MemFs::new();
        assert_eq!(fs.copy("/nope", "/dst"), Err(VfsError::NotFound));
    }

    #[test]
    fn copy_directory_error() {
        let mut fs = MemFs::new();
        fs.create_dir_all("/d");
        assert_eq!(fs.copy("/d", "/d2"), Err(VfsError::IsADirectory));
    }

    #[test]
    fn copy_permission_denied() {
        let mut fs = MemFs::new();
        fs.write_with_mode("/secret", "x".into(), 0o000);
        assert_eq!(
            fs.copy("/secret", "/dst"),
            Err(VfsError::PermissionDenied)
        );
    }

    #[test]
    fn copy_overwrites_destination() {
        let mut fs = MemFs::new();
        fs.write("/src", "new".into());
        fs.write("/dst", "old".into());
        assert!(fs.copy("/src", "/dst").is_ok());
        assert_eq!(fs.read_to_string("/dst"), Ok("new"));
    }

    // -- rename edge cases --------------------------------------------------

    #[test]
    fn rename_not_found() {
        let mut fs = MemFs::new();
        assert_eq!(fs.rename("/nope", "/dst"), Err(VfsError::NotFound));
    }

    #[test]
    fn rename_overwrites_destination() {
        let mut fs = MemFs::new();
        fs.write("/src", "new".into());
        fs.write("/dst", "old".into());
        assert!(fs.rename("/src", "/dst").is_ok());
        assert_eq!(fs.read_to_string("/dst"), Ok("new"));
        assert!(!fs.exists("/src"));
    }

    // -- read_dir edge cases ------------------------------------------------

    #[test]
    fn read_dir_not_found() {
        let fs = MemFs::new();
        assert_eq!(fs.read_dir("/nope"), Err(VfsError::NotFound));
    }

    #[test]
    fn read_dir_on_file() {
        let mut fs = MemFs::new();
        fs.write("/f.txt", "x".into());
        assert_eq!(fs.read_dir("/f.txt"), Err(VfsError::NotADirectory));
    }

    #[test]
    fn read_dir_empty() {
        let mut fs = MemFs::new();
        fs.create_dir_all("/empty");
        let entries = fs.read_dir("/empty").unwrap();
        assert!(entries.is_empty());
    }

    #[test]
    fn read_dir_skips_nested() {
        let mut fs = MemFs::new();
        fs.create_dir_all("/a/b/c");
        fs.write("/a/b/c/f.txt", "x".into());
        // read_dir("/a") should only show "b", not "b/c" or "b/c/f.txt"
        let entries = fs.read_dir("/a").unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "b");
    }

    #[test]
    fn read_dir_root_with_mixed_entries() {
        let mut fs = MemFs::new();
        fs.write("/file.txt", "".into());
        fs.create_dir_all("/dir");
        let entries = fs.read_dir("/").unwrap();
        let file_entry = entries.iter().find(|e| e.name == "file.txt").unwrap();
        let dir_entry = entries.iter().find(|e| e.name == "dir").unwrap();
        assert!(!file_entry.is_dir);
        assert!(dir_entry.is_dir);
    }

    // -- insert (low-level) -------------------------------------------------

    #[test]
    fn insert_raw_entry() {
        let mut fs = MemFs::new();
        fs.insert("/custom".into(), Entry::file_with_mode("data".into(), 0o755));
        let e = fs.get("/custom").unwrap();
        assert_eq!(e.content(), Some("data"));
        assert!(e.is_executable());
    }

    // -- normalize edge cases -----------------------------------------------

    #[test]
    fn normalize_dotdot_beyond_root() {
        // Going above root should clamp to root
        assert_eq!(MemFs::normalize("/a/../../.."), "/");
    }

    #[test]
    fn normalize_relative_components() {
        assert_eq!(MemFs::normalize("/a/./b/./c"), "/a/b/c");
    }

    #[test]
    fn normalize_empty_string() {
        assert_eq!(MemFs::normalize(""), "/");
    }

    // -- parent edge cases --------------------------------------------------

    #[test]
    fn parent_deeply_nested() {
        assert_eq!(MemFs::parent("/a/b/c/d"), Some("/a/b/c"));
    }

    // -- Default trait -------------------------------------------------------

    #[test]
    fn default_has_root() {
        let fs = MemFs::default();
        assert!(fs.is_dir("/"));
    }

    // -- VfsError Display ---------------------------------------------------

    #[test]
    fn error_display_messages() {
        assert_eq!(VfsError::NotFound.to_string(), "No such file or directory");
        assert_eq!(VfsError::IsADirectory.to_string(), "Is a directory");
        assert_eq!(VfsError::NotADirectory.to_string(), "Not a directory");
        assert_eq!(VfsError::PermissionDenied.to_string(), "Permission denied");
        assert_eq!(VfsError::AlreadyExists.to_string(), "File exists");
    }
}
