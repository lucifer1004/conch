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
fn bracket_syntax_works() {
    let mut s = shell_with_files(serde_json::json!({"f.txt": "x"}));
    let (_, code, _) = s.run_line("[ -f f.txt ]");
    assert_eq!(code, 0);
    let (_, code2, _) = s.run_line("[ -d f.txt ]");
    assert_ne!(code2, 0);
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

// test -r, -w, -x
#[test]
fn test_r_readable_file() {
    let mut s = shell_with_files(serde_json::json!({"f.txt": "x"}));
    let (_, code, _) = s.run_line("test -r f.txt");
    assert_eq!(code, 0);
}

#[test]
fn test_w_writable_file() {
    let mut s = shell_with_files(serde_json::json!({"f.txt": "x"}));
    let (_, code, _) = s.run_line("test -w f.txt");
    assert_eq!(code, 0);
}

#[test]
fn test_x_not_executable() {
    let mut s = shell_with_files(serde_json::json!({"f.txt": "x"}));
    let (_, code, _) = s.run_line("test -x f.txt");
    assert_ne!(code, 0);
}

#[test]
fn test_s_nonempty_file() {
    let mut s = shell_with_files(serde_json::json!({"f.txt": "data"}));
    let (_, code, _) = s.run_line("test -s f.txt");
    assert_eq!(code, 0);
}

#[test]
fn test_s_empty_file_fails() {
    let mut s = shell();
    s.run_line("touch empty.txt");
    let (_, code, _) = s.run_line("test -s empty.txt");
    assert_ne!(code, 0);
}

#[test]
fn test_z_empty_string() {
    let mut s = shell();
    let (_, code, _) = s.run_line("test -z ''");
    assert_eq!(code, 0);
}

#[test]
fn test_n_nonempty_string() {
    let mut s = shell();
    let (_, code, _) = s.run_line("test -n hello");
    assert_eq!(code, 0);
}

#[test]
fn test_string_inequality() {
    let mut s = shell();
    let (_, code, _) = s.run_line("test abc != def");
    assert_eq!(code, 0);
}

// stat on symlink
#[test]
fn stat_symlink_shows_type() {
    let mut s = shell_with_files(serde_json::json!({"real.txt": "x"}));
    s.run_line("ln -s real.txt link.txt");
    let (out, code, _) = s.run_line("stat link.txt");
    assert_eq!(code, 0);
    // stat follows symlinks, so should show "regular file"
    assert!(
        out.contains("regular file") || out.contains("Size"),
        "got {:?}",
        out
    );
}

// du on single file
#[test]
fn du_single_file() {
    let mut s = shell_with_files(serde_json::json!({"f.txt": "hello"}));
    let (out, code, _) = s.run_line("du f.txt");
    assert_eq!(code, 0);
    assert!(out.contains("f.txt"), "got {:?}", out);
}

#[test]
fn stat_shows_uid_gid() {
    let mut s = shell_with_files(serde_json::json!({"f.txt": "x"}));
    let (out, code, _) = s.run_line("stat f.txt");
    assert_eq!(code, 0);
    assert!(out.contains("Uid"), "got {:?}", out);
    assert!(out.contains("Gid"), "got {:?}", out);
}

#[test]
fn test_numeric_eq() {
    let mut s = shell();
    let (_, code, _) = s.run_line("test 5 -eq 5");
    assert_eq!(code, 0);
}

#[test]
fn test_numeric_ne() {
    let mut s = shell();
    let (_, code, _) = s.run_line("test 5 -ne 3");
    assert_eq!(code, 0);
}

#[test]
fn test_numeric_lt() {
    let mut s = shell();
    let (_, code, _) = s.run_line("test 3 -lt 5");
    assert_eq!(code, 0);
}

#[test]
fn test_numeric_gt_fails() {
    let mut s = shell();
    let (_, code, _) = s.run_line("test 3 -gt 5");
    assert_ne!(code, 0);
}

#[test]
fn du_nested_directories() {
    let mut s = shell();
    s.run_line("mkdir -p a/b");
    s.run_line("echo hello > a/b/f.txt");
    s.run_line("echo world > a/g.txt");
    let (out, code, _) = s.run_line("du a");
    assert_eq!(code, 0);
    // Should list subdirectories
    assert!(out.contains("a"), "got {:?}", out);
}

#[test]
fn tree_empty_directory() {
    let mut s = shell();
    s.run_line("mkdir empty");
    let (out, code, _) = s.run_line("tree empty");
    assert_eq!(code, 0);
    assert!(out.contains("empty"), "got {:?}", out);
}

// --- M6: test/[ no `!` operator ---
#[test]
fn test_negation() {
    let mut s = shell();
    let (_, code, _) = s.run_line("test ! -f nonexistent");
    assert_eq!(code, 0); // ! negates: -f fails, ! makes it succeed
}

#[test]
fn test_negation_inverts_true() {
    let mut s = shell_with_files(serde_json::json!({"f.txt": "x"}));
    let (_, code, _) = s.run_line("test ! -f f.txt");
    assert_ne!(code, 0); // file exists, so -f is true, ! makes it false
}
