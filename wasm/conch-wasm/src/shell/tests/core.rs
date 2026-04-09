use super::*;

#[test]
fn run_line_echo() {
    let mut s = shell();
    let (out, code, _) = s.run_line("echo hi");
    assert_eq!(code, 0);
    assert_eq!(out.trim_end(), "hi");
}

#[test]
fn run_line_unknown_command_exit_127() {
    let mut s = shell();
    let (out, code, _) = s.run_line("nosuchcmd");
    assert_eq!(code, 127);
    assert!(out.contains("not found"));
}

#[test]
fn and_skips_second_when_first_fails() {
    let mut s = shell();
    let (out, code, _) = s.run_line("false && echo no");
    assert_ne!(code, 0);
    assert!(!out.contains("no"));
}

#[test]
fn or_runs_second_when_first_fails() {
    let mut s = shell();
    let (out, code, _) = s.run_line("false || echo yes");
    assert_eq!(code, 0);
    assert!(out.contains("yes"));
}

#[test]
fn pipe_feeds_stdin() {
    let mut s = shell();
    let (out, code, _) = s.run_line("echo ab | wc");
    assert_eq!(code, 0);
    // stdin `wc` prints: lines, words, bytes — last field is byte count (`echo` has no trailing newline)
    let last = out.split_whitespace().last().expect("wc output");
    assert_eq!(last, "2");
}

#[test]
fn expand_replaces_home() {
    let s = shell();
    assert_eq!(s.expand("$HOME"), "/home/u");
}

#[test]
fn display_path_tilde_at_home() {
    let s = shell();
    assert_eq!(s.display_path(), "~");
}

#[test]
fn execute_entry_records_command() {
    let mut s = shell();
    let e = s.execute("pwd");
    assert_eq!(e.exit_code, 0);
    assert_eq!(e.command, "pwd");
    assert!(e.output.contains("/home/u") || e.output == "/home/u");
}

#[test]
fn redirect_overwrite_writes_and_reads_back() {
    let mut s = shell();
    let (_, c1, _) = s.run_line("echo payload > out.txt");
    assert_eq!(c1, 0);
    let (out, c2, _) = s.run_line("cat out.txt");
    assert_eq!(c2, 0);
    assert_eq!(out, "payload");
}

#[test]
fn redirect_append_accumulates() {
    let mut s = shell();
    s.run_line("echo first > log.txt");
    s.run_line("echo second >> log.txt");
    let (out, code, _) = s.run_line("cat log.txt");
    assert_eq!(code, 0);
    assert!(out.contains("first"), "got {:?}", out);
    assert!(out.contains("second"), "got {:?}", out);
}

#[test]
fn redirect_overwrite_rejects_read_only_file() {
    let mut s = shell_with_files(serde_json::json!({
        "ro.txt": { "content": "orig", "mode": 444 }
    }));
    let (out, code, _) = s.run_line("echo hijack > ro.txt");
    assert_eq!(code, 1);
    assert!(out.contains("Permission denied"), "got {:?}", out);
    let (content, _, _) = s.run_line("cat ro.txt");
    assert_eq!(content, "orig");
}

#[test]
fn redirect_append_rejects_read_only() {
    let mut s = shell_with_files(serde_json::json!({
        "ro.txt": { "content": "line1", "mode": 444 }
    }));
    let (out, code, _) = s.run_line("echo line2 >> ro.txt");
    assert_eq!(code, 1);
    assert!(out.contains("Permission denied"), "got {:?}", out);
}

#[test]
fn unterminated_quote_is_syntax_error() {
    let mut s = shell();
    let (out, code, _) = s.run_line("echo 'broken");
    assert_eq!(code, 2);
    assert!(out.contains("unterminated"), "got {:?}", out);
}

#[test]
fn true_and_runs_following_command() {
    let mut s = shell();
    let (out, code, _) = s.run_line("true && echo chained");
    assert_eq!(code, 0);
    assert_eq!(out.trim(), "chained");
}

#[test]
fn clear_resets_entries() {
    // clear is tested in lib.rs integration, but verify the output token
    let mut s = shell();
    let entry = s.execute("clear");
    assert_eq!(entry.output, "__CLEAR__");
    assert_eq!(entry.exit_code, 0);
}
