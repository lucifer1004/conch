use alloc::string::String;
use alloc::vec::Vec;

/// Normalize an absolute path: resolve `.` and `..`, collapse separators.
///
/// ```
/// assert_eq!(bare_vfs::normalize("/a/b/../c/./d"), "/a/c/d");
/// assert_eq!(bare_vfs::normalize("/"), "/");
/// ```
pub fn normalize(path: &str) -> String {
    let mut parts: Vec<&str> = Vec::new();
    for seg in path.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            _ => parts.push(seg),
        }
    }
    if parts.is_empty() {
        "/".into()
    } else {
        let mut result = String::new();
        for part in &parts {
            result.push('/');
            result.push_str(part);
        }
        result
    }
}

/// Return the parent directory path, or `None` for the root.
///
/// ```
/// assert_eq!(bare_vfs::parent("/a/b"), Some("/a"));
/// assert_eq!(bare_vfs::parent("/a"), Some("/"));
/// assert_eq!(bare_vfs::parent("/"), None);
/// ```
pub fn parent(path: &str) -> Option<&str> {
    if path == "/" {
        return None;
    }
    path.rsplit_once('/')
        .map(|(p, _)| if p.is_empty() { "/" } else { p })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_paths() {
        assert_eq!(normalize("/a/b/../c/./d"), "/a/c/d");
        assert_eq!(normalize("/a/b/../../"), "/");
        assert_eq!(normalize("/"), "/");
        assert_eq!(normalize("///a///b///"), "/a/b");
    }

    #[test]
    fn normalize_dotdot_beyond_root() {
        assert_eq!(normalize("/a/../../.."), "/");
    }

    #[test]
    fn normalize_relative_components() {
        assert_eq!(normalize("/a/./b/./c"), "/a/b/c");
    }

    #[test]
    fn normalize_empty_string() {
        assert_eq!(normalize(""), "/");
    }

    #[test]
    fn parent_paths() {
        assert_eq!(parent("/a/b"), Some("/a"));
        assert_eq!(parent("/a"), Some("/"));
        assert_eq!(parent("/"), None);
    }

    #[test]
    fn parent_deeply_nested() {
        assert_eq!(parent("/a/b/c/d"), Some("/a/b/c"));
    }
}
