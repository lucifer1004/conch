/// File metadata returned by [`crate::MemFs::metadata`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Metadata {
    is_file: bool,
    is_symlink: bool,
    size: usize,
    mode: u16,
}

impl Metadata {
    pub(crate) fn new(is_file: bool, size: usize, mode: u16) -> Self {
        Metadata {
            is_file,
            is_symlink: false,
            size,
            mode,
        }
    }

    pub(crate) fn new_symlink(mode: u16) -> Self {
        Metadata {
            is_file: false,
            is_symlink: true,
            size: 0,
            mode,
        }
    }

    /// Returns `true` if this entry is a file.
    pub fn is_file(&self) -> bool {
        self.is_file
    }

    /// Returns `true` if this entry is a directory.
    pub fn is_dir(&self) -> bool {
        !self.is_file && !self.is_symlink
    }

    /// Returns `true` if this entry is a symbolic link.
    pub fn is_symlink(&self) -> bool {
        self.is_symlink
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
    fn file_metadata() {
        let m = Metadata::new(true, 5, 0o755);
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
    fn dir_metadata() {
        let m = Metadata::new(false, 0, 0o500);
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
