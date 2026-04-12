use super::*;

/// Helper: create a Shell with a specific VFS date and optional files.
fn shell_with_date(date: &str, files: serde_json::Value) -> Shell {
    let v = serde_json::json!({
        "user": "u",
        "system": {
            "hostname": "h",
            "users": [{"name": "u", "home": "/home/u"}],
            "files": files,
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
// stat
// ---------------------------------------------------------------------------

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
    assert_eq!(code, 1);
    assert!(out.contains("No such file"), "got {:?}", out);
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
fn stat_symlink_shows_type() {
    let mut s = shell_with_files(serde_json::json!({"real.txt": "x"}));
    s.run_line("ln -s real.txt link.txt");
    let (out, code, _) = s.run_line("stat link.txt");
    assert_eq!(code, 0);
    assert!(out.contains("regular file"), "got {:?}", out);
}

#[test]
fn stat_shows_inode_and_links() {
    let mut s = shell();
    s.run_line("echo data > f.txt");
    let (out, code, _) = s.run_line("stat f.txt");
    assert_eq!(code, 0);
    assert!(
        out.contains("Inode:"),
        "expected Inode in stat output: {out}"
    );
    assert!(
        out.contains("Links:"),
        "expected Links in stat output: {out}"
    );
}

#[test]
fn stat_hard_link_shows_nlink_2() {
    let mut s = shell();
    s.run_line("echo data > a.txt");
    s.run_line("ln a.txt b.txt");
    let (out, _, _) = s.run_line("stat a.txt");
    assert!(out.contains("Links: 2"), "expected nlink=2 in stat: {out}");
}

// ---------------------------------------------------------------------------
// test / [ ] (single-bracket)
// ---------------------------------------------------------------------------

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
    assert_eq!(code, 1);
}

#[test]
fn test_f_file_is_true_d_is_false() {
    let mut s = shell_with_files(serde_json::json!({ "f.txt": "x" }));
    let (_, fc, _) = s.run_line("test -f f.txt");
    assert_eq!(fc, 0);
    let (_, dc, _) = s.run_line("test -d f.txt");
    assert_eq!(dc, 1);
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
    assert_eq!(code2, 1);
}

#[test]
fn bracket_test_works_like_test() {
    let mut s = shell_with_files(serde_json::json!({ "a.txt": "x" }));
    let (_, code, _) = s.run_line("[ -f a.txt ]");
    assert_eq!(code, 0);
}

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
    assert_eq!(code, 1);
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
    assert_eq!(code, 1);
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
    assert_eq!(code, 1);
}

#[test]
fn test_negation() {
    let mut s = shell();
    let (_, code, _) = s.run_line("test ! -f nonexistent");
    assert_eq!(code, 0);
}

#[test]
fn test_negation_inverts_true() {
    let mut s = shell_with_files(serde_json::json!({"f.txt": "x"}));
    let (_, code, _) = s.run_line("test ! -f f.txt");
    assert_eq!(code, 1);
}

// ---------------------------------------------------------------------------
// test -L / -h (symlink test)
// ---------------------------------------------------------------------------

#[test]
fn test_l_symlink_is_true() {
    let mut s = shell_with_files(serde_json::json!({"real.txt": "data"}));
    s.run_line("ln -s real.txt link.txt");
    let (_, code, _) = s.run_line("test -L link.txt");
    assert_eq!(code, 0);
}

#[test]
fn test_l_regular_file_is_false() {
    let mut s = shell_with_files(serde_json::json!({"real.txt": "data"}));
    let (_, code, _) = s.run_line("test -L real.txt");
    assert_eq!(code, 1);
}

#[test]
fn test_h_symlink_is_true() {
    let mut s = shell_with_files(serde_json::json!({"real.txt": "data"}));
    s.run_line("ln -s real.txt link.txt");
    let (_, code, _) = s.run_line("test -h link.txt");
    assert_eq!(code, 0);
}

#[test]
fn double_bracket_l_symlink() {
    let mut s = shell_with_files(serde_json::json!({"real.txt": "data"}));
    s.run_line("ln -s real.txt link.txt");
    let (_, code, _) = s.run_line("[[ -L link.txt ]]");
    assert_eq!(code, 0);
    let (_, code2, _) = s.run_line("[[ -L real.txt ]]");
    assert_eq!(code2, 1);
}

// ---------------------------------------------------------------------------
// stat -c FORMAT
// ---------------------------------------------------------------------------

#[test]
fn stat_c_size() {
    let mut s = shell_with_files(serde_json::json!({"f.txt": "hello"}));
    let (out, code, _) = s.run_line("stat -c '%s' f.txt");
    assert_eq!(code, 0);
    assert_eq!(out, "5\n");
}

#[test]
fn stat_c_filename() {
    let mut s = shell_with_files(serde_json::json!({"f.txt": "hello"}));
    let (out, code, _) = s.run_line("stat -c '%n' f.txt");
    assert_eq!(code, 0);
    assert_eq!(out, "f.txt\n");
}

#[test]
fn stat_c_octal_perms() {
    let mut s = shell_with_files(serde_json::json!({"f.txt": "hello"}));
    let (out, code, _) = s.run_line("stat -c '%a' f.txt");
    assert_eq!(code, 0);
    // Should be some octal like 644
    assert!(out.len() > 1, "got: {:?}", out);
}

#[test]
fn stat_c_file_type() {
    let mut s = shell_with_files(serde_json::json!({"f.txt": "hello"}));
    let (out, code, _) = s.run_line("stat -c '%F' f.txt");
    assert_eq!(code, 0);
    assert_eq!(out, "regular file\n");
}

// ---------------------------------------------------------------------------
// du
// ---------------------------------------------------------------------------

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
    assert_eq!(code, 1);
}

#[test]
fn du_human_readable() {
    let mut s = shell_with_files(serde_json::json!({ "data.txt": "hello" }));
    let (out, code, _) = s.run_line("du -sh data.txt");
    assert_eq!(code, 0);
    assert!(out.contains("data.txt"), "got {:?}", out);
}

#[test]
fn du_single_file() {
    let mut s = shell_with_files(serde_json::json!({"f.txt": "hello"}));
    let (out, code, _) = s.run_line("du f.txt");
    assert_eq!(code, 0);
    assert!(out.contains("f.txt"), "got {:?}", out);
}

#[test]
fn du_nested_directories() {
    let mut s = shell();
    s.run_line("mkdir -p a/b");
    s.run_line("echo hello > a/b/f.txt");
    s.run_line("echo world > a/g.txt");
    let (out, code, _) = s.run_line("du a");
    assert_eq!(code, 0);
    assert!(out.contains("a"), "got {:?}", out);
}

// ---------------------------------------------------------------------------
// tree
// ---------------------------------------------------------------------------

#[test]
fn tree_shows_nested_files() {
    let mut s = shell();
    assert_eq!(s.run_line("mkdir -p t/a").1, 0);
    assert_eq!(s.run_line("touch t/a/f.txt").1, 0);
    let (out, code, _) = s.run_line("tree t");
    assert_eq!(code, 0);
    assert!(out.contains("f.txt"), "got {:?}", out);
}

#[test]
fn tree_empty_directory() {
    let mut s = shell();
    s.run_line("mkdir empty");
    let (out, code, _) = s.run_line("tree empty");
    assert_eq!(code, 0);
    assert!(out.contains("empty"), "got {:?}", out);
}

#[test]
fn tree_depth_limit() {
    let mut s = shell();
    s.run_line("mkdir -p d/sub/deep");
    s.run_line("touch d/a.txt");
    s.run_line("touch d/sub/b.txt");
    s.run_line("touch d/sub/deep/c.txt");
    let (out, code, _) = s.run_line("tree -L 1 d");
    assert_eq!(code, 0);
    assert!(
        out.contains("a.txt"),
        "should show immediate children: {out}"
    );
    assert!(out.contains("sub"), "should show subdirectory name: {out}");
    assert!(!out.contains("b.txt"), "should not descend into sub: {out}");
    assert!(!out.contains("deep"), "should not show deep subdir: {out}");
}

// ---------------------------------------------------------------------------
// [[ ]] extended test
// ---------------------------------------------------------------------------

#[test]
fn double_bracket_file_test() {
    let mut s = shell_with_files(serde_json::json!({"f.txt": "x"}));
    let (_, code, _) = s.run_line("[[ -f f.txt ]]");
    assert_eq!(code, 0);
}

#[test]
fn double_bracket_dir_test() {
    let mut s = shell();
    s.run_line("mkdir mydir");
    let (_, code, _) = s.run_line("[[ -d mydir ]]");
    assert_eq!(code, 0);
}

#[test]
fn double_bracket_file_not_exists() {
    let mut s = shell();
    let (_, code, _) = s.run_line("[[ -f ghost.txt ]]");
    assert_eq!(code, 1);
}

#[test]
fn double_bracket_string_equality() {
    let mut s = shell();
    let (_, code, _) = s.run_line("[[ hello == hello ]]");
    assert_eq!(code, 0);
}

#[test]
fn double_bracket_string_inequality() {
    let mut s = shell();
    let (_, code, _) = s.run_line("[[ hello != world ]]");
    assert_eq!(code, 0);
}

#[test]
fn double_bracket_glob_match() {
    let mut s = shell();
    let (_, code, _) = s.run_line("[[ hello == h* ]]");
    assert_eq!(code, 0);
}

#[test]
fn double_bracket_glob_no_match() {
    let mut s = shell();
    let (_, code, _) = s.run_line("[[ hello == w* ]]");
    assert_eq!(code, 1);
}

#[test]
fn double_bracket_regex_match() {
    let mut s = shell();
    let (_, code, _) = s.run_line("[[ abc =~ ^[a-z]+$ ]]");
    assert_eq!(code, 0);
}

#[test]
fn double_bracket_regex_no_match() {
    let mut s = shell();
    let (_, code, _) = s.run_line("[[ 123 =~ ^[a-z]+$ ]]");
    assert_eq!(code, 1);
}

#[test]
fn double_bracket_regex_sets_bash_rematch() {
    let mut s = shell();
    let (_, code, _) = s.run_line("[[ hello123 =~ ^([a-z]+)([0-9]+)$ ]]");
    assert_eq!(code, 0);
    let (out, _, _) = s.run_line("echo $BASH_REMATCH_0");
    assert_eq!(out, "hello123\n");
    let (out, _, _) = s.run_line("echo $BASH_REMATCH_1");
    assert_eq!(out, "hello\n");
    let (out, _, _) = s.run_line("echo $BASH_REMATCH_2");
    assert_eq!(out, "123\n");
}

#[test]
fn double_bracket_and_operator() {
    let mut s = shell();
    let (_, code, _) = s.run_line("[[ -z '' && -n x ]]");
    assert_eq!(code, 0);
}

#[test]
fn double_bracket_or_operator() {
    let mut s = shell();
    let (_, code, _) = s.run_line("[[ -z notempty || -n x ]]");
    assert_eq!(code, 0);
}

#[test]
fn double_bracket_not_operator() {
    let mut s = shell();
    let (_, code, _) = s.run_line("[[ ! -d /nope ]]");
    assert_eq!(code, 0);
}

#[test]
fn double_bracket_not_inverts_true() {
    let mut s = shell_with_files(serde_json::json!({"f.txt": "x"}));
    let (_, code, _) = s.run_line("[[ ! -f f.txt ]]");
    assert_eq!(code, 1);
}

#[test]
fn double_bracket_var_is_set() {
    let mut s = shell();
    let (_, code, _) = s.run_line("[[ -v HOME ]]");
    assert_eq!(code, 0);
}

#[test]
fn double_bracket_var_not_set() {
    let mut s = shell();
    let (_, code, _) = s.run_line("[[ -v NONEXISTENT_VAR_XYZ ]]");
    assert_eq!(code, 1);
}

#[test]
fn double_bracket_lexicographic_less() {
    let mut s = shell();
    let (_, code, _) = s.run_line("[[ abc < def ]]");
    assert_eq!(code, 0);
}

#[test]
fn double_bracket_lexicographic_greater() {
    let mut s = shell();
    let (_, code, _) = s.run_line("[[ def > abc ]]");
    assert_eq!(code, 0);
}

#[test]
fn double_bracket_numeric_eq() {
    let mut s = shell();
    let (_, code, _) = s.run_line("[[ 5 -eq 5 ]]");
    assert_eq!(code, 0);
}

#[test]
fn double_bracket_numeric_ne() {
    let mut s = shell();
    let (_, code, _) = s.run_line("[[ 5 -ne 3 ]]");
    assert_eq!(code, 0);
}

#[test]
fn double_bracket_numeric_lt() {
    let mut s = shell();
    let (_, code, _) = s.run_line("[[ 3 -lt 5 ]]");
    assert_eq!(code, 0);
}

#[test]
fn double_bracket_numeric_gt() {
    let mut s = shell();
    let (_, code, _) = s.run_line("[[ 5 -gt 3 ]]");
    assert_eq!(code, 0);
}

#[test]
fn double_bracket_numeric_le() {
    let mut s = shell();
    let (_, code, _) = s.run_line("[[ 5 -le 5 ]]");
    assert_eq!(code, 0);
}

#[test]
fn double_bracket_numeric_ge() {
    let mut s = shell();
    let (_, code, _) = s.run_line("[[ 5 -ge 3 ]]");
    assert_eq!(code, 0);
}

#[test]
fn double_bracket_parenthesized_expr() {
    let mut s = shell();
    let (_, code, _) = s.run_line("[[ ( -z '' ) && ( -n x ) ]]");
    assert_eq!(code, 0);
}

#[test]
fn double_bracket_z_nonempty_fails() {
    let mut s = shell();
    let (_, code, _) = s.run_line("[[ -z notempty ]]");
    assert_eq!(code, 1);
}

#[test]
fn double_bracket_n_empty_fails() {
    let mut s = shell();
    let (_, code, _) = s.run_line("[[ -n '' ]]");
    assert_eq!(code, 1);
}

#[test]
fn double_bracket_e_existing_file() {
    let mut s = shell_with_files(serde_json::json!({"a.txt": "data"}));
    let (_, code, _) = s.run_line("[[ -e a.txt ]]");
    assert_eq!(code, 0);
}

#[test]
fn double_bracket_s_nonempty_file() {
    let mut s = shell_with_files(serde_json::json!({"a.txt": "data"}));
    let (_, code, _) = s.run_line("[[ -s a.txt ]]");
    assert_eq!(code, 0);
}

#[test]
fn double_bracket_s_empty_file_fails() {
    let mut s = shell();
    s.run_line("touch empty.txt");
    let (_, code, _) = s.run_line("[[ -s empty.txt ]]");
    assert_eq!(code, 1);
}

#[test]
fn double_bracket_complex_logic() {
    let mut s = shell();
    let (_, code, _) = s.run_line("[[ ( -n hello && ! -z world ) || -z '' ]]");
    assert_eq!(code, 0);
}

#[test]
fn double_bracket_in_if_then() {
    let mut s = shell();
    let (out, code) = s.run_script("if [[ hello == hello ]]; then echo yes; fi");
    assert_eq!(code, 0);
    assert_eq!(out, "yes\n");
}

#[test]
fn double_bracket_regex_partial_match() {
    let mut s = shell();
    // =~ does partial match (like grep), not full-string match
    let (_, code, _) = s.run_line("[[ foobar123 =~ [0-9]+ ]]");
    assert_eq!(code, 0);
    let (out, _, _) = s.run_line("echo $BASH_REMATCH_0");
    assert_eq!(out, "123\n");
}

// ---------------------------------------------------------------------------
// Feature 1: test -a / -o compound operators
// ---------------------------------------------------------------------------

#[test]
fn test_a_compound_and_both_true() {
    let mut s = shell_with_files(serde_json::json!({"f.txt": "x"}));
    s.run_line("mkdir dir");
    let (_, code, _) = s.run_line("test -f f.txt -a -d dir");
    assert_eq!(code, 0);
}

#[test]
fn test_a_compound_and_one_false() {
    let mut s = shell_with_files(serde_json::json!({"f.txt": "x"}));
    let (_, code, _) = s.run_line("test -f f.txt -a -d nosuchdir");
    assert_eq!(code, 1);
}

#[test]
fn test_o_compound_or_one_true() {
    let mut s = shell();
    let (_, code, _) = s.run_line("test -f nosuch -o -d /home");
    assert_eq!(code, 0);
}

#[test]
fn test_o_compound_or_both_false() {
    let mut s = shell();
    let (_, code, _) = s.run_line("test -f nosuch -o -d nosuchdir");
    assert_eq!(code, 1);
}

#[test]
fn bracket_a_compound_and_both_true() {
    let mut s = shell_with_files(serde_json::json!({"f.txt": "x"}));
    s.run_line("mkdir dir");
    let (_, code, _) = s.run_line("[ -f f.txt -a -d dir ]");
    assert_eq!(code, 0);
}

// ---------------------------------------------------------------------------
// Feature 2: test/[[ -nt / -ot (newer than / older than)
// ---------------------------------------------------------------------------

#[test]
fn test_nt_newer_file() {
    let mut s = shell();
    s.run_line("touch f1");
    s.run_line("sleep 1");
    s.run_line("touch f2");
    let (_, code, _) = s.run_line("test f2 -nt f1");
    assert_eq!(code, 0);
}

#[test]
fn test_ot_older_file() {
    let mut s = shell();
    s.run_line("touch f1");
    s.run_line("sleep 1");
    s.run_line("touch f2");
    let (_, code, _) = s.run_line("test f1 -ot f2");
    assert_eq!(code, 0);
}

#[test]
fn test_nt_same_mtime_fails() {
    let mut s = shell();
    s.run_line("touch f1");
    s.run_line("touch f2");
    // Same moment, so neither is newer
    let (_, code, _) = s.run_line("test f1 -nt f2");
    assert_eq!(code, 1);
}

#[test]
fn double_bracket_nt_newer_file() {
    let mut s = shell();
    s.run_line("touch f1");
    s.run_line("sleep 1");
    s.run_line("touch f2");
    let (_, code, _) = s.run_line("[[ f2 -nt f1 ]]");
    assert_eq!(code, 0);
}

#[test]
fn double_bracket_ot_older_file() {
    let mut s = shell();
    s.run_line("touch f1");
    s.run_line("sleep 1");
    s.run_line("touch f2");
    let (_, code, _) = s.run_line("[[ f1 -ot f2 ]]");
    assert_eq!(code, 0);
}

// ---------------------------------------------------------------------------
// du -d N / --max-depth N
// ---------------------------------------------------------------------------

#[test]
fn du_d_limits_depth() {
    let mut s = shell();
    s.run_line("mkdir -p d/sub/deep");
    s.run_line("echo a > d/a.txt");
    s.run_line("echo b > d/sub/b.txt");
    s.run_line("echo c > d/sub/deep/c.txt");
    let (out, code, _) = s.run_line("du -d 1 d");
    assert_eq!(code, 0);
    // Should show d and d/sub but NOT d/sub/deep or individual files
    assert!(out.contains("d"), "should show root dir: {out}");
    assert!(
        out.contains("d/sub"),
        "should show immediate child dir: {out}"
    );
    assert!(
        !out.contains("d/sub/deep"),
        "should not show deeper dirs: {out}"
    );
}

// ---------------------------------------------------------------------------
// du -c (grand total)
// ---------------------------------------------------------------------------

#[test]
fn du_c_shows_total() {
    let mut s = shell();
    s.run_line("mkdir d1 d2");
    s.run_line("echo hello > d1/a.txt");
    s.run_line("echo world > d2/b.txt");
    let (out, code, _) = s.run_line("du -c d1 d2");
    assert_eq!(code, 0);
    assert!(
        out.contains("total"),
        "du -c should show a total line: {out}"
    );
}

// ---------------------------------------------------------------------------
// tree summary line (N directories, N files)
// ---------------------------------------------------------------------------

#[test]
fn tree_shows_summary_line() {
    let mut s = shell();
    s.run_line("mkdir -p t/sub");
    s.run_line("touch t/a.txt");
    s.run_line("touch t/sub/b.txt");
    let (out, code, _) = s.run_line("tree t");
    assert_eq!(code, 0);
    // Should end with something like "1 directory, 2 files" or similar
    assert!(
        out.contains("director") && out.contains("file"),
        "tree should show summary with directories and files count: {out}"
    );
}

// ---------------------------------------------------------------------------
// tree -a (show hidden files)
// ---------------------------------------------------------------------------

#[test]
fn tree_a_shows_hidden_files() {
    let mut s = shell();
    s.run_line("mkdir t");
    s.run_line("touch t/.hidden");
    s.run_line("touch t/visible.txt");
    let (out_no_a, code1, _) = s.run_line("tree t");
    assert_eq!(code1, 0);
    assert!(
        !out_no_a.contains(".hidden"),
        "tree without -a should not show hidden: {out_no_a}"
    );
    let (out_a, code2, _) = s.run_line("tree -a t");
    assert_eq!(code2, 0);
    assert!(
        out_a.contains(".hidden"),
        "tree -a should show hidden: {out_a}"
    );
}

// ---------------------------------------------------------------------------
// stat timestamp display
// ---------------------------------------------------------------------------

#[test]
fn stat_shows_modify_timestamp() {
    // Create file via shell command so it gets the configured VFS timestamp
    let mut s = shell_with_date("Sat Apr 12 00:00:01 UTC 2026", serde_json::json!({}));
    s.run_line("echo hi > f.txt");
    let (out, code, _) = s.run_line("stat f.txt");
    assert_eq!(code, 0);
    assert!(out.contains("Modify:"), "expected Modify: in stat: {out}");
    assert!(out.contains("2026"), "expected year 2026 in Modify: {out}");
}

#[test]
fn stat_shows_access_and_change_timestamps() {
    let mut s = shell_with_date("Sat Apr 12 00:00:01 UTC 2026", serde_json::json!({}));
    s.run_line("echo hi > f.txt");
    let (out, code, _) = s.run_line("stat f.txt");
    assert_eq!(code, 0);
    assert!(out.contains("Access:"), "expected Access: in stat: {out}");
    assert!(out.contains("Change:"), "expected Change: in stat: {out}");
}

#[test]
fn stat_c_capital_y_returns_epoch_seconds() {
    // "Sat Apr 12 00:00:01 UTC 2026" → large epoch seconds
    let mut s = shell_with_date("Sat Apr 12 00:00:01 UTC 2026", serde_json::json!({}));
    s.run_line("echo hi > f.txt");
    let (out, code, _) = s.run_line("stat -c '%Y' f.txt");
    assert_eq!(code, 0);
    let epoch: u64 = out.trim().parse().expect("should be numeric epoch");
    // 2026 is well past 1_700_000_000
    assert!(epoch > 1_700_000_000, "epoch too small: {epoch}");
}

#[test]
fn stat_c_lowercase_y_returns_human_date() {
    let mut s = shell_with_date("Sat Apr 12 00:00:01 UTC 2026", serde_json::json!({}));
    s.run_line("echo hi > f.txt");
    let (out, code, _) = s.run_line("stat -c '%y' f.txt");
    assert_eq!(code, 0);
    assert!(out.contains("2026"), "expected 2026 in %y: {out}");
    assert!(out.contains("+0000"), "expected +0000 in %y: {out}");
}

// ---------------------------------------------------------------------------
// ls -l timestamp and permission tests
// ---------------------------------------------------------------------------

#[test]
fn ls_l_shows_file_permissions() {
    let mut s = shell_with_date("Sat Apr 12 00:00:01 UTC 2026", serde_json::json!({}));
    s.run_line("echo hello > f.txt");
    // ls -l on a directory lists entries in long format; use . to get long listing
    let (out, code, _) = s.run_line("ls -l .");
    assert_eq!(code, 0);
    // The f.txt line should start with - for regular file
    let file_line = out.lines().find(|l| l.contains("f.txt")).unwrap_or("");
    assert!(
        file_line.starts_with('-'),
        "expected - prefix for file line: {file_line}\nfull: {out}"
    );
    assert!(
        file_line.contains("u"),
        "expected owner in ls -l: {file_line}"
    );
}

#[test]
fn ls_l_shows_directory_d_prefix() {
    let mut s = shell_with_date("Sat Apr 12 00:00:01 UTC 2026", serde_json::json!({}));
    s.run_line("mkdir mydir");
    let (out, code, _) = s.run_line("ls -l");
    assert_eq!(code, 0);
    // mydir/ line should start with d
    let dir_line = out.lines().find(|l| l.contains("mydir")).unwrap_or("");
    assert!(
        dir_line.starts_with('d'),
        "expected d prefix for dir: {dir_line}"
    );
}

#[test]
fn ls_l_shows_mtime() {
    let mut s = shell_with_date("Sat Apr 12 00:00:01 UTC 2026", serde_json::json!({}));
    s.run_line("echo hi > f.txt");
    // List the directory to get long format with mtime
    let (out, code, _) = s.run_line("ls -l .");
    assert_eq!(code, 0);
    let file_line = out.lines().find(|l| l.contains("f.txt")).unwrap_or("");
    // Should contain Apr in the mtime field
    assert!(
        file_line.contains("Apr"),
        "expected Apr in ls -l mtime: {file_line}\nfull: {out}"
    );
}

// Regression: ls -l with file args should show long format, not just filenames
#[test]
fn ls_l_file_args_show_long_format() {
    let mut s = shell();
    s.run_line("echo hello > /tmp/f1.txt");
    let (out, code, _) = s.run_line("ls -l /tmp/f1.txt");
    assert_eq!(code, 0);
    // Exact format: "-rw-r--r--  1 u u     6 <date> f1.txt\n"
    // echo hello writes "hello\n" = 6 bytes
    let line = out.trim_end_matches('\n');
    assert!(
        line.starts_with("-rw-r--r--"),
        "should start with perms: {:?}",
        line
    );
    assert!(
        line.ends_with("f1.txt"),
        "should end with filename: {:?}",
        line
    );
    assert!(
        line.contains(" u u "),
        "should contain owner/group: {:?}",
        line
    );
    assert!(line.contains("     6 "), "should show size 6: {:?}", line);
}

// Regression: ls -l with multiple file args should not have blank lines between them
#[test]
fn ls_l_multiple_files_no_blank_line() {
    let mut s = shell();
    s.run_line("echo aa > /tmp/a.txt");
    s.run_line("echo bb > /tmp/b.txt");
    let (out, code, _) = s.run_line("ls -l /tmp/a.txt /tmp/b.txt");
    assert_eq!(code, 0);
    // Exactly 2 lines separated by \n, no blank lines
    let lines: Vec<&str> = out.trim_end_matches('\n').split('\n').collect();
    assert_eq!(lines.len(), 2, "expected exactly 2 lines: {:?}", out);
    assert!(
        lines[0].starts_with("-rw-r--r--"),
        "line 0 perms: {:?}",
        lines[0]
    );
    assert!(lines[0].ends_with("a.txt"), "line 0 name: {:?}", lines[0]);
    assert!(
        lines[1].starts_with("-rw-r--r--"),
        "line 1 perms: {:?}",
        lines[1]
    );
    assert!(lines[1].ends_with("b.txt"), "line 1 name: {:?}", lines[1]);
}

// Regression: ls -l files should show different mtimes after sleep
#[test]
fn ls_l_files_show_different_mtimes_after_sleep() {
    let mut s = shell_with_date("Sat Apr 12 00:00:00 UTC 2026", serde_json::json!({}));
    s.run_line("touch /tmp/before.txt");
    s.run_line("sleep 120");
    s.run_line("touch /tmp/after.txt");
    let (out, code, _) = s.run_line("ls -l /tmp/before.txt /tmp/after.txt");
    assert_eq!(code, 0);
    let lines: Vec<&str> = out.trim_end_matches('\n').split('\n').collect();
    assert_eq!(lines.len(), 2, "expected 2 lines: {:?}", out);
    // Line 0: before.txt at 00:00, Line 1: after.txt at 00:02
    assert!(
        lines[0].contains("00:00"),
        "before should be 00:00: {:?}",
        lines[0]
    );
    assert!(
        lines[1].contains("00:02"),
        "after should be 00:02 (2 min later): {:?}",
        lines[1]
    );
}

// Regression: jobs format should use fixed-width padding, no tabs
#[test]
fn jobs_format_has_proper_spacing() {
    let mut s = shell();
    s.run_line("echo test &");
    let (out, _, _) = s.run_line("jobs");
    let line = out.trim_end_matches('\n');
    // Exact format: "[1]+  Done                    echo test"
    assert!(!line.contains('\t'), "no tabs: {:?}", line);
    assert!(
        line.starts_with("[1]+  Done"),
        "starts with [1]+  Done: {:?}",
        line
    );
    assert!(line.ends_with("echo test"), "ends with cmd: {:?}", line);
    // Verify the padding: "Done" (4 chars) + spaces = 24 chars before command
    let done_pos = line.find("Done").expect("should contain Done");
    let cmd_pos = line.find("echo test").expect("should contain echo test");
    let gap = cmd_pos - done_pos;
    assert_eq!(
        gap, 24,
        "status field should be 24 chars wide, got {}: {:?}",
        gap, line
    );
}
