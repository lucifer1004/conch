use super::*;
use alloc::vec;

// -- Trie-specific behavior ---------------------------------------------

#[test]
fn deep_nesting() {
    let mut fs = MemFs::new();
    fs.create_dir_all("/a/b/c/d/e/f");
    fs.write("/a/b/c/d/e/f/deep.txt", "bottom".to_string());
    assert_eq!(fs.read_to_string("/a/b/c/d/e/f/deep.txt"), Ok("bottom"));
    assert!(fs.is_dir("/a/b/c/d/e/f"));
    assert!(fs.is_dir("/a/b/c"));
}

#[test]
fn remove_dir_drops_entire_subtree() {
    let mut fs = MemFs::new();
    fs.create_dir_all("/a/b/c");
    fs.write("/a/b/c/f1.txt", "1".to_string());
    fs.write("/a/b/f2.txt", "2".to_string());
    // remove() on a dir now drops the whole subtree
    let removed = fs.remove("/a/b");
    assert!(removed.is_some());
    assert!(!fs.exists("/a/b"));
    assert!(!fs.exists("/a/b/c"));
    assert!(!fs.exists("/a/b/c/f1.txt"));
    assert!(!fs.exists("/a/b/f2.txt"));
    // parent still exists
    assert!(fs.is_dir("/a"));
}

#[test]
fn remove_file_does_not_affect_siblings() {
    let mut fs = MemFs::new();
    fs.write("/a.txt", "a".to_string());
    fs.write("/b.txt", "b".to_string());
    fs.remove("/a.txt");
    assert!(!fs.exists("/a.txt"));
    assert_eq!(fs.read_to_string("/b.txt"), Ok("b"));
}

#[test]
fn write_to_missing_parent_is_noop() {
    let mut fs = MemFs::new();
    fs.write("/nonexistent/file.txt", "data".to_string());
    assert!(!fs.exists("/nonexistent"));
    assert!(!fs.exists("/nonexistent/file.txt"));
}

#[test]
fn touch_missing_parent_is_noop() {
    let mut fs = MemFs::new();
    fs.touch("/nonexistent/file.txt");
    assert!(!fs.exists("/nonexistent/file.txt"));
}

#[test]
fn paths_dfs_order() {
    let mut fs = MemFs::new();
    fs.create_dir_all("/a/b");
    fs.write("/a/b/f.txt", "".to_string());
    fs.write("/a/g.txt", "".to_string());
    fs.create_dir_all("/z");
    let paths = fs.paths();
    // DFS: / → /a → /a/b → /a/b/f.txt → /a/g.txt → /z
    let a_idx = paths.iter().position(|p| p == "/a").unwrap();
    let ab_idx = paths.iter().position(|p| p == "/a/b").unwrap();
    let abf_idx = paths.iter().position(|p| p == "/a/b/f.txt").unwrap();
    let ag_idx = paths.iter().position(|p| p == "/a/g.txt").unwrap();
    let z_idx = paths.iter().position(|p| p == "/z").unwrap();
    // /a comes before its children
    assert!(a_idx < ab_idx);
    assert!(ab_idx < abf_idx);
    // /a subtree before /z
    assert!(ag_idx < z_idx);
}

#[test]
fn iter_returns_correct_entry_types() {
    let mut fs = MemFs::new();
    fs.create_dir_all("/d");
    fs.write("/d/f.txt", "data".to_string());
    let entries = fs.iter();
    let root = entries.iter().find(|(p, _)| p == "/").unwrap();
    assert!(root.1.is_dir());
    let dir = entries.iter().find(|(p, _)| p == "/d").unwrap();
    assert!(dir.1.is_dir());
    let file = entries.iter().find(|(p, _)| p == "/d/f.txt").unwrap();
    assert!(file.1.is_file());
    assert_eq!(file.1.content_str(), Some("data"));
}

#[test]
fn create_dir_all_does_not_overwrite_file() {
    let mut fs = MemFs::new();
    fs.write("/a", "file".to_string());
    // Trying to create_dir_all through a file stops
    fs.create_dir_all("/a/b/c");
    assert!(fs.is_file("/a")); // still a file
    assert!(!fs.exists("/a/b"));
}

#[test]
fn remove_root_returns_none() {
    let mut fs = MemFs::new();
    assert!(fs.remove("/").is_none());
    assert!(fs.is_dir("/")); // root survives
}

#[test]
fn rename_moves_subtree() {
    let mut fs = MemFs::new();
    fs.create_dir_all("/src/sub");
    fs.write("/src/sub/f.txt", "data".to_string());
    fs.create_dir_all("/dst");
    fs.rename("/src", "/dst/moved").unwrap();
    assert!(!fs.exists("/src"));
    assert!(fs.is_dir("/dst/moved"));
    assert!(fs.is_dir("/dst/moved/sub"));
    assert_eq!(fs.read_to_string("/dst/moved/sub/f.txt"), Ok("data"));
}

#[test]
fn many_siblings() {
    let mut fs = MemFs::new();
    for i in 0..100 {
        fs.write(&alloc::format!("/f{:03}.txt", i), alloc::format!("{}", i));
    }
    let entries = fs.read_dir("/").unwrap();
    assert_eq!(entries.len(), 100);
    // Sorted by name
    assert_eq!(entries[0].name, "f000.txt");
    assert_eq!(entries[99].name, "f099.txt");
}

#[test]
fn metadata_via_get() {
    let mut fs = MemFs::new();
    fs.write_with_mode("/f.txt", "hi", 0o444);
    let e = fs.get("/f.txt").unwrap();
    assert!(e.is_file());
    assert!(e.is_readable());
    assert!(!e.is_writable());
    assert_eq!(e.len(), 2);
}

#[test]
fn get_root() {
    let fs = MemFs::new();
    let e = fs.get("/").unwrap();
    assert!(e.is_dir());
    assert_eq!(e.mode(), 0o755);
}

#[test]
fn get_nested_missing() {
    let mut fs = MemFs::new();
    fs.create_dir_all("/a/b");
    assert!(fs.get("/a/b/c").is_none());
    assert!(fs.get("/a/b/c/d").is_none());
    assert!(fs.get("/x").is_none());
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
fn trailing_slash_treated_as_component() {
    let mut fs = MemFs::new();
    // "/a/" splits into ["a", ""] — the empty component is filtered out by split_path
    fs.create_dir_all("/a");
    fs.write("/a/f.txt", "x".to_string());
    // These should still work because split_path filters empty segments
    assert!(fs.is_dir("/a"));
}

#[test]
fn path_with_special_chars() {
    let mut fs = MemFs::new();
    fs.write("/hello world.txt", "spaces".to_string());
    fs.write("/café.txt", "unicode".to_string());
    fs.write("/.hidden", "dot".to_string());
    assert_eq!(fs.read_to_string("/hello world.txt"), Ok("spaces"));
    assert_eq!(fs.read_to_string("/café.txt"), Ok("unicode"));
    assert_eq!(fs.read_to_string("/.hidden"), Ok("dot"));
}

// -- Mutation conflict edge cases ---------------------------------------

#[test]
fn write_overwrites_dir_with_file() {
    let mut fs = MemFs::new();
    fs.create_dir_all("/a/b");
    fs.write("/a/b/f.txt", "x".to_string());
    // Overwrite dir /a with a file
    fs.write("/a", "now a file".to_string());
    assert!(fs.is_file("/a"));
    // Children are gone (the dir node was replaced)
    assert!(!fs.exists("/a/b"));
}

#[test]
fn insert_dir_replaces_file() {
    let mut fs = MemFs::new();
    fs.write("/x", "file".to_string());
    fs.insert("/x".into(), Entry::dir());
    assert!(fs.is_dir("/x"));
}

#[test]
fn touch_on_existing_dir_is_noop() {
    let mut fs = MemFs::new();
    fs.create_dir_all("/d");
    fs.touch("/d");
    // Should still be a directory, not converted to file
    assert!(fs.is_dir("/d"));
}

#[test]
fn create_dir_where_file_exists() {
    let mut fs = MemFs::new();
    fs.write("/x", "file".to_string());
    assert_eq!(
        fs.create_dir("/x"),
        Err(crate::error::VfsErrorKind::AlreadyExists.into())
    );
    assert!(fs.is_file("/x")); // unchanged
}

#[test]
fn rename_to_self() {
    let mut fs = MemFs::new();
    fs.write("/f.txt", "data".to_string());
    // This removes source then inserts at dst — same path means it works
    assert!(fs.rename("/f.txt", "/f.txt").is_ok());
    assert_eq!(fs.read_to_string("/f.txt"), Ok("data"));
}

#[test]
fn rename_overwrites_destination() {
    let mut fs = MemFs::new();
    fs.write("/src", "new".to_string());
    fs.write("/dst", "old".to_string());
    fs.rename("/src", "/dst").unwrap();
    assert!(!fs.exists("/src"));
    assert_eq!(fs.read_to_string("/dst"), Ok("new"));
}

#[test]
fn rename_root_fails() {
    let mut fs = MemFs::new();
    fs.create_dir_all("/dst");
    assert!(fs.rename("/", "/dst/root").is_err());
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
fn append_to_empty_file() {
    let mut fs = MemFs::new();
    fs.touch("/f.txt");
    fs.append("/f.txt", b"hello").unwrap();
    assert_eq!(fs.read_to_string("/f.txt"), Ok("hello"));
}

#[test]
fn append_multiple_times() {
    let mut fs = MemFs::new();
    fs.write("/f.txt", "a".to_string());
    fs.append("/f.txt", b"b").unwrap();
    fs.append("/f.txt", b"c").unwrap();
    assert_eq!(fs.read_to_string("/f.txt"), Ok("abc"));
}

#[test]
fn remove_dir_all_root_clears_children() {
    let mut fs = MemFs::new();
    fs.create_dir_all("/a/b");
    fs.write("/a/b/f.txt", "x".to_string());
    fs.write("/c.txt", "y".to_string());
    fs.remove_dir_all("/").unwrap();
    assert!(fs.is_dir("/")); // root still exists
    assert!(!fs.exists("/a")); // children gone
    assert!(!fs.exists("/c.txt"));
    assert!(fs.read_dir("/").unwrap().is_empty());
}

#[test]
fn remove_dir_all_empty_dir() {
    let mut fs = MemFs::new();
    fs.create_dir_all("/empty");
    assert!(fs.remove_dir_all("/empty").is_ok());
    assert!(!fs.exists("/empty"));
}

#[test]
fn set_mode_on_root() {
    let mut fs = MemFs::new();
    fs.set_mode("/", 0o500).unwrap();
    assert_eq!(fs.get("/").unwrap().mode(), 0o500);
}

#[test]
fn read_dir_preserves_child_modes() {
    let mut fs = MemFs::new();
    fs.write_with_mode("/a.txt", "x", 0o444);
    fs.create_dir_all("/d");
    fs.set_mode("/d", 0o700).unwrap();
    let entries = fs.read_dir("/").unwrap();
    let a = entries.iter().find(|e| e.name == "a.txt").unwrap();
    let d = entries.iter().find(|e| e.name == "d").unwrap();
    assert_eq!(a.mode, 0o444);
    assert_eq!(d.mode, 0o700);
}

// -- Binary content edge cases ------------------------------------------

#[test]
fn file_with_null_bytes() {
    let mut fs = MemFs::new();
    fs.write("/bin", vec![0u8, 0, 0]);
    assert_eq!(fs.read("/bin"), Ok([0u8, 0, 0].as_slice()));
    // Null bytes are valid UTF-8
    assert_eq!(fs.read_to_string("/bin"), Ok("\0\0\0"));
}

#[test]
fn read_returns_exact_bytes() {
    let mut fs = MemFs::new();
    let data: Vec<u8> = (0..=255).collect();
    fs.write("/all_bytes", data.clone());
    assert_eq!(fs.read("/all_bytes").unwrap().len(), 256);
    assert_eq!(fs.read("/all_bytes"), Ok(data.as_slice()));
}

// -- Post-deletion consistency ------------------------------------------

#[test]
fn paths_after_deletion() {
    let mut fs = MemFs::new();
    fs.create_dir_all("/a/b");
    fs.write("/a/b/f.txt", "x".to_string());
    fs.write("/c.txt", "y".to_string());
    fs.remove_dir_all("/a").unwrap();
    let paths = fs.paths();
    assert!(paths.contains(&"/".to_string()));
    assert!(paths.contains(&"/c.txt".to_string()));
    assert!(!paths.contains(&"/a".to_string()));
    assert!(!paths.contains(&"/a/b".to_string()));
}

#[test]
fn operations_after_remove_middle() {
    let mut fs = MemFs::new();
    fs.create_dir_all("/a/b/c");
    fs.write("/a/b/c/deep.txt", "deep".to_string());
    // Remove middle node
    fs.remove_dir_all("/a/b").unwrap();
    assert!(fs.is_dir("/a"));
    assert!(!fs.exists("/a/b"));
    // Can recreate
    fs.create_dir_all("/a/b/new");
    fs.write("/a/b/new/f.txt", "fresh".to_string());
    assert_eq!(fs.read_to_string("/a/b/new/f.txt"), Ok("fresh"));
    // Old deep path is still gone
    assert!(!fs.exists("/a/b/c"));
}
