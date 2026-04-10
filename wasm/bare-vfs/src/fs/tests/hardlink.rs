use super::*;

// -- hard link tests --------------------------------------------------------

#[test]
fn hard_link_creates_second_name() {
    let mut fs = MemFs::new();
    fs.write("/a.txt", "hello");
    fs.hard_link("/a.txt", "/b.txt").unwrap();
    assert_eq!(fs.read_to_string("/b.txt").unwrap(), "hello");
}

#[test]
fn hard_link_append_visible_through_both() {
    let mut fs = MemFs::new();
    fs.write("/a.txt", "hello");
    fs.hard_link("/a.txt", "/b.txt").unwrap();
    fs.append("/a.txt", b" world").unwrap();
    assert_eq!(fs.read_to_string("/b.txt").unwrap(), "hello world");
}

#[test]
fn hard_link_shares_metadata() {
    let mut fs = MemFs::new();
    fs.write("/a.txt", "x");
    fs.hard_link("/a.txt", "/b.txt").unwrap();
    fs.set_mode("/a.txt", 0o755).unwrap();
    assert_eq!(fs.metadata("/b.txt").unwrap().mode(), 0o755);
}

#[test]
fn hard_link_nlink_increments() {
    let mut fs = MemFs::new();
    fs.write("/a.txt", "x");
    assert_eq!(fs.metadata("/a.txt").unwrap().nlink(), 1);
    fs.hard_link("/a.txt", "/b.txt").unwrap();
    assert_eq!(fs.metadata("/a.txt").unwrap().nlink(), 2);
    assert_eq!(fs.metadata("/b.txt").unwrap().nlink(), 2);
}

#[test]
fn hard_link_remove_decrements_nlink() {
    let mut fs = MemFs::new();
    fs.write("/a.txt", "x");
    fs.hard_link("/a.txt", "/b.txt").unwrap();
    fs.remove("/a.txt");
    assert_eq!(fs.metadata("/b.txt").unwrap().nlink(), 1);
    assert_eq!(fs.read_to_string("/b.txt").unwrap(), "x");
}

#[test]
fn hard_link_to_dir_fails() {
    let mut fs = MemFs::new();
    fs.create_dir("/d").unwrap();
    assert!(fs.hard_link("/d", "/d2").is_err());
}

#[test]
fn hard_link_dst_exists_fails() {
    let mut fs = MemFs::new();
    fs.write("/a", "x");
    fs.write("/b", "y");
    assert!(fs.hard_link("/a", "/b").is_err());
}

#[test]
fn hard_link_not_found() {
    let mut fs = MemFs::new();
    assert!(fs.hard_link("/nope", "/b").is_err());
}

#[test]
fn hard_link_same_ino() {
    let mut fs = MemFs::new();
    fs.write("/a", "x");
    fs.hard_link("/a", "/b").unwrap();
    assert_eq!(
        fs.metadata("/a").unwrap().ino(),
        fs.metadata("/b").unwrap().ino()
    );
}

#[test]
fn hard_link_write_visible_through_both() {
    let mut fs = MemFs::new();
    fs.write("/a.txt", "original");
    fs.hard_link("/a.txt", "/b.txt").unwrap();
    // Overwrite via write() — should update in-place, preserving the hard link
    fs.write("/a.txt", "updated");
    assert_eq!(fs.read_to_string("/b.txt").unwrap(), "updated");
    // Both should still share the same inode
    assert_eq!(
        fs.metadata("/a.txt").unwrap().ino(),
        fs.metadata("/b.txt").unwrap().ino()
    );
    assert_eq!(fs.metadata("/a.txt").unwrap().nlink(), 2);
}
