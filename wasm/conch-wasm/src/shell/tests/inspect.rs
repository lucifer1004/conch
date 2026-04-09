use super::*;

#[test]
fn stat_shows_file_info() {
    let mut s = shell_with_files(serde_json::json!({ "readme.txt": "hello" }));
    let (out, code, _) = s.run_line("stat readme.txt");
    assert_eq!(code, 0);
    assert!(out.contains("readme.txt"), "got {:?}", out);
    assert!(out.contains("regular file"), "got {:?}", out);
}

#[test]
fn stat_shows_directory_info() {
    let mut s = shell();
    s.run_line("mkdir mydir");
    let (out, code, _) = s.run_line("stat mydir");
    assert_eq!(code, 0);
    assert!(out.contains("directory"), "got {:?}", out);
}

#[test]
fn stat_missing_fails() {
    let mut s = shell();
    let (out, code, _) = s.run_line("stat ghost.txt");
    assert_ne!(code, 0);
    assert!(out.contains("No such file"), "got {:?}", out);
}

#[test]
fn test_e_existing_file_is_true() {
    let mut s = shell_with_files(serde_json::json!({ "x.txt": "" }));
    let (_, code, _) = s.run_line("test -e x.txt");
    assert_eq!(code, 0);
}

#[test]
fn test_e_missing_file_is_false() {
    let mut s = shell();
    let (_, code, _) = s.run_line("test -e ghost.txt");
    assert_ne!(code, 0);
}

#[test]
fn test_f_file_is_true_d_is_false() {
    let mut s = shell_with_files(serde_json::json!({ "f.txt": "x" }));
    let (_, fc, _) = s.run_line("test -f f.txt");
    assert_eq!(fc, 0);
    let (_, dc, _) = s.run_line("test -d f.txt");
    assert_ne!(dc, 0);
}

#[test]
fn test_d_directory_is_true() {
    let mut s = shell();
    s.run_line("mkdir d");
    let (_, code, _) = s.run_line("test -d d");
    assert_eq!(code, 0);
}

#[test]
fn test_string_equality() {
    let mut s = shell();
    let (_, eq, _) = s.run_line("test hello = hello");
    assert_eq!(eq, 0);
    let (_, ne, _) = s.run_line("test hello != world");
    assert_eq!(ne, 0);
}

#[test]
fn bracket_test_works_like_test() {
    let mut s = shell_with_files(serde_json::json!({ "a.txt": "x" }));
    let (_, code, _) = s.run_line("[ -f a.txt ]");
    assert_eq!(code, 0);
}

#[test]
fn du_summary_flag() {
    let mut s = shell_with_files(serde_json::json!({ "big.txt": "hello world" }));
    let (out, code, _) = s.run_line("du -s big.txt");
    assert_eq!(code, 0);
    assert!(out.contains("big.txt"), "got {:?}", out);
}

#[test]
fn du_missing_fails() {
    let mut s = shell();
    let (_, code, _) = s.run_line("du ghost_dir");
    assert_ne!(code, 0);
}

#[test]
fn du_human_readable() {
    let mut s = shell_with_files(serde_json::json!({ "data.txt": "hello" }));
    let (out, code, _) = s.run_line("du -sh data.txt");
    assert_eq!(code, 0);
    assert!(out.contains("data.txt"), "got {:?}", out);
}

#[test]
fn tree_shows_nested_files() {
    let mut s = shell();
    assert_eq!(s.run_line("mkdir -p t/a").1, 0);
    assert_eq!(s.run_line("touch t/a/f.txt").1, 0);
    let (out, code, _) = s.run_line("tree t");
    assert_eq!(code, 0);
    assert!(
        out.contains("f.txt") || out.contains(".txt"),
        "got {:?}",
        out
    );
}
