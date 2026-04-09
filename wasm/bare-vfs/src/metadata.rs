use crate::Entry;

/// File metadata returned by [`crate::MemFs::metadata`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Metadata {
    is_file: bool,
    size: usize,
    mode: u16,
}

impl Metadata {
    pub(crate) fn from_entry(entry: &Entry) -> Self {
        Metadata {
            is_file: entry.is_file(),
            size: entry.len(),
            mode: entry.mode(),
        }
    }

    /// Returns `true` if this entry is a file.
    pub fn is_file(&self) -> bool {
        self.is_file
    }

    /// Returns `true` if this entry is a directory.
    pub fn is_dir(&self) -> bool {
        !self.is_file
    }

    /// Returns the content size in bytes (0 for directories).
    pub fn len(&self) -> usize {
        self.size
    }

    /// Returns `true` if the content is empty (or if this is a directory).
    pub fn is_empty(&self) -> bool {
        self.size == 0
    }

    /// Returns the Unix permission mode.
    pub fn mode(&self) -> u16 {
        self.mode
    }

    /// Returns `true` if the owner read bit is set.
    pub fn is_readable(&self) -> bool {
        self.mode & 0o400 != 0
    }

    /// Returns `true` if the owner write bit is set.
    pub fn is_writable(&self) -> bool {
        self.mode & 0o200 != 0
    }

    /// Returns `true` if any execute bit is set.
    pub fn is_executable(&self) -> bool {
        self.mode & 0o111 != 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_file_entry() {
        let e = Entry::file_with_mode("hello", 0o755);
        let m = Metadata::from_entry(&e);
        assert!(m.is_file());
        assert!(!m.is_dir());
        assert_eq!(m.len(), 5);
        assert!(!m.is_empty());
        assert_eq!(m.mode(), 0o755);
        assert!(m.is_readable());
        assert!(m.is_writable());
        assert!(m.is_executable());
    }

    #[test]
    fn from_dir_entry() {
        let e = Entry::dir_with_mode(0o500);
        let m = Metadata::from_entry(&e);
        assert!(m.is_dir());
        assert!(!m.is_file());
        assert_eq!(m.len(), 0);
        assert!(m.is_empty());
        assert_eq!(m.mode(), 0o500);
        assert!(m.is_readable());
        assert!(!m.is_writable());
        assert!(m.is_executable());
    }
}
