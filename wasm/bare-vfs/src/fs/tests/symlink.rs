use super::*;
use crate::error::VfsErrorKind;

// -- symlink tests -------------------------------------------------------

#[test]
fn symlink_create_and_read_through() -> Result<(), VfsError> {
    let mut fs = MemFs::new();
    fs.write("/real.txt", "hello")?;
    fs.symlink("/real.txt", "/link.txt").unwrap();
    // is_symlink detects the link without following
    assert!(fs.is_symlink("/link.txt"));
    // reading through the link should yield the file content
    assert_eq!(fs.read_to_string("/link.txt"), Ok("hello"));
    // is_file follows symlinks
    assert!(fs.is_file("/link.txt"));
    assert!(!fs.is_dir("/link.txt"));
    Ok(())
}

#[test]
fn symlink_to_directory() -> Result<(), VfsError> {
    let mut fs = MemFs::new();
    fs.create_dir_all("/real/sub")?;
    fs.write("/real/sub/f.txt", "data")?;
    fs.symlink("/real", "/link").unwrap();
    // Traversal through link should reach the directory
    assert!(fs.is_dir("/link"));
    assert!(fs.is_file("/link/sub/f.txt"));
    assert_eq!(fs.read_to_string("/link/sub/f.txt"), Ok("data"));
    // read_dir through symlinked directory
    let entries = fs.read_dir("/link").unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].name, "sub");
    Ok(())
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
fn symlink_chain() -> Result<(), VfsError> {
    // a -> b -> c -> real file
    let mut fs = MemFs::new();
    fs.write("/real.txt", "chained")?;
    fs.symlink("/real.txt", "/c").unwrap();
    fs.symlink("/c", "/b").unwrap();
    fs.symlink("/b", "/a").unwrap();
    assert_eq!(fs.read_to_string("/a"), Ok("chained"));
    assert_eq!(fs.read_to_string("/b"), Ok("chained"));
    assert_eq!(fs.read_to_string("/c"), Ok("chained"));
    Ok(())
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
fn read_link_returns_target_without_following() -> Result<(), VfsError> {
    let mut fs = MemFs::new();
    fs.write("/real.txt", "data")?;
    fs.symlink("/real.txt", "/link").unwrap();
    assert_eq!(fs.read_link("/link"), Ok("/real.txt".to_string()));
    // read_link on a non-symlink returns NotASymlink
    assert_eq!(
        fs.read_link("/real.txt"),
        Err(VfsErrorKind::NotASymlink.into())
    );
    // read_link on missing path returns NotFound
    assert_eq!(fs.read_link("/missing"), Err(VfsErrorKind::NotFound.into()));
    Ok(())
}

#[test]
fn remove_symlink_does_not_remove_target() -> Result<(), VfsError> {
    let mut fs = MemFs::new();
    fs.write("/real.txt", "keep me")?;
    fs.symlink("/real.txt", "/link").unwrap();
    // Remove the symlink
    let removed = fs.remove("/link");
    assert!(removed.is_some());
    // The target should still exist
    assert_eq!(fs.read_to_string("/real.txt"), Ok("keep me"));
    // The symlink should be gone
    assert!(!fs.is_symlink("/link"));
    assert!(!fs.exists("/link"));
    Ok(())
}

#[test]
fn symlink_relative_resolution() -> Result<(), VfsError> {
    let mut fs = MemFs::new();
    fs.create_dir_all("/a/b")?;
    fs.write("/a/b/real.txt", "relative")?;
    // Create a relative symlink in /a/b pointing to real.txt (same dir)
    fs.symlink("real.txt", "/a/b/link").unwrap();
    assert_eq!(fs.read_to_string("/a/b/link"), Ok("relative"));
    // Also test a relative symlink going up one level
    fs.write("/a/top.txt", "top")?;
    fs.symlink("../top.txt", "/a/b/up_link").unwrap();
    assert_eq!(fs.read_to_string("/a/b/up_link"), Ok("top"));
    Ok(())
}

#[test]
fn symlink_in_intermediate_path_component() -> Result<(), VfsError> {
    let mut fs = MemFs::new();
    fs.create_dir_all("/real_dir")?;
    fs.write("/real_dir/file.txt", "found")?;
    // /link -> /real_dir; access /link/file.txt
    fs.symlink("/real_dir", "/link").unwrap();
    assert_eq!(fs.read_to_string("/link/file.txt"), Ok("found"));
    // write through symlinked directory
    fs.write("/link/new.txt", "new")?;
    assert_eq!(fs.read_to_string("/real_dir/new.txt"), Ok("new"));
    Ok(())
}

#[test]
fn symlink_metadata_is_symlink() -> Result<(), VfsError> {
    let mut fs = MemFs::new();
    fs.write("/real.txt", "x")?;
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
    Ok(())
}

#[test]
fn symlink_entry_is_symlink() -> Result<(), VfsError> {
    let mut fs = MemFs::new();
    fs.write("/real.txt", "x")?;
    fs.symlink("/real.txt", "/link").unwrap();
    // get() follows symlinks, so returns File
    let e = fs.get("/link").unwrap();
    assert!(e.is_file());
    assert!(!e.is_symlink());
    // insert/remove round-trip
    let removed = fs.remove("/link").unwrap();
    assert!(removed.is_symlink());
    Ok(())
}

#[test]
fn symlink_insert_via_entry() -> Result<(), VfsError> {
    let mut fs = MemFs::new();
    fs.write("/real.txt", "data")?;
    fs.insert("/link".to_string(), Entry::symlink("/real.txt"));
    assert!(fs.is_symlink("/link"));
    assert_eq!(fs.read_to_string("/link"), Ok("data"));
    Ok(())
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
fn is_symlink_on_non_symlink() -> Result<(), VfsError> {
    let mut fs = MemFs::new();
    fs.write("/f.txt", "x")?;
    fs.create_dir_all("/d")?;
    assert!(!fs.is_symlink("/f.txt"));
    assert!(!fs.is_symlink("/d"));
    assert!(!fs.is_symlink("/missing"));
    Ok(())
}
