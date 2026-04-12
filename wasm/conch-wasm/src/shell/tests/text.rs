use super::*;

// ---------------------------------------------------------------------------
// echo
// ---------------------------------------------------------------------------

#[test]
fn echo_e_interprets_escapes() {
    let mut s = shell();
    let (out, code, _) = s.run_line("echo -e 'a\\nb'");
    assert_eq!(code, 0);
    assert_eq!(out, "a\nb\n");
}

#[test]
fn echo_n_omits_nothing_extra() {
    let mut s = shell();
    let (out, code, _) = s.run_line("echo -n hello");
    assert_eq!(code, 0);
    assert_eq!(out, "hello");
}

#[test]
fn echo_n_suppresses_trailing_newline() {
    let mut s = shell();
    let (out, code, _) = s.run_line("echo -n hello");
    assert_eq!(code, 0);
    assert_eq!(out, "hello");
    let (out2, _, _) = s.run_line("echo hello");
    assert_eq!(out2, "hello\n");
}

// ---------------------------------------------------------------------------
// printf
// ---------------------------------------------------------------------------

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
    assert_eq!(out, "line1\nline2");
}

#[test]
fn printf_percent_literal() {
    let mut s = shell();
    let (out, code, _) = s.run_line("printf '100%%'");
    assert_eq!(code, 0);
    assert_eq!(out, "100%");
}

#[test]
fn printf_no_args_fails() {
    let mut s = shell();
    let (_, code, _) = s.run_line("printf");
    assert_eq!(code, 2);
}

// ---------------------------------------------------------------------------
// head / tail
// ---------------------------------------------------------------------------

#[test]
fn head_tail_take_lines_from_file() {
    let mut s = shell_with_files(serde_json::json!({
        "rows.txt": "a\nb\nc\n"
    }));
    let (h, c1, _) = s.run_line("head -n 2 rows.txt");
    assert_eq!(c1, 0);
    assert_eq!(h, "a\nb\n");
    let (t, c2, _) = s.run_line("tail -n 1 rows.txt");
    assert_eq!(c2, 0);
    assert_eq!(t, "c\n");
}

#[test]
fn head_from_stdin() {
    let mut s = shell();
    let (out, code, _) = s.run_line("echo 'a\nb\nc\nd\ne' | head -n 2");
    assert_eq!(code, 0);
    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(lines.len(), 2);
}

#[test]
fn tail_from_stdin() {
    let mut s = shell();
    let (out, code, _) = s.run_line("echo 'a\nb\nc' | tail -n 1");
    assert_eq!(code, 0);
    assert_eq!(out, "c\n");
}

#[test]
fn head_dash_n_shorthand() {
    let mut s = shell_with_files(serde_json::json!({"f.txt": "a\nb\nc\nd\ne"}));
    let (out, code, _) = s.run_line("head -3 f.txt");
    assert_eq!(code, 0);
    assert_eq!(out, "a\nb\nc\n");
}

#[test]
fn tail_dash_n_shorthand() {
    let mut s = shell_with_files(serde_json::json!({"f.txt": "a\nb\nc\nd\ne"}));
    let (out, code, _) = s.run_line("tail -2 f.txt");
    assert_eq!(code, 0);
    assert_eq!(out, "d\ne\n");
}

// ---------------------------------------------------------------------------
// wc
// ---------------------------------------------------------------------------

#[test]
fn wc_counts_file() {
    let mut s = shell_with_files(serde_json::json!({
        "w.txt": "one two\nthree\n"
    }));
    let (out, code, _) = s.run_line("wc w.txt");
    assert_eq!(code, 0);
    assert!(out.contains("w.txt"), "got {:?}", out);
    assert!(out.contains("3"), "expected 3 words, got {:?}", out);
}

#[test]
fn wc_from_stdin() {
    let mut s = shell();
    let (out, code, _) = s.run_line("echo 'hello world' | wc");
    assert_eq!(code, 0);
    assert!(out.contains("2"), "expected 2 words, got {:?}", out);
}

#[test]
fn wc_l_counts_newlines() {
    let mut s = shell_with_files(serde_json::json!({"f.txt": "no trailing newline"}));
    let (out, _, _) = s.run_line("wc -l f.txt");
    assert!(
        out.contains("0"),
        "expected 0 lines for no-newline file: {out}"
    );
}

// ---------------------------------------------------------------------------
// grep
// ---------------------------------------------------------------------------

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
    assert_eq!(out, "1:one\n3:one\n");
    let (cnt, c2, _) = s.run_line("grep -c one g.txt");
    assert_eq!(c2, 0);
    assert_eq!(cnt, "2\n");
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
    assert_eq!(out.lines().count(), 2);
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
fn grep_stdin_pipe() {
    let mut s = shell();
    let (out, code, _) = s.run_line("echo 'foo bar baz' | grep bar");
    assert_eq!(code, 0);
    assert!(out.contains("bar"));
}

// ---------------------------------------------------------------------------
// sort
// ---------------------------------------------------------------------------

#[test]
fn sort_sorts_and_numeric() {
    let mut s = shell_with_files(serde_json::json!({
        "letters.txt": "c\na\nb\n",
        "nums.txt": "10\n2\n"
    }));
    let (out, c1, _) = s.run_line("sort letters.txt");
    assert_eq!(c1, 0);
    assert_eq!(out, "a\nb\nc\n");
    let (nout, c2, _) = s.run_line("sort -n nums.txt");
    assert_eq!(c2, 0);
    assert_eq!(nout, "2\n10\n");
}

#[test]
fn sort_reverse_order() {
    let mut s = shell_with_files(serde_json::json!({
        "rev.txt": "a\nb\nc\n"
    }));
    let (out, code, _) = s.run_line("sort -r rev.txt");
    assert_eq!(code, 0);
    assert_eq!(out, "c\nb\na\n");
}

#[test]
fn sort_numeric_then_reverse() {
    let mut s = shell_with_files(serde_json::json!({
        "nums.txt": "2\n10\n1\n"
    }));
    let (out, code, _) = s.run_line("sort -n -r nums.txt");
    assert_eq!(code, 0);
    assert_eq!(out, "10\n2\n1\n");
}

#[test]
fn sort_with_delimiter_and_key() {
    let mut s = shell_with_files(serde_json::json!({
        "data.csv": "name,score\nalice,95\nbob,82\ncharlie,91"
    }));
    let (out, code, _) = s.run_line("sort -t, -k2 -rn data.csv");
    assert_eq!(code, 0);
    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(lines[0], "alice,95", "highest score first: {out}");
    assert_eq!(lines[1], "charlie,91");
    assert_eq!(lines[2], "bob,82");
    assert_eq!(lines[3], "name,score");
}

#[test]
fn sort_with_key_whitespace_default() {
    let mut s = shell_with_files(serde_json::json!({
        "data.txt": "alice 95\nbob 82\ncharlie 91"
    }));
    let (out, code, _) = s.run_line("sort -k2 -rn data.txt");
    assert_eq!(code, 0);
    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(lines[0], "alice 95");
    assert_eq!(lines[1], "charlie 91");
    assert_eq!(lines[2], "bob 82");
}

// ---------------------------------------------------------------------------
// uniq
// ---------------------------------------------------------------------------

#[test]
fn uniq_dedupes_and_counts() {
    let mut s = shell_with_files(serde_json::json!({
        "u.txt": "a\na\nb\n"
    }));
    let (out, c1, _) = s.run_line("uniq u.txt");
    assert_eq!(c1, 0);
    assert_eq!(out, "a\nb\n");
    let (cout, c2, _) = s.run_line("uniq -c u.txt");
    assert_eq!(c2, 0);
    assert_eq!(cout, "      2 a\n      1 b\n");
}

// ---------------------------------------------------------------------------
// cut
// ---------------------------------------------------------------------------

#[test]
fn cut_fields() {
    let mut s = shell_with_files(serde_json::json!({
        "csv.txt": "x,y\np,q\n"
    }));
    let (out, code, _) = s.run_line("cut -d, -f2 csv.txt");
    assert_eq!(code, 0);
    assert_eq!(out, "y\nq\n");
}

#[test]
fn cut_range_fields() {
    let mut s = shell_with_files(serde_json::json!({
        "data.txt": "a,b,c\nx,y,z"
    }));
    let (out, code, _) = s.run_line("cut -d, -f1,3 data.txt");
    assert_eq!(code, 0);
    assert!(out.contains("a") && out.contains("c"), "got {:?}", out);
}

// ---------------------------------------------------------------------------
// tr
// ---------------------------------------------------------------------------

#[test]
fn tr_substitutes_stdin() {
    let mut s = shell();
    let (out, code, _) = s.run_line("echo hello | tr h H");
    assert_eq!(code, 0);
    assert_eq!(out, "Hello\n");
}

#[test]
fn tr_delete_chars_from_stdin() {
    let mut s = shell();
    let (out, code, _) = s.run_line("echo hello | tr -d l");
    assert_eq!(code, 0);
    assert_eq!(out, "heo\n");
}

#[test]
fn tr_from_stdin_uppercase() {
    let mut s = shell();
    let (_out, code, _) = s.run_line("echo hello | tr a-z A-Z");
    assert_eq!(code, 0);
}

// ---------------------------------------------------------------------------
// rev / tac
// ---------------------------------------------------------------------------

#[test]
fn rev_reverses_file_lines() {
    let mut s = shell_with_files(serde_json::json!({ "r.txt": "abc\nxy\n" }));
    let (out, code, _) = s.run_line("rev r.txt");
    assert_eq!(code, 0);
    assert_eq!(out, "cba\nyx\n");
}

#[test]
fn rev_from_stdin() {
    let mut s = shell();
    let (out, code, _) = s.run_line("echo abc | rev");
    assert_eq!(code, 0);
    assert_eq!(out, "cba\n");
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
    assert_eq!(out, "only\n");
}

#[test]
fn tac_missing_file_fails() {
    let mut s = shell();
    let (_, code, _) = s.run_line("tac nowhere.txt");
    assert_eq!(code, 1);
}

#[test]
fn tac_empty_file() {
    let mut s = shell();
    s.run_line("touch empty.txt");
    let (out, code, _) = s.run_line("tac empty.txt");
    assert_eq!(code, 0);
    assert!(out.is_empty());
}

// ---------------------------------------------------------------------------
// wc multi-file total
// ---------------------------------------------------------------------------

#[test]
fn wc_multi_file_shows_total() {
    let mut s = shell_with_files(serde_json::json!({
        "f1.txt": "a\nb\n",
        "f2.txt": "c\n"
    }));
    let (out, code, _) = s.run_line("wc f1.txt f2.txt");
    assert_eq!(code, 0);
    let last_line = out.lines().last().unwrap_or("");
    assert!(
        last_line.contains("total"),
        "last line should contain 'total', got: {:?}",
        out
    );
}

// ---------------------------------------------------------------------------
// uniq -d / -u
// ---------------------------------------------------------------------------

#[test]
fn uniq_d_only_duplicates() {
    let mut s = shell_with_files(serde_json::json!({
        "u.txt": "a\na\nb\nc\nc"
    }));
    let (out, code, _) = s.run_line("uniq -d u.txt");
    assert_eq!(code, 0);
    assert_eq!(out, "a\nc\n");
}

#[test]
fn uniq_u_only_unique() {
    let mut s = shell_with_files(serde_json::json!({
        "u.txt": "a\na\nb\nc\nc"
    }));
    let (out, code, _) = s.run_line("uniq -u u.txt");
    assert_eq!(code, 0);
    assert_eq!(out, "b\n");
}

// ---------------------------------------------------------------------------
// seq -s / -w / float
// ---------------------------------------------------------------------------

#[test]
fn seq_custom_separator() {
    let mut s = shell();
    let (out, code, _) = s.run_line("seq -s ' ' 1 5");
    assert_eq!(code, 0);
    assert_eq!(out, "1 2 3 4 5\n");
}

#[test]
fn seq_equal_width_zero_pad() {
    let mut s = shell();
    let (out, code, _) = s.run_line("seq -w 1 10");
    assert_eq!(code, 0);
    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(lines[0], "01");
    assert_eq!(lines[9], "10");
}

#[test]
fn seq_float_support() {
    let mut s = shell();
    let (out, code, _) = s.run_line("seq 0.5 0.5 2.0");
    assert_eq!(code, 0);
    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(lines.len(), 4, "expected 4 lines, got {:?}", lines);
    assert_eq!(lines[0], "0.5");
    assert_eq!(lines[1], "1.0");
    assert_eq!(lines[2], "1.5");
    assert_eq!(lines[3], "2.0");
}

// ---------------------------------------------------------------------------
// seq
// ---------------------------------------------------------------------------

#[test]
fn seq_inclusive_range() {
    let mut s = shell();
    let (out, code, _) = s.run_line("seq 2 4");
    assert_eq!(code, 0);
    assert_eq!(out, "2\n3\n4\n");
}

#[test]
fn seq_single_arg() {
    let mut s = shell();
    let (out, code, _) = s.run_line("seq 3");
    assert_eq!(code, 0);
    assert_eq!(out, "1\n2\n3\n");
}

#[test]
fn seq_with_increment() {
    let mut s = shell();
    let (out, code, _) = s.run_line("seq 1 2 7");
    assert_eq!(code, 0);
    assert_eq!(out, "1\n3\n5\n7\n");
}

// ---------------------------------------------------------------------------
// nl
// ---------------------------------------------------------------------------

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
    assert_eq!(code, 1);
}

#[test]
fn nl_from_stdin() {
    let mut s = shell();
    let (out, code, _) = s.run_line("echo -e 'x\\ny' | nl");
    assert_eq!(code, 0);
    assert!(out.contains("1"), "got {:?}", out);
}

#[test]
fn nl_empty_file() {
    let mut s = shell();
    s.run_line("touch empty.txt");
    let (out, code, _) = s.run_line("nl empty.txt");
    assert_eq!(code, 0);
    assert!(out.is_empty());
}

// ---------------------------------------------------------------------------
// paste
// ---------------------------------------------------------------------------

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
    assert_eq!(code, 1);
}

#[test]
fn paste_single_file() {
    let mut s = shell_with_files(serde_json::json!({"a.txt": "line1\nline2"}));
    let (out, code, _) = s.run_line("paste a.txt");
    assert_eq!(code, 0);
    assert_eq!(out, "line1\nline2\n");
}

// ---------------------------------------------------------------------------
// sed (literal matching, not in-place — see also transform.rs)
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// grep -A / -B / -C context lines
// ---------------------------------------------------------------------------

#[test]
fn grep_a_shows_lines_after_match() {
    let mut s = shell_with_files(serde_json::json!({
        "f.txt": "line1\nmatch\nafter1\nafter2\nline5"
    }));
    let (out, code, _) = s.run_line("grep -A 1 match f.txt");
    assert_eq!(code, 0);
    assert!(out.contains("match"), "got {:?}", out);
    assert!(out.contains("after1"), "got {:?}", out);
    assert!(
        !out.contains("after2"),
        "should not include after2: {:?}",
        out
    );
}

#[test]
fn grep_b_shows_lines_before_match() {
    let mut s = shell_with_files(serde_json::json!({
        "f.txt": "before1\nbefore2\nmatch\nafter1"
    }));
    let (out, code, _) = s.run_line("grep -B 1 match f.txt");
    assert_eq!(code, 0);
    assert!(out.contains("before2"), "got {:?}", out);
    assert!(out.contains("match"), "got {:?}", out);
    assert!(
        !out.contains("before1"),
        "should not include before1: {:?}",
        out
    );
}

#[test]
fn grep_c_upper_shows_context_both_sides() {
    let mut s = shell_with_files(serde_json::json!({
        "f.txt": "a\nb\nmatch\nd\ne"
    }));
    let (out, code, _) = s.run_line("grep -C 1 match f.txt");
    assert_eq!(code, 0);
    assert!(out.contains("b"), "got {:?}", out);
    assert!(out.contains("match"), "got {:?}", out);
    assert!(out.contains("d"), "got {:?}", out);
    assert!(!out.contains("a\n"), "should not include 'a': {:?}", out);
    assert!(!out.contains("e"), "should not include 'e': {:?}", out);
}

#[test]
fn grep_context_separator_between_groups() {
    let mut s = shell_with_files(serde_json::json!({
        "f.txt": "a\nmatch1\nb\nc\nd\nmatch2\ne"
    }));
    let (out, code, _) = s.run_line("grep -A 0 match f.txt");
    assert_eq!(code, 0);
    // When context is used but groups are disjoint, a "--" separator appears
    assert!(
        out.contains("--"),
        "expected -- separator between groups: {:?}",
        out
    );
}

// ---------------------------------------------------------------------------
// sed (literal matching, not in-place — see also transform.rs)
// ---------------------------------------------------------------------------

#[test]
fn sed_is_literal_not_regex() {
    let mut s = shell_with_files(serde_json::json!({"f.txt": "hello123world"}));
    let (out, _, _) = s.run_line("sed 's/123//' f.txt");
    assert_eq!(out, "helloworld\n");
}

// ---------------------------------------------------------------------------
// printf format extensions
// ---------------------------------------------------------------------------

#[test]
fn printf_hex_lowercase() {
    let mut s = shell();
    let (out, code, _) = s.run_line("printf '%x' 255");
    assert_eq!(code, 0);
    assert_eq!(out, "ff");
}

#[test]
fn printf_hex_uppercase() {
    let mut s = shell();
    let (out, code, _) = s.run_line("printf '%X' 255");
    assert_eq!(code, 0);
    assert_eq!(out, "FF");
}

#[test]
fn printf_octal() {
    let mut s = shell();
    let (out, code, _) = s.run_line("printf '%o' 8");
    assert_eq!(code, 0);
    assert_eq!(out, "10");
}

#[test]
fn printf_zero_pad_decimal() {
    let mut s = shell();
    let (out, code, _) = s.run_line("printf '%02d' 5");
    assert_eq!(code, 0);
    assert_eq!(out, "05");
}

#[test]
fn printf_left_align_string() {
    let mut s = shell();
    let (out, code, _) = s.run_line("printf '%-10s|' hello");
    assert_eq!(code, 0);
    assert_eq!(out, "hello     |");
}

#[test]
fn printf_fixed_float() {
    let mut s = shell();
    let (out, code, _) = s.run_line("printf '%f' 3.14");
    assert_eq!(code, 0);
    assert!(out.starts_with("3.14"), "got {:?}", out);
}

#[test]
fn printf_scientific_notation() {
    let mut s = shell();
    let (out, code, _) = s.run_line("printf '%e' 314.0");
    assert_eq!(code, 0);
    assert!(out.contains('e') || out.contains('E'), "got {:?}", out);
}

#[test]
fn printf_width_right_align_decimal() {
    let mut s = shell();
    let (out, code, _) = s.run_line("printf '%5d' 42");
    assert_eq!(code, 0);
    assert_eq!(out, "   42");
}

// ---------------------------------------------------------------------------
// column command
// ---------------------------------------------------------------------------

#[test]
fn column_t_aligns_whitespace_columns() {
    let mut s = shell_with_files(serde_json::json!({
        "data.txt": "name age\nalice 30\nbob 25\n"
    }));
    let (out, code, _) = s.run_line("column -t data.txt");
    assert_eq!(code, 0);
    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(lines.len(), 3);
    // All lines should have same length (padded to column widths)
    let len0 = lines[0].len();
    let len1 = lines[1].len();
    let len2 = lines[2].len();
    assert_eq!(len0, len1, "lines 0 and 1 differ in length: {:?}", lines);
    assert_eq!(len1, len2, "lines 1 and 2 differ in length: {:?}", lines);
    assert!(out.contains("name"), "got {:?}", out);
    assert!(out.contains("alice"), "got {:?}", out);
}

#[test]
fn column_s_delimiter_csv() {
    let mut s = shell_with_files(serde_json::json!({
        "data.csv": "name,age\nalice,30\nbob,25\n"
    }));
    let (out, code, _) = s.run_line("column -s , -t data.csv");
    assert_eq!(code, 0);
    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(lines.len(), 3);
    assert!(out.contains("name"), "got {:?}", out);
    assert!(out.contains("alice"), "got {:?}", out);
    // Columns should be aligned (same line lengths)
    let len0 = lines[0].len();
    let len1 = lines[1].len();
    assert_eq!(len0, len1, "lines should be same length: {:?}", lines);
}

#[test]
fn column_t_from_stdin() {
    let mut s = shell();
    let (out, code, _) = s.run_line("printf 'a b\\nc d\\n' | column -t");
    assert_eq!(code, 0);
    assert!(out.contains('a'), "got {:?}", out);
    assert!(out.contains('b'), "got {:?}", out);
}
