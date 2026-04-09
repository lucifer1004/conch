use super::*;
use crate::types::Config;

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

#[test]
fn tilde_expansion_in_cat() {
    let mut s = shell_with_files(serde_json::json!({
        "note.txt": "from home"
    }));
    let (out, code, _) = s.run_line("cat ~/note.txt");
    assert_eq!(code, 0);
    assert_eq!(out, "from home");
}

#[test]
fn export_semicolon_echo_expands_var() {
    let mut s = shell();
    let (out, code, _) = s.run_line("export ZZ=hello; echo $ZZ");
    assert_eq!(code, 0);
    assert_eq!(out.trim(), "hello");
}

#[test]
fn export_adds_variable_to_env() {
    let mut s = shell();
    let (out, code, _) = s.run_line("export MYVAR=testval; env");
    assert_eq!(code, 0);
    assert!(out.contains("MYVAR=testval"), "got {:?}", out);
}

#[test]
fn env_and_printenv_list_variables() {
    let mut s = shell();
    let (eout, c1, _) = s.run_line("env");
    assert_eq!(c1, 0);
    assert!(eout.contains("HOME=/home/u"), "got {:?}", eout);
    assert!(eout.contains("USER=u"));
    let (pout, c2, _) = s.run_line("printenv");
    assert_eq!(c2, 0);
    assert_eq!(eout, pout);
}

#[test]
fn which_and_type_builtin() {
    let mut s = shell();
    let (w, c1, _) = s.run_line("which echo");
    assert_eq!(c1, 0);
    assert!(w.contains("/bin/echo"), "got {:?}", w);
    let (t, c2, _) = s.run_line("type cd");
    assert_eq!(c2, 0);
    assert!(t.contains("builtin"), "got {:?}", t);
}

#[test]
fn which_only_finds_builtins() {
    let mut s = shell();
    let (out, code, _) = s.run_line("which /bin/ls");
    assert_eq!(code, 1);
    assert!(
        out.contains("no /bin/ls") || out.contains("no "),
        "got {:?}",
        out
    );
}

#[test]
fn date_uses_config_env_date() {
    let c: Config = serde_json::from_value(serde_json::json!({
        "user": "u",
        "system": {
            "hostname": "h",
            "users": [{"name": "u", "home": "/home/u"}],
        },
        "commands": [],
        "date": "Wed Apr  8 12:00:00 UTC 2026",
    }))
    .unwrap();
    let mut s = Shell::new(&c);
    let (out, code, _) = s.run_line("date");
    assert_eq!(code, 0);
    assert_eq!(out, "Wed Apr  8 12:00:00 UTC 2026");
}

#[test]
fn basename_extracts_filename() {
    let mut s = shell();
    let (out, code, _) = s.run_line("basename /home/u/file.txt");
    assert_eq!(code, 0);
    assert_eq!(out, "file.txt");
}

#[test]
fn basename_strips_suffix() {
    let mut s = shell();
    let (out, code, _) = s.run_line("basename /home/u/file.txt .txt");
    assert_eq!(code, 0);
    assert_eq!(out, "file");
}

#[test]
fn dirname_extracts_directory() {
    let mut s = shell();
    let (out, code, _) = s.run_line("dirname /home/u/file.txt");
    assert_eq!(code, 0);
    assert_eq!(out, "/home/u");
}

#[test]
fn dirname_bare_filename_returns_dot() {
    let mut s = shell();
    let (out, code, _) = s.run_line("dirname file.txt");
    assert_eq!(code, 0);
    assert_eq!(out, ".");
}

#[test]
fn realpath_existing_file() {
    let mut s = shell_with_files(serde_json::json!({ "notes.txt": "hi" }));
    let (out, code, _) = s.run_line("realpath notes.txt");
    assert_eq!(code, 0);
    assert_eq!(out, "/home/u/notes.txt");
}

#[test]
fn realpath_missing_file_fails() {
    let mut s = shell();
    let (_, code, _) = s.run_line("realpath ghost.txt");
    assert_ne!(code, 0);
}

#[test]
fn realpath_resolves_relative() {
    let mut s = shell();
    s.run_line("mkdir sub");
    let (out, code, _) = s.run_line("realpath sub");
    assert_eq!(code, 0);
    assert_eq!(out, "/home/u/sub");
}

#[test]
fn id_shows_user_info() {
    let mut s = shell();
    let (out, code, _) = s.run_line("id");
    assert_eq!(code, 0);
    assert!(out.contains("uid="), "expected uid= in output: {:?}", out);
    assert!(out.contains("gid="), "expected gid= in output: {:?}", out);
    assert!(
        out.contains("(u)"),
        "expected username in output: {:?}",
        out
    );
}

#[test]
fn groups_shows_groups() {
    let mut s = shell();
    let (out, code, _) = s.run_line("groups");
    assert_eq!(code, 0);
    assert_eq!(out, "u");
}

#[test]
fn hostname_returns_configured_name() {
    let mut s = shell();
    let (out, code, _) = s.run_line("hostname");
    assert_eq!(code, 0);
    assert_eq!(out, "h");
}

#[test]
fn type_unknown_command() {
    let mut s = shell();
    let (out, code, _) = s.run_line("type nosuchcmd");
    assert_eq!(code, 0);
    assert!(out.contains("not found"), "got {:?}", out);
}
