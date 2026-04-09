use alloc::string::String;
use alloc::vec::Vec;

/// A node in the virtual filesystem — either a file, directory, or symlink.
#[derive(Debug, Clone)]
pub enum Entry {
    /// A regular file with byte content and a Unix permission mode.
    File {
        content: Vec<u8>,
        mode: u16,
        uid: u32,
        gid: u32,
        mtime: u64,
        ctime: u64,
    },
    /// A directory with a Unix permission mode.
    Dir {
        mode: u16,
        uid: u32,
        gid: u32,
        mtime: u64,
        ctime: u64,
    },
    /// A symbolic link pointing to `target`. Mode is always `0o777`.
    Symlink {
        target: String,
        uid: u32,
        gid: u32,
        mtime: u64,
        ctime: u64,
    },
}

impl Entry {
    /// Create a file with default permissions (`0o644`), owned by root.
    pub fn file(content: impl Into<Vec<u8>>) -> Self {
        Entry::File {
            content: content.into(),
            mode: 0o644,
            uid: 0,
            gid: 0,
            mtime: 0,
            ctime: 0,
        }
    }

    /// Create a file with explicit permissions, owned by root.
    pub fn file_with_mode(content: impl Into<Vec<u8>>, mode: u16) -> Self {
        Entry::File {
            content: content.into(),
            mode,
            uid: 0,
            gid: 0,
            mtime: 0,
            ctime: 0,
        }
    }

    /// Create a directory with default permissions (`0o755`), owned by root.
    pub fn dir() -> Self {
        Entry::Dir {
            mode: 0o755,
            uid: 0,
            gid: 0,
            mtime: 0,
            ctime: 0,
        }
    }

    /// Create a directory with explicit permissions, owned by root.
    pub fn dir_with_mode(mode: u16) -> Self {
        Entry::Dir {
            mode,
            uid: 0,
            gid: 0,
            mtime: 0,
            ctime: 0,
        }
    }

    /// Create a symbolic link pointing to `target`, owned by root.
    pub fn symlink(target: impl Into<String>) -> Self {
        Entry::Symlink {
            target: target.into(),
            uid: 0,
            gid: 0,
            mtime: 0,
            ctime: 0,
        }
    }

    /// Returns `true` if this entry is a directory.
    pub fn is_dir(&self) -> bool {
        matches!(self, Entry::Dir { .. })
    }

    /// Returns `true` if this entry is a file.
    pub fn is_file(&self) -> bool {
        matches!(self, Entry::File { .. })
    }

    /// Returns `true` if this entry is a symbolic link.
    pub fn is_symlink(&self) -> bool {
        matches!(self, Entry::Symlink { .. })
    }

    /// Returns the raw file content as bytes, or `None` for directories/symlinks.
    pub fn content(&self) -> Option<&[u8]> {
        match self {
            Entry::File { content, .. } => Some(content),
            Entry::Dir { .. } | Entry::Symlink { .. } => None,
        }
    }

    /// Returns the file content as a UTF-8 string, or `None` for directories/symlinks
    /// or files with invalid UTF-8.
    pub fn content_str(&self) -> Option<&str> {
        match self {
            Entry::File { content, .. } => core::str::from_utf8(content).ok(),
            Entry::Dir { .. } | Entry::Symlink { .. } => None,
        }
    }

    /// Returns the size of the file content in bytes, or 0 for directories/symlinks.
    pub fn len(&self) -> usize {
        match self {
            Entry::File { content, .. } => content.len(),
            Entry::Dir { .. } | Entry::Symlink { .. } => 0,
        }
    }

    /// Returns the Unix permission mode. Symlinks always report `0o777`.
    pub fn mode(&self) -> u16 {
        match self {
            Entry::File { mode, .. } | Entry::Dir { mode, .. } => *mode,
            Entry::Symlink { .. } => 0o777,
        }
    }

    /// Returns the owner user ID.
    pub fn uid(&self) -> u32 {
        match self {
            Entry::File { uid, .. } | Entry::Dir { uid, .. } | Entry::Symlink { uid, .. } => *uid,
        }
    }

    /// Returns the owner group ID.
    pub fn gid(&self) -> u32 {
        match self {
            Entry::File { gid, .. } | Entry::Dir { gid, .. } | Entry::Symlink { gid, .. } => *gid,
        }
    }

    /// Returns the last modification time.
    pub fn mtime(&self) -> u64 {
        match self {
            Entry::File { mtime, .. } | Entry::Dir { mtime, .. } | Entry::Symlink { mtime, .. } => {
                *mtime
            }
        }
    }

    /// Returns the last metadata change time.
    pub fn ctime(&self) -> u64 {
        match self {
            Entry::File { ctime, .. } | Entry::Dir { ctime, .. } | Entry::Symlink { ctime, .. } => {
                *ctime
            }
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

/// A borrowed view of a filesystem entry, returned by [`crate::MemFs::get`].
///
/// Like [`Entry`] but borrows file content instead of owning it.
#[derive(Debug, Clone, Copy)]
pub enum EntryRef<'a> {
    /// A regular file.
    File {
        content: &'a [u8],
        mode: u16,
        uid: u32,
        gid: u32,
        mtime: u64,
        ctime: u64,
    },
    /// A directory.
    Dir {
        mode: u16,
        uid: u32,
        gid: u32,
        mtime: u64,
        ctime: u64,
    },
    /// A symbolic link with a borrowed target path.
    Symlink {
        target: &'a str,
        uid: u32,
        gid: u32,
        mtime: u64,
        ctime: u64,
    },
}

impl<'a> EntryRef<'a> {
    /// Returns `true` if this entry is a directory.
    pub fn is_dir(&self) -> bool {
        matches!(self, EntryRef::Dir { .. })
    }

    /// Returns `true` if this entry is a file.
    pub fn is_file(&self) -> bool {
        matches!(self, EntryRef::File { .. })
    }

    /// Returns `true` if this entry is a symbolic link.
    pub fn is_symlink(&self) -> bool {
        matches!(self, EntryRef::Symlink { .. })
    }

    /// Returns the raw file content as bytes, or `None` for directories/symlinks.
    pub fn content(&self) -> Option<&'a [u8]> {
        match self {
            EntryRef::File { content, .. } => Some(content),
            EntryRef::Dir { .. } | EntryRef::Symlink { .. } => None,
        }
    }

    /// Returns the file content as a UTF-8 string, or `None` for directories/symlinks
    /// or files with invalid UTF-8.
    pub fn content_str(&self) -> Option<&'a str> {
        match self {
            EntryRef::File { content, .. } => core::str::from_utf8(content).ok(),
            EntryRef::Dir { .. } | EntryRef::Symlink { .. } => None,
        }
    }

    /// Returns the size of the file content in bytes, or 0 for directories/symlinks.
    pub fn len(&self) -> usize {
        match self {
            EntryRef::File { content, .. } => content.len(),
            EntryRef::Dir { .. } | EntryRef::Symlink { .. } => 0,
        }
    }

    /// Returns the Unix permission mode. Symlinks always report `0o777`.
    pub fn mode(&self) -> u16 {
        match self {
            EntryRef::File { mode, .. } | EntryRef::Dir { mode, .. } => *mode,
            EntryRef::Symlink { .. } => 0o777,
        }
    }

    /// Returns the owner user ID.
    pub fn uid(&self) -> u32 {
        match self {
            EntryRef::File { uid, .. }
            | EntryRef::Dir { uid, .. }
            | EntryRef::Symlink { uid, .. } => *uid,
        }
    }

    /// Returns the owner group ID.
    pub fn gid(&self) -> u32 {
        match self {
            EntryRef::File { gid, .. }
            | EntryRef::Dir { gid, .. }
            | EntryRef::Symlink { gid, .. } => *gid,
        }
    }

    /// Returns the last modification time.
    pub fn mtime(&self) -> u64 {
        match self {
            EntryRef::File { mtime, .. }
            | EntryRef::Dir { mtime, .. }
            | EntryRef::Symlink { mtime, .. } => *mtime,
        }
    }

    /// Returns the last metadata change time.
    pub fn ctime(&self) -> u64 {
        match self {
            EntryRef::File { ctime, .. }
            | EntryRef::Dir { ctime, .. }
            | EntryRef::Symlink { ctime, .. } => *ctime,
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
        assert_eq!(e.uid(), 0);
        assert_eq!(e.gid(), 0);
        assert_eq!(e.mtime(), 0);
        assert_eq!(e.ctime(), 0);
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
        assert_eq!(e.uid(), 0);
        assert_eq!(e.gid(), 0);
        assert_eq!(e.mtime(), 0);
        assert_eq!(e.ctime(), 0);
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

    // -- EntryRef tests -----------------------------------------------------

    #[test]
    fn entry_ref_file() {
        let data = vec![104, 101, 108, 108, 111]; // "hello"
        let r = EntryRef::File {
            content: &data,
            mode: 0o755,
            uid: 1000,
            gid: 1000,
            mtime: 5,
            ctime: 3,
        };
        assert!(r.is_file());
        assert!(!r.is_dir());
        assert_eq!(r.content(), Some(b"hello".as_slice()));
        assert_eq!(r.content_str(), Some("hello"));
        assert_eq!(r.len(), 5);
        assert_eq!(r.mode(), 0o755);
        assert_eq!(r.uid(), 1000);
        assert_eq!(r.gid(), 1000);
        assert_eq!(r.mtime(), 5);
        assert_eq!(r.ctime(), 3);
        assert!(r.is_readable());
        assert!(r.is_writable());
        assert!(r.is_executable());
    }

    #[test]
    fn entry_ref_dir() {
        let r = EntryRef::Dir {
            mode: 0o500,
            uid: 0,
            gid: 0,
            mtime: 0,
            ctime: 0,
        };
        assert!(r.is_dir());
        assert!(!r.is_file());
        assert_eq!(r.content(), None);
        assert_eq!(r.content_str(), None);
        assert_eq!(r.len(), 0);
        assert_eq!(r.mode(), 0o500);
        assert!(r.is_readable());
        assert!(!r.is_writable());
        assert!(r.is_executable());
    }

    #[test]
    fn entry_ref_binary_content_str_is_none() {
        let data = vec![0u8, 0xFF];
        let r = EntryRef::File {
            content: &data,
            mode: 0o644,
            uid: 0,
            gid: 0,
            mtime: 0,
            ctime: 0,
        };
        assert_eq!(r.content(), Some([0u8, 0xFF].as_slice()));
        assert_eq!(r.content_str(), None);
    }

    #[test]
    fn entry_ref_zero_mode() {
        let r = EntryRef::File {
            content: b"x",
            mode: 0o000,
            uid: 0,
            gid: 0,
            mtime: 0,
            ctime: 0,
        };
        assert!(!r.is_readable());
        assert!(!r.is_writable());
        assert!(!r.is_executable());
    }

    #[test]
    fn entryref_has_uid_gid() {
        let data = b"test";
        let r = EntryRef::File {
            content: data,
            mode: 0o644,
            uid: 42,
            gid: 99,
            mtime: 0,
            ctime: 0,
        };
        assert_eq!(r.uid(), 42);
        assert_eq!(r.gid(), 99);

        let d = EntryRef::Dir {
            mode: 0o755,
            uid: 1,
            gid: 2,
            mtime: 0,
            ctime: 0,
        };
        assert_eq!(d.uid(), 1);
        assert_eq!(d.gid(), 2);

        let s = EntryRef::Symlink {
            target: "/foo",
            uid: 500,
            gid: 500,
            mtime: 0,
            ctime: 0,
        };
        assert_eq!(s.uid(), 500);
        assert_eq!(s.gid(), 500);
    }
}
