use super::*;

// ---------------------------------------------------------------------------
// cat
// ---------------------------------------------------------------------------

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
fn cat_multiple_files_no_extra_newline() {
    let mut s = shell_with_files(serde_json::json!({"a.txt": "hello\n", "b.txt": "world\n"}));
    let (out, code, _) = s.run_line("cat a.txt b.txt");
    assert_eq!(code, 0);
    assert_eq!(out, "hello\nworld\n");
}

#[test]
fn cat_dash_reads_stdin() {
    let mut s = shell();
    let (out, code, _) = s.run_line("echo hello | cat -");
    assert_eq!(code, 0);
    assert_eq!(out, "hello\n");
}

#[test]
fn cat_dash_concatenates_stdin_and_file() {
    let mut s = shell_with_files(serde_json::json!({"f.txt": "world\n"}));
    let (out, code, _) = s.run_line("echo hello | cat - f.txt");
    assert_eq!(code, 0);
    assert_eq!(out, "hello\nworld\n");
}

// ---------------------------------------------------------------------------
// chmod
// ---------------------------------------------------------------------------

#[test]
fn chmod_restores_read_then_cat_succeeds() {
    let mut s = shell_with_files(serde_json::json!({
        "sec.txt": { "content": "ok", "mode": 0 }
    }));
    let (_, c1, _) = s.run_line("cat sec.txt");
    assert_eq!(c1, 1);
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
fn chmod_symbolic_plus_x() {
    let mut s = shell();
    s.run_line("touch f.txt");
    let (_, code, _) = s.run_line("chmod +x f.txt");
    assert_eq!(code, 0);
    let (out, _, _) = s.run_line("stat f.txt");
    assert!(out.contains("x"), "should have execute bit: {out}");
}

// ---------------------------------------------------------------------------
// mkdir
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// touch
// ---------------------------------------------------------------------------

#[test]
fn touch_creates_empty_file_then_cat() {
    let mut s = shell();
    let (_, c1, _) = s.run_line("touch empty.txt");
    assert_eq!(c1, 0);
    let (out, c2, _) = s.run_line("cat empty.txt");
    assert_eq!(c2, 0);
    assert_eq!(out, "");
}

// ---------------------------------------------------------------------------
// cp
// ---------------------------------------------------------------------------

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
fn cp_multiple_sources_to_dir() {
    let mut s = shell();
    s.run_line("echo a > a.txt");
    s.run_line("echo b > b.txt");
    s.run_line("mkdir dest");
    let (_, code, _) = s.run_line("cp a.txt b.txt dest");
    assert_eq!(code, 0);
    let (a, _, _) = s.run_line("cat dest/a.txt");
    let (b, _, _) = s.run_line("cat dest/b.txt");
    assert_eq!(a, "a\n");
    assert_eq!(b, "b\n");
}

#[test]
fn cp_r_copies_directory() {
    let mut s = shell();
    s.run_line("mkdir -p src/sub");
    s.run_line("echo hello > src/file.txt");
    s.run_line("echo world > src/sub/deep.txt");
    let (_, code, _) = s.run_line("cp -r src dst");
    assert_eq!(code, 0);
    let (out, _, _) = s.run_line("cat dst/file.txt");
    assert_eq!(out, "hello\n");
    let (out2, _, _) = s.run_line("cat dst/sub/deep.txt");
    assert_eq!(out2, "world\n");
}

#[test]
fn cp_r_on_file_works_like_regular_cp() {
    let mut s = shell();
    s.run_line("echo data > a.txt");
    let (_, code, _) = s.run_line("cp -r a.txt b.txt");
    assert_eq!(code, 0);
    let (out, _, _) = s.run_line("cat b.txt");
    assert_eq!(out, "data\n");
}

#[test]
fn cp_without_r_rejects_directory() {
    let mut s = shell();
    s.run_line("mkdir d");
    let (out, code, _) = s.run_line("cp d d2");
    assert_eq!(code, 1);
    assert!(out.contains("omitting directory"), "got: {out}");
}

// ---------------------------------------------------------------------------
// mv
// ---------------------------------------------------------------------------

#[test]
fn mv_removes_source() {
    let mut s = shell_with_files(serde_json::json!({
        "a.txt": "moved"
    }));
    let (_, c1, _) = s.run_line("mv a.txt z.txt");
    assert_eq!(c1, 0);
    let (_, c2, _) = s.run_line("cat a.txt");
    assert_eq!(c2, 1);
    let (out, c3, _) = s.run_line("cat z.txt");
    assert_eq!(c3, 0);
    assert_eq!(out, "moved");
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
fn mv_multiple_sources_to_dir() {
    let mut s = shell_with_files(serde_json::json!({"a.txt": "aaa", "b.txt": "bbb"}));
    s.run_line("mkdir dest");
    let (_, code, _) = s.run_line("mv a.txt b.txt dest");
    assert_eq!(code, 0);
    let (a, ca, _) = s.run_line("cat dest/a.txt");
    assert_eq!(ca, 0);
    assert_eq!(a, "aaa");
    let (b, cb, _) = s.run_line("cat dest/b.txt");
    assert_eq!(cb, 0);
    assert_eq!(b, "bbb");
    // sources removed
    let (_, gone_a, _) = s.run_line("cat a.txt");
    assert_eq!(gone_a, 1);
    let (_, gone_b, _) = s.run_line("cat b.txt");
    assert_eq!(gone_b, 1);
}

// ---------------------------------------------------------------------------
// rm / rmdir
// ---------------------------------------------------------------------------

#[test]
fn rm_removes_file() {
    let mut s = shell_with_files(serde_json::json!({
        "gone.txt": "bye"
    }));
    let (_, c1, _) = s.run_line("rm gone.txt");
    assert_eq!(c1, 0);
    let (_, c2, _) = s.run_line("cat gone.txt");
    assert_eq!(c2, 1);
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
    assert_eq!(code, 2);
}

#[test]
fn rm_rf_nonexistent_with_force_succeeds() {
    let mut s = shell();
    let (_, code, _) = s.run_line("rm -rf nonexistent_dir");
    assert_eq!(code, 0);
}

#[test]
fn rmdir_removes_empty_dir() {
    let mut s = shell();
    s.run_line("mkdir emptydir");
    let (_, code, _) = s.run_line("rmdir emptydir");
    assert_eq!(code, 0);
    let (_, ls_code, _) = s.run_line("ls emptydir");
    assert_eq!(ls_code, 2);
}

#[test]
fn rmdir_fails_on_nonempty_dir() {
    let mut s = shell_with_files(serde_json::json!({ "filled/a.txt": "x" }));
    let (out, code, _) = s.run_line("rmdir filled");
    assert_eq!(code, 1);
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
    assert_eq!(code, 1);
}

#[test]
fn rmdir_fails_on_nonempty_uses_is_empty_dir() {
    let mut s = shell();
    s.run_line("mkdir -p d/sub");
    let (out, code, _) = s.run_line("rmdir d");
    assert_eq!(code, 1);
    assert!(
        out.contains("not empty") || out.contains("Directory not empty"),
        "got: {out}"
    );
}

// ---------------------------------------------------------------------------
// ls
// ---------------------------------------------------------------------------

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
fn ls_multiple_paths() {
    let mut s = shell();
    s.run_line("mkdir d1 d2");
    s.run_line("touch d1/a d2/b");
    let (out, code, _) = s.run_line("ls d1 d2");
    assert_eq!(code, 0);
    assert!(out.contains("a"), "should list d1 contents: {out}");
    assert!(out.contains("b"), "should list d2 contents: {out}");
}

#[test]
fn ls_long_shows_size_and_nlink() {
    let mut s = shell();
    s.run_line("echo hello > f.txt");
    let (out, code, _) = s.run_line("ls -l");
    assert_eq!(code, 0);
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

#[test]
fn ls_long_symlink_to_dir_shows_l() {
    let mut s = shell();
    s.run_line("mkdir realdir");
    s.run_line("ln -s realdir linkdir");
    let (out, code, _) = s.run_line("ls -l");
    assert_eq!(code, 0);
    let link_line = out.lines().find(|l| l.contains("linkdir"));
    assert!(link_line.is_some(), "linkdir not in ls output: {out}");
    let ll = link_line.unwrap_or("");
    assert!(
        ll.starts_with('l'),
        "symlink line should start with 'l', got: {ll}"
    );
}

// ---------------------------------------------------------------------------
// find
// ---------------------------------------------------------------------------

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
fn find_path_first_arg() {
    let mut s = shell();
    s.run_line("mkdir -p d/sub");
    s.run_line("touch d/sub/f.txt");
    let (out, code, _) = s.run_line("find d -type f");
    assert_eq!(code, 0);
    assert!(out.contains("f.txt"), "got: {out}");
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

// ---------------------------------------------------------------------------
// tee
// ---------------------------------------------------------------------------

#[test]
fn tee_writes_file_and_passes_through() {
    let mut s = shell();
    let (out, code, _) = s.run_line("echo hello | tee copy.txt");
    assert_eq!(code, 0);
    assert_eq!(out, "hello\n");
    let (content, c2, _) = s.run_line("cat copy.txt");
    assert_eq!(c2, 0);
    assert_eq!(content, "hello\n");
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
fn tee_multiple_files() {
    let mut s = shell();
    let (out, code, _) = s.run_line("echo hello | tee a.txt b.txt");
    assert_eq!(code, 0);
    assert_eq!(out, "hello\n");
    let (a, _, _) = s.run_line("cat a.txt");
    let (b, _, _) = s.run_line("cat b.txt");
    assert_eq!(a, "hello\n");
    assert_eq!(b, "hello\n");
}

#[test]
fn tee_append_no_extra_newline() {
    let mut s = shell();
    s.run_line("echo -n hello | tee f.txt");
    s.run_line("echo -n world | tee -a f.txt");
    let (out, _, _) = s.run_line("cat f.txt");
    assert_eq!(out, "helloworld");
}

// ---------------------------------------------------------------------------
// ln (symlinks and hard links)
// ---------------------------------------------------------------------------

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
    assert_eq!(out, "real.txt\n");
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

#[test]
fn ln_s_dangling_symlink() {
    let mut s = shell();
    s.run_line("ln -s nowhere.txt dangling");
    let (out, code, _) = s.run_line("cat dangling");
    assert_eq!(code, 1);
    assert!(out.contains("No such file"), "got {:?}", out);
}

#[test]
fn readlink_non_symlink_fails() {
    let mut s = shell_with_files(serde_json::json!({"f.txt": "x"}));
    let (_, code, _) = s.run_line("readlink f.txt");
    assert_eq!(code, 1);
}

#[test]
fn readlink_missing_fails() {
    let mut s = shell();
    let (_, code, _) = s.run_line("readlink nope");
    assert_eq!(code, 1);
}

#[test]
fn ln_s_intermediate_symlink() {
    let mut s = shell_with_files(serde_json::json!({"sub/file.txt": "content"}));
    s.run_line("ln -s /home/u/sub link");
    let (out, code, _) = s.run_line("cat link/file.txt");
    assert_eq!(code, 0);
    assert_eq!(out, "content");
}

#[test]
fn ln_hard_link_creates_shared_file() {
    let mut s = shell();
    s.run_line("echo hello > orig.txt");
    let (out, code, _) = s.run_line("ln orig.txt link.txt");
    assert_eq!(code, 0);
    assert_eq!(out, "");
    let (content, _, _) = s.run_line("cat link.txt");
    assert_eq!(content, "hello\n");
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
    assert_eq!(code, 1);
    assert!(
        out.contains("Permission denied") || out.contains("denied"),
        "got: {out}"
    );
}

#[test]
fn ln_hard_link_to_missing_source_fails() {
    let mut s = shell();
    let (_, code, _) = s.run_line("ln nope.txt link.txt");
    assert_eq!(code, 1);
}

// ---------------------------------------------------------------------------
// mktemp
// ---------------------------------------------------------------------------

#[test]
fn mktemp_creates_file() {
    let mut s = shell();
    let (out, code, _) = s.run_line("mktemp");
    assert_eq!(code, 0);
    assert!(out.starts_with("/tmp/tmp."), "got {:?}", out);
    let (_, cat_code, _) = s.run_line(&format!("cat {}", out.trim()));
    assert_eq!(cat_code, 0);
}

#[test]
fn mktemp_d_creates_directory() {
    let mut s = shell();
    let (out, code, _) = s.run_line("mktemp -d");
    assert_eq!(code, 0);
    assert!(out.starts_with("/tmp/tmp."), "got {:?}", out);
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
fn mktemp_no_collision_after_delete() {
    let mut s = shell();
    let (path1, code1, _) = s.run_line("mktemp");
    assert_eq!(code1, 0);
    s.run_line(&format!("rm {}", path1.trim()));
    let (path2, code2, _) = s.run_line("mktemp");
    assert_eq!(code2, 0);
    assert_ne!(path1.trim(), path2.trim(), "should not reuse deleted name");
}

// ---------------------------------------------------------------------------
// chown / chgrp
// ---------------------------------------------------------------------------

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

#[test]
fn chown_missing_fails() {
    let mut s = shell();
    let (_, code, _) = s.run_line("chown 1000 nope.txt");
    assert_eq!(code, 1);
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

// ---------------------------------------------------------------------------
// realpath
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// sed -i (filesystem side-effect)
// ---------------------------------------------------------------------------

#[test]
fn sed_i_no_stdout() {
    let mut s = shell_with_files(serde_json::json!({"f.txt": "hello world"}));
    let (out, code, _) = s.run_line("sed -i 's/hello/bye/' f.txt");
    assert_eq!(code, 0);
    assert_eq!(out, "", "sed -i should produce no stdout");
    let (content, _, _) = s.run_line("cat f.txt");
    assert_eq!(content, "bye world");
}

// ---------------------------------------------------------------------------
// chmod -R (recursive)
// ---------------------------------------------------------------------------

#[test]
fn chmod_recursive_changes_all_children() {
    let mut s = shell();
    s.run_line("mkdir -p d/sub");
    s.run_line("touch d/f.txt");
    s.run_line("touch d/sub/g.txt");
    let (_, code, _) = s.run_line("chmod -R 755 d");
    assert_eq!(code, 0);
    let (stat_f, _, _) = s.run_line("stat d/f.txt");
    assert!(
        stat_f.contains("0755"),
        "expected 0755 in stat d/f.txt: {stat_f}"
    );
    let (stat_g, _, _) = s.run_line("stat d/sub/g.txt");
    assert!(
        stat_g.contains("0755"),
        "expected 0755 in stat d/sub/g.txt: {stat_g}"
    );
    let (stat_d, _, _) = s.run_line("stat d");
    assert!(stat_d.contains("0755"), "expected 0755 in stat d: {stat_d}");
}

// ---------------------------------------------------------------------------
// ls -R (recursive)
// ---------------------------------------------------------------------------

#[test]
fn ls_recursive_shows_subdirectory_contents() {
    let mut s = shell();
    s.run_line("mkdir -p d/sub");
    s.run_line("touch d/a.txt");
    s.run_line("touch d/sub/b.txt");
    let (out, code, _) = s.run_line("ls -R d");
    assert_eq!(code, 0);
    assert!(out.contains("a.txt"), "should list d contents: {out}");
    assert!(out.contains("b.txt"), "should list d/sub contents: {out}");
    assert!(
        out.contains("d/sub:"),
        "should have subdirectory header: {out}"
    );
}

// ---------------------------------------------------------------------------
// ls -1 (one per line)
// ---------------------------------------------------------------------------

#[test]
fn ls_one_per_line() {
    let mut s = shell();
    s.run_line("touch a.txt b.txt c.txt");
    let (out, code, _) = s.run_line("ls -1");
    assert_eq!(code, 0);
    // With -1, entries should be separated by \n, not double space
    assert!(
        out.contains("a.txt\n"),
        "should have newline-separated entries: {out}"
    );
    assert!(
        !out.contains("a.txt  "),
        "should not have double-space separation: {out}"
    );
}

// ---------------------------------------------------------------------------
// ls -l date column
// ---------------------------------------------------------------------------

#[test]
fn ls_long_shows_date_column() {
    let mut s = shell();
    s.run_line("echo hello > f.txt");
    let (out, code, _) = s.run_line("ls -l");
    assert_eq!(code, 0);
    // Date column should contain a month abbreviation
    let has_month = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ]
    .iter()
    .any(|m| out.contains(m));
    assert!(has_month, "expected month in ls -l output: {out}");
    // Should also contain a colon from the time (HH:MM)
    assert!(
        out.contains(':'),
        "expected time with colon in ls -l output: {out}"
    );
}

// ---------------------------------------------------------------------------
// find -maxdepth N
// ---------------------------------------------------------------------------

#[test]
fn find_maxdepth_limits_recursion() {
    let mut s = shell();
    s.run_line("mkdir -p a/b/c");
    s.run_line("touch a/top.txt");
    s.run_line("touch a/b/mid.txt");
    s.run_line("touch a/b/c/deep.txt");
    let (out, code, _) = s.run_line("find a -maxdepth 1 -name '*.txt'");
    assert_eq!(code, 0);
    assert!(out.contains("top.txt"), "should find top.txt: {out}");
    assert!(!out.contains("mid.txt"), "should not descend into b: {out}");
    assert!(
        !out.contains("deep.txt"),
        "should not descend into b/c: {out}"
    );
}

// ---------------------------------------------------------------------------
// find -exec cmd {} \;
// ---------------------------------------------------------------------------

#[test]
fn find_exec_runs_command_for_each_match() {
    let mut s = shell();
    s.run_line("mkdir d");
    s.run_line("echo aaa > d/a.txt");
    s.run_line("echo bbb > d/b.txt");
    s.run_line("touch d/c.dat");
    let (out, code, _) = s.run_line("find d -name '*.txt' -exec cat {} ;");
    assert_eq!(code, 0);
    assert!(out.contains("aaa"), "should cat a.txt: {out}");
    assert!(out.contains("bbb"), "should cat b.txt: {out}");
    assert!(!out.contains("c.dat"), "should not process c.dat");
}

// ---------------------------------------------------------------------------
// ls -h (human-readable sizes in long format)
// ---------------------------------------------------------------------------

#[test]
fn ls_lh_shows_human_readable_size() {
    let mut s = shell();
    // Create a file with known content to verify human-readable formatting
    // Write 2048 bytes to get a "2.0K" display
    s.run_line("touch big.txt");
    // Write content: 2048 'x' chars
    let content = "x".repeat(2048);
    s.fs.write("/home/u/big.txt", content.as_bytes()).unwrap();
    let (out, code, _) = s.run_line("ls -lh");
    assert_eq!(code, 0);
    assert!(out.contains("big.txt"), "should list the file: {out}");
    // In human-readable mode, 2048 bytes should show as "2.0K"
    assert!(
        out.contains("K"),
        "should show human-readable size with K suffix: {out}"
    );
}

// ---------------------------------------------------------------------------
// ls -t (sort by modification time)
// ---------------------------------------------------------------------------

#[test]
fn ls_t_sorts_by_mtime_descending() {
    let mut s = shell();
    // Create files with increasing mtime by using sleep between them
    s.run_line("echo a > first.txt");
    s.run_line("sleep 1");
    s.run_line("echo b > second.txt");
    s.run_line("sleep 1");
    s.run_line("echo c > third.txt");
    let (out, code, _) = s.run_line("ls -t");
    assert_eq!(code, 0);
    // Most recently modified should come first
    let lines: Vec<&str> = out.split_whitespace().collect();
    let pos_third = lines.iter().position(|l| l.contains("third.txt"));
    let pos_first = lines.iter().position(|l| l.contains("first.txt"));
    assert!(
        pos_third.is_some() && pos_first.is_some(),
        "should list both files: {out}"
    );
    assert!(
        pos_third.unwrap() < pos_first.unwrap(),
        "third.txt (newest) should come before first.txt (oldest): {out}"
    );
}

// ---------------------------------------------------------------------------
// ls -a shows . and ..
// ---------------------------------------------------------------------------

#[test]
fn ls_a_shows_dot_and_dotdot() {
    let mut s = shell();
    s.run_line("mkdir sub");
    s.run_line("touch sub/visible.txt");
    let (out, code, _) = s.run_line("ls -a sub");
    assert_eq!(code, 0);
    // Directories are displayed with trailing /, so . becomes ./
    let entries: Vec<&str> = out.split_whitespace().collect();
    assert!(
        entries.contains(&"./") || entries.contains(&"."),
        "ls -a should show '.': {out}"
    );
    assert!(
        entries.contains(&"../") || entries.contains(&".."),
        "ls -a should show '..': {out}"
    );
}

// ---------------------------------------------------------------------------
// cp -n (no-clobber)
// ---------------------------------------------------------------------------

#[test]
fn cp_n_does_not_overwrite_existing() {
    let mut s = shell_with_files(serde_json::json!({
        "src.txt": "new_content",
        "existing.txt": "original"
    }));
    let (_, code, _) = s.run_line("cp -n src.txt existing.txt");
    assert_eq!(code, 0);
    let (out, _, _) = s.run_line("cat existing.txt");
    assert_eq!(out, "original", "cp -n should not overwrite existing file");
}

// ---------------------------------------------------------------------------
// cp -p (preserve permissions)
// ---------------------------------------------------------------------------

#[test]
fn cp_p_preserves_permissions() {
    let mut s = shell();
    s.run_line("echo hello > src.txt");
    s.run_line("chmod 755 src.txt");
    let (_, code, _) = s.run_line("cp -p src.txt dst.txt");
    assert_eq!(code, 0);
    let (out, _, _) = s.run_line("stat -c '%a' dst.txt");
    assert_eq!(
        out.trim(),
        "755",
        "cp -p should preserve permissions: {out}"
    );
}

// ---------------------------------------------------------------------------
// ln -f (force overwrite existing)
// ---------------------------------------------------------------------------

#[test]
fn ln_sf_force_overwrites_existing_symlink() {
    let mut s = shell_with_files(serde_json::json!({
        "target1.txt": "first",
        "target2.txt": "second"
    }));
    s.run_line("ln -s target1.txt mylink");
    let (out1, _, _) = s.run_line("cat mylink");
    assert_eq!(out1, "first");
    // Force overwrite
    let (_, code, _) = s.run_line("ln -sf target2.txt mylink");
    assert_eq!(code, 0);
    let (out2, _, _) = s.run_line("cat mylink");
    assert_eq!(out2, "second", "ln -sf should update the link target");
}

// ---------------------------------------------------------------------------
// find -iname (case-insensitive name match)
// ---------------------------------------------------------------------------

#[test]
fn find_iname_case_insensitive() {
    let mut s = shell();
    s.run_line("mkdir d");
    s.run_line("touch d/file.txt");
    s.run_line("touch d/OTHER.TXT");
    s.run_line("touch d/skip.dat");
    let (out, code, _) = s.run_line("find d -iname '*.TXT'");
    assert_eq!(code, 0);
    assert!(out.contains("file.txt"), "should match lowercase: {out}");
    assert!(out.contains("OTHER.TXT"), "should match uppercase: {out}");
    assert!(
        !out.contains("skip.dat"),
        "should not match non-txt files: {out}"
    );
}

// ---------------------------------------------------------------------------
// find -delete
// ---------------------------------------------------------------------------

#[test]
fn find_delete_removes_matching_files() {
    let mut s = shell();
    s.run_line("mkdir d");
    s.run_line("touch d/a.tmp");
    s.run_line("touch d/b.tmp");
    s.run_line("touch d/keep.txt");
    let (_, code, _) = s.run_line("find d -name '*.tmp' -delete");
    assert_eq!(code, 0);
    let (_, c1, _) = s.run_line("test -f d/a.tmp");
    assert_eq!(c1, 1, "a.tmp should be deleted");
    let (_, c2, _) = s.run_line("test -f d/b.tmp");
    assert_eq!(c2, 1, "b.tmp should be deleted");
    let (_, c3, _) = s.run_line("test -f d/keep.txt");
    assert_eq!(c3, 0, "keep.txt should still exist");
}

// ---------------------------------------------------------------------------
// find -path PATTERN
// ---------------------------------------------------------------------------

#[test]
fn find_path_matches_full_path() {
    let mut s = shell();
    s.run_line("mkdir -p d/sub");
    s.run_line("touch d/sub/file.txt");
    s.run_line("touch d/other.txt");
    let (out, code, _) = s.run_line("find d -path '*/sub/*.txt'");
    assert_eq!(code, 0);
    assert!(out.contains("sub/file.txt"), "should match sub path: {out}");
    assert!(
        !out.contains("other.txt"),
        "should not match other.txt: {out}"
    );
}
