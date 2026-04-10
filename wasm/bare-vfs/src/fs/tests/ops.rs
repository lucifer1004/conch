use super::*;
use crate::error::VfsErrorKind;
use alloc::vec;

// -- read / read_to_string / write --------------------------------------

#[test]
fn write_and_read_string() -> Result<(), VfsError> {
    let mut fs = MemFs::new();
    fs.write("/hello.txt", "world".to_string())?;
    assert_eq!(fs.read_to_string("/hello.txt"), Ok("world"));
    assert_eq!(fs.read("/hello.txt"), Ok(b"world".as_slice()));
    Ok(())
}

#[test]
fn write_and_read_bytes() -> Result<(), VfsError> {
    let mut fs = MemFs::new();
    fs.write("/bin", vec![0u8, 1, 0xFF])?;
    assert_eq!(fs.read("/bin"), Ok([0u8, 1, 0xFF].as_slice()));
    assert_eq!(
        fs.read_to_string("/bin"),
        Err(VfsErrorKind::InvalidUtf8.into())
    );
    Ok(())
}

#[test]
fn write_overwrites_existing() -> Result<(), VfsError> {
    let mut fs = MemFs::new();
    fs.write("/f.txt", "old".to_string())?;
    fs.write("/f.txt", "new".to_string())?;
    assert_eq!(fs.read_to_string("/f.txt"), Ok("new"));
    Ok(())
}

#[test]
fn write_with_mode_sets_permissions() -> Result<(), VfsError> {
    let mut fs = MemFs::new();
    fs.write_with_mode("/secret", "x".to_string(), 0o000)?;
    fs.set_current_user(1000, 1000);
    assert_eq!(
        fs.read("/secret"),
        Err(VfsErrorKind::PermissionDenied.into())
    );
    Ok(())
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
fn read_directory() -> Result<(), VfsError> {
    let mut fs = MemFs::new();
    fs.create_dir_all("/a")?;
    assert_eq!(fs.read("/a"), Err(VfsErrorKind::IsADirectory.into()));
    Ok(())
}

// -- append -------------------------------------------------------------

#[test]
fn append_to_file() -> Result<(), VfsError> {
    let mut fs = MemFs::new();
    fs.write("/log", "line1\n".to_string())?;
    fs.append("/log", b"line2\n").unwrap();
    assert_eq!(fs.read_to_string("/log"), Ok("line1\nline2\n"));
    Ok(())
}

#[test]
fn append_not_found() {
    let mut fs = MemFs::new();
    assert_eq!(fs.append("/nope", b"x"), Err(VfsErrorKind::NotFound.into()));
}

#[test]
fn append_to_directory() -> Result<(), VfsError> {
    let mut fs = MemFs::new();
    fs.create_dir_all("/d")?;
    assert_eq!(
        fs.append("/d", b"x"),
        Err(VfsErrorKind::IsADirectory.into())
    );
    Ok(())
}

#[test]
fn append_permission_denied() -> Result<(), VfsError> {
    let mut fs = MemFs::new();
    fs.write_with_mode("/ro", "x", 0o444)?;
    fs.set_current_user(1000, 1000);
    assert_eq!(
        fs.append("/ro", b"y"),
        Err(VfsErrorKind::PermissionDenied.into())
    );
    Ok(())
}

// -- create_dir ---------------------------------------------------------

#[test]
fn create_dir_single() {
    let mut fs = MemFs::new();
    assert!(fs.create_dir("/sub").is_ok());
    assert!(fs.is_dir("/sub"));
}

#[test]
fn create_dir_already_exists() -> Result<(), VfsError> {
    let mut fs = MemFs::new();
    fs.create_dir_all("/sub")?;
    assert_eq!(
        fs.create_dir("/sub"),
        Err(VfsErrorKind::AlreadyExists.into())
    );
    Ok(())
}

#[test]
fn create_dir_parent_missing() {
    let mut fs = MemFs::new();
    assert_eq!(fs.create_dir("/a/b"), Err(VfsErrorKind::NotFound.into()));
}

#[test]
fn create_dir_all_and_list() -> Result<(), VfsError> {
    let mut fs = MemFs::new();
    fs.create_dir_all("/a/b/c")?;
    assert!(fs.is_dir("/a"));
    assert!(fs.is_dir("/a/b"));
    assert!(fs.is_dir("/a/b/c"));

    let children = fs.read_dir("/a").unwrap();
    assert_eq!(children.len(), 1);
    assert_eq!(children[0].name, "b");
    assert!(children[0].is_dir);
    Ok(())
}

#[test]
fn create_dir_all_idempotent() -> Result<(), VfsError> {
    let mut fs = MemFs::new();
    fs.create_dir_all("/a/b/c")?;
    fs.create_dir_all("/a/b/c")?;
    assert!(fs.is_dir("/a/b/c"));
    Ok(())
}

// -- touch --------------------------------------------------------------

#[test]
fn touch_creates_empty_file() -> Result<(), VfsError> {
    let mut fs = MemFs::new();
    fs.touch("/new.txt")?;
    assert_eq!(fs.read("/new.txt"), Ok(b"".as_slice()));
    Ok(())
}

#[test]
fn touch_does_not_overwrite() -> Result<(), VfsError> {
    let mut fs = MemFs::new();
    fs.write("/f.txt", "data".to_string())?;
    fs.touch("/f.txt")?;
    assert_eq!(fs.read_to_string("/f.txt"), Ok("data"));
    Ok(())
}

// -- remove / remove_dir_all --------------------------------------------

#[test]
fn remove_file() -> Result<(), VfsError> {
    let mut fs = MemFs::new();
    fs.write("/f.txt", "x".to_string())?;
    assert!(fs.remove("/f.txt").is_some());
    assert!(!fs.exists("/f.txt"));
    Ok(())
}

#[test]
fn remove_nonexistent() {
    let mut fs = MemFs::new();
    assert!(fs.remove("/nope").is_none());
}

#[test]
fn remove_dir_all_recursive() -> Result<(), VfsError> {
    let mut fs = MemFs::new();
    fs.create_dir_all("/a/b")?;
    fs.write("/a/b/f.txt", "x".to_string())?;
    fs.write("/a/g.txt", "y".to_string())?;
    assert!(fs.remove_dir_all("/a").is_ok());
    assert!(!fs.exists("/a"));
    assert!(!fs.exists("/a/b"));
    assert!(!fs.exists("/a/b/f.txt"));
    Ok(())
}

#[test]
fn remove_dir_all_preserves_siblings() -> Result<(), VfsError> {
    let mut fs = MemFs::new();
    fs.create_dir_all("/a/target")?;
    fs.write("/a/target/f.txt", "x".to_string())?;
    fs.write("/a/sibling.txt", "keep".to_string())?;
    fs.remove_dir_all("/a/target").unwrap();
    assert!(!fs.exists("/a/target"));
    assert_eq!(fs.read_to_string("/a/sibling.txt"), Ok("keep"));
    Ok(())
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
fn remove_dir_all_on_file() -> Result<(), VfsError> {
    let mut fs = MemFs::new();
    fs.write("/f.txt", "x".to_string())?;
    assert_eq!(
        fs.remove_dir_all("/f.txt"),
        Err(VfsErrorKind::NotADirectory.into())
    );
    Ok(())
}

// -- set_mode -----------------------------------------------------------

#[test]
fn set_mode_file() -> Result<(), VfsError> {
    let mut fs = MemFs::new();
    fs.write("/f.txt", "x".to_string())?;
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
    Ok(())
}

#[test]
fn set_mode_dir() -> Result<(), VfsError> {
    let mut fs = MemFs::new();
    fs.create_dir_all("/d")?;
    fs.set_mode("/d", 0o500).unwrap();
    assert_eq!(fs.get("/d").unwrap().mode(), 0o500);
    Ok(())
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
fn copy_file() -> Result<(), VfsError> {
    let mut fs = MemFs::new();
    fs.write("/a.txt", "hello".to_string())?;
    fs.copy("/a.txt", "/b.txt").unwrap();
    assert_eq!(fs.read_to_string("/b.txt"), Ok("hello"));
    assert_eq!(fs.read_to_string("/a.txt"), Ok("hello"));
    Ok(())
}

#[test]
fn copy_not_found() {
    let mut fs = MemFs::new();
    assert_eq!(fs.copy("/nope", "/dst"), Err(VfsErrorKind::NotFound.into()));
}

#[test]
fn copy_directory() -> Result<(), VfsError> {
    let mut fs = MemFs::new();
    fs.create_dir_all("/d")?;
    assert_eq!(fs.copy("/d", "/d2"), Err(VfsErrorKind::IsADirectory.into()));
    Ok(())
}

#[test]
fn copy_permission_denied() -> Result<(), VfsError> {
    let mut fs = MemFs::new();
    fs.write_with_mode("/secret", "x", 0o000)?;
    fs.set_current_user(1000, 1000);
    assert_eq!(
        fs.copy("/secret", "/dst"),
        Err(VfsErrorKind::PermissionDenied.into())
    );
    Ok(())
}

#[test]
fn copy_overwrites_destination() -> Result<(), VfsError> {
    let mut fs = MemFs::new();
    fs.write("/src", "new".to_string())?;
    fs.write("/dst", "old".to_string())?;
    fs.copy("/src", "/dst").unwrap();
    assert_eq!(fs.read_to_string("/dst"), Ok("new"));
    Ok(())
}

// -- rename -------------------------------------------------------------

#[test]
fn rename_file() -> Result<(), VfsError> {
    let mut fs = MemFs::new();
    fs.write("/old.txt", "data".to_string())?;
    fs.rename("/old.txt", "/new.txt").unwrap();
    assert!(!fs.exists("/old.txt"));
    assert_eq!(fs.read_to_string("/new.txt"), Ok("data"));
    Ok(())
}

#[test]
fn rename_not_found() {
    let mut fs = MemFs::new();
    assert_eq!(
        fs.rename("/nope", "/dst"),
        Err(VfsErrorKind::NotFound.into())
    );
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

// -- Truncate tests -----------------------------------------------------

#[test]
fn truncate_shorter() -> Result<(), VfsError> {
    let mut fs = MemFs::new();
    fs.write("/a", "hello world")?;
    fs.truncate("/a", 5).unwrap();
    assert_eq!(fs.read_to_string("/a").unwrap(), "hello");
    Ok(())
}

#[test]
fn truncate_longer_zero_fills() -> Result<(), VfsError> {
    let mut fs = MemFs::new();
    fs.write("/a", "hi")?;
    fs.truncate("/a", 5).unwrap();
    let bytes = fs.read("/a").unwrap();
    assert_eq!(bytes, &[b'h', b'i', 0, 0, 0]);
    Ok(())
}

#[test]
fn truncate_to_zero() -> Result<(), VfsError> {
    let mut fs = MemFs::new();
    fs.write("/a", "data")?;
    fs.truncate("/a", 0).unwrap();
    assert_eq!(fs.read("/a").unwrap(), &[] as &[u8]);
    Ok(())
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
fn truncate_permission_denied() -> Result<(), VfsError> {
    let mut fs = MemFs::new();
    fs.write_with_mode("/a", "data", 0o444)?;
    fs.set_current_user(1000, 1000);
    assert!(
        matches!(fs.truncate("/a", 0), Err(ref e) if *e.kind() == VfsErrorKind::PermissionDenied)
    );
    Ok(())
}

#[test]
fn truncate_updates_timestamps() -> Result<(), VfsError> {
    let mut fs = MemFs::new();
    fs.write("/a", "hello")?;
    let t1 = fs.metadata("/a").unwrap().mtime();
    fs.truncate("/a", 3).unwrap();
    let m = fs.metadata("/a").unwrap();
    assert!(m.mtime() > t1);
    assert!(m.ctime() > t1);
    Ok(())
}

// -- remove_file / remove_dir -----------------------------------------------

#[test]
fn remove_file_not_a_dir() -> Result<(), VfsError> {
    let mut fs = MemFs::new();
    fs.create_dir("/d")?;
    let err = fs.remove_file("/d");
    assert!(matches!(err, Err(ref e) if *e.kind() == VfsErrorKind::IsADirectory));
    Ok(())
}

#[test]
fn remove_dir_not_empty() -> Result<(), VfsError> {
    let mut fs = MemFs::new();
    fs.create_dir_all("/d/sub")?;
    let err = fs.remove_dir("/d");
    assert!(matches!(err, Err(ref e) if *e.kind() == VfsErrorKind::DirectoryNotEmpty));
    Ok(())
}

#[test]
fn remove_dir_empty() -> Result<(), VfsError> {
    let mut fs = MemFs::new();
    fs.create_dir("/d")?;
    fs.remove_dir("/d")?;
    assert!(!fs.exists("/d"));
    Ok(())
}

#[test]
fn remove_file_missing() -> Result<(), VfsError> {
    let mut fs = MemFs::new();
    let err = fs.remove_file("/nope");
    assert!(matches!(err, Err(ref e) if *e.kind() == VfsErrorKind::NotFound));
    Ok(())
}

#[test]
fn remove_dir_on_file() -> Result<(), VfsError> {
    let mut fs = MemFs::new();
    fs.write("/f", "data")?;
    let err = fs.remove_dir("/f");
    assert!(matches!(err, Err(ref e) if *e.kind() == VfsErrorKind::NotADirectory));
    Ok(())
}

#[test]
fn remove_dir_root_fails() -> Result<(), VfsError> {
    let mut fs = MemFs::new();
    let err = fs.remove_dir("/");
    assert!(err.is_err());
    Ok(())
}

// -- copy_recursive ---------------------------------------------------------

#[test]
fn copy_recursive_dir() -> Result<(), VfsError> {
    let mut fs = MemFs::new();
    fs.create_dir_all("/src/sub")?;
    fs.write("/src/a.txt", "hello")?;
    fs.write("/src/sub/b.txt", "world")?;
    fs.copy_recursive("/src", "/dst")?;
    assert_eq!(fs.read_to_string("/dst/a.txt")?, "hello");
    assert_eq!(fs.read_to_string("/dst/sub/b.txt")?, "world");
    Ok(())
}

#[test]
fn copy_recursive_preserves_modes() -> Result<(), VfsError> {
    let mut fs = MemFs::new();
    fs.set_umask(0o000);
    fs.create_dir_all("/src")?;
    fs.write_with_mode("/src/exec.sh", "#!/bin/sh", 0o755)?;
    fs.copy_recursive("/src", "/dst")?;
    assert_eq!(fs.metadata("/dst/exec.sh")?.mode(), 0o755);
    Ok(())
}

#[test]
fn copy_recursive_file_works_like_copy() -> Result<(), VfsError> {
    let mut fs = MemFs::new();
    fs.write("/f.txt", "hello")?;
    fs.copy_recursive("/f.txt", "/g.txt")?;
    assert_eq!(fs.read_to_string("/g.txt")?, "hello");
    Ok(())
}

#[test]
fn copy_recursive_missing_source() -> Result<(), VfsError> {
    let mut fs = MemFs::new();
    assert!(fs.copy_recursive("/nope", "/dst").is_err());
    Ok(())
}

// -- create_dir_with_mode ---------------------------------------------------

#[test]
fn create_dir_with_mode_test() -> Result<(), VfsError> {
    let mut fs = MemFs::new();
    fs.set_umask(0o000);
    fs.create_dir_with_mode("/d", 0o700)?;
    assert_eq!(fs.metadata("/d")?.mode(), 0o700);
    Ok(())
}

#[test]
fn create_dir_all_with_mode_test() -> Result<(), VfsError> {
    let mut fs = MemFs::new();
    fs.set_umask(0o000);
    fs.create_dir_all_with_mode("/a/b/c", 0o700)?;
    assert_eq!(fs.metadata("/a")?.mode(), 0o700);
    assert_eq!(fs.metadata("/a/b/c")?.mode(), 0o700);
    Ok(())
}

// -- insert_raw public ------------------------------------------------------

#[test]
fn insert_raw_is_public() -> Result<(), VfsError> {
    let mut fs = MemFs::new();
    let entry = crate::Entry::file("test");
    fs.insert_raw("/f".to_string(), entry);
    assert_eq!(fs.read_to_string("/f")?, "test");
    Ok(())
}

#[test]
fn insert_raw_preserves_timestamps() -> Result<(), VfsError> {
    let mut fs = MemFs::new();
    fs.set_time(100);
    fs.write("/a", "x")?;
    let mtime = fs.metadata("/a")?.mtime();
    let entry = fs.remove("/a");
    if let Some(e) = entry {
        fs.set_time(999);
        fs.insert_raw("/b".to_string(), e);
        // insert_raw should preserve the original timestamps, not use 999
        assert_eq!(fs.metadata("/b")?.mtime(), mtime);
    }
    Ok(())
}
