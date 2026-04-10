use super::*;

// -- Serde round-trip tests -----------------------------------------------

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
fn roundtrip_hard_links() {
    let mut fs = MemFs::new();
    fs.write("/a.txt", "shared");
    fs.hard_link("/a.txt", "/b.txt").unwrap();

    let json = serde_json::to_string(&fs).unwrap();
    let fs2: MemFs = serde_json::from_str(&json).unwrap();

    // Both names exist and share content
    assert_eq!(fs2.read_to_string("/a.txt").unwrap(), "shared");
    assert_eq!(fs2.read_to_string("/b.txt").unwrap(), "shared");

    // They share the same inode (hard link preserved)
    assert_eq!(
        fs2.metadata("/a.txt").unwrap().ino(),
        fs2.metadata("/b.txt").unwrap().ino()
    );
    assert_eq!(fs2.metadata("/a.txt").unwrap().nlink(), 2);

    // Mutating through one name is visible through the other
    let mut fs2 = fs2;
    fs2.append("/a.txt", b" data").unwrap();
    assert_eq!(fs2.read_to_string("/b.txt").unwrap(), "shared data");
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

// -- Security fix 4: Serde deserialization validates root_ino ----------------

#[test]
fn deserialize_invalid_root_ino_fails() {
    // root_ino=999 is not present in the inode table
    let json = r#"{"inodes":[],"paths":[],"next_ino":10,"root_ino":999,"current_uid":0,"current_gid":0,"supplementary_gids":[],"time":0,"umask":18}"#;
    let result: Result<MemFs, _> = serde_json::from_str(json);
    assert!(result.is_err());
}

#[test]
fn deserialize_root_ino_pointing_to_file_fails() {
    // Construct a snapshot where root_ino points to a File inode, not a Dir
    let mut fs = MemFs::new();
    fs.write("/f", "data");
    let mut snap: serde_json::Value = serde_json::to_value(&fs).unwrap();
    // Find the inode number of /f and set root_ino to it
    let file_ino = fs.metadata("/f").unwrap().ino();
    snap["root_ino"] = serde_json::json!(file_ino);
    let result: Result<MemFs, _> = serde_json::from_value(snap);
    assert!(result.is_err());
}
