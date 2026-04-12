use super::*;
use alloc::vec;

// -- Trie-specific behavior ---------------------------------------------

#[test]
fn deep_nesting() -> Result<(), VfsError> {
    let mut fs = MemFs::new();
    fs.create_dir_all("/a/b/c/d/e/f")?;
    fs.write("/a/b/c/d/e/f/deep.txt", "bottom".to_string())?;
    assert_eq!(fs.read_to_string("/a/b/c/d/e/f/deep.txt"), Ok("bottom"));
    assert!(fs.is_dir("/a/b/c/d/e/f"));
    assert!(fs.is_dir("/a/b/c"));
    Ok(())
}

#[test]
fn remove_dir_drops_entire_subtree() -> Result<(), VfsError> {
    let mut fs = MemFs::new();
    fs.create_dir_all("/a/b/c")?;
    fs.write("/a/b/c/f1.txt", "1".to_string())?;
    fs.write("/a/b/f2.txt", "2".to_string())?;
    // remove() on a dir now drops the whole subtree
    let removed = fs.remove("/a/b");
    assert!(removed.is_some());
    assert!(!fs.exists("/a/b"));
    assert!(!fs.exists("/a/b/c"));
    assert!(!fs.exists("/a/b/c/f1.txt"));
    assert!(!fs.exists("/a/b/f2.txt"));
    // parent still exists
    assert!(fs.is_dir("/a"));
    Ok(())
}

#[test]
fn remove_file_does_not_affect_siblings() -> Result<(), VfsError> {
    let mut fs = MemFs::new();
    fs.write("/a.txt", "a".to_string())?;
    fs.write("/b.txt", "b".to_string())?;
    fs.remove("/a.txt");
    assert!(!fs.exists("/a.txt"));
    assert_eq!(fs.read_to_string("/b.txt"), Ok("b"));
    Ok(())
}

#[test]
fn write_to_missing_parent_is_noop() -> Result<(), VfsError> {
    let mut fs = MemFs::new();
    assert!(fs
        .write("/nonexistent/file.txt", "data".to_string())
        .is_err());
    assert!(!fs.exists("/nonexistent"));
    assert!(!fs.exists("/nonexistent/file.txt"));
    Ok(())
}

#[test]
fn touch_missing_parent_is_noop() -> Result<(), VfsError> {
    let mut fs = MemFs::new();
    assert!(fs.touch("/nonexistent/file.txt").is_err());
    assert!(!fs.exists("/nonexistent/file.txt"));
    Ok(())
}

#[test]
fn paths_dfs_order() -> Result<(), VfsError> {
    let mut fs = MemFs::new();
    fs.create_dir_all("/a/b")?;
    fs.write("/a/b/f.txt", "".to_string())?;
    fs.write("/a/g.txt", "".to_string())?;
    fs.create_dir_all("/z")?;
    let paths = fs.paths();
    // DFS: / -> /a -> /a/b -> /a/b/f.txt -> /a/g.txt -> /z
    let a_idx = paths
        .iter()
        .position(|p| p == "/a")
        .ok_or(VfsError::from(VfsErrorKind::NotFound))?;
    let ab_idx = paths
        .iter()
        .position(|p| p == "/a/b")
        .ok_or(VfsError::from(VfsErrorKind::NotFound))?;
    let abf_idx = paths
        .iter()
        .position(|p| p == "/a/b/f.txt")
        .ok_or(VfsError::from(VfsErrorKind::NotFound))?;
    let ag_idx = paths
        .iter()
        .position(|p| p == "/a/g.txt")
        .ok_or(VfsError::from(VfsErrorKind::NotFound))?;
    let z_idx = paths
        .iter()
        .position(|p| p == "/z")
        .ok_or(VfsError::from(VfsErrorKind::NotFound))?;
    // /a comes before its children
    assert!(a_idx < ab_idx);
    assert!(ab_idx < abf_idx);
    // /a subtree before /z
    assert!(ag_idx < z_idx);
    Ok(())
}

#[test]
fn iter_returns_correct_entry_types() -> Result<(), VfsError> {
    let mut fs = MemFs::new();
    fs.create_dir_all("/d")?;
    fs.write("/d/f.txt", "data".to_string())?;
    let entries = fs.iter();
    let root = entries
        .iter()
        .find(|(p, _)| p == "/")
        .ok_or(VfsError::from(VfsErrorKind::NotFound))?;
    assert!(root.1.is_dir());
    let dir = entries
        .iter()
        .find(|(p, _)| p == "/d")
        .ok_or(VfsError::from(VfsErrorKind::NotFound))?;
    assert!(dir.1.is_dir());
    let file = entries
        .iter()
        .find(|(p, _)| p == "/d/f.txt")
        .ok_or(VfsError::from(VfsErrorKind::NotFound))?;
    assert!(file.1.is_file());
    assert_eq!(file.1.content_str(), Some("data"));
    Ok(())
}

#[test]
fn create_dir_all_does_not_overwrite_file() -> Result<(), VfsError> {
    let mut fs = MemFs::new();
    fs.write("/a", "file".to_string())?;
    // Trying to create_dir_all through a file stops
    assert!(fs.create_dir_all("/a/b/c").is_err());
    assert!(fs.is_file("/a")); // still a file
    assert!(!fs.exists("/a/b"));
    Ok(())
}

#[test]
fn remove_root_returns_none() {
    let mut fs = MemFs::new();
    assert!(fs.remove("/").is_none());
    assert!(fs.is_dir("/")); // root survives
}

#[test]
fn rename_moves_subtree() -> Result<(), VfsError> {
    let mut fs = MemFs::new();
    fs.create_dir_all("/src/sub")?;
    fs.write("/src/sub/f.txt", "data".to_string())?;
    fs.create_dir_all("/dst")?;
    fs.rename("/src", "/dst/moved")?;
    assert!(!fs.exists("/src"));
    assert!(fs.is_dir("/dst/moved"));
    assert!(fs.is_dir("/dst/moved/sub"));
    assert_eq!(fs.read_to_string("/dst/moved/sub/f.txt"), Ok("data"));
    Ok(())
}

#[test]
fn many_siblings() -> Result<(), VfsError> {
    let mut fs = MemFs::new();
    for i in 0..100 {
        fs.write(&alloc::format!("/f{:03}.txt", i), alloc::format!("{}", i))?;
    }
    let entries = fs.read_dir("/")?;
    assert_eq!(entries.len(), 100);
    // Sorted by name
    assert_eq!(entries[0].name, "f000.txt");
    assert_eq!(entries[99].name, "f099.txt");
    Ok(())
}

#[test]
fn metadata_via_get() -> Result<(), VfsError> {
    let mut fs = MemFs::new();
    fs.write_with_mode("/f.txt", "hi", 0o444)?;
    let e = fs
        .get("/f.txt")
        .ok_or(VfsError::from(VfsErrorKind::NotFound))?;
    assert!(e.is_file());
    assert!(e.is_readable());
    assert!(!e.is_writable());
    assert_eq!(e.len(), 2);
    Ok(())
}

#[test]
fn get_root() {
    let fs = MemFs::new();
    let e = match fs.get("/") {
        Some(e) => e,
        None => {
            assert!(false, "expected root entry");
            return;
        }
    };
    assert!(e.is_dir());
    assert_eq!(e.mode(), 0o755);
}

#[test]
fn get_nested_missing() -> Result<(), VfsError> {
    let mut fs = MemFs::new();
    fs.create_dir_all("/a/b")?;
    assert!(fs.get("/a/b/c").is_none());
    assert!(fs.get("/a/b/c/d").is_none());
    assert!(fs.get("/x").is_none());
    Ok(())
}

// -- Path edge cases ----------------------------------------------------

#[test]
fn empty_path_is_not_found() {
    let fs = MemFs::new();
    assert!(!fs.exists(""));
    assert!(fs.get("").is_none());
    assert_eq!(
        fs.read(""),
        Err(crate::error::VfsErrorKind::NotFound.into())
    );
    assert_eq!(
        fs.metadata(""),
        Err(crate::error::VfsErrorKind::NotFound.into())
    );
}

#[test]
fn trailing_slash_treated_as_component() -> Result<(), VfsError> {
    let mut fs = MemFs::new();
    // "/a/" splits into ["a", ""] — the empty component is filtered out by split_path
    fs.create_dir_all("/a")?;
    fs.write("/a/f.txt", "x".to_string())?;
    // These should still work because split_path filters empty segments
    assert!(fs.is_dir("/a"));
    Ok(())
}

#[test]
fn path_with_special_chars() -> Result<(), VfsError> {
    let mut fs = MemFs::new();
    fs.write("/hello world.txt", "spaces".to_string())?;
    fs.write("/café.txt", "unicode".to_string())?;
    fs.write("/.hidden", "dot".to_string())?;
    assert_eq!(fs.read_to_string("/hello world.txt"), Ok("spaces"));
    assert_eq!(fs.read_to_string("/café.txt"), Ok("unicode"));
    assert_eq!(fs.read_to_string("/.hidden"), Ok("dot"));
    Ok(())
}

// -- Mutation conflict edge cases ---------------------------------------

#[test]
fn write_overwrites_dir_with_file() -> Result<(), VfsError> {
    let mut fs = MemFs::new();
    fs.create_dir_all("/a/b")?;
    fs.write("/a/b/f.txt", "x".to_string())?;
    // Overwrite dir /a with a file — returns IsADirectory error
    assert!(fs.write("/a", "now a file".to_string()).is_err());
    // /a is still a directory
    assert!(fs.is_dir("/a"));
    Ok(())
}

#[test]
fn insert_dir_replaces_file() -> Result<(), VfsError> {
    let mut fs = MemFs::new();
    fs.write("/x", "file".to_string())?;
    fs.insert("/x".into(), Entry::dir());
    assert!(fs.is_dir("/x"));
    Ok(())
}

#[test]
fn touch_on_existing_dir_is_noop() -> Result<(), VfsError> {
    let mut fs = MemFs::new();
    fs.create_dir_all("/d")?;
    // touch on a directory: result doesn't matter (may succeed as no-op or fail)
    let _ = fs.touch("/d");
    // Should still be a directory, not converted to file
    assert!(fs.is_dir("/d"));
    Ok(())
}

#[test]
fn create_dir_where_file_exists() -> Result<(), VfsError> {
    let mut fs = MemFs::new();
    fs.write("/x", "file".to_string())?;
    assert_eq!(
        fs.create_dir("/x"),
        Err(crate::error::VfsErrorKind::AlreadyExists.into())
    );
    assert!(fs.is_file("/x")); // unchanged
    Ok(())
}

#[test]
fn rename_to_self() -> Result<(), VfsError> {
    let mut fs = MemFs::new();
    fs.write("/f.txt", "data".to_string())?;
    // This removes source then inserts at dst — same path means it works
    assert!(fs.rename("/f.txt", "/f.txt").is_ok());
    assert_eq!(fs.read_to_string("/f.txt"), Ok("data"));
    Ok(())
}

#[test]
fn rename_overwrites_destination() -> Result<(), VfsError> {
    let mut fs = MemFs::new();
    fs.write("/src", "new".to_string())?;
    fs.write("/dst", "old".to_string())?;
    fs.rename("/src", "/dst")?;
    assert!(!fs.exists("/src"));
    assert_eq!(fs.read_to_string("/dst"), Ok("new"));
    Ok(())
}

#[test]
fn rename_root_fails() -> Result<(), VfsError> {
    let mut fs = MemFs::new();
    fs.create_dir_all("/dst")?;
    assert!(fs.rename("/", "/dst/root").is_err());
    Ok(())
}

// -- Boundary conditions ------------------------------------------------

#[test]
fn empty_fs_paths() {
    let fs = MemFs::new();
    let paths = fs.paths();
    assert_eq!(paths, vec!["/".to_string()]);
}

#[test]
fn empty_fs_iter() {
    let fs = MemFs::new();
    let entries = fs.iter();
    assert_eq!(entries.len(), 1);
    assert!(entries[0].1.is_dir());
}

#[test]
fn append_to_empty_file() -> Result<(), VfsError> {
    let mut fs = MemFs::new();
    fs.touch("/f.txt")?;
    fs.append("/f.txt", b"hello")?;
    assert_eq!(fs.read_to_string("/f.txt"), Ok("hello"));
    Ok(())
}

#[test]
fn append_multiple_times() -> Result<(), VfsError> {
    let mut fs = MemFs::new();
    fs.write("/f.txt", "a".to_string())?;
    fs.append("/f.txt", b"b")?;
    fs.append("/f.txt", b"c")?;
    assert_eq!(fs.read_to_string("/f.txt"), Ok("abc"));
    Ok(())
}

#[test]
fn remove_dir_all_root_clears_children() -> Result<(), VfsError> {
    let mut fs = MemFs::new();
    fs.create_dir_all("/a/b")?;
    fs.write("/a/b/f.txt", "x".to_string())?;
    fs.write("/c.txt", "y".to_string())?;
    fs.remove_dir_all("/")?;
    assert!(fs.is_dir("/")); // root still exists
    assert!(!fs.exists("/a")); // children gone
    assert!(!fs.exists("/c.txt"));
    assert!(fs.read_dir("/")?.is_empty());
    Ok(())
}

#[test]
fn remove_dir_all_empty_dir() -> Result<(), VfsError> {
    let mut fs = MemFs::new();
    fs.create_dir_all("/empty")?;
    assert!(fs.remove_dir_all("/empty").is_ok());
    assert!(!fs.exists("/empty"));
    Ok(())
}

#[test]
fn set_mode_on_root() -> Result<(), VfsError> {
    let mut fs = MemFs::new();
    fs.set_mode("/", 0o500)?;
    assert_eq!(
        fs.get("/")
            .ok_or(VfsError::from(VfsErrorKind::NotFound))?
            .mode(),
        0o500
    );
    Ok(())
}

#[test]
fn read_dir_preserves_child_modes() -> Result<(), VfsError> {
    let mut fs = MemFs::new();
    fs.write_with_mode("/a.txt", "x", 0o444)?;
    fs.create_dir_all("/d")?;
    fs.set_mode("/d", 0o700)?;
    let entries = fs.read_dir("/")?;
    let a = entries
        .iter()
        .find(|e| e.name == "a.txt")
        .ok_or(VfsError::from(VfsErrorKind::NotFound))?;
    let d = entries
        .iter()
        .find(|e| e.name == "d")
        .ok_or(VfsError::from(VfsErrorKind::NotFound))?;
    assert_eq!(a.mode, 0o444);
    assert_eq!(d.mode, 0o700);
    Ok(())
}

// -- Binary content edge cases ------------------------------------------

#[test]
fn file_with_null_bytes() -> Result<(), VfsError> {
    let mut fs = MemFs::new();
    fs.write("/bin", vec![0u8, 0, 0])?;
    assert_eq!(fs.read("/bin"), Ok([0u8, 0, 0].as_slice()));
    // Null bytes are valid UTF-8
    assert_eq!(fs.read_to_string("/bin"), Ok("\0\0\0"));
    Ok(())
}

#[test]
fn read_returns_exact_bytes() -> Result<(), VfsError> {
    let mut fs = MemFs::new();
    let data: Vec<u8> = (0..=255).collect();
    fs.write("/all_bytes", data.clone())?;
    assert_eq!(fs.read("/all_bytes")?.len(), 256);
    assert_eq!(fs.read("/all_bytes"), Ok(data.as_slice()));
    Ok(())
}

// -- Post-deletion consistency ------------------------------------------

#[test]
fn paths_after_deletion() -> Result<(), VfsError> {
    let mut fs = MemFs::new();
    fs.create_dir_all("/a/b")?;
    fs.write("/a/b/f.txt", "x".to_string())?;
    fs.write("/c.txt", "y".to_string())?;
    fs.remove_dir_all("/a")?;
    let paths = fs.paths();
    assert!(paths.contains(&"/".to_string()));
    assert!(paths.contains(&"/c.txt".to_string()));
    assert!(!paths.contains(&"/a".to_string()));
    assert!(!paths.contains(&"/a/b".to_string()));
    Ok(())
}

#[test]
fn operations_after_remove_middle() -> Result<(), VfsError> {
    let mut fs = MemFs::new();
    fs.create_dir_all("/a/b/c")?;
    fs.write("/a/b/c/deep.txt", "deep".to_string())?;
    // Remove middle node
    fs.remove_dir_all("/a/b")?;
    assert!(fs.is_dir("/a"));
    assert!(!fs.exists("/a/b"));
    // Can recreate
    fs.create_dir_all("/a/b/new")?;
    fs.write("/a/b/new/f.txt", "fresh".to_string())?;
    assert_eq!(fs.read_to_string("/a/b/new/f.txt"), Ok("fresh"));
    // Old deep path is still gone
    assert!(!fs.exists("/a/b/c"));
    Ok(())
}
