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
fn ln_without_s_fails() {
    let mut s = shell();
    let (out, code, _) = s.run_line("ln a b");
    assert_eq!(code, 1);
    assert!(out.contains("hard links"), "got {:?}", out);
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
