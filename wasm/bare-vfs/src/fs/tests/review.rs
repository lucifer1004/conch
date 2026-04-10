use super::*;
use crate::error::VfsErrorKind;

// -- Path safety tests ------------------------------------------------------

#[test]
fn normalize_dotdot_in_path() -> Result<(), VfsError> {
    let mut fs = MemFs::new();
    fs.create_dir_all("/a/b")?;
    fs.write("/a/b/file", "data")?;
    // Access via unnormalized path with ..
    assert_eq!(fs.read_to_string("/a/b/../b/file").unwrap(), "data");
    Ok(())
}

#[test]
fn normalize_dot_in_path() -> Result<(), VfsError> {
    let mut fs = MemFs::new();
    fs.write("/file", "x")?;
    assert_eq!(fs.read_to_string("/./file").unwrap(), "x");
    Ok(())
}

#[test]
fn canonical_path_resolves_symlinks() -> Result<(), VfsError> {
    let mut fs = MemFs::new();
    fs.create_dir_all("/real/dir")?;
    fs.write("/real/dir/file", "x")?;
    fs.symlink("/real/dir", "/link").unwrap();
    let canon = fs.canonical_path("/link/file").unwrap();
    assert_eq!(canon, "/real/dir/file");
    Ok(())
}

#[test]
fn canonical_path_normalizes() -> Result<(), VfsError> {
    let mut fs = MemFs::new();
    fs.create_dir_all("/a/b")?;
    let canon = fs.canonical_path("/a/b/../b").unwrap();
    assert_eq!(canon, "/a/b");
    Ok(())
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

// -- Review fix regression tests ----------------------------------------

#[test]
fn write_preserves_hard_link() -> Result<(), VfsError> {
    // Issue #1: write() used to break hard links by allocating new inode
    let mut fs = MemFs::new();
    fs.write("/a", "original")?;
    fs.hard_link("/a", "/b").unwrap();
    let ino_before = fs.metadata("/a").unwrap().ino();

    fs.write("/a", "updated")?;

    // Same inode (not a new one)
    assert_eq!(fs.metadata("/a").unwrap().ino(), ino_before);
    // Visible through both names
    assert_eq!(fs.read_to_string("/b").unwrap(), "updated");
    // nlink preserved
    assert_eq!(fs.metadata("/a").unwrap().nlink(), 2);
    Ok(())
}

#[test]
fn write_with_mode_preserves_hard_link() -> Result<(), VfsError> {
    let mut fs = MemFs::new();
    fs.write("/a", "x")?;
    fs.hard_link("/a", "/b").unwrap();
    fs.write_with_mode("/a", "y", 0o755)?;
    assert_eq!(fs.read_to_string("/b").unwrap(), "y");
    assert_eq!(fs.metadata("/b").unwrap().mode(), 0o755);
    Ok(())
}

#[test]
fn rename_to_missing_parent_fails() -> Result<(), VfsError> {
    // Issue #4: rename() used to orphan inodes when dst parent was missing
    let mut fs = MemFs::new();
    fs.write("/a", "data")?;
    let result = fs.rename("/a", "/no/such/dir/b");
    assert!(result.is_err());
    // Source should still exist (not orphaned)
    assert_eq!(fs.read_to_string("/a").unwrap(), "data");
    Ok(())
}

#[test]
fn copy_to_missing_parent_fails() -> Result<(), VfsError> {
    // Issue #6: copy() used to leak inodes when dst parent was missing
    let mut fs = MemFs::new();
    fs.write("/a", "data")?;
    let result = fs.copy("/a", "/no/such/dir/b");
    assert!(result.is_err());
    // Source unchanged
    assert_eq!(fs.read_to_string("/a").unwrap(), "data");
    Ok(())
}

#[test]
fn symlink_fails_if_link_path_exists() -> Result<(), VfsError> {
    // Issue #7: symlink() used to silently overwrite existing entries
    let mut fs = MemFs::new();
    fs.write("/existing", "data")?;
    let result = fs.symlink("/target", "/existing");
    assert!(matches!(
        result,
        Err(ref e) if *e.kind() == VfsErrorKind::AlreadyExists
    ));
    // Original file untouched
    assert!(fs.is_file("/existing"));
    assert_eq!(fs.read_to_string("/existing").unwrap(), "data");
    Ok(())
}

#[test]
fn symlink_fails_if_dir_exists_at_path() {
    let mut fs = MemFs::new();
    fs.create_dir("/d").unwrap();
    let result = fs.symlink("/target", "/d");
    assert!(result.is_err());
    assert!(fs.is_dir("/d"));
}

#[test]
fn clone_memfs_is_independent() -> Result<(), VfsError> {
    // Issue #9: MemFs now implements Clone
    let mut fs = MemFs::new();
    fs.write("/a", "hello")?;
    let mut clone = fs.clone();
    clone.write("/a", "changed")?;
    // Original unaffected
    assert_eq!(fs.read_to_string("/a").unwrap(), "hello");
    assert_eq!(clone.read_to_string("/a").unwrap(), "changed");
    Ok(())
}

#[test]
fn hard_link_nlink_after_remove_all_links() -> Result<(), VfsError> {
    // Verify inode is freed when last link is removed
    let mut fs = MemFs::new();
    fs.write("/a", "data")?;
    fs.hard_link("/a", "/b").unwrap();
    fs.remove("/a");
    fs.remove("/b");
    // Both paths should be gone
    assert!(!fs.exists("/a"));
    assert!(!fs.exists("/b"));
    Ok(())
}

#[test]
fn write_to_nonexistent_creates_new_inode() -> Result<(), VfsError> {
    // write() to new path should still work (not just in-place update)
    let mut fs = MemFs::new();
    fs.write("/new", "content")?;
    assert_eq!(fs.read_to_string("/new").unwrap(), "content");
    assert_eq!(fs.metadata("/new").unwrap().nlink(), 1);
    Ok(())
}

#[test]
fn write_overwrites_dir_creates_new_inode() -> Result<(), VfsError> {
    // write() over a directory returns IsADirectory error
    let mut fs = MemFs::new();
    fs.create_dir("/d").unwrap();
    assert!(fs.write("/d", "now a file").is_err());
    // /d is still a directory
    assert!(fs.is_dir("/d"));
    Ok(())
}
