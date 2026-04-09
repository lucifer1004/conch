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
