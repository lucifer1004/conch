use super::*;

// -- hard link tests --------------------------------------------------------

#[test]
fn hard_link_creates_second_name() -> Result<(), VfsError> {
    let mut fs = MemFs::new();
    fs.write("/a.txt", "hello")?;
    fs.hard_link("/a.txt", "/b.txt")?;
    assert_eq!(fs.read_to_string("/b.txt")?, "hello");
    Ok(())
}

#[test]
fn hard_link_append_visible_through_both() -> Result<(), VfsError> {
    let mut fs = MemFs::new();
    fs.write("/a.txt", "hello")?;
    fs.hard_link("/a.txt", "/b.txt")?;
    fs.append("/a.txt", b" world")?;
    assert_eq!(fs.read_to_string("/b.txt")?, "hello world");
    Ok(())
}

#[test]
fn hard_link_shares_metadata() -> Result<(), VfsError> {
    let mut fs = MemFs::new();
    fs.write("/a.txt", "x")?;
    fs.hard_link("/a.txt", "/b.txt")?;
    fs.set_mode("/a.txt", 0o755)?;
    assert_eq!(fs.metadata("/b.txt")?.mode(), 0o755);
    Ok(())
}

#[test]
fn hard_link_nlink_increments() -> Result<(), VfsError> {
    let mut fs = MemFs::new();
    fs.write("/a.txt", "x")?;
    assert_eq!(fs.metadata("/a.txt")?.nlink(), 1);
    fs.hard_link("/a.txt", "/b.txt")?;
    assert_eq!(fs.metadata("/a.txt")?.nlink(), 2);
    assert_eq!(fs.metadata("/b.txt")?.nlink(), 2);
    Ok(())
}

#[test]
fn hard_link_remove_decrements_nlink() -> Result<(), VfsError> {
    let mut fs = MemFs::new();
    fs.write("/a.txt", "x")?;
    fs.hard_link("/a.txt", "/b.txt")?;
    fs.remove("/a.txt");
    assert_eq!(fs.metadata("/b.txt")?.nlink(), 1);
    assert_eq!(fs.read_to_string("/b.txt")?, "x");
    Ok(())
}

#[test]
fn hard_link_to_dir_fails() -> Result<(), VfsError> {
    let mut fs = MemFs::new();
    fs.create_dir("/d")?;
    assert!(fs.hard_link("/d", "/d2").is_err());
    Ok(())
}

#[test]
fn hard_link_dst_exists_fails() -> Result<(), VfsError> {
    let mut fs = MemFs::new();
    fs.write("/a", "x")?;
    fs.write("/b", "y")?;
    assert!(fs.hard_link("/a", "/b").is_err());
    Ok(())
}

#[test]
fn hard_link_not_found() {
    let mut fs = MemFs::new();
    assert!(fs.hard_link("/nope", "/b").is_err());
}

#[test]
fn hard_link_same_ino() -> Result<(), VfsError> {
    let mut fs = MemFs::new();
    fs.write("/a", "x")?;
    fs.hard_link("/a", "/b")?;
    assert_eq!(fs.metadata("/a")?.ino(), fs.metadata("/b")?.ino());
    Ok(())
}

#[test]
fn hard_link_write_visible_through_both() -> Result<(), VfsError> {
    let mut fs = MemFs::new();
    fs.write("/a.txt", "original")?;
    fs.hard_link("/a.txt", "/b.txt")?;
    // Overwrite via write() — should update in-place, preserving the hard link
    fs.write("/a.txt", "updated")?;
    assert_eq!(fs.read_to_string("/b.txt")?, "updated");
    // Both should still share the same inode
    assert_eq!(fs.metadata("/a.txt")?.ino(), fs.metadata("/b.txt")?.ino());
    assert_eq!(fs.metadata("/a.txt")?.nlink(), 2);
    Ok(())
}
