use super::*;
use crate::error::VfsErrorKind;
use alloc::vec;

// -- Walk iterator tests ----------------------------------------------------

#[test]
fn walk_yields_all_entries() -> Result<(), VfsError> {
    let mut fs = MemFs::new();
    fs.create_dir_all("/a/b")?;
    fs.write("/a/b/f.txt", "x")?;
    fs.write("/other", "y")?;

    let entries: alloc::vec::Vec<_> = fs.walk().collect();
    let paths: alloc::vec::Vec<&str> = entries.iter().map(|(p, _)| p.as_str()).collect();

    assert!(paths.contains(&"/"));
    assert!(paths.contains(&"/a"));
    assert!(paths.contains(&"/a/b"));
    assert!(paths.contains(&"/a/b/f.txt"));
    assert!(paths.contains(&"/other"));
    Ok(())
}

#[test]
fn walk_matches_iter() -> Result<(), VfsError> {
    let mut fs = MemFs::new();
    fs.create_dir_all("/x/y")?;
    fs.write("/x/y/z", "data")?;
    fs.write("/top", "hi")?;

    let walk_entries: alloc::vec::Vec<_> = fs.walk().collect();
    let iter_entries = fs.iter();
    assert_eq!(walk_entries.len(), iter_entries.len());
    Ok(())
}

#[test]
fn walk_prefix_subtree() -> Result<(), VfsError> {
    let mut fs = MemFs::new();
    fs.create_dir_all("/a/b")?;
    fs.write("/a/b/f.txt", "x")?;
    fs.write("/other", "y")?;

    let entries: alloc::vec::Vec<_> = fs.walk_prefix("/a").collect();
    let paths: alloc::vec::Vec<&str> = entries.iter().map(|(p, _)| p.as_str()).collect();

    assert!(paths.contains(&"/a"));
    assert!(paths.contains(&"/a/b"));
    assert!(paths.contains(&"/a/b/f.txt"));
    assert!(!paths.contains(&"/other"));
    Ok(())
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
fn read_dir_iter_yields_entries() -> Result<(), VfsError> {
    let mut fs = MemFs::new();
    fs.write("/a", "x")?;
    fs.write("/b", "y")?;
    fs.create_dir("/c")?;

    let entries: alloc::vec::Vec<_> = fs.read_dir_iter("/")?.collect();
    assert_eq!(entries.len(), 3);
    assert_eq!(entries[0].name, "a");
    assert_eq!(entries[1].name, "b");
    assert_eq!(entries[2].name, "c");
    Ok(())
}

#[test]
fn read_dir_iter_exact_size() -> Result<(), VfsError> {
    let mut fs = MemFs::new();
    fs.write("/a", "x")?;
    fs.write("/b", "y")?;

    let iter = fs.read_dir_iter("/")?;
    assert_eq!(iter.len(), 2);
    Ok(())
}

#[test]
fn read_dir_iter_not_found() {
    let fs = MemFs::new();
    assert!(fs.read_dir_iter("/nope").is_err());
}

#[test]
fn read_dir_iter_on_file() -> Result<(), VfsError> {
    let mut fs = MemFs::new();
    fs.write("/f", "x")?;
    let err = match fs.read_dir_iter("/f") {
        Err(e) => e,
        Ok(_) => {
            assert!(false, "expected NotADirectory error");
            return Ok(());
        }
    };
    assert_eq!(*err.kind(), VfsErrorKind::NotADirectory);
    Ok(())
}

// -- Subtree iteration tests ----------------------------------------------

#[test]
fn iter_prefix_subtree() -> Result<(), VfsError> {
    let mut fs = MemFs::new();
    fs.create_dir_all("/a/b/c")?;
    fs.write("/a/b/f.txt", "x")?;
    fs.write("/other", "y")?;
    let entries = fs.iter_prefix("/a");
    let paths: Vec<&str> = entries.iter().map(|(p, _)| p.as_str()).collect();
    assert!(paths.contains(&"/a"));
    assert!(paths.contains(&"/a/b"));
    assert!(paths.contains(&"/a/b/c"));
    assert!(paths.contains(&"/a/b/f.txt"));
    assert!(!paths.contains(&"/other"));
    Ok(())
}

#[test]
fn iter_prefix_root_equals_iter() -> Result<(), VfsError> {
    let mut fs = MemFs::new();
    fs.write("/a", "x")?;
    fs.create_dir("/d")?;
    let all = fs.iter();
    let root = fs.iter_prefix("/");
    assert_eq!(all.len(), root.len());
    Ok(())
}

#[test]
fn iter_prefix_single_file() -> Result<(), VfsError> {
    let mut fs = MemFs::new();
    fs.write("/f", "hello")?;
    let entries = fs.iter_prefix("/f");
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].0, "/f");
    Ok(())
}

#[test]
fn iter_prefix_missing() {
    let fs = MemFs::new();
    assert!(fs.iter_prefix("/nope").is_empty());
}

#[test]
fn paths_prefix_subtree() -> Result<(), VfsError> {
    let mut fs = MemFs::new();
    fs.create_dir_all("/x/y")?;
    fs.write("/x/y/z", "data")?;
    fs.write("/other", "data")?;
    let paths = fs.paths_prefix("/x");
    assert_eq!(paths, vec!["/x", "/x/y", "/x/y/z"]);
    Ok(())
}

// -- DirEntry symlink tests ----------------------------------------------

#[test]
fn dir_entry_symlink_flag() -> Result<(), VfsError> {
    let mut fs = MemFs::new();
    fs.write("/file", "x")?;
    fs.create_dir("/dir")?;
    fs.symlink("/file", "/link")?;
    let entries = fs.read_dir("/")?;
    let dir_e = entries
        .iter()
        .find(|e| e.name == "dir")
        .ok_or(VfsError::from(VfsErrorKind::NotFound))?;
    let file_e = entries
        .iter()
        .find(|e| e.name == "file")
        .ok_or(VfsError::from(VfsErrorKind::NotFound))?;
    let link_e = entries
        .iter()
        .find(|e| e.name == "link")
        .ok_or(VfsError::from(VfsErrorKind::NotFound))?;
    assert!(!dir_e.is_symlink);
    assert!(!file_e.is_symlink);
    assert!(link_e.is_symlink);
    assert!(!link_e.is_dir);
    Ok(())
}
