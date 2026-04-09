use alloc::string::String;
use alloc::vec::Vec;

/// A node in the virtual filesystem — either a file or a directory.
#[derive(Debug, Clone)]
pub enum Entry {
    /// A regular file with byte content and a Unix permission mode.
    File { content: Vec<u8>, mode: u16 },
    /// A directory with a Unix permission mode.
    Dir { mode: u16 },
}

impl Entry {
    /// Create a file with default permissions (`0o644`).
    pub fn file(content: impl Into<Vec<u8>>) -> Self {
        Entry::File {
            content: content.into(),
            mode: 0o644,
        }
    }

    /// Create a file with explicit permissions.
    pub fn file_with_mode(content: impl Into<Vec<u8>>, mode: u16) -> Self {
        Entry::File {
            content: content.into(),
            mode,
        }
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

    /// Returns the raw file content as bytes, or `None` for directories.
    pub fn content(&self) -> Option<&[u8]> {
        match self {
            Entry::File { content, .. } => Some(content),
            Entry::Dir { .. } => None,
        }
    }

    /// Returns the file content as a UTF-8 string, or `None` for directories
    /// or files with invalid UTF-8.
    pub fn content_str(&self) -> Option<&str> {
        match self {
            Entry::File { content, .. } => core::str::from_utf8(content).ok(),
            Entry::Dir { .. } => None,
        }
    }

    /// Returns the size of the file content in bytes, or 0 for directories.
    pub fn len(&self) -> usize {
        match self {
            Entry::File { content, .. } => content.len(),
            Entry::Dir { .. } => 0,
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

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::ToString;
    use alloc::vec;

    #[test]
    fn file_defaults() {
        let e = Entry::file("hello");
        assert!(e.is_file());
        assert!(!e.is_dir());
        assert_eq!(e.content(), Some(b"hello".as_slice()));
        assert_eq!(e.content_str(), Some("hello"));
        assert_eq!(e.len(), 5);
        assert_eq!(e.mode(), 0o644);
        assert!(e.is_readable());
        assert!(e.is_writable());
        assert!(!e.is_executable());
    }

    #[test]
    fn file_from_string() {
        let e = Entry::file("data".to_string());
        assert_eq!(e.content_str(), Some("data"));
    }

    #[test]
    fn file_from_bytes() {
        let e = Entry::file(vec![0u8, 1, 2, 0xFF]);
        assert_eq!(e.content(), Some([0u8, 1, 2, 0xFF].as_slice()));
        assert_eq!(e.content_str(), None); // not valid UTF-8
    }

    #[test]
    fn dir_defaults() {
        let e = Entry::dir();
        assert!(e.is_dir());
        assert!(!e.is_file());
        assert_eq!(e.content(), None);
        assert_eq!(e.content_str(), None);
        assert_eq!(e.len(), 0);
        assert_eq!(e.mode(), 0o755);
        assert!(e.is_readable());
        assert!(e.is_writable());
        assert!(e.is_executable());
    }

    #[test]
    fn file_with_mode() {
        let e = Entry::file_with_mode("x", 0o400);
        assert!(e.is_readable());
        assert!(!e.is_writable());
        assert!(!e.is_executable());
    }

    #[test]
    fn dir_with_mode() {
        let e = Entry::dir_with_mode(0o500);
        assert_eq!(e.mode(), 0o500);
        assert!(e.is_readable());
        assert!(!e.is_writable());
        assert!(e.is_executable());
    }

    #[test]
    fn format_mode_strings() {
        assert_eq!(Entry::format_mode(0o755), "rwxr-xr-x");
        assert_eq!(Entry::format_mode(0o644), "rw-r--r--");
        assert_eq!(Entry::format_mode(0o000), "---------");
        assert_eq!(Entry::format_mode(0o777), "rwxrwxrwx");
        assert_eq!(Entry::format_mode(0o100), "--x------");
        assert_eq!(Entry::format_mode(0o421), "r---w---x");
    }
}
