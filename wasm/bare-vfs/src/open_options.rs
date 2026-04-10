use crate::error::{VfsError, VfsErrorKind};
use crate::fs::MemFs;
use crate::handle::FileHandle;

/// Builder for opening files with specific access modes, mirroring
/// [`std::fs::OpenOptions`].
///
/// Use [`MemFs::open_options`] or `OpenOptions::new()` to create one,
/// chain builder methods, then call `.open()`.
#[derive(Debug, Clone)]
pub struct OpenOptions {
    read: bool,
    write: bool,
    append: bool,
    truncate: bool,
    create: bool,
    create_new: bool,
    mode: u16,
}

impl OpenOptions {
    /// Create a new set of options with all flags set to `false`
    /// and default mode `0o644`.
    pub fn new() -> Self {
        OpenOptions {
            read: false,
            write: false,
            append: false,
            truncate: false,
            create: false,
            create_new: false,
            mode: 0o644,
        }
    }

    /// Set read access. The file must exist unless `create` is set.
    pub fn read(&mut self, read: bool) -> &mut Self {
        self.read = read;
        self
    }

    /// Set write access.
    pub fn write(&mut self, write: bool) -> &mut Self {
        self.write = write;
        self
    }

    /// Set append mode. Implies write. The cursor starts at the end.
    pub fn append(&mut self, append: bool) -> &mut Self {
        self.append = append;
        self
    }

    /// Truncate the file to zero length on open. Requires write.
    pub fn truncate(&mut self, truncate: bool) -> &mut Self {
        self.truncate = truncate;
        self
    }

    /// Create the file if it does not exist. Requires write.
    pub fn create(&mut self, create: bool) -> &mut Self {
        self.create = create;
        self
    }

    /// Create a new file, failing if it already exists.
    pub fn create_new(&mut self, create_new: bool) -> &mut Self {
        self.create_new = create_new;
        self
    }

    /// Set the permission mode for newly created files (default: `0o644`).
    /// Masked by the filesystem's umask.
    pub fn mode(&mut self, mode: u16) -> &mut Self {
        self.mode = mode;
        self
    }

    /// Open the file at `path` on the given filesystem.
    ///
    /// This may create or truncate the file depending on the flags set.
    pub fn open(&self, fs: &mut MemFs, path: &str) -> Result<FileHandle, VfsError> {
        let exists = fs.exists(path);

        // create_new: fail if exists
        if self.create_new && exists {
            return Err(VfsErrorKind::AlreadyExists.into());
        }

        // create or create_new: create if missing
        if (self.create || self.create_new) && !exists {
            if !self.write && !self.append {
                return Err(VfsErrorKind::PermissionDenied.into());
            }
            fs.write_with_mode(path, b"" as &[u8], self.mode);
        } else if !exists {
            return Err(VfsErrorKind::NotFound.into());
        }

        // Check it's a file (not a directory)
        if fs.is_dir(path) {
            return Err(VfsErrorKind::IsADirectory.into());
        }

        // truncate: clear content
        if self.truncate && self.write {
            fs.truncate(path, 0)?;
        }

        // Read the content into a handle
        let bytes = fs.read(path)?;
        let mut handle = FileHandle::new(bytes.to_vec());

        // append: seek to end
        if self.append {
            use std::io::{Seek, SeekFrom};
            handle
                .seek(SeekFrom::End(0))
                .map_err(|_| VfsError::from(VfsErrorKind::NotFound))?;
        }

        Ok(handle)
    }
}

impl Default for OpenOptions {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Seek, SeekFrom, Write};

    #[test]
    fn open_read_only() {
        let mut fs = MemFs::new();
        fs.write("/f.txt", "hello");
        let mut handle = OpenOptions::new()
            .read(true)
            .open(&mut fs, "/f.txt")
            .unwrap();
        let mut buf = alloc::string::String::new();
        handle.read_to_string(&mut buf).unwrap();
        assert_eq!(buf, "hello");
    }

    #[test]
    fn open_missing_without_create_fails() {
        let mut fs = MemFs::new();
        let err = OpenOptions::new()
            .read(true)
            .open(&mut fs, "/nope")
            .unwrap_err();
        assert_eq!(*err.kind(), VfsErrorKind::NotFound);
    }

    #[test]
    fn open_create_new_file() {
        let mut fs = MemFs::new();
        let mut handle = OpenOptions::new()
            .write(true)
            .create(true)
            .open(&mut fs, "/new.txt")
            .unwrap();
        handle.write_all(b"created").unwrap();
        fs.commit("/new.txt", handle);
        assert_eq!(fs.read_to_string("/new.txt").unwrap(), "created");
    }

    #[test]
    fn open_create_new_fails_if_exists() {
        let mut fs = MemFs::new();
        fs.write("/f.txt", "existing");
        let err = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&mut fs, "/f.txt")
            .unwrap_err();
        assert_eq!(*err.kind(), VfsErrorKind::AlreadyExists);
    }

    #[test]
    fn open_truncate() {
        let mut fs = MemFs::new();
        fs.write("/f.txt", "hello world");
        let handle = OpenOptions::new()
            .write(true)
            .truncate(true)
            .open(&mut fs, "/f.txt")
            .unwrap();
        assert_eq!(handle.len(), 0);
    }

    #[test]
    fn open_append_seeks_to_end() {
        let mut fs = MemFs::new();
        fs.write("/f.txt", "hello");
        let mut handle = OpenOptions::new()
            .write(true)
            .append(true)
            .open(&mut fs, "/f.txt")
            .unwrap();
        assert_eq!(handle.position(), 5); // cursor at end
        handle.write_all(b" world").unwrap();
        fs.commit("/f.txt", handle);
        assert_eq!(fs.read_to_string("/f.txt").unwrap(), "hello world");
    }

    #[test]
    fn open_read_write() {
        let mut fs = MemFs::new();
        fs.write("/f.txt", "hello");
        let mut handle = OpenOptions::new()
            .read(true)
            .write(true)
            .open(&mut fs, "/f.txt")
            .unwrap();
        // Read first
        let mut buf = [0u8; 5];
        handle.read_exact(&mut buf).unwrap();
        assert_eq!(&buf, b"hello");
        // Seek back and overwrite
        handle.seek(SeekFrom::Start(0)).unwrap();
        handle.write_all(b"world").unwrap();
        fs.commit("/f.txt", handle);
        assert_eq!(fs.read_to_string("/f.txt").unwrap(), "world");
    }

    #[test]
    fn open_directory_fails() {
        let mut fs = MemFs::new();
        fs.create_dir("/d").unwrap();
        let err = OpenOptions::new()
            .read(true)
            .open(&mut fs, "/d")
            .unwrap_err();
        assert_eq!(*err.kind(), VfsErrorKind::IsADirectory);
    }

    #[test]
    fn open_with_custom_mode() {
        let mut fs = MemFs::new();
        fs.set_umask(0o000); // no masking
        OpenOptions::new()
            .write(true)
            .create(true)
            .mode(0o755)
            .open(&mut fs, "/script.sh")
            .unwrap();
        assert_eq!(fs.metadata("/script.sh").unwrap().mode(), 0o755);
    }
}
