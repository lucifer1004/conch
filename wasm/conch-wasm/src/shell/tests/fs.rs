use super::*;

#[test]
fn cat_reads_seeded_file() {
    let mut s = shell_with_files(serde_json::json!({
        "note.txt": "hello"
    }));
    let (out, code, _) = s.run_line("cat note.txt");
    assert_eq!(code, 0);
    assert_eq!(out, "hello");
}

#[test]
fn cat_unreadable_mode_zero() {
    let mut s = shell_with_files(serde_json::json!({
        "sec.txt": { "content": "secret", "mode": 0 }
    }));
    let (out, code, _) = s.run_line("cat sec.txt");
    assert_eq!(code, 1);
    assert!(out.contains("Permission denied"), "got {:?}", out);
}

#[test]
fn cat_missing_file() {
    let mut s = shell();
    let (out, code, _) = s.run_line("cat missing.txt");
    assert_eq!(code, 1);
    assert!(out.contains("No such file"), "got {:?}", out);
}

#[test]
fn cat_n_shows_line_numbers() {
    let mut s = shell_with_files(serde_json::json!({
        "lines.txt": "alpha\nbeta"
    }));
    let (out, code, _) = s.run_line("cat -n lines.txt");
    assert_eq!(code, 0);
    assert!(out.contains("1"), "got {:?}", out);
    assert!(out.contains("2"), "got {:?}", out);
    assert!(out.contains("alpha"), "got {:?}", out);
    assert!(out.contains("beta"), "got {:?}", out);
}

#[test]
fn chmod_restores_read_then_cat_succeeds() {
    let mut s = shell_with_files(serde_json::json!({
        "sec.txt": { "content": "ok", "mode": 0 }
    }));
    let (_, c1, _) = s.run_line("cat sec.txt");
    assert_ne!(c1, 0);
    let (_, c2, _) = s.run_line("chmod 644 sec.txt");
    assert_eq!(c2, 0);
    let (out, c3, _) = s.run_line("cat sec.txt");
    assert_eq!(c3, 0);
    assert_eq!(out, "ok");
}

#[test]
fn chmod_missing_target() {
    let mut s = shell();
    let (out, code, _) = s.run_line("chmod 644 nope.txt");
    assert_eq!(code, 1);
    assert!(out.contains("cannot access"), "got {:?}", out);
}

#[test]
fn chmod_invalid_mode() {
    let mut s = shell_with_files(serde_json::json!({ "f.txt": "x" }));
    let (out, code, _) = s.run_line("chmod zzz f.txt");
    assert_eq!(code, 1);
    assert!(out.contains("invalid mode"), "got {:?}", out);
}

#[test]
fn mkdir_touch_ls_lists_new_file() {
    let mut s = shell();
    let (_, c1, _) = s.run_line("mkdir d");
    assert_eq!(c1, 0);
    let (_, c2, _) = s.run_line("touch d/x.txt");
    assert_eq!(c2, 0);
    let (listing, c3, _) = s.run_line("ls d");
    assert_eq!(c3, 0);
    assert!(
        listing.contains("x.txt"),
        "expected x.txt in listing, got {:?}",
        listing
    );
}

#[test]
fn mkdir_nested_without_p_fails() {
    let mut s = shell();
    let (out, code, _) = s.run_line("mkdir a/b/c");
    assert_eq!(code, 1);
    assert!(
        out.contains("cannot create") || out.contains("No such file"),
        "got {:?}",
        out
    );
}

#[test]
fn mkdir_p_nested() {
    let mut s = shell();
    assert_eq!(s.run_line("mkdir -p x/y/z").1, 0);
    let (listing, code, _) = s.run_line("ls x/y");
    assert_eq!(code, 0);
    assert!(listing.contains("z"), "got {:?}", listing);
}

#[test]
fn touch_creates_empty_file_then_cat() {
    let mut s = shell();
    let (_, c1, _) = s.run_line("touch empty.txt");
    assert_eq!(c1, 0);
    let (out, c2, _) = s.run_line("cat empty.txt");
    assert_eq!(c2, 0);
    assert_eq!(out, "");
}

#[test]
fn cp_copies_file() {
    let mut s = shell_with_files(serde_json::json!({
        "a.txt": "alpha"
    }));
    let (_, c1, _) = s.run_line("cp a.txt b.txt");
    assert_eq!(c1, 0);
    let (out, c2, _) = s.run_line("cat b.txt");
    assert_eq!(c2, 0);
    assert_eq!(out, "alpha");
}

#[test]
fn cp_into_existing_directory() {
    let mut s = shell_with_files(serde_json::json!({
        "src.txt": "payload"
    }));
    assert_eq!(s.run_line("mkdir bin").1, 0);
    let (_, c1, _) = s.run_line("cp src.txt bin");
    assert_eq!(c1, 0);
    let (out, c2, _) = s.run_line("cat bin/src.txt");
    assert_eq!(c2, 0);
    assert_eq!(out, "payload");
}

#[test]
fn cp_source_unreadable() {
    let mut s = shell_with_files(serde_json::json!({
        "locked.txt": { "content": "x", "mode": 0 }
    }));
    let (out, code, _) = s.run_line("cp locked.txt copy.txt");
    assert_eq!(code, 1);
    assert!(out.contains("Permission denied"), "got {:?}", out);
}

#[test]
fn cp_missing_source() {
    let mut s = shell();
    let (out, code, _) = s.run_line("cp nowhere.txt out.txt");
    assert_eq!(code, 1);
    assert!(out.contains("cannot stat"), "got {:?}", out);
}

#[test]
fn cp_source_directory_omits() {
    let mut s = shell();
    assert_eq!(s.run_line("mkdir srcdir").1, 0);
    let (out, code, _) = s.run_line("cp srcdir out.txt");
    assert_eq!(code, 1);
    assert!(out.contains("omitting directory"), "got {:?}", out);
}

#[test]
fn mv_removes_source() {
    let mut s = shell_with_files(serde_json::json!({
        "a.txt": "moved"
    }));
    let (_, c1, _) = s.run_line("mv a.txt z.txt");
    assert_eq!(c1, 0);
    let (_, c2, _) = s.run_line("cat a.txt");
    assert_ne!(c2, 0);
    let (out, c3, _) = s.run_line("cat z.txt");
    assert_eq!(c3, 0);
    assert_eq!(out, "moved");
}

#[test]
fn rm_removes_file() {
    let mut s = shell_with_files(serde_json::json!({
        "gone.txt": "bye"
    }));
    let (_, c1, _) = s.run_line("rm gone.txt");
    assert_eq!(c1, 0);
    let (_, c2, _) = s.run_line("cat gone.txt");
    assert_ne!(c2, 0);
}

#[test]
fn rm_directory_requires_recursive() {
    let mut s = shell();
    let (_, c1, _) = s.run_line("mkdir mydir");
    assert_eq!(c1, 0);
    let (out, code, _) = s.run_line("rm mydir");
    assert_eq!(code, 1);
    assert!(out.contains("Is a directory"), "got {:?}", out);
}

#[test]
fn rm_force_missing_succeeds() {
    let mut s = shell();
    let (_, code, _) = s.run_line("rm -f does_not_exist.txt");
    assert_eq!(code, 0);
}

#[test]
fn rm_rf_removes_directory_tree() {
    let mut s = shell();
    assert_eq!(s.run_line("mkdir -p tree/sub").1, 0);
    assert_eq!(s.run_line("touch tree/sub/f.txt").1, 0);
    assert_eq!(s.run_line("rm -rf tree").1, 0);
    let (_, code, _) = s.run_line("ls tree");
    assert_ne!(code, 0);
}

#[test]
fn find_name_filter() {
    let mut s = shell_with_files(serde_json::json!({
        "sub/a.rs": "",
        "sub/b.txt": ""
    }));
    let (out, code, _) = s.run_line("find sub -name '*.rs'");
    assert_eq!(code, 0);
    assert!(out.contains("a.rs"), "got {:?}", out);
    assert!(!out.contains("b.txt"));
}

#[test]
fn find_missing_root() {
    let mut s = shell();
    let (out, code, _) = s.run_line("find nowhere -name '*'");
    assert_eq!(code, 1);
    assert!(out.contains("No such file"), "got {:?}", out);
}

#[test]
fn ls_missing_path() {
    let mut s = shell();
    let (out, code, _) = s.run_line("ls ghost_dir");
    assert_eq!(code, 2);
    assert!(
        out.contains("cannot access") || out.contains("No such file"),
        "got {:?}",
        out
    );
}

#[test]
fn glob_expansion_matches_files() {
    let mut s = shell_with_files(serde_json::json!({
        "a.txt": "aa",
        "b.txt": "bb",
        "c.rs": "cc"
    }));
    let (out, code, _) = s.run_line("echo *.txt");
    assert_eq!(code, 0);
    assert!(out.contains("a.txt"), "got {:?}", out);
    assert!(out.contains("b.txt"), "got {:?}", out);
    assert!(!out.contains("c.rs"), "got {:?}", out);
}

#[test]
fn tee_writes_file_and_passes_through() {
    let mut s = shell();
    let (out, code, _) = s.run_line("echo hello | tee copy.txt");
    assert_eq!(code, 0);
    assert_eq!(out, "hello");
    let (content, c2, _) = s.run_line("cat copy.txt");
    assert_eq!(c2, 0);
    assert_eq!(content, "hello");
}

#[test]
fn tee_append_mode() {
    let mut s = shell_with_files(serde_json::json!({
        "log.txt": "line1"
    }));
    let (_, code, _) = s.run_line("echo line2 | tee -a log.txt");
    assert_eq!(code, 0);
    let (out, _, _) = s.run_line("cat log.txt");
    assert!(out.contains("line1"), "got {:?}", out);
    assert!(out.contains("line2"), "got {:?}", out);
}

#[test]
fn tee_rejects_read_only_target() {
    let mut s = shell_with_files(serde_json::json!({
        "ro.txt": { "content": "orig", "mode": 444 }
    }));
    let (out, code, _) = s.run_line("echo x | tee ro.txt");
    assert_eq!(code, 1);
    assert!(out.contains("Permission denied"), "got {:?}", out);
}

#[test]
fn ln_s_creates_symlink_and_cat_reads_through() {
    let mut s = shell_with_files(serde_json::json!({
        "real.txt": "hello"
    }));
    let (_, c1, _) = s.run_line("ln -s real.txt link.txt");
    assert_eq!(c1, 0);
    let (out, c2, _) = s.run_line("cat link.txt");
    assert_eq!(c2, 0);
    assert_eq!(out, "hello");
}

#[test]
fn readlink_shows_target() {
    let mut s = shell_with_files(serde_json::json!({
        "real.txt": "data"
    }));
    s.run_line("ln -s real.txt link.txt");
    let (out, code, _) = s.run_line("readlink link.txt");
    assert_eq!(code, 0);
    assert_eq!(out, "real.txt");
}

#[test]
fn ln_hard_link_missing_source_fails() {
    let mut s = shell();
    let (out, code, _) = s.run_line("ln a b");
    assert_eq!(code, 1);
    assert!(out.contains("ln:"), "got {:?}", out);
}

#[test]
fn ln_s_existing_target_fails() {
    let mut s = shell_with_files(serde_json::json!({
        "a.txt": "a",
        "b.txt": "b"
    }));
    let (out, code, _) = s.run_line("ln -s a.txt b.txt");
    assert_eq!(code, 1);
    assert!(out.contains("File exists"), "got {:?}", out);
}

#[test]
fn rmdir_removes_empty_dir() {
    let mut s = shell();
    s.run_line("mkdir emptydir");
    let (_, code, _) = s.run_line("rmdir emptydir");
    assert_eq!(code, 0);
    let (_, ls_code, _) = s.run_line("ls emptydir");
    assert_ne!(ls_code, 0);
}

#[test]
fn rmdir_fails_on_nonempty_dir() {
    let mut s = shell_with_files(serde_json::json!({ "filled/a.txt": "x" }));
    let (out, code, _) = s.run_line("rmdir filled");
    assert_ne!(code, 0);
    assert!(
        out.contains("not empty") || out.contains("Directory"),
        "got {:?}",
        out
    );
}

#[test]
fn rmdir_fails_on_missing_dir() {
    let mut s = shell();
    let (_, code, _) = s.run_line("rmdir nosuchdir");
    assert_ne!(code, 0);
}

#[test]
fn mktemp_creates_file() {
    let mut s = shell();
    let (out, code, _) = s.run_line("mktemp");
    assert_eq!(code, 0);
    assert!(out.starts_with("/tmp/tmp."), "got {:?}", out);
    // file should exist
    let (_, cat_code, _) = s.run_line(&format!("cat {}", out.trim()));
    assert_eq!(cat_code, 0);
}

#[test]
fn mktemp_d_creates_directory() {
    let mut s = shell();
    let (out, code, _) = s.run_line("mktemp -d");
    assert_eq!(code, 0);
    assert!(out.starts_with("/tmp/tmp."), "got {:?}", out);
    // directory should exist — cd into it
    let (_, cd_code, _) = s.run_line(&format!("cd {}", out.trim()));
    assert_eq!(cd_code, 0);
}

#[test]
fn mktemp_names_are_unique() {
    let mut s = shell();
    let (out1, _, _) = s.run_line("mktemp");
    let (out2, _, _) = s.run_line("mktemp");
    assert_ne!(out1.trim(), out2.trim(), "mktemp names should differ");
}

#[test]
fn chown_changes_file_owner() {
    let mut s = shell_with_files(serde_json::json!({ "f.txt": "data" }));
    let (_, code, _) = s.run_line("sudo chown 500 f.txt");
    assert_eq!(code, 0);
    let (stat_out, _, _) = s.run_line("stat f.txt");
    assert!(
        stat_out.contains("500"),
        "expected uid 500 in stat: {:?}",
        stat_out
    );
}

#[test]
fn chown_user_colon_group() {
    let mut s = shell_with_files(serde_json::json!({ "g.txt": "data" }));
    let (_, code, _) = s.run_line("sudo chown 42:99 g.txt");
    assert_eq!(code, 0);
    let (stat_out, _, _) = s.run_line("stat g.txt");
    assert!(
        stat_out.contains("42"),
        "expected uid 42 in stat: {:?}",
        stat_out
    );
    assert!(
        stat_out.contains("99"),
        "expected gid 99 in stat: {:?}",
        stat_out
    );
}

#[test]
fn chgrp_changes_file_group() {
    let mut s = shell_with_files(serde_json::json!({ "h.txt": "data" }));
    let (_, code, _) = s.run_line("sudo chgrp 777 h.txt");
    assert_eq!(code, 0);
    let (stat_out, _, _) = s.run_line("stat h.txt");
    assert!(
        stat_out.contains("777"),
        "expected gid 777 in stat: {:?}",
        stat_out
    );
}

// ln: symlink to directory
#[test]
fn ln_s_symlink_to_dir_and_ls() {
    let mut s = shell();
    s.run_line("mkdir mydir");
    s.run_line("touch mydir/a.txt");
    let (_, c, _) = s.run_line("ln -s mydir link");
    assert_eq!(c, 0);
    let (out, code, _) = s.run_line("ls link");
    assert_eq!(code, 0);
    assert!(out.contains("a.txt"), "got {:?}", out);
}

// ln: dangling symlink
#[test]
fn ln_s_dangling_symlink() {
    let mut s = shell();
    s.run_line("ln -s nowhere.txt dangling");
    let (out, code, _) = s.run_line("cat dangling");
    assert_ne!(code, 0);
    assert!(out.contains("No such file"), "got {:?}", out);
}

// readlink on non-symlink
#[test]
fn readlink_non_symlink_fails() {
    let mut s = shell_with_files(serde_json::json!({"f.txt": "x"}));
    let (_, code, _) = s.run_line("readlink f.txt");
    assert_ne!(code, 0);
}

// readlink on missing
#[test]
fn readlink_missing_fails() {
    let mut s = shell();
    let (_, code, _) = s.run_line("readlink nope");
    assert_ne!(code, 0);
}

// chown -R recursive
#[test]
fn chown_recursive() {
    let mut s = shell();
    s.run_line("mkdir -p d/sub");
    s.run_line("touch d/sub/f.txt");
    let (_, code, _) = s.run_line("sudo chown -R 2000:2000 d");
    assert_eq!(code, 0);
    let (out, _, _) = s.run_line("stat d/sub/f.txt");
    assert!(out.contains("2000"), "got {:?}", out);
}

// chown missing file
#[test]
fn chown_missing_fails() {
    let mut s = shell();
    let (_, code, _) = s.run_line("chown 1000 nope.txt");
    assert_ne!(code, 0);
}

#[test]
fn ln_s_intermediate_symlink() {
    // /link -> /dir, then cat /link/file.txt should work
    let mut s = shell_with_files(serde_json::json!({"sub/file.txt": "content"}));
    s.run_line("ln -s /home/u/sub link");
    let (out, code, _) = s.run_line("cat link/file.txt");
    assert_eq!(code, 0);
    assert_eq!(out, "content");
}

#[test]
fn chown_numeric_ids() {
    let mut s = shell_with_files(serde_json::json!({"f.txt": "x"}));
    let (_, code, _) = s.run_line("sudo chown 2000:3000 f.txt");
    assert_eq!(code, 0);
    let (out, _, _) = s.run_line("stat f.txt");
    assert!(out.contains("2000"), "got {:?}", out);
    assert!(out.contains("3000"), "got {:?}", out);
}

#[test]
fn mv_into_directory() {
    let mut s = shell_with_files(serde_json::json!({"src.txt": "data"}));
    s.run_line("mkdir dest");
    let (_, code, _) = s.run_line("mv src.txt dest");
    assert_eq!(code, 0);
    let (out, c2, _) = s.run_line("cat dest/src.txt");
    assert_eq!(c2, 0);
    assert_eq!(out, "data");
}

#[test]
fn tee_multiple_files() {
    let mut s = shell();
    let (out, code, _) = s.run_line("echo hello | tee a.txt b.txt");
    assert_eq!(code, 0);
    assert_eq!(out, "hello");
    let (a, _, _) = s.run_line("cat a.txt");
    let (b, _, _) = s.run_line("cat b.txt");
    assert_eq!(a, "hello");
    assert_eq!(b, "hello");
}

// -- Hard link tests --------------------------------------------------------

#[test]
fn ln_hard_link_creates_shared_file() {
    let mut s = shell();
    s.run_line("echo hello > orig.txt");
    let (out, code, _) = s.run_line("ln orig.txt link.txt");
    assert_eq!(code, 0);
    assert_eq!(out, "");
    let (content, _, _) = s.run_line("cat link.txt");
    assert_eq!(content, "hello");
}

#[test]
fn ln_hard_link_shares_content_via_append() {
    let mut s = shell();
    s.run_line("echo hello > orig.txt");
    s.run_line("ln orig.txt link.txt");
    s.run_line("echo world >> orig.txt");
    let (content, _, _) = s.run_line("cat link.txt");
    assert!(content.contains("hello"), "got: {content}");
    assert!(content.contains("world"), "got: {content}");
}

#[test]
fn ln_hard_link_to_directory_fails() {
    let mut s = shell();
    s.run_line("mkdir mydir");
    let (out, code, _) = s.run_line("ln mydir link");
    assert_ne!(code, 0);
    assert!(
        out.contains("Permission denied") || out.contains("denied"),
        "got: {out}"
    );
}

#[test]
fn ln_hard_link_to_missing_source_fails() {
    let mut s = shell();
    let (_, code, _) = s.run_line("ln nope.txt link.txt");
    assert_ne!(code, 0);
}

// -- ls -l enriched output --------------------------------------------------

#[test]
fn ls_long_shows_size_and_nlink() {
    let mut s = shell();
    s.run_line("echo hello > f.txt");
    let (out, code, _) = s.run_line("ls -l");
    assert_eq!(code, 0);
    // Should contain nlink count and file size
    assert!(out.contains("1"), "expected nlink in output: {out}");
    assert!(
        out.contains("5") || out.contains("6"),
        "expected file size in output: {out}"
    );
}

#[test]
fn ls_long_shows_symlink_type() {
    let mut s = shell();
    s.run_line("echo x > target.txt");
    s.run_line("ln -s target.txt link.txt");
    let (out, code, _) = s.run_line("ls -l");
    assert_eq!(code, 0);
    assert!(
        out.contains("l"),
        "expected symlink type 'l' in output: {out}"
    );
}

// -- stat enriched output ---------------------------------------------------

#[test]
fn stat_shows_inode_and_links() {
    let mut s = shell();
    s.run_line("echo data > f.txt");
    let (out, code, _) = s.run_line("stat f.txt");
    assert_eq!(code, 0);
    assert!(
        out.contains("Inode:"),
        "expected Inode in stat output: {out}"
    );
    assert!(
        out.contains("Links:"),
        "expected Links in stat output: {out}"
    );
}

#[test]
fn stat_hard_link_shows_nlink_2() {
    let mut s = shell();
    s.run_line("echo data > a.txt");
    s.run_line("ln a.txt b.txt");
    let (out, _, _) = s.run_line("stat a.txt");
    assert!(out.contains("Links: 2"), "expected nlink=2 in stat: {out}");
}

// -- rmdir uses is_empty_dir ------------------------------------------------

#[test]
fn rmdir_fails_on_nonempty_uses_is_empty_dir() {
    let mut s = shell();
    s.run_line("mkdir -p d/sub");
    let (out, code, _) = s.run_line("rmdir d");
    assert_ne!(code, 0);
    assert!(
        out.contains("not empty") || out.contains("Directory not empty"),
        "got: {out}"
    );
}

// -- realpath resolves symlinks ---------------------------------------------

#[test]
fn realpath_resolves_symlink() {
    let mut s = shell();
    s.run_line("mkdir -p real/dir");
    s.run_line("echo x > real/dir/file.txt");
    s.run_line("ln -s real/dir link");
    let (out, code, _) = s.run_line("realpath link/file.txt");
    assert_eq!(code, 0);
    assert!(out.trim().ends_with("/real/dir/file.txt"), "got: {out}");
}

// -- find uses walk_prefix --------------------------------------------------

// --- H1: cat joins files with \n — should concatenate raw ---
#[test]
fn cat_multiple_files_no_extra_newline() {
    let mut s = shell_with_files(serde_json::json!({"a.txt": "hello\n", "b.txt": "world\n"}));
    let (out, code, _) = s.run_line("cat a.txt b.txt");
    assert_eq!(code, 0);
    assert_eq!(out, "hello\nworld\n"); // no double newline between files
}

// --- H3: ls ignores paths after first ---
#[test]
fn ls_multiple_paths() {
    let mut s = shell();
    s.run_line("mkdir d1 d2");
    s.run_line("touch d1/a d2/b");
    let (out, code, _) = s.run_line("ls d1 d2");
    assert_eq!(code, 0);
    assert!(out.contains("a"), "should list d1 contents: {out}");
    assert!(out.contains("b"), "should list d2 contents: {out}");
}

// --- H4: rm -rf swallows errors ---
#[test]
fn rm_rf_nonexistent_with_force_succeeds() {
    let mut s = shell();
    let (_, code, _) = s.run_line("rm -rf nonexistent_dir");
    assert_eq!(code, 0);
}

// --- H5: sed -i leaks content to stdout ---
#[test]
fn sed_i_no_stdout() {
    let mut s = shell_with_files(serde_json::json!({"f.txt": "hello world"}));
    let (out, code, _) = s.run_line("sed -i 's/hello/bye/' f.txt");
    assert_eq!(code, 0);
    assert_eq!(out, "", "sed -i should produce no stdout");
    let (content, _, _) = s.run_line("cat f.txt");
    assert_eq!(content, "bye world");
}

// --- H6: mktemp name collision after delete ---
#[test]
fn mktemp_no_collision_after_delete() {
    let mut s = shell();
    let (path1, code1, _) = s.run_line("mktemp");
    assert_eq!(code1, 0);
    s.run_line(&format!("rm {}", path1.trim()));
    let (path2, code2, _) = s.run_line("mktemp");
    assert_eq!(code2, 0);
    assert_ne!(path1.trim(), path2.trim(), "should not reuse deleted name");
}

// --- M1: cp multiple sources to directory ---
#[test]
fn cp_multiple_sources_to_dir() {
    let mut s = shell();
    s.run_line("echo a > a.txt");
    s.run_line("echo b > b.txt");
    s.run_line("mkdir dest");
    let (_, code, _) = s.run_line("cp a.txt b.txt dest");
    assert_eq!(code, 0);
    let (a, _, _) = s.run_line("cat dest/a.txt");
    let (b, _, _) = s.run_line("cat dest/b.txt");
    assert_eq!(a, "a");
    assert_eq!(b, "b");
}

// --- M2: chmod symbolic modes ---
#[test]
fn chmod_symbolic_plus_x() {
    let mut s = shell();
    s.run_line("touch f.txt");
    let (_, code, _) = s.run_line("chmod +x f.txt");
    assert_eq!(code, 0);
    let (out, _, _) = s.run_line("stat f.txt");
    assert!(out.contains("x"), "should have execute bit: {out}");
}

// --- M4: tee -a spurious newline ---
#[test]
fn tee_append_no_extra_newline() {
    let mut s = shell();
    s.run_line("echo -n hello | tee f.txt");
    s.run_line("echo -n world | tee -a f.txt");
    let (out, _, _) = s.run_line("cat f.txt");
    assert_eq!(out, "helloworld"); // no spurious newline between
}

// --- M10: find path after flags ---
#[test]
fn find_path_first_arg() {
    let mut s = shell();
    s.run_line("mkdir -p d/sub");
    s.run_line("touch d/sub/f.txt");
    let (out, code, _) = s.run_line("find d -type f");
    assert_eq!(code, 0);
    assert!(out.contains("f.txt"), "got: {out}");
}

// --- L1: ls -l symlink-to-dir shows 'l' not 'd' ---
#[test]
fn ls_long_symlink_to_dir_shows_l() {
    let mut s = shell();
    s.run_line("mkdir realdir");
    s.run_line("ln -s realdir linkdir");
    let (out, code, _) = s.run_line("ls -l");
    assert_eq!(code, 0);
    // Find the line for linkdir — it should start with 'l' not 'd'
    let link_line = out.lines().find(|l| l.contains("linkdir"));
    assert!(link_line.is_some(), "linkdir not in ls output: {out}");
    let ll = link_line.unwrap_or("");
    assert!(
        ll.starts_with('l'),
        "symlink line should start with 'l', got: {ll}"
    );
}

#[test]
fn find_in_subtree() {
    let mut s = shell();
    s.run_line("mkdir -p a/b");
    s.run_line("echo x > a/b/f.txt");
    s.run_line("echo y > other.txt");
    let (out, code, _) = s.run_line("find a -name f.txt");
    assert_eq!(code, 0);
    assert!(out.contains("f.txt"), "got: {out}");
    assert!(
        !out.contains("other.txt"),
        "should not find other.txt: {out}"
    );
}
