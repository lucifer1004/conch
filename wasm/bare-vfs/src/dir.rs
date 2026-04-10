use alloc::string::String;

/// A single entry returned by [`crate::MemFs::read_dir`].
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct DirEntry {
    /// File or directory name (no leading slash).
    pub name: String,
    /// `true` when the entry is a directory.
    pub is_dir: bool,
    /// `true` when the entry is a symbolic link.
    pub is_symlink: bool,
    /// Unix permission mode of the entry.
    pub mode: u16,
    /// Last modification time (monotonic counter).
    pub mtime: u64,
    /// Content size in bytes (0 for directories and symlinks).
    pub size: usize,
}
