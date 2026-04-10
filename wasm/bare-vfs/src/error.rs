use alloc::string::String;
use core::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum VfsErrorKind {
    NotFound,
    IsADirectory,
    NotADirectory,
    PermissionDenied,
    AlreadyExists,
    InvalidUtf8,
    TooManySymlinks,
    NotASymlink,
    DirectoryNotEmpty,
}

impl fmt::Display for VfsErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            VfsErrorKind::NotFound => f.write_str("No such file or directory"),
            VfsErrorKind::IsADirectory => f.write_str("Is a directory"),
            VfsErrorKind::NotADirectory => f.write_str("Not a directory"),
            VfsErrorKind::PermissionDenied => f.write_str("Permission denied"),
            VfsErrorKind::AlreadyExists => f.write_str("File exists"),
            VfsErrorKind::InvalidUtf8 => f.write_str("Invalid UTF-8"),
            VfsErrorKind::TooManySymlinks => f.write_str("Too many levels of symbolic links"),
            VfsErrorKind::NotASymlink => f.write_str("Not a symbolic link"),
            VfsErrorKind::DirectoryNotEmpty => f.write_str("Directory not empty"),
        }
    }
}

/// Filesystem error with optional path context.
#[derive(Debug, Clone)]
pub struct VfsError {
    kind: VfsErrorKind,
    path: Option<String>,
}

impl VfsError {
    pub fn new(kind: VfsErrorKind) -> Self {
        VfsError { kind, path: None }
    }

    pub fn kind(&self) -> &VfsErrorKind {
        &self.kind
    }

    pub fn path(&self) -> Option<&str> {
        self.path.as_deref()
    }

    pub fn with_path(mut self, path: impl Into<String>) -> Self {
        self.path = Some(path.into());
        self
    }
}

impl fmt::Display for VfsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.kind)?;
        if let Some(ref path) = self.path {
            write!(f, ": {}", path)?;
        }
        Ok(())
    }
}

impl PartialEq for VfsError {
    fn eq(&self, other: &Self) -> bool {
        self.kind == other.kind
    }
}

impl Eq for VfsError {}

impl From<VfsErrorKind> for VfsError {
    fn from(kind: VfsErrorKind) -> Self {
        VfsError::new(kind)
    }
}

#[cfg(feature = "std")]
impl std::error::Error for VfsError {}

#[cfg(feature = "std")]
impl From<VfsError> for std::io::Error {
    fn from(err: VfsError) -> Self {
        let kind = match err.kind {
            VfsErrorKind::NotFound => std::io::ErrorKind::NotFound,
            VfsErrorKind::PermissionDenied => std::io::ErrorKind::PermissionDenied,
            VfsErrorKind::AlreadyExists => std::io::ErrorKind::AlreadyExists,
            VfsErrorKind::InvalidUtf8 => std::io::ErrorKind::InvalidData,
            _ => std::io::ErrorKind::Other,
        };
        std::io::Error::new(kind, err)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::format;
    use alloc::string::ToString;

    #[test]
    fn display_messages() {
        assert_eq!(
            VfsErrorKind::NotFound.to_string(),
            "No such file or directory"
        );
        assert_eq!(VfsErrorKind::IsADirectory.to_string(), "Is a directory");
        assert_eq!(VfsErrorKind::NotADirectory.to_string(), "Not a directory");
        assert_eq!(
            VfsErrorKind::PermissionDenied.to_string(),
            "Permission denied"
        );
        assert_eq!(VfsErrorKind::AlreadyExists.to_string(), "File exists");
        assert_eq!(VfsErrorKind::InvalidUtf8.to_string(), "Invalid UTF-8");
        assert_eq!(
            VfsErrorKind::DirectoryNotEmpty.to_string(),
            "Directory not empty"
        );
    }

    #[test]
    fn error_with_path() {
        let err = VfsError::from(VfsErrorKind::NotFound).with_path("/foo/bar");
        assert_eq!(format!("{}", err), "No such file or directory: /foo/bar");
        assert_eq!(err.path(), Some("/foo/bar"));
    }

    #[test]
    fn error_without_path() {
        let err = VfsError::from(VfsErrorKind::PermissionDenied);
        assert_eq!(format!("{}", err), "Permission denied");
        assert_eq!(err.path(), None);
    }

    #[test]
    fn error_eq_ignores_path() {
        let a = VfsError::from(VfsErrorKind::NotFound).with_path("/a");
        let b = VfsError::from(VfsErrorKind::NotFound).with_path("/b");
        assert_eq!(a, b);
    }

    #[test]
    fn from_kind() {
        let err: VfsError = VfsErrorKind::AlreadyExists.into();
        assert_eq!(*err.kind(), VfsErrorKind::AlreadyExists);
    }
}
