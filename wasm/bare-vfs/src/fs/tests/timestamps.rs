use super::*;
use crate::error::VfsErrorKind;

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
fn timestamps_tick_on_write() -> Result<(), VfsError> {
    let mut fs = MemFs::new();
    fs.write("/a", "hello")?;
    assert_eq!(fs.time(), 1);
    let meta = fs.metadata("/a").unwrap();
    assert_eq!(meta.mtime(), 1);
    assert_eq!(meta.ctime(), 1);

    fs.write("/b", "world")?;
    assert_eq!(fs.time(), 2);
    let meta = fs.metadata("/b").unwrap();
    assert_eq!(meta.mtime(), 2);
    assert_eq!(meta.ctime(), 2);
    Ok(())
}

#[test]
fn timestamps_set_time_override() -> Result<(), VfsError> {
    let mut fs = MemFs::new();
    fs.set_time(1000);
    fs.write("/a", "hello")?;
    assert_eq!(fs.time(), 1001);
    let meta = fs.metadata("/a").unwrap();
    assert_eq!(meta.mtime(), 1001);
    Ok(())
}

#[test]
fn timestamps_append_updates_mtime() -> Result<(), VfsError> {
    let mut fs = MemFs::new();
    fs.write("/a", "hello")?;
    let t1 = fs.metadata("/a").unwrap().mtime();
    fs.append("/a", b" world").unwrap();
    let meta = fs.metadata("/a").unwrap();
    assert!(meta.mtime() > t1);
    assert!(meta.ctime() > t1);
    Ok(())
}

#[test]
fn timestamps_touch_updates_existing_mtime() -> Result<(), VfsError> {
    let mut fs = MemFs::new();
    fs.write("/a", "hello")?;
    let t1 = fs.metadata("/a").unwrap().mtime();
    fs.touch("/a")?;
    let t2 = fs.metadata("/a").unwrap().mtime();
    assert!(t2 > t1);
    // Content unchanged
    assert_eq!(fs.read_to_string("/a").unwrap(), "hello");
    Ok(())
}

#[test]
fn timestamps_touch_creates_with_timestamps() -> Result<(), VfsError> {
    let mut fs = MemFs::new();
    fs.touch("/new")?;
    let meta = fs.metadata("/new").unwrap();
    assert!(meta.mtime() > 0);
    assert!(meta.ctime() > 0);
    Ok(())
}

#[test]
fn timestamps_set_mode_only_updates_ctime() -> Result<(), VfsError> {
    let mut fs = MemFs::new();
    fs.write("/a", "hello")?;
    let m = fs.metadata("/a").unwrap();
    let orig_mtime = m.mtime();
    let orig_ctime = m.ctime();
    fs.set_mode("/a", 0o755).unwrap();
    let m2 = fs.metadata("/a").unwrap();
    assert_eq!(m2.mtime(), orig_mtime); // mtime unchanged
    assert!(m2.ctime() > orig_ctime); // ctime updated
    Ok(())
}

#[test]
fn timestamps_chown_only_updates_ctime() -> Result<(), VfsError> {
    let mut fs = MemFs::new();
    fs.write("/a", "hello")?;
    let m = fs.metadata("/a").unwrap();
    let orig_mtime = m.mtime();
    let orig_ctime = m.ctime();
    fs.chown("/a", 1000, 1000).unwrap();
    let m2 = fs.metadata("/a").unwrap();
    assert_eq!(m2.mtime(), orig_mtime);
    assert!(m2.ctime() > orig_ctime);
    Ok(())
}

#[test]
fn timestamps_rename_preserves() -> Result<(), VfsError> {
    let mut fs = MemFs::new();
    fs.write("/a", "hello")?;
    let m = fs.metadata("/a").unwrap();
    let orig_mtime = m.mtime();
    let orig_ctime = m.ctime();
    fs.rename("/a", "/b").unwrap();
    let m2 = fs.metadata("/b").unwrap();
    assert_eq!(m2.mtime(), orig_mtime);
    assert_eq!(m2.ctime(), orig_ctime);
    Ok(())
}

#[test]
fn timestamps_copy_gets_new_timestamps() -> Result<(), VfsError> {
    let mut fs = MemFs::new();
    fs.write("/a", "hello")?;
    let src_time = fs.metadata("/a").unwrap().mtime();
    fs.copy("/a", "/b").unwrap();
    let dst_time = fs.metadata("/b").unwrap().mtime();
    assert!(dst_time > src_time);
    Ok(())
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
fn timestamps_symlink() -> Result<(), VfsError> {
    let mut fs = MemFs::new();
    fs.write("/target", "x")?;
    fs.symlink("/target", "/link").unwrap();
    let meta = fs.symlink_metadata("/link").unwrap();
    assert!(meta.mtime() > 0);
    Ok(())
}

#[test]
fn timestamps_ordering() -> Result<(), VfsError> {
    let mut fs = MemFs::new();
    fs.write("/first", "a")?;
    fs.write("/second", "b")?;
    fs.write("/third", "c")?;
    let t1 = fs.metadata("/first").unwrap().mtime();
    let t2 = fs.metadata("/second").unwrap().mtime();
    let t3 = fs.metadata("/third").unwrap().mtime();
    assert!(t1 < t2);
    assert!(t2 < t3);
    Ok(())
}

#[test]
fn timestamps_in_dir_entry() -> Result<(), VfsError> {
    let mut fs = MemFs::new();
    fs.write("/a", "hello")?;
    let entries = fs.read_dir("/").unwrap();
    let e = &entries[0];
    assert_eq!(e.name, "a");
    assert!(e.mtime > 0);
    Ok(())
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
fn atime_initialized_with_mtime() -> Result<(), VfsError> {
    let mut fs = MemFs::new();
    fs.write("/f.txt", "hello")?;
    let meta = fs.metadata("/f.txt").unwrap();
    assert_eq!(meta.atime(), meta.mtime());
    Ok(())
}

#[test]
fn atime_not_updated_on_set_mode() -> Result<(), VfsError> {
    let mut fs = MemFs::new();
    fs.write("/f.txt", "hello")?;
    let atime_before = fs.metadata("/f.txt").unwrap().atime();
    fs.set_mode("/f.txt", 0o600).unwrap();
    let atime_after = fs.metadata("/f.txt").unwrap().atime();
    assert_eq!(atime_before, atime_after);
    Ok(())
}

#[test]
fn set_atime_explicit() -> Result<(), VfsError> {
    let mut fs = MemFs::new();
    fs.write("/f.txt", "hello")?;
    fs.set_atime("/f.txt", 999).unwrap();
    assert_eq!(fs.metadata("/f.txt").unwrap().atime(), 999);
    Ok(())
}

#[test]
fn set_atime_not_found() {
    let mut fs = MemFs::new();
    assert_eq!(fs.set_atime("/nope", 1), Err(VfsErrorKind::NotFound.into()));
}

#[test]
fn nlink_file_is_one() -> Result<(), VfsError> {
    let mut fs = MemFs::new();
    fs.write("/f.txt", "hello")?;
    assert_eq!(fs.metadata("/f.txt").unwrap().nlink(), 1);
    Ok(())
}

#[test]
fn nlink_dir_is_two() {
    let mut fs = MemFs::new();
    fs.create_dir("/d").unwrap();
    assert_eq!(fs.metadata("/d").unwrap().nlink(), 2);
}

#[test]
fn dir_entry_has_size() -> Result<(), VfsError> {
    let mut fs = MemFs::new();
    fs.write("/file.txt", "hello world")?;
    fs.create_dir("/sub").unwrap();
    let entries = fs.read_dir("/").unwrap();
    let file_e = entries.iter().find(|e| e.name == "file.txt").unwrap();
    let dir_e = entries.iter().find(|e| e.name == "sub").unwrap();
    assert_eq!(file_e.size, 11);
    assert_eq!(dir_e.size, 0);
    Ok(())
}
