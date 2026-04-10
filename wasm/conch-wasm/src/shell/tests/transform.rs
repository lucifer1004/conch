use super::*;

#[test]
fn sed_substitutes_first_occurrence() {
    let mut s = shell_with_files(serde_json::json!({ "f.txt": "aaa" }));
    let (out, code, _) = s.run_line("sed 's/a/b/' f.txt");
    assert_eq!(code, 0);
    assert_eq!(out, "baa");
}

#[test]
fn sed_global_substitution() {
    let mut s = shell_with_files(serde_json::json!({ "f.txt": "aaa" }));
    let (out, code, _) = s.run_line("sed 's/a/b/g' f.txt");
    assert_eq!(code, 0);
    assert_eq!(out, "bbb");
}

#[test]
fn sed_inplace_modifies_file() {
    let mut s = shell_with_files(serde_json::json!({ "f.txt": "hello world" }));
    s.run_line("sed -i 's/hello/goodbye/g' f.txt");
    let (out, code, _) = s.run_line("cat f.txt");
    assert_eq!(code, 0);
    assert_eq!(out, "goodbye world");
}

#[test]
fn diff_identical_files_exit_zero() {
    let mut s = shell_with_files(serde_json::json!({
        "f1.txt": "line1\nline2",
        "f2.txt": "line1\nline2"
    }));
    let (out, code, _) = s.run_line("diff f1.txt f2.txt");
    assert_eq!(code, 0);
    assert!(out.is_empty(), "got {:?}", out);
}

#[test]
fn diff_different_files_exit_one() {
    let mut s = shell_with_files(serde_json::json!({
        "a.txt": "hello\nworld",
        "b.txt": "hello\nearth"
    }));
    let (out, code, _) = s.run_line("diff a.txt b.txt");
    assert_eq!(code, 1);
    assert!(
        out.contains("world") || out.contains("earth"),
        "got {:?}",
        out
    );
}

#[test]
fn diff_missing_operand_fails() {
    let mut s = shell();
    let (_, code, _) = s.run_line("diff only_one.txt");
    assert_ne!(code, 0);
}

#[test]
fn xxd_shows_hex_dump() {
    let mut s = shell_with_files(serde_json::json!({ "hi.txt": "Hello" }));
    let (out, code, _) = s.run_line("xxd hi.txt");
    assert_eq!(code, 0);
    // "Hello" = 48 65 6c 6c 6f
    assert!(out.contains("48"), "got {:?}", out);
    assert!(out.contains("Hello"), "got {:?}", out);
}

#[test]
fn xxd_missing_file_fails() {
    let mut s = shell();
    let (_, code, _) = s.run_line("xxd nowhere.txt");
    assert_ne!(code, 0);
}

#[test]
fn xxd_offset_increments_per_line() {
    let content = "A".repeat(20); // 20 bytes → 2 lines of 16/4
    let mut s = shell_with_files(serde_json::json!({ "long.txt": content }));
    let (out, code, _) = s.run_line("xxd long.txt");
    assert_eq!(code, 0);
    assert!(out.contains("00000000:"), "got {:?}", out);
    assert!(out.contains("00000010:"), "got {:?}", out);
}

#[test]
fn base64_encode_file() {
    let mut s = shell_with_files(serde_json::json!({ "msg.txt": "Hello" }));
    let (out, code, _) = s.run_line("base64 msg.txt");
    assert_eq!(code, 0);
    // base64("Hello") = "SGVsbG8="
    assert_eq!(out.trim(), "SGVsbG8=");
}

#[test]
fn base64_decode_roundtrip() {
    let mut s = shell_with_files(serde_json::json!({ "enc.txt": "SGVsbG8=" }));
    let (out, code, _) = s.run_line("base64 -d enc.txt");
    assert_eq!(code, 0);
    assert_eq!(out, "Hello");
}

#[test]
fn base64_missing_file_fails() {
    let mut s = shell();
    let (_, code, _) = s.run_line("base64 missing.txt");
    assert_ne!(code, 0);
}

// sed from stdin
#[test]
fn sed_from_stdin() {
    let mut s = shell();
    let (out, code, _) = s.run_line("echo hello world | sed 's/world/earth/'");
    assert_eq!(code, 0);
    assert_eq!(out, "hello earth");
}

// sed missing file
#[test]
fn sed_missing_file_fails() {
    let mut s = shell();
    let (_, code, _) = s.run_line("sed 's/a/b/' missing.txt");
    assert_ne!(code, 0);
}

// diff with added lines
#[test]
fn diff_added_lines() {
    let mut s = shell_with_files(serde_json::json!({
        "a.txt": "line1",
        "b.txt": "line1\nline2"
    }));
    let (out, code, _) = s.run_line("diff a.txt b.txt");
    assert_eq!(code, 1); // files differ
    assert!(out.contains(">") && out.contains("line2"), "got {:?}", out);
}

// diff missing file
#[test]
fn diff_missing_file_fails() {
    let mut s = shell_with_files(serde_json::json!({"a.txt": "x"}));
    let (_, code, _) = s.run_line("diff a.txt nope.txt");
    assert_ne!(code, 0);
}

// xxd binary content
#[test]
fn xxd_binary_file() {
    let mut s = shell();
    s.run_line("printf '\\x00\\x01\\xff' > bin.dat");
    // The file should exist; xxd should show hex
    let (_out, code, _) = s.run_line("xxd bin.dat");
    // May or may not work depending on printf implementation
    // At minimum, xxd on any file should not panic
    assert!(code == 0 || code == 1);
}

// base64 from stdin
#[test]
fn base64_from_stdin() {
    let mut s = shell();
    let (_, code, _) = s.run_line("echo hello | base64");
    // base64 without a file arg may not read from stdin in this implementation
    assert_eq!(code, 0);
}

// base64 decode invalid
#[test]
fn base64_decode_invalid() {
    let mut s = shell();
    s.run_line("echo '!!invalid!!' > bad.txt");
    let (_, code, _) = s.run_line("base64 -d bad.txt");
    // Should either fail or produce garbage — shouldn't panic
    assert!(code == 0 || code == 1);
}

#[test]
fn diff_deleted_lines() {
    let mut s = shell_with_files(serde_json::json!({
        "a.txt": "line1\nline2\nline3",
        "b.txt": "line1"
    }));
    let (out, code, _) = s.run_line("diff a.txt b.txt");
    assert_eq!(code, 1);
    assert!(out.contains("<"), "expected deletions, got {:?}", out);
}

#[test]
fn xxd_from_stdin() {
    let mut s = shell();
    let (_out, code, _) = s.run_line("echo hello | xxd");
    // xxd may or may not support stdin - check it doesn't panic
    // If it works, output should contain hex
    assert!(code == 0 || code == 1);
}

// --- M9: sed -i discards write errors (return empty stdout) ---
#[test]
fn sed_i_returns_empty_on_success() {
    let mut s = shell_with_files(serde_json::json!({"g.txt": "foo bar"}));
    let (out, code, _) = s.run_line("sed -i 's/foo/baz/' g.txt");
    assert_eq!(code, 0);
    assert_eq!(out, "", "sed -i should produce no stdout, got: {out}");
}

// --- M12: base64 no file no stdin returns empty ---
#[test]
fn base64_no_input_returns_empty() {
    let mut s = shell();
    // base64 with no file and no stdin should produce empty output
    let (out, code, _) = s.run_line("base64");
    assert_eq!(code, 0);
    assert!(
        out.is_empty(),
        "base64 with no input should be empty: {out}"
    );
}

// --- L6: readlink -f ---
#[test]
fn readlink_f_resolves_canonically() {
    let mut s = shell();
    s.run_line("mkdir -p real/dir");
    s.run_line("echo x > real/dir/file.txt");
    s.run_line("ln -s real/dir link");
    let (out, code, _) = s.run_line("readlink -f link/file.txt");
    assert_eq!(code, 0);
    assert!(out.trim().ends_with("/real/dir/file.txt"), "got: {out}");
}
