use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::fmt;

use crate::dir::DirEntry;
use crate::entry::Entry;
use crate::error::VfsError;
use crate::metadata::Metadata;

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

    /// Read the raw byte content of a file, checking read permission.
    pub fn read(&self, path: &str) -> Result<&[u8], VfsError> {
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

    /// Read the content of a file as a UTF-8 string, checking read permission.
    ///
    /// Returns [`VfsError::InvalidUtf8`] if the content is not valid UTF-8.
    pub fn read_to_string(&self, path: &str) -> Result<&str, VfsError> {
        let bytes = self.read(path)?;
        core::str::from_utf8(bytes).map_err(|_| VfsError::InvalidUtf8)
    }

    /// Get metadata for an entry.
    pub fn metadata(&self, path: &str) -> Result<Metadata, VfsError> {
        match self.entries.get(path) {
            Some(entry) => Ok(Metadata::from_entry(entry)),
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
    pub fn write(&mut self, path: &str, content: impl Into<Vec<u8>>) {
        self.entries.insert(path.to_string(), Entry::file(content));
    }

    /// Write a file with explicit permissions. Overwrites if it exists.
    pub fn write_with_mode(&mut self, path: &str, content: impl Into<Vec<u8>>, mode: u16) {
        self.entries
            .insert(path.to_string(), Entry::file_with_mode(content, mode));
    }

    /// Append data to an existing file. Checks write permission.
    ///
    /// Returns [`VfsError::NotFound`] if the file does not exist.
    pub fn append(&mut self, path: &str, data: &[u8]) -> Result<(), VfsError> {
        match self.entries.get_mut(path) {
            Some(Entry::File { content, mode }) => {
                if *mode & 0o200 == 0 {
                    return Err(VfsError::PermissionDenied);
                }
                content.extend_from_slice(data);
                Ok(())
            }
            Some(Entry::Dir { .. }) => Err(VfsError::IsADirectory),
            None => Err(VfsError::NotFound),
        }
    }

    /// Create a single directory. Fails if the parent does not exist.
    pub fn create_dir(&mut self, path: &str) -> Result<(), VfsError> {
        if self.entries.contains_key(path) {
            return Err(VfsError::AlreadyExists);
        }
        let parent = crate::parent(path).unwrap_or("/");
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
            .or_insert_with(|| Entry::file(Vec::new()));
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
        match self.entries.get_mut(path) {
            Some(Entry::File { mode: m, .. }) | Some(Entry::Dir { mode: m }) => {
                *m = mode;
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
        f.debug_struct("MemFs")
            .field("entries", &self.entries.len())
            .finish()
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
    fn get_returns_entry() {
        let mut fs = MemFs::new();
        fs.write("/f.txt", "data".to_string());
        let e = fs.get("/f.txt").unwrap();
        assert!(e.is_file());
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
        let paths: Vec<&str> = fs.paths().collect();
        assert!(paths.contains(&"/"));
        assert!(paths.contains(&"/a"));
        assert!(paths.contains(&"/a/f.txt"));
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
}
