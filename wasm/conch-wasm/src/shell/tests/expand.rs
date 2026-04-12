use super::*;

// ---------------------------------------------------------------------------
// Word expansion via word-parser
// ---------------------------------------------------------------------------

#[test]
fn expand_via_word_parser_variables() {
    let mut s = shell();
    s.run_line("export FOO=bar");
    let (out, _, _) = s.run_line("echo $FOO");
    assert_eq!(out, "bar\n");
}

#[test]
fn expand_via_word_parser_command_subst() {
    let mut s = shell();
    let (out, _, _) = s.run_line("echo $(echo hello)");
    assert_eq!(out, "hello\n");
}

#[test]
fn expand_via_word_parser_tilde() {
    let mut s = shell();
    let (out, _, _) = s.run_line("echo ~");
    assert_eq!(out, format!("{}\n", s.ident.home));
}

#[test]
fn expand_via_word_parser_brace_expr() {
    let mut s = shell();
    s.run_line("export X=hello");
    let (out, _, _) = s.run_line("echo ${X}world");
    assert_eq!(out, "helloworld\n");
}

#[test]
fn expand_via_word_parser_special_params() {
    let mut s = shell();
    let (out, _, _) = s.run_line("echo $$");
    let pid: u32 = out.trim().parse().expect("$$ should expand via new path");
    assert_eq!(pid, s.procs.shell_pid());
}

// ---------------------------------------------------------------------------
// Nameref (declare -n)
// ---------------------------------------------------------------------------

#[test]
fn nameref_basic() {
    let mut s = shell();
    s.run_line("x=hello");
    s.run_line("declare -n ref=x");
    let (out, _, _) = s.run_line("echo $ref");
    assert_eq!(out, "hello\n");
}

#[test]
fn nameref_write_through() {
    let mut s = shell();
    s.run_line("x=old");
    s.run_line("declare -n ref=x");
    s.run_line("ref=new");
    let (out, _, _) = s.run_line("echo $x");
    assert_eq!(out, "new\n");
}

#[test]
fn nameref_chain() {
    let mut s = shell();
    s.run_line("val=42");
    s.run_line("declare -n ref1=val");
    s.run_line("declare -n ref2=ref1");
    let (out, _, _) = s.run_line("echo $ref2");
    assert_eq!(out, "42\n");
}

// ---------------------------------------------------------------------------
// ${!var} indirect expansion
// ---------------------------------------------------------------------------

#[test]
fn indirect_expansion() {
    let mut s = shell();
    s.run_line("target=hello");
    s.run_line("ptr=target");
    let (out, _, _) = s.run_line("echo ${!ptr}");
    assert_eq!(out, "hello\n");
}

#[test]
fn indirect_expansion_empty() {
    let mut s = shell();
    s.run_line("ptr=nonexistent");
    let (out, _, _) = s.run_line("echo ${!ptr}");
    assert_eq!(out, "\n");
}

#[test]
fn indirect_expansion_array_indices() {
    let mut s = shell();
    s.run_line("arr=(a b c d)");
    let (out, _, _) = s.run_line("echo ${!arr[@]}");
    assert_eq!(out, "0 1 2 3\n");
}
