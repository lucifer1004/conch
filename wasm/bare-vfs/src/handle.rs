//! File handles with `Read` and `Seek` support (requires `std` feature).

use alloc::vec::Vec;
use std::io::{self, Cursor, Read, Seek, SeekFrom};

use crate::error::VfsError;
use crate::fs::MemFs;

/// A read-only file handle backed by a copy of the file content.
///
/// Implements [`Read`] and [`Seek`], allowing integration with any library
/// that accepts generic readers.
///
/// Obtained via [`MemFs::open`].
#[derive(Debug)]
pub struct FileHandle {
    cursor: Cursor<Vec<u8>>,
}

impl FileHandle {
    pub(crate) fn new(content: Vec<u8>) -> Self {
        FileHandle {
            cursor: Cursor::new(content),
        }
    }

    /// Consume the handle and return the underlying byte buffer.
    pub fn into_inner(self) -> Vec<u8> {
        self.cursor.into_inner()
    }

    /// Returns the total length of the file content.
    pub fn len(&self) -> usize {
        self.cursor.get_ref().len()
    }

    /// Returns `true` if the file content is empty.
    pub fn is_empty(&self) -> bool {
        self.cursor.get_ref().is_empty()
    }

    /// Returns the current cursor position.
    pub fn position(&self) -> u64 {
        self.cursor.position()
    }
}

impl Read for FileHandle {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.cursor.read(buf)
    }
}

impl Seek for FileHandle {
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        self.cursor.seek(pos)
    }
}

impl MemFs {
    /// Open a file for reading. Returns a [`FileHandle`] that implements
    /// [`Read`] and [`Seek`].
    ///
    /// The handle owns a copy of the file content — mutations to the
    /// filesystem after opening are not reflected in the handle.
    pub fn open(&self, path: &str) -> Result<FileHandle, VfsError> {
        let bytes = self.read(path)?;
        Ok(FileHandle::new(bytes.to_vec()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::{String, ToString};
    use alloc::vec;
    use alloc::vec::Vec;
    use std::io::Read;

    #[test]
    fn open_and_read() {
        let mut fs = MemFs::new();
        fs.write("/f.txt", "hello".to_string());
        let mut h = fs.open("/f.txt").unwrap();
        let mut buf = String::new();
        h.read_to_string(&mut buf).unwrap();
        assert_eq!(buf, "hello");
    }

    #[test]
    fn open_and_seek() {
        let mut fs = MemFs::new();
        fs.write("/f.txt", "abcdef".to_string());
        let mut h = fs.open("/f.txt").unwrap();
        h.seek(SeekFrom::Start(3)).unwrap();
        let mut buf = String::new();
        h.read_to_string(&mut buf).unwrap();
        assert_eq!(buf, "def");
    }

    #[test]
    fn open_missing() {
        let fs = MemFs::new();
        assert!(matches!(fs.open("/nope"), Err(VfsError::NotFound)));
    }

    #[test]
    fn open_permission_denied() {
        let mut fs = MemFs::new();
        fs.write_with_mode("/secret", "x", 0o000);
        assert!(matches!(
            fs.open("/secret"),
            Err(VfsError::PermissionDenied)
        ));
    }

    #[test]
    fn open_binary() {
        let mut fs = MemFs::new();
        fs.write("/bin", vec![0u8, 1, 2, 0xFF]);
        let mut h = fs.open("/bin").unwrap();
        let mut buf = Vec::new();
        h.read_to_end(&mut buf).unwrap();
        assert_eq!(buf, vec![0u8, 1, 2, 0xFF]);
    }

    #[test]
    fn handle_len_and_position() {
        let mut fs = MemFs::new();
        fs.write("/f.txt", "abc".to_string());
        let mut h = fs.open("/f.txt").unwrap();
        assert_eq!(h.len(), 3);
        assert!(!h.is_empty());
        assert_eq!(h.position(), 0);
        let mut buf = [0u8; 2];
        h.read(&mut buf).unwrap();
        assert_eq!(h.position(), 2);
    }

    #[test]
    fn into_inner() {
        let mut fs = MemFs::new();
        fs.write("/f.txt", "data".to_string());
        let h = fs.open("/f.txt").unwrap();
        assert_eq!(h.into_inner(), b"data");
    }
}
