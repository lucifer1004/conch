use super::*;
use crate::error::VfsErrorKind;
#[cfg(feature = "std")]
use std::io::Write as _;

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
    // Make root world-writable so non-root user can create files in it
    fs.set_mode("/", 0o777).unwrap();
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
    // Make root world-writable so non-root user can create files in it
    fs.set_mode("/", 0o777).unwrap();
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

// -- Bug 1: set_mode() and chown() authorization checks --------------------

#[test]
fn set_mode_requires_owner_or_root() {
    let mut fs = MemFs::new();
    fs.write("/f", "data");
    // As root, change owner to 1000
    fs.chown("/f", 1000, 1000).unwrap();
    // As a different user (2000), set_mode should fail
    fs.set_current_user(2000, 2000);
    assert_eq!(
        fs.set_mode("/f", 0o777),
        Err(VfsErrorKind::PermissionDenied.into())
    );
    // As the file owner (1000), set_mode should succeed
    fs.set_current_user(1000, 1000);
    assert!(fs.set_mode("/f", 0o777).is_ok());
}

#[test]
fn chown_change_uid_requires_root() {
    let mut fs = MemFs::new();
    fs.write("/f", "data");
    // Non-root cannot change owner uid
    fs.set_current_user(1000, 1000);
    assert!(fs.chown("/f", 2000, 1000).is_err());
    // Root can change owner
    fs.set_current_user(0, 0);
    assert!(fs.chown("/f", 2000, 2000).is_ok());
    let m = fs.metadata("/f").unwrap();
    assert_eq!(m.uid(), 2000);
    assert_eq!(m.gid(), 2000);
}

#[test]
fn chown_owner_can_change_group_to_own_group() {
    let mut fs = MemFs::new();
    fs.write("/f", "data");
    fs.chown("/f", 1000, 1000).unwrap(); // root sets owner
    fs.set_current_user(1000, 1000);
    fs.add_supplementary_gid(500);
    // Owner can change gid to a group they belong to
    assert!(fs.chown("/f", 1000, 500).is_ok());
    assert_eq!(fs.metadata("/f").unwrap().gid(), 500);
    // Owner can change gid to their primary group
    assert!(fs.chown("/f", 1000, 1000).is_ok());
}

#[test]
fn chown_owner_cannot_change_group_to_foreign_group() {
    let mut fs = MemFs::new();
    fs.write("/f", "data");
    fs.chown("/f", 1000, 1000).unwrap();
    fs.set_current_user(1000, 1000);
    // Owner cannot change gid to a group they don't belong to
    assert!(fs.chown("/f", 1000, 9999).is_err());
}

#[test]
fn chown_non_owner_cannot_change_group() {
    let mut fs = MemFs::new();
    fs.write("/f", "data");
    fs.chown("/f", 1000, 1000).unwrap();
    fs.set_current_user(2000, 2000); // different user
                                     // Not the file owner — cannot change anything
    assert!(fs.chown("/f", 1000, 2000).is_err());
}

// -- Bug 3: read_dir() requires read permission on directory ---------------

#[test]
fn read_dir_requires_read_permission() {
    let mut fs = MemFs::new();
    fs.create_dir("/d").unwrap();
    fs.write("/d/f", "x");
    // As root, set owner and mode (exec-only, no read)
    fs.chown("/d", 1000, 1000).unwrap();
    fs.set_mode("/d", 0o100).unwrap(); // execute only, no read
                                       // As owner with no read bit, read_dir should fail
    fs.set_current_user(1000, 1000);
    assert_eq!(
        fs.read_dir("/d"),
        Err(VfsErrorKind::PermissionDenied.into())
    );
}

#[test]
fn read_dir_iter_requires_read_permission() {
    let mut fs = MemFs::new();
    fs.create_dir("/d").unwrap();
    fs.write("/d/f", "x");
    fs.chown("/d", 1000, 1000).unwrap();
    fs.set_mode("/d", 0o100).unwrap(); // execute only
    fs.set_current_user(1000, 1000);
    assert_eq!(
        fs.read_dir_iter("/d").map(|_| ()),
        Err(VfsErrorKind::PermissionDenied.into())
    );
}

#[test]
fn read_dir_root_bypasses_read_permission() {
    let mut fs = MemFs::new();
    fs.create_dir("/d").unwrap();
    fs.write("/d/f", "x");
    fs.set_mode("/d", 0o000).unwrap(); // no permissions at all
                                       // root can still read directory
    assert!(fs.read_dir("/d").is_ok());
}

// -- Bug 2: OpenOptions write-only open doesn't need read permission -------

#[cfg(feature = "std")]
#[test]
fn open_write_only_succeeds_without_read_perm() {
    use crate::open_options::OpenOptions;
    let mut fs = MemFs::new();
    fs.write("/f", "data");
    // As root, set owner and mode (write-only for owner)
    fs.chown("/f", 1000, 1000).unwrap();
    fs.set_mode("/f", 0o200).unwrap(); // write-only for owner
    fs.set_current_user(1000, 1000);
    // Write-only open should succeed (no read permission needed)
    let handle = OpenOptions::new().write(true).open(&mut fs, "/f");
    assert!(
        handle.is_ok(),
        "write-only open should succeed without read perm"
    );
}

#[cfg(feature = "std")]
#[test]
fn open_read_only_fails_without_read_perm() {
    use crate::open_options::OpenOptions;
    let mut fs = MemFs::new();
    fs.write("/f", "data");
    fs.chown("/f", 1000, 1000).unwrap();
    fs.set_mode("/f", 0o200).unwrap(); // write-only
    fs.set_current_user(1000, 1000);
    let handle = OpenOptions::new().read(true).open(&mut fs, "/f");
    assert_eq!(
        handle.map(|_| ()),
        Err(VfsErrorKind::PermissionDenied.into())
    );
}

// -- Bug 4: create_dir_all() follows symlinks in intermediate components ---

#[test]
fn create_dir_all_follows_symlinks() {
    let mut fs = MemFs::new();
    fs.create_dir("/real").unwrap();
    fs.symlink("/real", "/link").unwrap();
    fs.create_dir_all("/link/sub/deep");
    assert!(fs.is_dir("/real/sub/deep"), "/real/sub/deep should exist");
    assert!(
        fs.is_dir("/link/sub/deep"),
        "/link/sub/deep should be accessible through symlink"
    );
}

// -- Security fix 1: write()/write_with_mode() respect file write permission -

#[test]
fn write_respects_file_permission() {
    let mut fs = MemFs::new();
    fs.write("/f", "original");
    fs.chown("/f", 1000, 1000).unwrap();
    fs.set_mode("/f", 0o444).unwrap(); // read-only
    fs.set_current_user(2000, 2000); // different non-root user
    fs.write("/f", "hacked");
    assert_eq!(fs.read_to_string("/f").unwrap(), "original"); // unchanged
}

#[test]
fn write_with_mode_respects_file_permission() {
    let mut fs = MemFs::new();
    fs.write_with_mode("/f", "original", 0o444);
    fs.chown("/f", 1000, 1000).unwrap();
    fs.set_current_user(2000, 2000);
    fs.write_with_mode("/f", "hacked", 0o644);
    assert_eq!(fs.read_to_string("/f").unwrap(), "original"); // unchanged
}

#[test]
fn write_root_bypasses_file_permission() {
    let mut fs = MemFs::new();
    fs.write_with_mode("/f", "original", 0o444);
    // uid=0 (root) can still overwrite
    fs.write("/f", "updated");
    assert_eq!(fs.read_to_string("/f").unwrap(), "updated");
}

// -- Security fix 2: parent directory write permission enforced --------------

#[test]
fn cannot_create_file_in_readonly_dir() {
    let mut fs = MemFs::new();
    fs.create_dir("/d").unwrap();
    fs.set_mode("/d", 0o555).unwrap(); // read+exec, no write
    fs.set_current_user(1000, 1000);
    fs.write("/d/newfile", "data");
    assert!(!fs.exists("/d/newfile")); // creation should fail
}

#[test]
fn cannot_remove_from_readonly_dir() {
    let mut fs = MemFs::new();
    fs.create_dir("/d").unwrap();
    fs.write("/d/f", "data");
    fs.set_mode("/d", 0o555).unwrap();
    fs.set_current_user(1000, 1000);
    assert!(fs.remove("/d/f").is_none()); // should fail
    assert!(fs.exists("/d/f")); // still there
}

#[test]
fn cannot_symlink_in_readonly_dir() {
    let mut fs = MemFs::new();
    fs.create_dir("/d").unwrap();
    fs.set_mode("/d", 0o555).unwrap();
    fs.set_current_user(1000, 1000);
    assert!(fs.symlink("/target", "/d/link").is_err());
}

#[test]
fn root_bypasses_parent_dir_write_check() {
    let mut fs = MemFs::new();
    fs.create_dir("/d").unwrap();
    fs.set_mode("/d", 0o555).unwrap();
    // uid=0 (root) should still be able to write
    fs.write("/d/f", "data");
    assert!(fs.exists("/d/f"));
}

// -- Security fix 3: FileHandle Write respects access mode -------------------

#[test]
fn read_only_handle_rejects_write() {
    let mut fs = MemFs::new();
    fs.write("/f", "data");
    let mut handle = fs.open("/f").unwrap();
    let result = handle.write(b"nope");
    assert!(result.is_err());
}

#[test]
fn writable_handle_accepts_write() {
    let mut fs = MemFs::new();
    fs.write("/f", "data");
    let mut handle = crate::OpenOptions::new()
        .read(true)
        .write(true)
        .open(&mut fs, "/f")
        .unwrap();
    assert!(handle.write(b"ok").is_ok());
}

#[test]
fn commit_on_readonly_handle_is_noop() {
    let mut fs = MemFs::new();
    fs.write("/f", "original");
    let handle = fs.open("/f").unwrap();
    assert!(!handle.is_writable());
    // commit should be a no-op — content should not change
    fs.commit("/f", handle);
    assert_eq!(fs.read_to_string("/f").unwrap(), "original");
}
