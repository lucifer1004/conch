use super::*;
use crate::error::VfsErrorKind;

// -- new / default has root ---------------------------------------------

#[test]
fn new_has_root() {
    let fs = MemFs::new();
    assert!(fs.is_dir("/"));
}

#[test]
fn default_has_root() {
    let fs = MemFs::default();
    assert!(fs.is_dir("/"));
}

// -- exists / is_file / is_dir / get ------------------------------------

#[test]
fn exists_and_type_checks() {
    let mut fs = MemFs::new();
    fs.write("/f.txt", "".to_string());
    fs.create_dir_all("/d");

    assert!(fs.exists("/f.txt"));
    assert!(fs.exists("/d"));
    assert!(!fs.exists("/nope"));
    assert!(fs.is_file("/f.txt"));
    assert!(!fs.is_file("/d"));
    assert!(fs.is_dir("/d"));
    assert!(!fs.is_dir("/f.txt"));
}

#[test]
fn get_returns_entry_ref() {
    let mut fs = MemFs::new();
    fs.write("/f.txt", "data".to_string());
    let e = fs.get("/f.txt").unwrap();
    assert!(e.is_file());
    assert_eq!(e.content_str(), Some("data"));
    assert!(fs.get("/missing").is_none());
}

// -- metadata -----------------------------------------------------------

#[test]
fn metadata_file() {
    let mut fs = MemFs::new();
    fs.write_with_mode("/f.txt", "hello", 0o755);
    let m = fs.metadata("/f.txt").unwrap();
    assert!(m.is_file());
    assert_eq!(m.len(), 5);
    assert_eq!(m.mode(), 0o755);
}

#[test]
fn metadata_dir() {
    let fs = MemFs::new();
    let m = fs.metadata("/").unwrap();
    assert!(m.is_dir());
    assert_eq!(m.len(), 0);
}

#[test]
fn metadata_not_found() {
    let fs = MemFs::new();
    assert_eq!(fs.metadata("/nope"), Err(VfsErrorKind::NotFound.into()));
}

// -- read_dir -----------------------------------------------------------

#[test]
fn read_dir_sorted() {
    let mut fs = MemFs::new();
    fs.write("/c.txt", "".to_string());
    fs.write("/a.txt", "".to_string());
    fs.write("/b.txt", "".to_string());
    let entries = fs.read_dir("/").unwrap();
    let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
    assert_eq!(names, &["a.txt", "b.txt", "c.txt"]);
}

#[test]
fn read_dir_not_found() {
    let fs = MemFs::new();
    assert_eq!(fs.read_dir("/nope"), Err(VfsErrorKind::NotFound.into()));
}

#[test]
fn read_dir_on_file() {
    let mut fs = MemFs::new();
    fs.write("/f.txt", "x".to_string());
    assert_eq!(
        fs.read_dir("/f.txt"),
        Err(VfsErrorKind::NotADirectory.into())
    );
}

#[test]
fn read_dir_empty() {
    let mut fs = MemFs::new();
    fs.create_dir_all("/empty");
    assert!(fs.read_dir("/empty").unwrap().is_empty());
}

#[test]
fn read_dir_skips_nested() {
    let mut fs = MemFs::new();
    fs.create_dir_all("/a/b/c");
    fs.write("/a/b/c/f.txt", "x".to_string());
    let entries = fs.read_dir("/a").unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].name, "b");
}

#[test]
fn read_dir_mixed_entries() {
    let mut fs = MemFs::new();
    fs.write("/file.txt", "".to_string());
    fs.create_dir_all("/dir");
    let entries = fs.read_dir("/").unwrap();
    let file_e = entries.iter().find(|e| e.name == "file.txt").unwrap();
    let dir_e = entries.iter().find(|e| e.name == "dir").unwrap();
    assert!(!file_e.is_dir);
    assert!(dir_e.is_dir);
}

// -- iter / paths -------------------------------------------------------

#[test]
fn iter_all_entries() {
    let mut fs = MemFs::new();
    fs.create_dir_all("/a");
    fs.write("/a/f.txt", "x".to_string());
    let paths = fs.paths();
    assert!(paths.contains(&"/".to_string()));
    assert!(paths.contains(&"/a".to_string()));
    assert!(paths.contains(&"/a/f.txt".to_string()));
}

// -- is_empty_dir tests -------------------------------------------------

#[test]
fn is_empty_dir_empty() {
    let mut fs = MemFs::new();
    fs.create_dir("/d").unwrap();
    assert!(fs.is_empty_dir("/d"));
}

#[test]
fn is_empty_dir_non_empty() {
    let mut fs = MemFs::new();
    fs.create_dir_all("/d/sub");
    assert!(!fs.is_empty_dir("/d"));
}

#[test]
fn is_empty_dir_file() {
    let mut fs = MemFs::new();
    fs.write("/f", "x");
    assert!(!fs.is_empty_dir("/f"));
}

#[test]
fn is_empty_dir_missing() {
    let fs = MemFs::new();
    assert!(!fs.is_empty_dir("/nope"));
}

#[test]
fn is_empty_dir_root_empty() {
    let fs = MemFs::new();
    assert!(fs.is_empty_dir("/"));
}

// -- Display tests --------------------------------------------------------

#[test]
fn display_empty_fs() {
    let fs = MemFs::new();
    let out = alloc::format!("{}", fs);
    assert!(out.contains("/"));
}

#[test]
fn display_nested_tree() {
    let mut fs = MemFs::new();
    fs.create_dir_all("/a/b");
    fs.write("/a/b/f.txt", "x");
    fs.write("/z.txt", "y");
    let out = alloc::format!("{}", fs);
    assert!(out.contains("a/"));
    assert!(out.contains("b/"));
    assert!(out.contains("f.txt"));
    assert!(out.contains("z.txt"));
    assert!(out.contains("├── ") || out.contains("└── "));
}
