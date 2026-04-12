use super::*;

// ---------------------------------------------------------------------------
// sed
// ---------------------------------------------------------------------------

#[test]
fn sed_substitutes_first_occurrence() {
    let mut s = shell_with_files(serde_json::json!({ "f.txt": "aaa" }));
    let (out, code, _) = s.run_line("sed 's/a/b/' f.txt");
    assert_eq!(code, 0);
    assert_eq!(out, "baa\n");
}

#[test]
fn sed_global_substitution() {
    let mut s = shell_with_files(serde_json::json!({ "f.txt": "aaa" }));
    let (out, code, _) = s.run_line("sed 's/a/b/g' f.txt");
    assert_eq!(code, 0);
    assert_eq!(out, "bbb\n");
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
fn sed_from_stdin() {
    let mut s = shell();
    let (out, code, _) = s.run_line("echo hello world | sed 's/world/earth/'");
    assert_eq!(code, 0);
    assert_eq!(out, "hello earth\n");
}

#[test]
fn sed_missing_file_fails() {
    let mut s = shell();
    let (_, code, _) = s.run_line("sed 's/a/b/' missing.txt");
    assert_eq!(code, 1);
}

#[test]
fn sed_i_returns_empty_on_success() {
    let mut s = shell_with_files(serde_json::json!({"g.txt": "foo bar"}));
    let (out, code, _) = s.run_line("sed -i 's/foo/baz/' g.txt");
    assert_eq!(code, 0);
    assert_eq!(out, "", "sed -i should produce no stdout, got: {out}");
}

#[test]
fn sed_multiple_e_flags_applied_in_order() {
    let mut s = shell_with_files(serde_json::json!({ "f.txt": "ac" }));
    let (out, code, _) = s.run_line("sed -e 's/a/b/' -e 's/c/d/' f.txt");
    assert_eq!(code, 0);
    assert_eq!(out, "bd\n");
}

#[test]
fn sed_escaped_delimiter_in_pattern() {
    let mut s = shell_with_files(serde_json::json!({ "f.txt": "foo/bar" }));
    let (out, code, _) = s.run_line(r"sed 's/foo\/bar/baz/' f.txt");
    assert_eq!(code, 0);
    assert_eq!(out, "baz\n");
}

// ---------------------------------------------------------------------------
// diff
// ---------------------------------------------------------------------------

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
    assert_eq!(code, 2);
}

#[test]
fn diff_added_lines() {
    let mut s = shell_with_files(serde_json::json!({
        "a.txt": "line1",
        "b.txt": "line1\nline2"
    }));
    let (out, code, _) = s.run_line("diff a.txt b.txt");
    assert_eq!(code, 1);
    assert!(out.contains(">") && out.contains("line2"), "got {:?}", out);
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
fn diff_missing_file_fails() {
    let mut s = shell_with_files(serde_json::json!({"a.txt": "x"}));
    let (_, code, _) = s.run_line("diff a.txt nope.txt");
    assert_eq!(code, 2);
}

#[test]
fn diff_change_format_no_spaces() {
    let mut s = shell_with_files(serde_json::json!({
        "a.txt": "hello",
        "b.txt": "world"
    }));
    let (out, code, _) = s.run_line("diff a.txt b.txt");
    assert_eq!(code, 1);
    // Must contain "1c1" without spaces around the 'c'
    assert!(out.contains("1c1"), "expected '1c1' format, got {:?}", out);
    assert!(
        !out.contains("1 c 1"),
        "should not have spaces around 'c', got {:?}",
        out
    );
}

// ---------------------------------------------------------------------------
// xxd
// ---------------------------------------------------------------------------

#[test]
fn xxd_shows_hex_dump() {
    let mut s = shell_with_files(serde_json::json!({ "hi.txt": "Hello" }));
    let (out, code, _) = s.run_line("xxd hi.txt");
    assert_eq!(code, 0);
    assert!(out.contains("48"), "got {:?}", out);
    assert!(out.contains("Hello"), "got {:?}", out);
}

#[test]
fn xxd_missing_file_fails() {
    let mut s = shell();
    let (_, code, _) = s.run_line("xxd nowhere.txt");
    assert_eq!(code, 1);
}

#[test]
fn xxd_offset_increments_per_line() {
    let content = "A".repeat(20);
    let mut s = shell_with_files(serde_json::json!({ "long.txt": content }));
    let (out, code, _) = s.run_line("xxd long.txt");
    assert_eq!(code, 0);
    assert!(out.contains("00000000:"), "got {:?}", out);
    assert!(out.contains("00000010:"), "got {:?}", out);
}

#[test]
fn xxd_binary_file() {
    let mut s = shell();
    s.run_line("printf '\\x00\\x01\\xff' > bin.dat");
    let (_out, code, _) = s.run_line("xxd bin.dat");
    assert_eq!(code, 0);
}

#[test]
fn xxd_from_stdin() {
    let mut s = shell();
    let (out, code, _) = s.run_line("echo hello | xxd");
    assert_eq!(code, 0);
    assert!(out.contains("68"), "should contain hex for 'h': {out}");
}

// ---------------------------------------------------------------------------
// base64
// ---------------------------------------------------------------------------

#[test]
fn base64_encode_file() {
    let mut s = shell_with_files(serde_json::json!({ "msg.txt": "Hello" }));
    let (out, code, _) = s.run_line("base64 msg.txt");
    assert_eq!(code, 0);
    assert_eq!(out, "SGVsbG8=\n");
}

#[test]
fn base64_decode_roundtrip() {
    let mut s = shell_with_files(serde_json::json!({ "enc.txt": "SGVsbG8=" }));
    let (out, code, _) = s.run_line("base64 -d enc.txt");
    assert_eq!(code, 0);
    assert_eq!(out, "Hello\n");
}

#[test]
fn base64_missing_file_fails() {
    let mut s = shell();
    let (_, code, _) = s.run_line("base64 missing.txt");
    assert_eq!(code, 1);
}

#[test]
fn base64_from_stdin() {
    let mut s = shell();
    let (_, code, _) = s.run_line("echo hello | base64");
    assert_eq!(code, 0);
}

#[test]
fn base64_decode_invalid() {
    let mut s = shell();
    s.run_line("echo '!!invalid!!' > bad.txt");
    let (_, code, _) = s.run_line("base64 -d bad.txt");
    assert_eq!(code, 1);
}

#[test]
fn base64_encode_stdin() {
    let mut s = shell();
    // "echo Hello" produces "Hello\n"; base64 of "Hello\n" is "SGVsbG8K"
    let (out, code, _) = s.run_line("echo Hello | base64");
    assert_eq!(code, 0);
    assert_eq!(out, "SGVsbG8K\n");
}

#[test]
fn base64_no_input_returns_empty() {
    let mut s = shell();
    let (out, code, _) = s.run_line("base64");
    assert_eq!(code, 0);
    assert!(
        out.is_empty(),
        "base64 with no input should be empty: {out}"
    );
}

// ---------------------------------------------------------------------------
// readlink -f
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// diff LCS-based algorithm
// ---------------------------------------------------------------------------

#[test]
fn diff_insert_in_middle() {
    let mut s = shell_with_files(serde_json::json!({
        "a.txt": "line1\nline3",
        "b.txt": "line1\nline2\nline3"
    }));
    let (out, code, _) = s.run_line("diff a.txt b.txt");
    assert_eq!(code, 1);
    assert!(
        out.contains("> line2"),
        "should show line2 as addition: {out}"
    );
    assert!(out.contains("a"), "should have 'a' (add) marker: {out}");
}

#[test]
fn diff_delete_in_middle() {
    let mut s = shell_with_files(serde_json::json!({
        "a.txt": "line1\nline2\nline3",
        "b.txt": "line1\nline3"
    }));
    let (out, code, _) = s.run_line("diff a.txt b.txt");
    assert_eq!(code, 1);
    assert!(
        out.contains("< line2"),
        "should show line2 as deletion: {out}"
    );
    assert!(out.contains("d"), "should have 'd' (delete) marker: {out}");
}

#[test]
fn diff_unified_format() {
    let mut s = shell_with_files(serde_json::json!({
        "a.txt": "hello\nworld",
        "b.txt": "hello\nearth"
    }));
    let (out, code, _) = s.run_line("diff -u a.txt b.txt");
    assert_eq!(code, 1);
    assert!(out.contains("---"), "unified should have --- header: {out}");
    assert!(out.contains("+++"), "unified should have +++ header: {out}");
    assert!(
        out.contains("@@"),
        "unified should have @@ hunk marker: {out}"
    );
}

// ---------------------------------------------------------------------------
// xxd stdin
// ---------------------------------------------------------------------------

#[test]
fn xxd_stdin_hex_dump() {
    let mut s = shell();
    let (out, code, _) = s.run_line("echo -n ABC | xxd");
    assert_eq!(code, 0);
    assert!(out.contains("4142"), "should contain hex for AB: {out}");
    assert!(out.contains("ABC"), "should contain ascii: {out}");
}

// ---------------------------------------------------------------------------
// 2A.8-2A.9: sed enhancements
// ---------------------------------------------------------------------------

#[test]
fn sed_n_with_pattern_print() {
    let mut s = shell_with_files(serde_json::json!({
        "f.txt": "apple\nbanana\napricot\n"
    }));
    let (out, code, _) = s.run_line("sed -n '/foo/p' f.txt");
    assert_eq!(code, 0);
    assert!(
        out.trim().is_empty(),
        "no lines should match, got {:?}",
        out
    );

    let (out2, code2, _) = s.run_line("sed -n '/ap/p' f.txt");
    assert_eq!(code2, 0);
    assert!(out2.contains("apple"), "got {:?}", out2);
    assert!(out2.contains("apricot"), "got {:?}", out2);
    assert!(!out2.contains("banana"), "got {:?}", out2);
}

#[test]
fn sed_pattern_delete() {
    let mut s = shell_with_files(serde_json::json!({
        "f.txt": "keep\ndelete this\nkeep too\n"
    }));
    let (out, code, _) = s.run_line("sed '/delete/d' f.txt");
    assert_eq!(code, 0);
    assert!(out.contains("keep"), "got {:?}", out);
    assert!(out.contains("keep too"), "got {:?}", out);
    assert!(!out.contains("delete"), "got {:?}", out);
}

#[test]
fn sed_line_range_delete() {
    let mut s = shell_with_files(serde_json::json!({
        "f.txt": "line1\nline2\nline3\nline4\nline5\n"
    }));
    let (out, code, _) = s.run_line("sed '2,4d' f.txt");
    assert_eq!(code, 0);
    assert!(out.contains("line1"), "got {:?}", out);
    assert!(out.contains("line5"), "got {:?}", out);
    assert!(!out.contains("line2"), "got {:?}", out);
    assert!(!out.contains("line3"), "got {:?}", out);
    assert!(!out.contains("line4"), "got {:?}", out);
}

#[test]
fn sed_alternate_delimiter() {
    let mut s = shell_with_files(serde_json::json!({
        "f.txt": "/usr/bin\n"
    }));
    let (out, code, _) = s.run_line("sed 's|/usr/bin|/usr/local/bin|' f.txt");
    assert_eq!(code, 0);
    assert_eq!(out, "/usr/local/bin\n");
}

#[test]
fn sed_e_regex_substitution() {
    let mut s = shell_with_files(serde_json::json!({
        "f.txt": "abc 123 def 456\n"
    }));
    let (out, code, _) = s.run_line("sed -E 's/[0-9]+/NUM/g' f.txt");
    assert_eq!(code, 0);
    assert_eq!(out, "abc NUM def NUM\n");
}

#[test]
fn sed_subst_print_flag() {
    let mut s = shell_with_files(serde_json::json!({
        "f.txt": "hello world\nfoo bar\n"
    }));
    let (out, code, _) = s.run_line("sed -n 's/hello/HI/p' f.txt");
    assert_eq!(code, 0);
    assert_eq!(out, "HI world\n");
}

#[test]
fn sed_line_number_delete() {
    let mut s = shell_with_files(serde_json::json!({
        "f.txt": "first\nsecond\nthird\n"
    }));
    let (out, code, _) = s.run_line("sed '2d' f.txt");
    assert_eq!(code, 0);
    assert!(out.contains("first"), "got {:?}", out);
    assert!(out.contains("third"), "got {:?}", out);
    assert!(!out.contains("second"), "got {:?}", out);
}

// ---------------------------------------------------------------------------
// sed a\TEXT (append), i\TEXT (insert), c\TEXT (change)
// ---------------------------------------------------------------------------

#[test]
fn sed_append_after_line() {
    let mut s = shell_with_files(serde_json::json!({
        "f.txt": "line1\nline2\nline3"
    }));
    let (out, code, _) = s.run_line(r"sed '2a\new' f.txt");
    assert_eq!(code, 0);
    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(
        lines,
        vec!["line1", "line2", "new", "line3"],
        "got {:?}",
        out
    );
}

#[test]
fn sed_insert_before_line() {
    let mut s = shell_with_files(serde_json::json!({
        "f.txt": "line1\nline2\nline3"
    }));
    let (out, code, _) = s.run_line(r"sed '1i\header' f.txt");
    assert_eq!(code, 0);
    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(
        lines,
        vec!["header", "line1", "line2", "line3"],
        "got {:?}",
        out
    );
}

#[test]
fn sed_change_line() {
    let mut s = shell_with_files(serde_json::json!({
        "f.txt": "line1\nline2\nline3"
    }));
    let (out, code, _) = s.run_line(r"sed '2c\replaced' f.txt");
    assert_eq!(code, 0);
    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(lines, vec!["line1", "replaced", "line3"], "got {:?}", out);
}

// ---------------------------------------------------------------------------
// sed backreferences \1 in replacement
// ---------------------------------------------------------------------------

#[test]
fn sed_backreference_swap_words() {
    let mut s = shell_with_files(serde_json::json!({
        "f.txt": "hello world"
    }));
    let (out, code, _) = s.run_line(r"sed -E 's/(\w+) (\w+)/\2 \1/' f.txt");
    assert_eq!(code, 0);
    assert_eq!(out, "world hello\n");
}

// ---------------------------------------------------------------------------
// diff -q (brief)
// ---------------------------------------------------------------------------

#[test]
fn diff_q_reports_files_differ() {
    let mut s = shell_with_files(serde_json::json!({
        "a.txt": "hello",
        "b.txt": "world"
    }));
    let (out, code, _) = s.run_line("diff -q a.txt b.txt");
    assert_eq!(code, 1);
    assert!(
        out.contains("Files a.txt and b.txt differ"),
        "expected 'Files ... differ', got {:?}",
        out
    );
}

#[test]
fn diff_q_silent_when_same() {
    let mut s = shell_with_files(serde_json::json!({
        "a.txt": "same",
        "b.txt": "same"
    }));
    let (out, code, _) = s.run_line("diff -q a.txt b.txt");
    assert_eq!(code, 0);
    assert!(
        out.trim().is_empty(),
        "expected empty output when files are same, got {:?}",
        out
    );
}

// ---------------------------------------------------------------------------
// sed address ranges + multi-command (enhancement)
// ---------------------------------------------------------------------------

#[test]
fn sed_line_range_1_3_delete() {
    let mut s = shell_with_files(serde_json::json!({
        "f.txt": "line1\nline2\nline3\nline4\nline5\n"
    }));
    let (out, code, _) = s.run_line("sed '1,3d' f.txt");
    assert_eq!(code, 0);
    assert_eq!(out, "line4\nline5\n");
}

#[test]
fn sed_range_to_pattern_delete() {
    let mut s = shell_with_files(serde_json::json!({
        "f.txt": "start\nfoo\nend\nafter\n"
    }));
    // delete from line 2 to "end" match
    let (out, code, _) = s.run_line("sed '2,/end/d' f.txt");
    assert_eq!(code, 0);
    assert_eq!(out, "start\nafter\n");
}

#[test]
fn sed_multi_e_both_substitutions() {
    let mut s = shell_with_files(serde_json::json!({
        "f.txt": "ac\n"
    }));
    let (out, code, _) = s.run_line("sed -e 's/a/b/' -e 's/c/d/' f.txt");
    assert_eq!(code, 0);
    assert_eq!(out, "bd\n");
}

#[test]
fn sed_dollar_delete_last_line() {
    let mut s = shell_with_files(serde_json::json!({
        "f.txt": "first\nsecond\nlast\n"
    }));
    let (out, code, _) = s.run_line("sed '$d' f.txt");
    assert_eq!(code, 0);
    assert_eq!(out, "first\nsecond\n");
}

#[test]
fn sed_semicolon_multi_command() {
    let mut s = shell_with_files(serde_json::json!({
        "f.txt": "ac\n"
    }));
    let (out, code, _) = s.run_line("sed 's/a/b/;s/c/d/' f.txt");
    assert_eq!(code, 0);
    assert_eq!(out, "bd\n");
}

// ---------------------------------------------------------------------------
// xargs
// ---------------------------------------------------------------------------

#[test]
fn xargs_basic_echo() {
    let mut s = shell();
    // echo -e "a\nb\nc" | xargs echo -> "a b c\n"
    let (out, code, _) = s.run_line("printf 'a\\nb\\nc\\n' | xargs echo");
    assert_eq!(code, 0);
    assert_eq!(out, "a b c\n");
}

#[test]
fn xargs_n1_one_arg_per_invocation() {
    let mut s = shell();
    // echo -e "a\nb\nc" | xargs -n 1 echo -> "a\nb\nc\n"
    let (out, code, _) = s.run_line("printf 'a\\nb\\nc\\n' | xargs -n 1 echo");
    assert_eq!(code, 0);
    assert_eq!(out, "a\nb\nc\n");
}

#[test]
fn xargs_replace_i() {
    let mut s = shell();
    // echo -e "a\nb" | xargs -I {} echo hello {} -> "hello a\nhello b\n"
    let (out, code, _) = s.run_line("printf 'a\\nb\\n' | xargs -I {} echo hello {}");
    assert_eq!(code, 0);
    assert_eq!(out, "hello a\nhello b\n");
}

#[test]
fn xargs_no_stdin_runs_command_no_extra_args() {
    let mut s = shell();
    // xargs with empty stdin should run command with no extra args
    let (out, code, _) = s.run_line("printf '' | xargs echo");
    assert_eq!(code, 0);
    assert_eq!(out, "\n");
}

#[test]
fn xargs_default_command_is_echo() {
    let mut s = shell();
    let (out, code, _) = s.run_line("printf 'hello\\n' | xargs");
    assert_eq!(code, 0);
    assert_eq!(out, "hello\n");
}
