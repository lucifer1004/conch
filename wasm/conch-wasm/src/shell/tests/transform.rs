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
