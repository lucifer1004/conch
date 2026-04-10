use super::*;

#[test]
fn bash_runs_script_from_file() {
    let mut s = shell_with_files(serde_json::json!({
        "run.sh": { "content": "echo frombash\n", "mode": 755 }
    }));
    let (out, code, _) = s.run_line("bash run.sh");
    assert_eq!(code, 0);
    assert!(out.contains("frombash"), "got {:?}", out);
}

#[test]
fn bash_missing_script() {
    let mut s = shell();
    let (out, code, _) = s.run_line("bash nowhere.sh");
    assert_eq!(code, 1);
    assert!(out.contains("No such file"), "got {:?}", out);
}

#[test]
fn bash_rejects_directory() {
    let mut s = shell();
    assert_eq!(s.run_line("mkdir adir").1, 0);
    let (out, code, _) = s.run_line("bash adir");
    assert_eq!(code, 1);
    assert!(out.contains("Is a directory"), "got {:?}", out);
}

#[test]
fn bash_rejects_unreadable_script() {
    let mut s = shell_with_files(serde_json::json!({
        "secret.sh": { "content": "echo no\n", "mode": 0 }
    }));
    let (out, code, _) = s.run_line("bash secret.sh");
    assert_eq!(code, 1);
    assert!(out.contains("Permission denied"), "got {:?}", out);
}

#[test]
fn exec_dot_slash_runs_with_execute_bit() {
    let mut s = shell_with_files(serde_json::json!({
        "hello.sh": { "content": "echo from_exec\n", "mode": 755 }
    }));
    let (out, code, _) = s.run_line("./hello.sh");
    assert_eq!(code, 0);
    assert!(out.contains("from_exec"), "got {:?}", out);
}

#[test]
fn exec_rejects_non_executable_file() {
    let mut s = shell_with_files(serde_json::json!({
        "no_x.sh": { "content": "echo x\n", "mode": 644 }
    }));
    let (out, code, _) = s.run_line("./no_x.sh");
    assert_eq!(code, 126);
    assert!(out.contains("Permission denied"), "got {:?}", out);
}

#[test]
fn exec_rejects_unreadable_script() {
    let mut s = shell_with_files(serde_json::json!({
        "locked.sh": { "content": "x", "mode": 0 }
    }));
    let (out, code, _) = s.run_line("./locked.sh");
    assert_eq!(code, 126);
    assert!(out.contains("Permission denied"), "got {:?}", out);
}

#[test]
fn exec_missing_script() {
    let mut s = shell();
    let (out, code, _) = s.run_line("./missing.sh");
    assert_eq!(code, 127);
    assert!(out.contains("No such file"), "got {:?}", out);
}

#[test]
fn sh_runs_script_like_bash() {
    let mut s = shell_with_files(serde_json::json!({
        "run.sh": { "content": "echo fromsh\n", "mode": 755 }
    }));
    let (out, code, _) = s.run_line("sh run.sh");
    assert_eq!(code, 0);
    assert!(out.contains("fromsh"), "got {:?}", out);
}

#[test]
fn sh_missing_script() {
    let mut s = shell();
    let (out, code, _) = s.run_line("sh nope.sh");
    assert_eq!(code, 1);
    assert!(out.contains("No such file"), "got {:?}", out);
}

#[test]
fn source_runs_script_in_current_context() {
    let mut s = shell();
    s.run_line("echo 'export MSG=hello' > setup.sh");
    s.run_line("source setup.sh");
    let (out, _, _) = s.run_line("echo $MSG");
    assert_eq!(out, "hello");
}

#[test]
fn dot_is_alias_for_source() {
    let mut s = shell();
    s.run_line("echo 'export X=42' > env.sh");
    s.run_line(". env.sh");
    let (out, _, _) = s.run_line("echo $X");
    assert_eq!(out, "42");
}

#[test]
fn source_missing_file_fails() {
    let mut s = shell();
    let (out, code, _) = s.run_line("source nope.sh");
    assert_ne!(code, 0);
    assert!(out.contains("nope.sh"), "got {:?}", out);
}

#[test]
fn source_no_args_fails() {
    let mut s = shell();
    let (out, code, _) = s.run_line("source");
    assert_eq!(code, 2);
    assert!(out.contains("filename"), "got {:?}", out);
}
