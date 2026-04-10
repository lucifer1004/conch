use super::*;
use crate::error::VfsErrorKind;
use alloc::vec;

// -- Walk iterator tests ----------------------------------------------------

#[test]
fn walk_yields_all_entries() {
    let mut fs = MemFs::new();
    fs.create_dir_all("/a/b");
    fs.write("/a/b/f.txt", "x");
    fs.write("/other", "y");

    let entries: alloc::vec::Vec<_> = fs.walk().collect();
    let paths: alloc::vec::Vec<&str> = entries.iter().map(|(p, _)| p.as_str()).collect();

    assert!(paths.contains(&"/"));
    assert!(paths.contains(&"/a"));
    assert!(paths.contains(&"/a/b"));
    assert!(paths.contains(&"/a/b/f.txt"));
    assert!(paths.contains(&"/other"));
}

#[test]
fn walk_matches_iter() {
    let mut fs = MemFs::new();
    fs.create_dir_all("/x/y");
    fs.write("/x/y/z", "data");
    fs.write("/top", "hi");

    let walk_entries: alloc::vec::Vec<_> = fs.walk().collect();
    let iter_entries = fs.iter();
    assert_eq!(walk_entries.len(), iter_entries.len());
}

#[test]
fn walk_prefix_subtree() {
    let mut fs = MemFs::new();
    fs.create_dir_all("/a/b");
    fs.write("/a/b/f.txt", "x");
    fs.write("/other", "y");

    let entries: alloc::vec::Vec<_> = fs.walk_prefix("/a").collect();
    let paths: alloc::vec::Vec<&str> = entries.iter().map(|(p, _)| p.as_str()).collect();

    assert!(paths.contains(&"/a"));
    assert!(paths.contains(&"/a/b"));
    assert!(paths.contains(&"/a/b/f.txt"));
    assert!(!paths.contains(&"/other"));
}

#[test]
fn walk_prefix_missing() {
    let fs = MemFs::new();
    let entries: alloc::vec::Vec<_> = fs.walk_prefix("/nope").collect();
    assert!(entries.is_empty());
}

#[test]
fn walk_empty_fs() {
    let fs = MemFs::new();
    let entries: alloc::vec::Vec<_> = fs.walk().collect();
    assert_eq!(entries.len(), 1); // just root
}

// -- ReadDirIter tests ------------------------------------------------------

#[test]
fn read_dir_iter_yields_entries() {
    let mut fs = MemFs::new();
    fs.write("/a", "x");
    fs.write("/b", "y");
    fs.create_dir("/c").unwrap();

    let entries: alloc::vec::Vec<_> = fs.read_dir_iter("/").unwrap().collect();
    assert_eq!(entries.len(), 3);
    assert_eq!(entries[0].name, "a");
    assert_eq!(entries[1].name, "b");
    assert_eq!(entries[2].name, "c");
}

#[test]
fn read_dir_iter_exact_size() {
    let mut fs = MemFs::new();
    fs.write("/a", "x");
    fs.write("/b", "y");

    let iter = fs.read_dir_iter("/").unwrap();
    assert_eq!(iter.len(), 2);
}

#[test]
fn read_dir_iter_not_found() {
    let fs = MemFs::new();
    assert!(fs.read_dir_iter("/nope").is_err());
}

#[test]
fn read_dir_iter_on_file() {
    let mut fs = MemFs::new();
    fs.write("/f", "x");
    let err = fs.read_dir_iter("/f").unwrap_err();
    assert_eq!(*err.kind(), VfsErrorKind::NotADirectory);
}

// -- Subtree iteration tests ----------------------------------------------

#[test]
fn iter_prefix_subtree() {
    let mut fs = MemFs::new();
    fs.create_dir_all("/a/b/c");
    fs.write("/a/b/f.txt", "x");
    fs.write("/other", "y");
    let entries = fs.iter_prefix("/a");
    let paths: Vec<&str> = entries.iter().map(|(p, _)| p.as_str()).collect();
    assert!(paths.contains(&"/a"));
    assert!(paths.contains(&"/a/b"));
    assert!(paths.contains(&"/a/b/c"));
    assert!(paths.contains(&"/a/b/f.txt"));
    assert!(!paths.contains(&"/other"));
}

#[test]
fn iter_prefix_root_equals_iter() {
    let mut fs = MemFs::new();
    fs.write("/a", "x");
    fs.create_dir("/d").unwrap();
    let all = fs.iter();
    let root = fs.iter_prefix("/");
    assert_eq!(all.len(), root.len());
}

#[test]
fn iter_prefix_single_file() {
    let mut fs = MemFs::new();
    fs.write("/f", "hello");
    let entries = fs.iter_prefix("/f");
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].0, "/f");
}

#[test]
fn iter_prefix_missing() {
    let fs = MemFs::new();
    assert!(fs.iter_prefix("/nope").is_empty());
}

#[test]
fn paths_prefix_subtree() {
    let mut fs = MemFs::new();
    fs.create_dir_all("/x/y");
    fs.write("/x/y/z", "data");
    fs.write("/other", "data");
    let paths = fs.paths_prefix("/x");
    assert_eq!(paths, vec!["/x", "/x/y", "/x/y/z"]);
}

// -- DirEntry symlink tests ----------------------------------------------

#[test]
fn dir_entry_symlink_flag() {
    let mut fs = MemFs::new();
    fs.write("/file", "x");
    fs.create_dir("/dir").unwrap();
    fs.symlink("/file", "/link").unwrap();
    let entries = fs.read_dir("/").unwrap();
    let dir_e = entries.iter().find(|e| e.name == "dir").unwrap();
    let file_e = entries.iter().find(|e| e.name == "file").unwrap();
    let link_e = entries.iter().find(|e| e.name == "link").unwrap();
    assert!(!dir_e.is_symlink);
    assert!(!file_e.is_symlink);
    assert!(link_e.is_symlink);
    assert!(!link_e.is_dir);
}
