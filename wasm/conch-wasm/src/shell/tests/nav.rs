use super::*;

/// Helper: create a shell with a specific VFS date.
fn shell_with_date_nav(date: &str) -> Shell {
    let v = serde_json::json!({
        "user": "u",
        "system": {
            "hostname": "h",
            "users": [{"name": "u", "home": "/home/u"}],
            "files": {},
        },
        "commands": [],
        "date": date,
    });
    let c: crate::types::Config = serde_json::from_value(v).expect("config parse");
    let mut s = Shell::new(&c);
    s.color = false;
    s
}

// ---------------------------------------------------------------------------
// cd / pwd
// ---------------------------------------------------------------------------

#[test]
fn cd_subdir_then_pwd() {
    let mut s = shell();
    assert_eq!(s.run_line("mkdir deep").1, 0);
    assert_eq!(s.run_line("cd deep").1, 0);
    let (out, code, _) = s.run_line("pwd");
    assert_eq!(code, 0);
    assert!(out.contains("/home/u/deep"), "got {:?}", out);
}

#[test]
fn cd_without_args_returns_home() {
    let mut s = shell();
    assert_eq!(s.run_line("mkdir deep").1, 0);
    assert_eq!(s.run_line("cd deep").1, 0);
    assert_eq!(s.run_line("cd").1, 0);
    let (out, code, _) = s.run_line("pwd");
    assert_eq!(code, 0);
    assert!(
        out.contains("/home/u") && !out.contains("deep"),
        "got {:?}",
        out
    );
}

#[test]
fn cd_fails_when_target_is_file() {
    let mut s = shell_with_files(serde_json::json!({ "notdir.txt": "x" }));
    let (out, code, _) = s.run_line("cd notdir.txt");
    assert_eq!(code, 1);
    assert!(out.contains("not a directory"), "got {:?}", out);
}

// ---------------------------------------------------------------------------
// Tilde expansion
// ---------------------------------------------------------------------------

#[test]
fn tilde_expansion_in_cat() {
    let mut s = shell_with_files(serde_json::json!({
        "note.txt": "from home"
    }));
    let (out, code, _) = s.run_line("cat ~/note.txt");
    assert_eq!(code, 0);
    assert_eq!(out, "from home");
}

// ---------------------------------------------------------------------------
// basename / dirname
// ---------------------------------------------------------------------------

#[test]
fn basename_extracts_filename() {
    let mut s = shell();
    let (out, code, _) = s.run_line("basename /home/u/file.txt");
    assert_eq!(code, 0);
    assert_eq!(out, "file.txt\n");
}

#[test]
fn basename_strips_suffix() {
    let mut s = shell();
    let (out, code, _) = s.run_line("basename /home/u/file.txt .txt");
    assert_eq!(code, 0);
    assert_eq!(out, "file\n");
}

#[test]
fn dirname_extracts_directory() {
    let mut s = shell();
    let (out, code, _) = s.run_line("dirname /home/u/file.txt");
    assert_eq!(code, 0);
    assert_eq!(out, "/home/u\n");
}

#[test]
fn dirname_bare_filename_returns_dot() {
    let mut s = shell();
    let (out, code, _) = s.run_line("dirname file.txt");
    assert_eq!(code, 0);
    assert_eq!(out, ".\n");
}

// ---------------------------------------------------------------------------
// realpath
// ---------------------------------------------------------------------------

#[test]
fn realpath_existing_file() {
    let mut s = shell_with_files(serde_json::json!({ "notes.txt": "hi" }));
    let (out, code, _) = s.run_line("realpath notes.txt");
    assert_eq!(code, 0);
    assert_eq!(out, "/home/u/notes.txt\n");
}

#[test]
fn realpath_missing_file_fails() {
    let mut s = shell();
    let (_, code, _) = s.run_line("realpath ghost.txt");
    assert_eq!(code, 1);
}

#[test]
fn realpath_resolves_relative() {
    let mut s = shell();
    s.run_line("mkdir sub");
    let (out, code, _) = s.run_line("realpath sub");
    assert_eq!(code, 0);
    assert_eq!(out, "/home/u/sub\n");
}

// ---------------------------------------------------------------------------
// cd - (OLDPWD)
// ---------------------------------------------------------------------------

#[test]
fn cd_dash_swaps_to_oldpwd() {
    let mut s = shell();
    s.run_line("mkdir /home/u/a");
    s.run_line("mkdir /home/u/b");
    s.run_line("cd /home/u/a");
    s.run_line("cd /home/u/b");
    let (out, code, _) = s.run_line("cd -");
    assert_eq!(code, 0);
    assert!(
        out.contains("/home/u/a"),
        "cd - should print old dir: {}",
        out
    );
    let (pwd, _, _) = s.run_line("pwd");
    assert_eq!(pwd, "/home/u/a\n");
}

#[test]
fn cd_dash_fails_without_oldpwd() {
    let mut s = shell();
    s.vars.env.remove("OLDPWD");
    let (out, code, _) = s.run_line("cd -");
    assert_eq!(code, 1);
    assert!(out.contains("OLDPWD not set"), "got: {}", out);
}

// ---------------------------------------------------------------------------
// pushd / popd / dirs
// ---------------------------------------------------------------------------

#[test]
fn pushd_changes_directory() {
    let mut s = shell();
    s.run_line("mkdir /home/u/target");
    let (out, code, _) = s.run_line("pushd /home/u/target");
    assert_eq!(code, 0);
    assert!(out.contains("/home/u/target"), "pushd output: {}", out);
    let (pwd, _, _) = s.run_line("pwd");
    assert_eq!(pwd, "/home/u/target\n");
}

#[test]
fn popd_returns_to_previous() {
    let mut s = shell();
    s.run_line("mkdir /home/u/target");
    s.run_line("pushd /home/u/target");
    let (out, code, _) = s.run_line("popd");
    assert_eq!(code, 0);
    let (pwd, _, _) = s.run_line("pwd");
    assert_eq!(pwd, "/home/u\n");
    assert!(out.contains("/home/u"), "popd output: {}", out);
}

#[test]
fn popd_empty_stack_fails() {
    let mut s = shell();
    let (out, code, _) = s.run_line("popd");
    assert_eq!(code, 1);
    assert!(out.contains("stack empty"), "got: {}", out);
}

#[test]
fn dirs_shows_stack() {
    let mut s = shell();
    s.run_line("mkdir /home/u/a");
    s.run_line("mkdir /home/u/b");
    s.run_line("pushd /home/u/a");
    s.run_line("pushd /home/u/b");
    let (out, code, _) = s.run_line("dirs");
    assert_eq!(code, 0);
    assert!(out.contains("/home/u/b"), "dirs output: {}", out);
    assert!(out.contains("/home/u/a"), "dirs output: {}", out);
}

// ---------------------------------------------------------------------------
// Feature 4: printenv VAR (single variable lookup)
// ---------------------------------------------------------------------------

#[test]
fn printenv_single_var() {
    let mut s = shell();
    let (out, code, _) = s.run_line("printenv HOME");
    assert_eq!(code, 0);
    assert_eq!(out, "/home/u\n");
}

#[test]
fn printenv_nonexistent_exits_1() {
    let mut s = shell();
    let (_, code, _) = s.run_line("printenv NONEXISTENT");
    assert_eq!(code, 1);
}

#[test]
fn printenv_no_args_lists_all() {
    let mut s = shell();
    let (out, code, _) = s.run_line("printenv");
    assert_eq!(code, 0);
    assert!(
        out.contains("HOME="),
        "printenv should list all vars: {}",
        out
    );
}

// ---------------------------------------------------------------------------
// Feature 6: command -V (verbose type info)
// ---------------------------------------------------------------------------

#[test]
fn command_v_upper_builtin() {
    let mut s = shell();
    let (out, code, _) = s.run_line("command -V cd");
    assert_eq!(code, 0);
    assert!(
        out.contains("builtin"),
        "command -V cd should say builtin: {}",
        out
    );
}

#[test]
fn command_v_upper_external() {
    let mut s = shell();
    let (out, code, _) = s.run_line("command -V echo");
    assert_eq!(code, 0);
    // echo is a builtin in conch, so it should say builtin
    assert!(
        out.contains("builtin") || out.contains("/bin/echo"),
        "got: {}",
        out
    );
}

#[test]
fn command_v_upper_function() {
    let mut s = shell();
    s.run_script("myfn() { echo hi; }");
    let (out, code, _) = s.run_line("command -V myfn");
    assert_eq!(code, 0);
    assert!(
        out.contains("function"),
        "command -V myfn should say function: {}",
        out
    );
}

#[test]
fn command_v_upper_not_found() {
    let mut s = shell();
    let (_, code, _) = s.run_line("command -V nosuchcmd");
    assert_eq!(code, 1);
}

// ---------------------------------------------------------------------------
// date command with VFS time
// ---------------------------------------------------------------------------

#[test]
fn date_returns_current_vfs_time_as_string() {
    let mut s = shell_with_date_nav("Sat Apr 12 00:00:00 UTC 2026");
    let (out, code, _) = s.run_line("date");
    assert_eq!(code, 0);
    // Should contain 2026 and UTC
    assert!(out.contains("2026"), "expected 2026 in date output: {out}");
    assert!(out.contains("UTC"), "expected UTC in date output: {out}");
}

#[test]
fn date_format_year() {
    let mut s = shell_with_date_nav("Sat Apr 12 00:00:00 UTC 2026");
    let (out, code, _) = s.run_line("date +%Y");
    assert_eq!(code, 0);
    assert_eq!(out, "2026\n");
}

#[test]
fn date_plus_s_returns_epoch_seconds() {
    let mut s = shell_with_date_nav("Sat Apr 12 00:00:00 UTC 2026");
    let (out, code, _) = s.run_line("date +%s");
    assert_eq!(code, 0);
    let epoch: u64 = out.trim().parse().expect("should be numeric");
    assert!(epoch > 1_700_000_000, "epoch too small: {epoch}");
}

#[test]
fn date_format_iso_date() {
    let mut s = shell_with_date_nav("Sat Apr 12 00:00:00 UTC 2026");
    let (out, code, _) = s.run_line("date +%F");
    assert_eq!(code, 0);
    assert_eq!(out, "2026-04-12\n");
}

#[test]
fn date_advances_after_sleep() {
    let mut s = shell_with_date_nav("Sat Apr 12 00:00:00 UTC 2026");
    let (before, _, _) = s.run_line("date +%s");
    s.run_line("sleep 2");
    let (after, _, _) = s.run_line("date +%s");
    let t_before: u64 = before.trim().parse().unwrap();
    let t_after: u64 = after.trim().parse().unwrap();
    assert!(
        t_after > t_before,
        "date should advance after sleep: before={t_before} after={t_after}"
    );
}
