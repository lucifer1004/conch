use core::fmt;

/// Errors returned by filesystem operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VfsError {
    /// The path does not exist.
    NotFound,
    /// Expected a file but found a directory.
    IsADirectory,
    /// Expected a directory but found a file.
    NotADirectory,
    /// Insufficient permissions for the operation.
    PermissionDenied,
    /// An entry already exists at the path.
    AlreadyExists,
    /// File content is not valid UTF-8 (returned by [`crate::MemFs::read_to_string`]).
    InvalidUtf8,
    /// Too many levels of symbolic links encountered during traversal.
    TooManySymlinks,
    /// The path is not a symbolic link (returned by [`crate::MemFs::read_link`]).
    NotASymlink,
}

impl fmt::Display for VfsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            VfsError::NotFound => f.write_str("No such file or directory"),
            VfsError::IsADirectory => f.write_str("Is a directory"),
            VfsError::NotADirectory => f.write_str("Not a directory"),
            VfsError::PermissionDenied => f.write_str("Permission denied"),
            VfsError::AlreadyExists => f.write_str("File exists"),
            VfsError::InvalidUtf8 => f.write_str("Invalid UTF-8"),
            VfsError::TooManySymlinks => f.write_str("Too many levels of symbolic links"),
            VfsError::NotASymlink => f.write_str("Not a symbolic link"),
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for VfsError {}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::ToString;

    #[test]
    fn display_messages() {
        assert_eq!(VfsError::NotFound.to_string(), "No such file or directory");
        assert_eq!(VfsError::IsADirectory.to_string(), "Is a directory");
        assert_eq!(VfsError::NotADirectory.to_string(), "Not a directory");
        assert_eq!(VfsError::PermissionDenied.to_string(), "Permission denied");
        assert_eq!(VfsError::AlreadyExists.to_string(), "File exists");
        assert_eq!(VfsError::InvalidUtf8.to_string(), "Invalid UTF-8");
    }
}
