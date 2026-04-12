use super::*;

// ---------------------------------------------------------------------------
// Extended globs: ?(pat), *(pat), +(pat), @(pat), !(pat)
// ---------------------------------------------------------------------------

#[test]
fn extglob_question_zero_match() {
    let _s = shell();
    assert!(Shell::glob_match_str("?(foo)bar", "bar"));
}

#[test]
fn extglob_question_one_match() {
    let _s = shell();
    assert!(Shell::glob_match_str("?(foo)bar", "foobar"));
}

#[test]
fn extglob_question_no_double() {
    let _s = shell();
    assert!(!Shell::glob_match_str("?(foo)bar", "foofoobar"));
}

#[test]
fn extglob_star_zero_match() {
    let _s = shell();
    assert!(Shell::glob_match_str("*(foo)bar", "bar"));
}

#[test]
fn extglob_star_multi_match() {
    let _s = shell();
    assert!(Shell::glob_match_str("*(foo)bar", "foofoobar"));
    assert!(Shell::glob_match_str("*(foo)bar", "foobar"));
}

#[test]
fn extglob_plus_one_match() {
    let _s = shell();
    assert!(Shell::glob_match_str("+(foo)bar", "foobar"));
}

#[test]
fn extglob_plus_multi_match() {
    let _s = shell();
    assert!(Shell::glob_match_str("+(foo)bar", "foofoobar"));
}

#[test]
fn extglob_plus_no_zero() {
    let _s = shell();
    assert!(!Shell::glob_match_str("+(foo)bar", "bar"));
}

#[test]
fn extglob_at_one_match() {
    let _s = shell();
    assert!(Shell::glob_match_str("@(foo|baz)bar", "foobar"));
    assert!(Shell::glob_match_str("@(foo|baz)bar", "bazbar"));
}

#[test]
fn extglob_at_no_zero() {
    let _s = shell();
    assert!(!Shell::glob_match_str("@(foo|baz)bar", "bar"));
}

#[test]
fn extglob_not_match() {
    let _s = shell();
    assert!(Shell::glob_match_str("!(foo)", "bar"));
    assert!(Shell::glob_match_str("!(foo)", "baz"));
    assert!(!Shell::glob_match_str("!(foo)", "foo"));
}

#[test]
fn extglob_not_with_alternatives() {
    let _s = shell();
    assert!(!Shell::glob_match_str("!(foo|bar)", "foo"));
    assert!(!Shell::glob_match_str("!(foo|bar)", "bar"));
    assert!(Shell::glob_match_str("!(foo|bar)", "baz"));
}

#[test]
fn extglob_alternatives_with_pipe() {
    let _s = shell();
    assert!(Shell::glob_match_str("@(cat|dog)", "cat"));
    assert!(Shell::glob_match_str("@(cat|dog)", "dog"));
    assert!(!Shell::glob_match_str("@(cat|dog)", "fish"));
}

#[test]
fn extglob_combined_with_regular_glob() {
    let _s = shell();
    assert!(Shell::glob_match_str("*.@(txt|md)", "readme.txt"));
    assert!(Shell::glob_match_str("*.@(txt|md)", "readme.md"));
    assert!(!Shell::glob_match_str("*.@(txt|md)", "readme.rs"));
}

// ---------------------------------------------------------------------------
// Glob expansion on filesystem
// ---------------------------------------------------------------------------

#[test]
fn glob_expansion_matches_files() {
    let mut s = shell_with_files(serde_json::json!({
        "a.txt": "aa",
        "b.txt": "bb",
        "c.rs": "cc"
    }));
    let (out, code, _) = s.run_line("echo *.txt");
    assert_eq!(code, 0);
    assert_eq!(out, "a.txt b.txt\n");
}

// ---------------------------------------------------------------------------
// set -f (noglob)
// ---------------------------------------------------------------------------

#[test]
fn set_noglob_disables_glob_expansion() {
    let mut s = shell();
    s.run_line("mkdir /tmp/glob_test");
    s.run_line("touch /tmp/glob_test/foo.txt");
    s.run_line("touch /tmp/glob_test/bar.txt");
    s.run_line("cd /tmp/glob_test");
    let (out_before, _) = s.run_script("echo *.txt");
    assert!(
        out_before.contains("foo.txt") || out_before.contains("bar.txt"),
        "got: {:?}",
        out_before
    );
    let (out_after, _) = s.run_script("set -f; echo *.txt");
    assert_eq!(out_after, "*.txt\n");
}

#[test]
fn set_noglob_unset() {
    let mut s = shell();
    assert!(!s.exec.opts.noglob);
    s.run_line("set -f");
    assert!(s.exec.opts.noglob);
    s.run_line("set +f");
    assert!(!s.exec.opts.noglob);
}
