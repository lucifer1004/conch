use super::*;

#[test]
fn echo_e_interprets_escapes() {
    let mut s = shell();
    let (out, code, _) = s.run_line("echo -e 'a\\nb'");
    assert_eq!(code, 0);
    assert!(out.contains('\n'), "expected newline, got {:?}", out);
    assert!(out.starts_with('a'), "got {:?}", out);
}

#[test]
fn echo_n_omits_nothing_extra() {
    let mut s = shell();
    // -n in real bash suppresses trailing newline; in our impl it's a flag
    let (out, code, _) = s.run_line("echo -n hello");
    assert_eq!(code, 0);
    assert_eq!(out, "hello");
}

#[test]
fn head_tail_take_lines_from_file() {
    let mut s = shell_with_files(serde_json::json!({
        "rows.txt": "a\nb\nc\n"
    }));
    let (h, c1, _) = s.run_line("head -n 2 rows.txt");
    assert_eq!(c1, 0);
    assert_eq!(h, "a\nb");
    let (t, c2, _) = s.run_line("tail -n 1 rows.txt");
    assert_eq!(c2, 0);
    assert_eq!(t, "c");
}

#[test]
fn wc_counts_file() {
    let mut s = shell_with_files(serde_json::json!({
        "w.txt": "one two\nthree\n"
    }));
    let (out, code, _) = s.run_line("wc w.txt");
    assert_eq!(code, 0);
    // Should contain line count, word count, byte count, filename
    assert!(out.contains("w.txt"), "got {:?}", out);
    assert!(out.contains("3"), "expected 3 words, got {:?}", out);
}

#[test]
fn grep_file_filter() {
    let mut s = shell_with_files(serde_json::json!({
        "lines.txt": "alpha\nbeta\nalpha2\n"
    }));
    let (out, code, _) = s.run_line("grep alpha lines.txt");
    assert_eq!(code, 0);
    assert!(out.contains("alpha"));
    assert!(!out.contains("beta"));
}

#[test]
fn grep_line_numbers_and_count() {
    let mut s = shell_with_files(serde_json::json!({
        "g.txt": "one\ntwo\none\n"
    }));
    let (out, c1, _) = s.run_line("grep -n one g.txt");
    assert_eq!(c1, 0);
    assert!(out.contains("1") && out.contains("3"), "got {:?}", out);
    let (cnt, c2, _) = s.run_line("grep -c one g.txt");
    assert_eq!(c2, 0);
    assert!(cnt.contains('2'), "got {:?}", cnt);
}

#[test]
fn grep_no_match_exits_one() {
    let mut s = shell_with_files(serde_json::json!({
        "only.txt": "foo\nbar\n"
    }));
    let (out, code, _) = s.run_line("grep nomatch only.txt");
    assert_eq!(code, 1);
    assert!(out.is_empty());
}

#[test]
fn grep_multiple_files_shows_filename_prefix() {
    let mut s = shell_with_files(serde_json::json!({
        "ga.txt": "hit\n",
        "gb.txt": "hit\n"
    }));
    let (out, code, _) = s.run_line("grep hit ga.txt gb.txt");
    assert_eq!(code, 0);
    assert!(out.contains("ga.txt"), "got {:?}", out);
    assert!(out.contains("gb.txt"), "got {:?}", out);
    assert!(out.lines().count() >= 2);
}

#[test]
fn grep_ignore_case() {
    let mut s = shell_with_files(serde_json::json!({
        "mix.txt": "Hello\n"
    }));
    let (out, code, _) = s.run_line("grep -i hello mix.txt");
    assert_eq!(code, 0);
    assert!(out.contains("Hello"), "got {:?}", out);
}

#[test]
fn grep_v_inverts_match() {
    let mut s = shell_with_files(serde_json::json!({
        "data.txt": "keep\ndrop\nkeep2\n"
    }));
    let (out, code, _) = s.run_line("grep -v drop data.txt");
    assert_eq!(code, 0);
    assert!(out.contains("keep"), "got {:?}", out);
    assert!(!out.contains("drop"), "got {:?}", out);
}

#[test]
fn sort_sorts_and_numeric() {
    let mut s = shell_with_files(serde_json::json!({
        "letters.txt": "c\na\nb\n",
        "nums.txt": "10\n2\n"
    }));
    let (out, c1, _) = s.run_line("sort letters.txt");
    assert_eq!(c1, 0);
    assert_eq!(out, "a\nb\nc");
    let (nout, c2, _) = s.run_line("sort -n nums.txt");
    assert_eq!(c2, 0);
    assert_eq!(nout, "2\n10");
}

#[test]
fn sort_reverse_order() {
    let mut s = shell_with_files(serde_json::json!({
        "rev.txt": "a\nb\nc\n"
    }));
    let (out, code, _) = s.run_line("sort -r rev.txt");
    assert_eq!(code, 0);
    assert_eq!(out, "c\nb\na");
}

#[test]
fn sort_numeric_then_reverse() {
    let mut s = shell_with_files(serde_json::json!({
        "nums.txt": "2\n10\n1\n"
    }));
    let (out, code, _) = s.run_line("sort -n -r nums.txt");
    assert_eq!(code, 0);
    assert_eq!(out, "10\n2\n1");
}

#[test]
fn uniq_dedupes_and_counts() {
    let mut s = shell_with_files(serde_json::json!({
        "u.txt": "a\na\nb\n"
    }));
    let (out, c1, _) = s.run_line("uniq u.txt");
    assert_eq!(c1, 0);
    assert_eq!(out, "a\nb");
    let (cout, c2, _) = s.run_line("uniq -c u.txt");
    assert_eq!(c2, 0);
    assert!(cout.contains("2") && cout.contains("a"), "got {:?}", cout);
}

#[test]
fn cut_fields() {
    let mut s = shell_with_files(serde_json::json!({
        "csv.txt": "x,y\np,q\n"
    }));
    let (out, code, _) = s.run_line("cut -d, -f2 csv.txt");
    assert_eq!(code, 0);
    assert_eq!(out, "y\nq");
}

#[test]
fn tr_substitutes_stdin() {
    let mut s = shell();
    let (out, code, _) = s.run_line("echo hello | tr h H");
    assert_eq!(code, 0);
    assert_eq!(out, "Hello");
}

#[test]
fn tr_delete_chars_from_stdin() {
    let mut s = shell();
    let (out, code, _) = s.run_line("echo hello | tr -d l");
    assert_eq!(code, 0);
    assert_eq!(out, "heo");
}

#[test]
fn rev_reverses_file_lines() {
    let mut s = shell_with_files(serde_json::json!({ "r.txt": "abc\nxy\n" }));
    let (out, code, _) = s.run_line("rev r.txt");
    assert_eq!(code, 0);
    assert_eq!(out, "cba\nyx");
}

#[test]
fn seq_inclusive_range() {
    let mut s = shell();
    let (out, code, _) = s.run_line("seq 2 4");
    assert_eq!(code, 0);
    assert_eq!(out, "2\n3\n4");
}

#[test]
fn tac_reverses_lines() {
    let mut s = shell_with_files(serde_json::json!({ "lines.txt": "a\nb\nc" }));
    let (out, code, _) = s.run_line("tac lines.txt");
    assert_eq!(code, 0);
    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(lines, vec!["c", "b", "a"]);
}

#[test]
fn tac_single_line_unchanged() {
    let mut s = shell_with_files(serde_json::json!({ "one.txt": "only" }));
    let (out, code, _) = s.run_line("tac one.txt");
    assert_eq!(code, 0);
    assert_eq!(out, "only");
}

#[test]
fn tac_missing_file_fails() {
    let mut s = shell();
    let (_, code, _) = s.run_line("tac nowhere.txt");
    assert_ne!(code, 0);
}

#[test]
fn nl_numbers_lines() {
    let mut s = shell_with_files(serde_json::json!({ "abc.txt": "alpha\nbeta\ngamma" }));
    let (out, code, _) = s.run_line("nl abc.txt");
    assert_eq!(code, 0);
    assert!(out.contains("1"), "got {:?}", out);
    assert!(out.contains("alpha"), "got {:?}", out);
    assert!(out.contains("3"), "got {:?}", out);
    assert!(out.contains("gamma"), "got {:?}", out);
}

#[test]
fn nl_missing_file_fails() {
    let mut s = shell();
    let (_, code, _) = s.run_line("nl nope.txt");
    assert_ne!(code, 0);
}

#[test]
fn nl_from_stdin() {
    let mut s = shell();
    let (out, code, _) = s.run_line("echo -e 'x\\ny' | nl");
    assert_eq!(code, 0);
    assert!(out.contains("1"), "got {:?}", out);
}

#[test]
fn paste_merges_two_files() {
    let mut s = shell_with_files(serde_json::json!({
        "a.txt": "1\n2\n3",
        "b.txt": "a\nb\nc"
    }));
    let (out, code, _) = s.run_line("paste a.txt b.txt");
    assert_eq!(code, 0);
    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(lines.len(), 3);
    assert!(
        lines[0].contains('1') && lines[0].contains('a'),
        "got {:?}",
        lines[0]
    );
}

#[test]
fn paste_custom_delimiter() {
    let mut s = shell_with_files(serde_json::json!({
        "x.txt": "A\nB",
        "y.txt": "1\n2"
    }));
    let (out, code, _) = s.run_line("paste -d , x.txt y.txt");
    assert_eq!(code, 0);
    assert!(out.contains("A,1"), "got {:?}", out);
}

#[test]
fn paste_missing_file_fails() {
    let mut s = shell();
    let (_, code, _) = s.run_line("paste ghost.txt");
    assert_ne!(code, 0);
}

#[test]
fn printf_string_substitution() {
    let mut s = shell();
    let (out, code, _) = s.run_line("printf '%s world' hello");
    assert_eq!(code, 0);
    assert_eq!(out, "hello world");
}

#[test]
fn printf_decimal_substitution() {
    let mut s = shell();
    let (out, code, _) = s.run_line("printf '%d items' 42");
    assert_eq!(code, 0);
    assert_eq!(out, "42 items");
}

#[test]
fn printf_escape_newline_tab() {
    let mut s = shell();
    let (out, code, _) = s.run_line(r#"printf "line1\nline2""#);
    assert_eq!(code, 0);
    assert!(out.contains('\n'), "expected newline in {:?}", out);
}
