use super::*;
use crate::error::VfsErrorKind;
use alloc::vec;

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

// -- read / read_to_string / write --------------------------------------

#[test]
fn write_and_read_string() {
    let mut fs = MemFs::new();
    fs.write("/hello.txt", "world".to_string());
    assert_eq!(fs.read_to_string("/hello.txt"), Ok("world"));
    assert_eq!(fs.read("/hello.txt"), Ok(b"world".as_slice()));
}

#[test]
fn write_and_read_bytes() {
    let mut fs = MemFs::new();
    fs.write("/bin", vec![0u8, 1, 0xFF]);
    assert_eq!(fs.read("/bin"), Ok([0u8, 1, 0xFF].as_slice()));
    assert_eq!(
        fs.read_to_string("/bin"),
        Err(VfsErrorKind::InvalidUtf8.into())
    );
}

#[test]
fn write_overwrites_existing() {
    let mut fs = MemFs::new();
    fs.write("/f.txt", "old".to_string());
    fs.write("/f.txt", "new".to_string());
    assert_eq!(fs.read_to_string("/f.txt"), Ok("new"));
}

#[test]
fn write_with_mode_sets_permissions() {
    let mut fs = MemFs::new();
    fs.write_with_mode("/secret", "x".to_string(), 0o000);
    fs.set_current_user(1000, 1000);
    assert_eq!(
        fs.read("/secret"),
        Err(VfsErrorKind::PermissionDenied.into())
    );
}

#[test]
fn read_missing() {
    let fs = MemFs::new();
    assert_eq!(
        fs.read_to_string("/nope"),
        Err(VfsErrorKind::NotFound.into())
    );
}

#[test]
fn read_directory() {
    let mut fs = MemFs::new();
    fs.create_dir_all("/a");
    assert_eq!(fs.read("/a"), Err(VfsErrorKind::IsADirectory.into()));
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

// -- append -------------------------------------------------------------

#[test]
fn append_to_file() {
    let mut fs = MemFs::new();
    fs.write("/log", "line1\n".to_string());
    fs.append("/log", b"line2\n").unwrap();
    assert_eq!(fs.read_to_string("/log"), Ok("line1\nline2\n"));
}

#[test]
fn append_not_found() {
    let mut fs = MemFs::new();
    assert_eq!(fs.append("/nope", b"x"), Err(VfsErrorKind::NotFound.into()));
}

#[test]
fn append_to_directory() {
    let mut fs = MemFs::new();
    fs.create_dir_all("/d");
    assert_eq!(
        fs.append("/d", b"x"),
        Err(VfsErrorKind::IsADirectory.into())
    );
}

#[test]
fn append_permission_denied() {
    let mut fs = MemFs::new();
    fs.write_with_mode("/ro", "x", 0o444);
    fs.set_current_user(1000, 1000);
    assert_eq!(
        fs.append("/ro", b"y"),
        Err(VfsErrorKind::PermissionDenied.into())
    );
}

// -- create_dir ---------------------------------------------------------

#[test]
fn create_dir_single() {
    let mut fs = MemFs::new();
    assert!(fs.create_dir("/sub").is_ok());
    assert!(fs.is_dir("/sub"));
}

#[test]
fn create_dir_already_exists() {
    let mut fs = MemFs::new();
    fs.create_dir_all("/sub");
    assert_eq!(
        fs.create_dir("/sub"),
        Err(VfsErrorKind::AlreadyExists.into())
    );
}

#[test]
fn create_dir_parent_missing() {
    let mut fs = MemFs::new();
    assert_eq!(fs.create_dir("/a/b"), Err(VfsErrorKind::NotFound.into()));
}

#[test]
fn create_dir_all_and_list() {
    let mut fs = MemFs::new();
    fs.create_dir_all("/a/b/c");
    assert!(fs.is_dir("/a"));
    assert!(fs.is_dir("/a/b"));
    assert!(fs.is_dir("/a/b/c"));

    let children = fs.read_dir("/a").unwrap();
    assert_eq!(children.len(), 1);
    assert_eq!(children[0].name, "b");
    assert!(children[0].is_dir);
}

#[test]
fn create_dir_all_idempotent() {
    let mut fs = MemFs::new();
    fs.create_dir_all("/a/b/c");
    fs.create_dir_all("/a/b/c");
    assert!(fs.is_dir("/a/b/c"));
}

// -- touch --------------------------------------------------------------

#[test]
fn touch_creates_empty_file() {
    let mut fs = MemFs::new();
    fs.touch("/new.txt");
    assert_eq!(fs.read("/new.txt"), Ok(b"".as_slice()));
}

#[test]
fn touch_does_not_overwrite() {
    let mut fs = MemFs::new();
    fs.write("/f.txt", "data".to_string());
    fs.touch("/f.txt");
    assert_eq!(fs.read_to_string("/f.txt"), Ok("data"));
}

// -- remove / remove_dir_all --------------------------------------------

#[test]
fn remove_file() {
    let mut fs = MemFs::new();
    fs.write("/f.txt", "x".to_string());
    assert!(fs.remove("/f.txt").is_some());
    assert!(!fs.exists("/f.txt"));
}

#[test]
fn remove_nonexistent() {
    let mut fs = MemFs::new();
    assert!(fs.remove("/nope").is_none());
}

#[test]
fn remove_dir_all_recursive() {
    let mut fs = MemFs::new();
    fs.create_dir_all("/a/b");
    fs.write("/a/b/f.txt", "x".to_string());
    fs.write("/a/g.txt", "y".to_string());
    assert!(fs.remove_dir_all("/a").is_ok());
    assert!(!fs.exists("/a"));
    assert!(!fs.exists("/a/b"));
    assert!(!fs.exists("/a/b/f.txt"));
}

#[test]
fn remove_dir_all_preserves_siblings() {
    let mut fs = MemFs::new();
    fs.create_dir_all("/a/target");
    fs.write("/a/target/f.txt", "x".to_string());
    fs.write("/a/sibling.txt", "keep".to_string());
    fs.remove_dir_all("/a/target").unwrap();
    assert!(!fs.exists("/a/target"));
    assert_eq!(fs.read_to_string("/a/sibling.txt"), Ok("keep"));
}

#[test]
fn remove_dir_all_not_found() {
    let mut fs = MemFs::new();
    assert_eq!(
        fs.remove_dir_all("/nope"),
        Err(VfsErrorKind::NotFound.into())
    );
}

#[test]
fn remove_dir_all_on_file() {
    let mut fs = MemFs::new();
    fs.write("/f.txt", "x".to_string());
    assert_eq!(
        fs.remove_dir_all("/f.txt"),
        Err(VfsErrorKind::NotADirectory.into())
    );
}

// -- set_mode -----------------------------------------------------------

#[test]
fn set_mode_file() {
    let mut fs = MemFs::new();
    fs.write("/f.txt", "x".to_string());
    fs.set_mode("/f.txt", 0o000).unwrap();
    fs.set_current_user(1000, 1000);
    assert_eq!(
        fs.read("/f.txt"),
        Err(VfsErrorKind::PermissionDenied.into())
    );
    // Switch back to root to re-enable read
    fs.set_current_user(0, 0);
    fs.set_mode("/f.txt", 0o644).unwrap();
    assert_eq!(fs.read_to_string("/f.txt"), Ok("x"));
}

#[test]
fn set_mode_dir() {
    let mut fs = MemFs::new();
    fs.create_dir_all("/d");
    fs.set_mode("/d", 0o500).unwrap();
    assert_eq!(fs.get("/d").unwrap().mode(), 0o500);
}

#[test]
fn set_mode_not_found() {
    let mut fs = MemFs::new();
    assert_eq!(
        fs.set_mode("/nope", 0o644),
        Err(VfsErrorKind::NotFound.into())
    );
}

// -- copy ---------------------------------------------------------------

#[test]
fn copy_file() {
    let mut fs = MemFs::new();
    fs.write("/a.txt", "hello".to_string());
    fs.copy("/a.txt", "/b.txt").unwrap();
    assert_eq!(fs.read_to_string("/b.txt"), Ok("hello"));
    assert_eq!(fs.read_to_string("/a.txt"), Ok("hello"));
}

#[test]
fn copy_not_found() {
    let mut fs = MemFs::new();
    assert_eq!(fs.copy("/nope", "/dst"), Err(VfsErrorKind::NotFound.into()));
}

#[test]
fn copy_directory() {
    let mut fs = MemFs::new();
    fs.create_dir_all("/d");
    assert_eq!(fs.copy("/d", "/d2"), Err(VfsErrorKind::IsADirectory.into()));
}

#[test]
fn copy_permission_denied() {
    let mut fs = MemFs::new();
    fs.write_with_mode("/secret", "x", 0o000);
    fs.set_current_user(1000, 1000);
    assert_eq!(
        fs.copy("/secret", "/dst"),
        Err(VfsErrorKind::PermissionDenied.into())
    );
}

#[test]
fn copy_overwrites_destination() {
    let mut fs = MemFs::new();
    fs.write("/src", "new".to_string());
    fs.write("/dst", "old".to_string());
    fs.copy("/src", "/dst").unwrap();
    assert_eq!(fs.read_to_string("/dst"), Ok("new"));
}

// -- rename -------------------------------------------------------------

#[test]
fn rename_file() {
    let mut fs = MemFs::new();
    fs.write("/old.txt", "data".to_string());
    fs.rename("/old.txt", "/new.txt").unwrap();
    assert!(!fs.exists("/old.txt"));
    assert_eq!(fs.read_to_string("/new.txt"), Ok("data"));
}

#[test]
fn rename_not_found() {
    let mut fs = MemFs::new();
    assert_eq!(
        fs.rename("/nope", "/dst"),
        Err(VfsErrorKind::NotFound.into())
    );
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

// -- insert (low-level) -------------------------------------------------

#[test]
fn insert_raw_entry() {
    let mut fs = MemFs::new();
    fs.insert("/custom".into(), Entry::file_with_mode("data", 0o755));
    let e = fs.get("/custom").unwrap();
    assert_eq!(e.content_str(), Some("data"));
    assert!(e.is_executable());
}

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
    assert_eq!(fs.read(""), Err(VfsErrorKind::NotFound.into()));
    assert_eq!(fs.metadata(""), Err(VfsErrorKind::NotFound.into()));
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
    assert_eq!(fs.create_dir("/x"), Err(VfsErrorKind::AlreadyExists.into()));
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

// -- symlink tests -------------------------------------------------------

#[test]
fn symlink_create_and_read_through() {
    let mut fs = MemFs::new();
    fs.write("/real.txt", "hello");
    fs.symlink("/real.txt", "/link.txt").unwrap();
    // is_symlink detects the link without following
    assert!(fs.is_symlink("/link.txt"));
    // reading through the link should yield the file content
    assert_eq!(fs.read_to_string("/link.txt"), Ok("hello"));
    // is_file follows symlinks
    assert!(fs.is_file("/link.txt"));
    assert!(!fs.is_dir("/link.txt"));
}

#[test]
fn symlink_to_directory() {
    let mut fs = MemFs::new();
    fs.create_dir_all("/real/sub");
    fs.write("/real/sub/f.txt", "data");
    fs.symlink("/real", "/link").unwrap();
    // Traversal through link should reach the directory
    assert!(fs.is_dir("/link"));
    assert!(fs.is_file("/link/sub/f.txt"));
    assert_eq!(fs.read_to_string("/link/sub/f.txt"), Ok("data"));
    // read_dir through symlinked directory
    let entries = fs.read_dir("/link").unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].name, "sub");
}

#[test]
fn symlink_dangling_returns_not_found() {
    let mut fs = MemFs::new();
    fs.symlink("/nonexistent.txt", "/dangling").unwrap();
    assert!(fs.is_symlink("/dangling"));
    // Following a dangling symlink should return NotFound
    assert_eq!(fs.read("/dangling"), Err(VfsErrorKind::NotFound.into()));
    assert_eq!(
        fs.read_to_string("/dangling"),
        Err(VfsErrorKind::NotFound.into())
    );
    // exists() follows symlinks, so dangling link returns false
    assert!(!fs.exists("/dangling"));
}

#[test]
fn symlink_chain() {
    // a -> b -> c -> real file
    let mut fs = MemFs::new();
    fs.write("/real.txt", "chained");
    fs.symlink("/real.txt", "/c").unwrap();
    fs.symlink("/c", "/b").unwrap();
    fs.symlink("/b", "/a").unwrap();
    assert_eq!(fs.read_to_string("/a"), Ok("chained"));
    assert_eq!(fs.read_to_string("/b"), Ok("chained"));
    assert_eq!(fs.read_to_string("/c"), Ok("chained"));
}

#[test]
fn symlink_loop_returns_too_many_symlinks() {
    let mut fs = MemFs::new();
    // a -> b -> a  (loop)
    fs.symlink("/b", "/a").unwrap();
    fs.symlink("/a", "/b").unwrap();
    // Reading through a loop should fail (not panic), returning TooManySymlinks
    assert_eq!(fs.read("/a"), Err(VfsErrorKind::TooManySymlinks.into()));
    assert_eq!(fs.read("/b"), Err(VfsErrorKind::TooManySymlinks.into()));
    assert!(!fs.exists("/a"));
}

#[test]
fn read_link_returns_target_without_following() {
    let mut fs = MemFs::new();
    fs.write("/real.txt", "data");
    fs.symlink("/real.txt", "/link").unwrap();
    assert_eq!(fs.read_link("/link"), Ok("/real.txt".to_string()));
    // read_link on a non-symlink returns NotASymlink
    assert_eq!(
        fs.read_link("/real.txt"),
        Err(VfsErrorKind::NotASymlink.into())
    );
    // read_link on missing path returns NotFound
    assert_eq!(fs.read_link("/missing"), Err(VfsErrorKind::NotFound.into()));
}

#[test]
fn remove_symlink_does_not_remove_target() {
    let mut fs = MemFs::new();
    fs.write("/real.txt", "keep me");
    fs.symlink("/real.txt", "/link").unwrap();
    // Remove the symlink
    let removed = fs.remove("/link");
    assert!(removed.is_some());
    // The target should still exist
    assert_eq!(fs.read_to_string("/real.txt"), Ok("keep me"));
    // The symlink should be gone
    assert!(!fs.is_symlink("/link"));
    assert!(!fs.exists("/link"));
}

#[test]
fn symlink_relative_resolution() {
    let mut fs = MemFs::new();
    fs.create_dir_all("/a/b");
    fs.write("/a/b/real.txt", "relative");
    // Create a relative symlink in /a/b pointing to real.txt (same dir)
    fs.symlink("real.txt", "/a/b/link").unwrap();
    assert_eq!(fs.read_to_string("/a/b/link"), Ok("relative"));
    // Also test a relative symlink going up one level
    fs.write("/a/top.txt", "top");
    fs.symlink("../top.txt", "/a/b/up_link").unwrap();
    assert_eq!(fs.read_to_string("/a/b/up_link"), Ok("top"));
}

#[test]
fn symlink_in_intermediate_path_component() {
    let mut fs = MemFs::new();
    fs.create_dir_all("/real_dir");
    fs.write("/real_dir/file.txt", "found");
    // /link -> /real_dir; access /link/file.txt
    fs.symlink("/real_dir", "/link").unwrap();
    assert_eq!(fs.read_to_string("/link/file.txt"), Ok("found"));
    // write through symlinked directory
    fs.write("/link/new.txt", "new");
    assert_eq!(fs.read_to_string("/real_dir/new.txt"), Ok("new"));
}

#[test]
fn symlink_metadata_is_symlink() {
    let mut fs = MemFs::new();
    fs.write("/real.txt", "x");
    fs.symlink("/real.txt", "/link").unwrap();
    // symlink_metadata does NOT follow the final symlink
    let m = fs.symlink_metadata("/link").unwrap();
    assert!(m.is_symlink());
    assert!(!m.is_file());
    assert!(!m.is_dir());
    // regular metadata DOES follow it
    let m2 = fs.metadata("/link").unwrap();
    assert!(m2.is_file());
    assert!(!m2.is_symlink());
}

#[test]
fn symlink_entry_is_symlink() {
    let mut fs = MemFs::new();
    fs.write("/real.txt", "x");
    fs.symlink("/real.txt", "/link").unwrap();
    // get() follows symlinks, so returns File
    let e = fs.get("/link").unwrap();
    assert!(e.is_file());
    assert!(!e.is_symlink());
    // insert/remove round-trip
    let removed = fs.remove("/link").unwrap();
    assert!(removed.is_symlink());
}

#[test]
fn symlink_insert_via_entry() {
    let mut fs = MemFs::new();
    fs.write("/real.txt", "data");
    fs.insert("/link".to_string(), Entry::symlink("/real.txt"));
    assert!(fs.is_symlink("/link"));
    assert_eq!(fs.read_to_string("/link"), Ok("data"));
}

#[test]
fn symlink_parent_missing_returns_error() {
    let mut fs = MemFs::new();
    assert_eq!(
        fs.symlink("/target", "/no_parent/link"),
        Err(VfsErrorKind::NotFound.into())
    );
}

#[test]
fn is_symlink_on_non_symlink() {
    let mut fs = MemFs::new();
    fs.write("/f.txt", "x");
    fs.create_dir_all("/d");
    assert!(!fs.is_symlink("/f.txt"));
    assert!(!fs.is_symlink("/d"));
    assert!(!fs.is_symlink("/missing"));
}

// -- uid/gid tests -------------------------------------------------------

#[test]
fn chown_changes_ownership() {
    let mut fs = MemFs::new();
    fs.write("/f.txt", "data");
    fs.chown("/f.txt", 1000, 2000).unwrap();
    let m = fs.metadata("/f.txt").unwrap();
    assert_eq!(m.uid(), 1000);
    assert_eq!(m.gid(), 2000);
}

#[test]
fn chown_directory() {
    let mut fs = MemFs::new();
    fs.create_dir_all("/d");
    fs.chown("/d", 500, 500).unwrap();
    let m = fs.metadata("/d").unwrap();
    assert_eq!(m.uid(), 500);
    assert_eq!(m.gid(), 500);
}

#[test]
fn chown_not_found() {
    let mut fs = MemFs::new();
    assert_eq!(
        fs.chown("/nope", 1000, 1000),
        Err(VfsErrorKind::NotFound.into())
    );
}

#[test]
fn permission_owner_bits() {
    let mut fs = MemFs::new();
    // Create file owned by uid=1000, readable by owner only (0o400)
    fs.write_with_mode("/f.txt", "data", 0o400);
    fs.chown("/f.txt", 1000, 2000).unwrap();

    // As owner: read allowed
    fs.set_current_user(1000, 9999);
    assert_eq!(fs.read_to_string("/f.txt"), Ok("data"));

    // As group member: no group read bit
    fs.set_current_user(9999, 2000);
    assert_eq!(
        fs.read("/f.txt"),
        Err(VfsErrorKind::PermissionDenied.into())
    );

    // As other: no other read bit
    fs.set_current_user(9999, 9999);
    assert_eq!(
        fs.read("/f.txt"),
        Err(VfsErrorKind::PermissionDenied.into())
    );
}

#[test]
fn permission_group_bits() {
    let mut fs = MemFs::new();
    // Readable by group only (0o040)
    fs.write_with_mode("/f.txt", "data", 0o040);
    fs.chown("/f.txt", 1000, 2000).unwrap();

    // Owner but not group: owner bits empty
    fs.set_current_user(1000, 9999);
    assert_eq!(
        fs.read("/f.txt"),
        Err(VfsErrorKind::PermissionDenied.into())
    );

    // Group member: allowed
    fs.set_current_user(9999, 2000);
    assert_eq!(fs.read_to_string("/f.txt"), Ok("data"));

    // Other: not allowed
    fs.set_current_user(9999, 9999);
    assert_eq!(
        fs.read("/f.txt"),
        Err(VfsErrorKind::PermissionDenied.into())
    );
}

#[test]
fn permission_other_bits() {
    let mut fs = MemFs::new();
    // World-readable (0o004)
    fs.write_with_mode("/f.txt", "data", 0o004);
    fs.chown("/f.txt", 1000, 2000).unwrap();

    // Owner: owner bits empty
    fs.set_current_user(1000, 9999);
    assert_eq!(
        fs.read("/f.txt"),
        Err(VfsErrorKind::PermissionDenied.into())
    );

    // Group: group bits empty
    fs.set_current_user(9999, 2000);
    assert_eq!(
        fs.read("/f.txt"),
        Err(VfsErrorKind::PermissionDenied.into())
    );

    // Other: allowed
    fs.set_current_user(9999, 9999);
    assert_eq!(fs.read_to_string("/f.txt"), Ok("data"));
}

#[test]
fn root_bypasses_permissions() {
    let mut fs = MemFs::new();
    fs.write_with_mode("/secret", "x", 0o000);
    // root (uid=0) can always read
    assert_eq!(fs.read_to_string("/secret"), Ok("x"));
    // root can always append
    assert!(fs.append("/secret", b"y").is_ok());
}

#[test]
fn supplementary_gids() {
    let mut fs = MemFs::new();
    fs.write_with_mode("/f.txt", "data", 0o040);
    fs.chown("/f.txt", 1000, 2000).unwrap();

    // User 9999, primary gid 9999, supplementary gid 2000
    fs.set_current_user(9999, 9999);
    fs.add_supplementary_gid(2000);
    assert_eq!(fs.read_to_string("/f.txt"), Ok("data"));
}

#[test]
fn new_files_inherit_current_user() {
    let mut fs = MemFs::new();
    fs.set_current_user(1000, 2000);

    fs.write("/f.txt", "data");
    let m = fs.metadata("/f.txt").unwrap();
    assert_eq!(m.uid(), 1000);
    assert_eq!(m.gid(), 2000);

    fs.create_dir_all("/d");
    let md = fs.metadata("/d").unwrap();
    assert_eq!(md.uid(), 1000);
    assert_eq!(md.gid(), 2000);

    fs.touch("/t.txt");
    let mt = fs.metadata("/t.txt").unwrap();
    assert_eq!(mt.uid(), 1000);
    assert_eq!(mt.gid(), 2000);

    fs.symlink("/f.txt", "/link").unwrap();
    // symlink_metadata to get the symlink's own uid/gid
    let ms = fs.symlink_metadata("/link").unwrap();
    assert_eq!(ms.uid(), 1000);
    assert_eq!(ms.gid(), 2000);
}

#[test]
fn entryref_has_uid_gid() {
    let mut fs = MemFs::new();
    fs.set_current_user(42, 99);
    fs.write("/f.txt", "hello");
    let e = fs.get("/f.txt").unwrap();
    assert_eq!(e.uid(), 42);
    assert_eq!(e.gid(), 99);
}

#[test]
fn current_uid_gid_getters() {
    let mut fs = MemFs::new();
    assert_eq!(fs.current_uid(), 0);
    assert_eq!(fs.current_gid(), 0);
    fs.set_current_user(500, 600);
    assert_eq!(fs.current_uid(), 500);
    assert_eq!(fs.current_gid(), 600);
}

// -- Timestamp tests ----------------------------------------------------

#[test]
fn timestamps_default_zero() {
    let fs = MemFs::new();
    assert_eq!(fs.time(), 0);
    let meta = fs.metadata("/").unwrap();
    assert_eq!(meta.mtime(), 0);
    assert_eq!(meta.ctime(), 0);
}

#[test]
fn timestamps_tick_on_write() {
    let mut fs = MemFs::new();
    fs.write("/a", "hello");
    assert_eq!(fs.time(), 1);
    let meta = fs.metadata("/a").unwrap();
    assert_eq!(meta.mtime(), 1);
    assert_eq!(meta.ctime(), 1);

    fs.write("/b", "world");
    assert_eq!(fs.time(), 2);
    let meta = fs.metadata("/b").unwrap();
    assert_eq!(meta.mtime(), 2);
    assert_eq!(meta.ctime(), 2);
}

#[test]
fn timestamps_set_time_override() {
    let mut fs = MemFs::new();
    fs.set_time(1000);
    fs.write("/a", "hello");
    assert_eq!(fs.time(), 1001);
    let meta = fs.metadata("/a").unwrap();
    assert_eq!(meta.mtime(), 1001);
}

#[test]
fn timestamps_append_updates_mtime() {
    let mut fs = MemFs::new();
    fs.write("/a", "hello");
    let t1 = fs.metadata("/a").unwrap().mtime();
    fs.append("/a", b" world").unwrap();
    let meta = fs.metadata("/a").unwrap();
    assert!(meta.mtime() > t1);
    assert!(meta.ctime() > t1);
}

#[test]
fn timestamps_touch_updates_existing_mtime() {
    let mut fs = MemFs::new();
    fs.write("/a", "hello");
    let t1 = fs.metadata("/a").unwrap().mtime();
    fs.touch("/a");
    let t2 = fs.metadata("/a").unwrap().mtime();
    assert!(t2 > t1);
    // Content unchanged
    assert_eq!(fs.read_to_string("/a").unwrap(), "hello");
}

#[test]
fn timestamps_touch_creates_with_timestamps() {
    let mut fs = MemFs::new();
    fs.touch("/new");
    let meta = fs.metadata("/new").unwrap();
    assert!(meta.mtime() > 0);
    assert!(meta.ctime() > 0);
}

#[test]
fn timestamps_set_mode_only_updates_ctime() {
    let mut fs = MemFs::new();
    fs.write("/a", "hello");
    let m = fs.metadata("/a").unwrap();
    let orig_mtime = m.mtime();
    let orig_ctime = m.ctime();
    fs.set_mode("/a", 0o755).unwrap();
    let m2 = fs.metadata("/a").unwrap();
    assert_eq!(m2.mtime(), orig_mtime); // mtime unchanged
    assert!(m2.ctime() > orig_ctime); // ctime updated
}

#[test]
fn timestamps_chown_only_updates_ctime() {
    let mut fs = MemFs::new();
    fs.write("/a", "hello");
    let m = fs.metadata("/a").unwrap();
    let orig_mtime = m.mtime();
    let orig_ctime = m.ctime();
    fs.chown("/a", 1000, 1000).unwrap();
    let m2 = fs.metadata("/a").unwrap();
    assert_eq!(m2.mtime(), orig_mtime);
    assert!(m2.ctime() > orig_ctime);
}

#[test]
fn timestamps_rename_preserves() {
    let mut fs = MemFs::new();
    fs.write("/a", "hello");
    let m = fs.metadata("/a").unwrap();
    let orig_mtime = m.mtime();
    let orig_ctime = m.ctime();
    fs.rename("/a", "/b").unwrap();
    let m2 = fs.metadata("/b").unwrap();
    assert_eq!(m2.mtime(), orig_mtime);
    assert_eq!(m2.ctime(), orig_ctime);
}

#[test]
fn timestamps_copy_gets_new_timestamps() {
    let mut fs = MemFs::new();
    fs.write("/a", "hello");
    let src_time = fs.metadata("/a").unwrap().mtime();
    fs.copy("/a", "/b").unwrap();
    let dst_time = fs.metadata("/b").unwrap().mtime();
    assert!(dst_time > src_time);
}

#[test]
fn timestamps_create_dir() {
    let mut fs = MemFs::new();
    fs.create_dir("/d").unwrap();
    let meta = fs.metadata("/d").unwrap();
    assert!(meta.mtime() > 0);
    assert_eq!(meta.mtime(), meta.ctime());
}

#[test]
fn timestamps_symlink() {
    let mut fs = MemFs::new();
    fs.write("/target", "x");
    fs.symlink("/target", "/link").unwrap();
    let meta = fs.symlink_metadata("/link").unwrap();
    assert!(meta.mtime() > 0);
}

#[test]
fn timestamps_ordering() {
    let mut fs = MemFs::new();
    fs.write("/first", "a");
    fs.write("/second", "b");
    fs.write("/third", "c");
    let t1 = fs.metadata("/first").unwrap().mtime();
    let t2 = fs.metadata("/second").unwrap().mtime();
    let t3 = fs.metadata("/third").unwrap().mtime();
    assert!(t1 < t2);
    assert!(t2 < t3);
}

#[test]
fn timestamps_in_dir_entry() {
    let mut fs = MemFs::new();
    fs.write("/a", "hello");
    let entries = fs.read_dir("/").unwrap();
    let e = &entries[0];
    assert_eq!(e.name, "a");
    assert!(e.mtime > 0);
}

// -- Umask tests ---------------------------------------------------------

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

// -- Truncate tests -----------------------------------------------------

#[test]
fn truncate_shorter() {
    let mut fs = MemFs::new();
    fs.write("/a", "hello world");
    fs.truncate("/a", 5).unwrap();
    assert_eq!(fs.read_to_string("/a").unwrap(), "hello");
}

#[test]
fn truncate_longer_zero_fills() {
    let mut fs = MemFs::new();
    fs.write("/a", "hi");
    fs.truncate("/a", 5).unwrap();
    let bytes = fs.read("/a").unwrap();
    assert_eq!(bytes, &[b'h', b'i', 0, 0, 0]);
}

#[test]
fn truncate_to_zero() {
    let mut fs = MemFs::new();
    fs.write("/a", "data");
    fs.truncate("/a", 0).unwrap();
    assert_eq!(fs.read("/a").unwrap(), &[] as &[u8]);
}

#[test]
fn truncate_missing() {
    let mut fs = MemFs::new();
    assert!(matches!(fs.truncate("/x", 0), Err(ref e) if *e.kind() == VfsErrorKind::NotFound));
}

#[test]
fn truncate_directory() {
    let mut fs = MemFs::new();
    fs.create_dir("/d").unwrap();
    assert!(matches!(fs.truncate("/d", 0), Err(ref e) if *e.kind() == VfsErrorKind::IsADirectory));
}

#[test]
fn truncate_permission_denied() {
    let mut fs = MemFs::new();
    fs.write_with_mode("/a", "data", 0o444);
    fs.set_current_user(1000, 1000);
    assert!(
        matches!(fs.truncate("/a", 0), Err(ref e) if *e.kind() == VfsErrorKind::PermissionDenied)
    );
}

#[test]
fn truncate_updates_timestamps() {
    let mut fs = MemFs::new();
    fs.write("/a", "hello");
    let t1 = fs.metadata("/a").unwrap().mtime();
    fs.truncate("/a", 3).unwrap();
    let m = fs.metadata("/a").unwrap();
    assert!(m.mtime() > t1);
    assert!(m.ctime() > t1);
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

// -- Umask tests ---------------------------------------------------------

#[test]
fn umask_default_is_022() {
    let fs = MemFs::new();
    assert_eq!(fs.umask(), 0o022);
}

#[test]
fn umask_set_returns_old() {
    let mut fs = MemFs::new();
    let old = fs.set_umask(0o077);
    assert_eq!(old, 0o022);
    assert_eq!(fs.umask(), 0o077);
}

#[test]
fn umask_affects_write() {
    let mut fs = MemFs::new();
    fs.set_umask(0o077);
    fs.write("/a", "hello");
    assert_eq!(fs.metadata("/a").unwrap().mode(), 0o600);
}

#[test]
fn umask_affects_write_with_mode() {
    let mut fs = MemFs::new();
    fs.set_umask(0o077);
    fs.write_with_mode("/a", "hello", 0o755);
    assert_eq!(fs.metadata("/a").unwrap().mode(), 0o700);
}

#[test]
fn umask_affects_create_dir() {
    let mut fs = MemFs::new();
    fs.set_umask(0o077);
    fs.create_dir("/d").unwrap();
    assert_eq!(fs.metadata("/d").unwrap().mode(), 0o700);
}

#[test]
fn umask_affects_create_dir_all() {
    let mut fs = MemFs::new();
    fs.set_umask(0o077);
    fs.create_dir_all("/a/b/c");
    assert_eq!(fs.metadata("/a").unwrap().mode(), 0o700);
    assert_eq!(fs.metadata("/a/b/c").unwrap().mode(), 0o700);
}

#[test]
fn umask_affects_touch() {
    let mut fs = MemFs::new();
    fs.set_umask(0o077);
    fs.touch("/new");
    assert_eq!(fs.metadata("/new").unwrap().mode(), 0o600);
}

#[test]
fn umask_affects_copy() {
    let mut fs = MemFs::new();
    fs.write("/src", "data");
    fs.set_umask(0o077);
    fs.copy("/src", "/dst").unwrap();
    assert_eq!(fs.metadata("/dst").unwrap().mode(), 0o600);
}

#[test]
fn umask_default_preserves_644_755() {
    // Default umask 0o022 doesn't change 0o644 or 0o755
    let mut fs = MemFs::new();
    fs.write("/f", "x");
    assert_eq!(fs.metadata("/f").unwrap().mode(), 0o644);
    fs.create_dir("/d").unwrap();
    assert_eq!(fs.metadata("/d").unwrap().mode(), 0o755);
}

#[test]
fn timestamps_insert_sets_current_time() {
    let mut fs = MemFs::new();
    fs.set_time(100);
    fs.insert("/x".to_string(), Entry::file("data"));
    let meta = fs.metadata("/x").unwrap();
    assert_eq!(meta.mtime(), 101);
}

// -- atime / nlink / DirEntry::size tests ----------------------------------

#[test]
fn atime_initialized_with_mtime() {
    let mut fs = MemFs::new();
    fs.write("/f.txt", "hello");
    let meta = fs.metadata("/f.txt").unwrap();
    assert_eq!(meta.atime(), meta.mtime());
}

#[test]
fn atime_not_updated_on_set_mode() {
    let mut fs = MemFs::new();
    fs.write("/f.txt", "hello");
    let atime_before = fs.metadata("/f.txt").unwrap().atime();
    fs.set_mode("/f.txt", 0o600).unwrap();
    let atime_after = fs.metadata("/f.txt").unwrap().atime();
    assert_eq!(atime_before, atime_after);
}

#[test]
fn set_atime_explicit() {
    let mut fs = MemFs::new();
    fs.write("/f.txt", "hello");
    fs.set_atime("/f.txt", 999).unwrap();
    assert_eq!(fs.metadata("/f.txt").unwrap().atime(), 999);
}

#[test]
fn set_atime_not_found() {
    let mut fs = MemFs::new();
    assert_eq!(fs.set_atime("/nope", 1), Err(VfsErrorKind::NotFound.into()));
}

#[test]
fn nlink_always_one() {
    let mut fs = MemFs::new();
    fs.write("/f.txt", "hello");
    fs.create_dir("/d").unwrap();
    assert_eq!(fs.metadata("/f.txt").unwrap().nlink(), 1);
    assert_eq!(fs.metadata("/d").unwrap().nlink(), 1);
}

#[test]
fn dir_entry_has_size() {
    let mut fs = MemFs::new();
    fs.write("/file.txt", "hello world");
    fs.create_dir("/sub").unwrap();
    let entries = fs.read_dir("/").unwrap();
    let file_e = entries.iter().find(|e| e.name == "file.txt").unwrap();
    let dir_e = entries.iter().find(|e| e.name == "sub").unwrap();
    assert_eq!(file_e.size, 11);
    assert_eq!(dir_e.size, 0);
}

// -- Directory execute permission tests ------------------------------------

#[test]
fn traverse_requires_dir_execute() {
    let mut fs = MemFs::new();
    fs.create_dir_all("/a/b");
    fs.write("/a/b/file", "data");
    fs.set_mode("/a", 0o644).unwrap(); // remove execute from /a
    fs.set_current_user(1000, 1000);
    assert!(fs.read("/a/b/file").is_err()); // can't traverse /a without execute
}

#[test]
fn traverse_dir_execute_root_bypass() {
    let mut fs = MemFs::new();
    fs.create_dir_all("/a/b");
    fs.write("/a/b/file", "data");
    fs.set_mode("/a", 0o000).unwrap();
    // uid=0 (root) bypasses permission checks
    assert!(fs.read("/a/b/file").is_ok());
}

#[test]
fn symlink_loop_returns_too_many_symlinks_from_traverse() {
    let mut fs = MemFs::new();
    fs.symlink("/b", "/a").unwrap();
    fs.symlink("/a", "/b").unwrap();
    let err = fs.read("/a").unwrap_err();
    assert_eq!(*err.kind(), VfsErrorKind::TooManySymlinks);
}

// -- access() tests --------------------------------------------------------

#[test]
fn access_existence() {
    let mut fs = MemFs::new();
    fs.write("/f", "x");
    assert!(fs.access("/f", crate::AccessMode::F_OK).is_ok());
    assert!(fs.access("/nope", crate::AccessMode::F_OK).is_err());
}

#[test]
fn access_read_permission() {
    let mut fs = MemFs::new();
    fs.write_with_mode("/f", "x", 0o644);
    // root can always read
    assert!(fs.access("/f", crate::AccessMode::R_OK).is_ok());
}

#[test]
fn access_permission_denied_non_root() {
    let mut fs = MemFs::new();
    fs.write_with_mode("/f", "x", 0o000);
    fs.set_current_user(1000, 1000);
    assert!(fs.access("/f", crate::AccessMode::R_OK).is_err());
    assert!(fs.access("/f", crate::AccessMode::W_OK).is_err());
    assert!(fs.access("/f", crate::AccessMode::X_OK).is_err());
}

#[test]
fn access_combined_modes() {
    let mut fs = MemFs::new();
    fs.write_with_mode("/f", "x", 0o755);
    assert!(fs
        .access("/f", crate::AccessMode::R_OK | crate::AccessMode::X_OK)
        .is_ok());
}

#[test]
fn access_f_ok_does_not_check_permissions() {
    let mut fs = MemFs::new();
    fs.write_with_mode("/f", "x", 0o000);
    fs.set_current_user(1000, 1000);
    // F_OK only checks existence, not permissions
    assert!(fs.access("/f", crate::AccessMode::F_OK).is_ok());
}

// -- format_mode special bit tests -----------------------------------------

#[test]
fn format_mode_setuid() {
    assert_eq!(crate::Entry::format_mode(0o4755), "rwsr-xr-x");
    assert_eq!(crate::Entry::format_mode(0o4644), "rwSr--r--");
}

#[test]
fn format_mode_setgid() {
    assert_eq!(crate::Entry::format_mode(0o2755), "rwxr-sr-x");
    assert_eq!(crate::Entry::format_mode(0o2745), "rwxr-Sr-x");
}

#[test]
fn format_mode_sticky() {
    assert_eq!(crate::Entry::format_mode(0o1755), "rwxr-xr-t");
    assert_eq!(crate::Entry::format_mode(0o1754), "rwxr-xr-T");
}

#[cfg(feature = "serde")]
mod serde_tests {
    use super::*;

    #[test]
    fn roundtrip_empty() {
        let fs = MemFs::new();
        let json = serde_json::to_string(&fs).unwrap();
        let fs2: MemFs = serde_json::from_str(&json).unwrap();
        assert!(fs2.is_dir("/"));
        assert!(fs2.is_empty_dir("/"));
    }

    #[test]
    fn roundtrip_with_files() {
        let mut fs = MemFs::new();
        fs.create_dir_all("/a/b");
        fs.write("/a/b/file.txt", "hello world");
        fs.write_with_mode("/secret", "data", 0o600);
        fs.symlink("/a/b/file.txt", "/link").unwrap();

        let json = serde_json::to_string(&fs).unwrap();
        let fs2: MemFs = serde_json::from_str(&json).unwrap();

        assert_eq!(fs2.read_to_string("/a/b/file.txt").unwrap(), "hello world");
        assert_eq!(fs2.metadata("/secret").unwrap().mode(), 0o600);
        assert!(fs2.is_symlink("/link"));
        assert_eq!(fs2.read_link("/link").unwrap(), "/a/b/file.txt");
    }

    #[test]
    fn roundtrip_preserves_settings() {
        let mut fs = MemFs::new();
        fs.set_current_user(1000, 1000);
        fs.set_umask(0o077);
        fs.set_time(500);
        fs.write("/f", "x");

        let json = serde_json::to_string(&fs).unwrap();
        let fs2: MemFs = serde_json::from_str(&json).unwrap();

        assert_eq!(fs2.current_uid(), 1000);
        assert_eq!(fs2.current_gid(), 1000);
        assert_eq!(fs2.umask(), 0o077);
        assert_eq!(fs2.time(), fs.time());
    }

    #[test]
    fn roundtrip_preserves_timestamps() {
        let mut fs = MemFs::new();
        fs.write("/a", "data");
        let mtime = fs.metadata("/a").unwrap().mtime();

        let json = serde_json::to_string(&fs).unwrap();
        let fs2: MemFs = serde_json::from_str(&json).unwrap();

        assert_eq!(fs2.metadata("/a").unwrap().mtime(), mtime);
    }

    #[test]
    fn roundtrip_supplementary_gids() {
        let mut fs = MemFs::new();
        fs.add_supplementary_gid(100);
        fs.add_supplementary_gid(200);

        let json = serde_json::to_string(&fs).unwrap();
        let fs2: MemFs = serde_json::from_str(&json).unwrap();

        assert_eq!(fs2.supplementary_gids(), &[100u32, 200u32]);
    }

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

    // -- Path safety tests ------------------------------------------------------

    #[test]
    fn normalize_dotdot_in_path() {
        let mut fs = MemFs::new();
        fs.create_dir_all("/a/b");
        fs.write("/a/b/file", "data");
        // Access via unnormalized path with ..
        assert_eq!(fs.read_to_string("/a/b/../b/file").unwrap(), "data");
    }

    #[test]
    fn normalize_dot_in_path() {
        let mut fs = MemFs::new();
        fs.write("/file", "x");
        assert_eq!(fs.read_to_string("/./file").unwrap(), "x");
    }

    #[test]
    fn canonical_path_resolves_symlinks() {
        let mut fs = MemFs::new();
        fs.create_dir_all("/real/dir");
        fs.write("/real/dir/file", "x");
        fs.symlink("/real/dir", "/link").unwrap();
        let canon = fs.canonical_path("/link/file").unwrap();
        assert_eq!(canon, "/real/dir/file");
    }

    #[test]
    fn canonical_path_normalizes() {
        let mut fs = MemFs::new();
        fs.create_dir_all("/a/b");
        let canon = fs.canonical_path("/a/b/../b").unwrap();
        assert_eq!(canon, "/a/b");
    }

    #[test]
    fn canonical_path_not_found() {
        let fs = MemFs::new();
        assert!(fs.canonical_path("/nope").is_err());
    }

    #[test]
    fn validate_rejects_empty() {
        assert!(crate::path::validate("").is_err());
    }

    #[test]
    fn validate_rejects_relative() {
        assert!(crate::path::validate("relative/path").is_err());
    }

    #[test]
    fn validate_accepts_absolute() {
        assert!(crate::path::validate("/").is_ok());
        assert!(crate::path::validate("/a/b").is_ok());
    }
}
