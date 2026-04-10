//! File handles with `Read`, `Write`, and `Seek` support (requires `std` feature).

use alloc::vec::Vec;
use std::io::{self, Cursor, Read, Seek, SeekFrom, Write};

use crate::error::VfsError;
use crate::fs::MemFs;

/// A file handle backed by a copy of the file content.
///
/// Implements [`Read`], [`Write`], and [`Seek`]. The handle operates on an
/// in-memory buffer; call [`MemFs::commit`] to persist changes back to the
/// filesystem.
///
/// **Important**: Writes are NOT automatically persisted. You must call
/// [`MemFs::commit`] to save changes back to the filesystem. Dropping a
/// written handle without committing will silently discard all changes.
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

impl Write for FileHandle {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.cursor.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.cursor.flush()
    }
}

impl MemFs {
    /// Create an [`OpenOptions`] builder for opening files with specific access modes.
    ///
    /// # Example
    /// ```ignore
    /// let mut handle = MemFs::open_options()
    ///     .read(true)
    ///     .write(true)
    ///     .create(true)
    ///     .open(&mut fs, "/path")?;
    /// ```
    pub fn open_options() -> crate::open_options::OpenOptions {
        crate::open_options::OpenOptions::new()
    }

    /// Open a file for reading. Returns a [`FileHandle`] that implements
    /// [`Read`] and [`Seek`].
    ///
    /// The handle owns a copy of the file content — mutations to the
    /// filesystem after opening are not reflected in the handle.
    pub fn open(&self, path: &str) -> Result<FileHandle, VfsError> {
        let bytes = self.read(path)?;
        Ok(FileHandle::new(bytes.to_vec()))
    }

    /// Persist the contents of a [`FileHandle`] back to the filesystem,
    /// overwriting the file at `path`.
    pub fn commit(&mut self, path: &str, handle: FileHandle) {
        self.write(path, handle.into_inner());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::VfsErrorKind;
    use alloc::string::{String, ToString};
    use alloc::vec;
    use alloc::vec::Vec;
    use std::io::{Read, Write};

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
        let err = fs.open("/nope").unwrap_err();
        assert_eq!(*err.kind(), VfsErrorKind::NotFound);
    }

    #[test]
    fn open_permission_denied() {
        let mut fs = MemFs::new();
        fs.write_with_mode("/secret", "x", 0o000);
        fs.set_current_user(1000, 1000);
        let err = fs.open("/secret").unwrap_err();
        assert_eq!(*err.kind(), VfsErrorKind::PermissionDenied);
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
        h.read_exact(&mut buf).unwrap();
        assert_eq!(h.position(), 2);
    }

    #[test]
    fn into_inner() {
        let mut fs = MemFs::new();
        fs.write("/f.txt", "data".to_string());
        let h = fs.open("/f.txt").unwrap();
        assert_eq!(h.into_inner(), b"data");
    }

    #[test]
    fn write_and_commit() {
        let mut fs = MemFs::new();
        fs.write("/f.txt", "hello");
        let mut h = fs.open("/f.txt").unwrap();
        // Seek to end and append
        h.seek(SeekFrom::End(0)).unwrap();
        h.write_all(b" world").unwrap();
        fs.commit("/f.txt", h);
        assert_eq!(fs.read_to_string("/f.txt").unwrap(), "hello world");
    }

    #[test]
    fn write_at_position() {
        let mut fs = MemFs::new();
        fs.write("/f.txt", "hello");
        let mut h = fs.open("/f.txt").unwrap();
        h.seek(SeekFrom::Start(1)).unwrap();
        h.write_all(b"a").unwrap();
        fs.commit("/f.txt", h);
        assert_eq!(fs.read_to_string("/f.txt").unwrap(), "hallo");
    }

    #[test]
    fn write_extends_buffer() {
        let mut fs = MemFs::new();
        fs.write("/f.txt", "hi");
        let mut h = fs.open("/f.txt").unwrap();
        h.seek(SeekFrom::End(0)).unwrap();
        h.write_all(b"!!").unwrap();
        assert_eq!(h.len(), 4);
        fs.commit("/f.txt", h);
        assert_eq!(fs.read_to_string("/f.txt").unwrap(), "hi!!");
    }

    #[test]
    fn commit_to_different_path() {
        let mut fs = MemFs::new();
        fs.write("/src.txt", "original");
        let h = fs.open("/src.txt").unwrap();
        fs.commit("/dst.txt", h);
        assert_eq!(fs.read_to_string("/dst.txt").unwrap(), "original");
        assert_eq!(fs.read_to_string("/src.txt").unwrap(), "original");
    }
}
