use alloc::string::String;

/// A single entry returned by [`crate::MemFs::read_dir`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirEntry {
    /// File or directory name (no leading slash).
    pub name: String,
    /// `true` when the entry is a directory.
    pub is_dir: bool,
    /// Unix permission mode of the entry.
    pub mode: u16,
}
