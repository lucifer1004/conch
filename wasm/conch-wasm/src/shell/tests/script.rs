use super::*;

// ---------------------------------------------------------------------------
// bash (script execution)
// ---------------------------------------------------------------------------

#[test]
fn bash_runs_script_from_file() {
    let mut s = shell_with_files(serde_json::json!({
        "run.sh": { "content": "echo frombash\n", "mode": 755 }
    }));
    let (out, code, _) = s.run_line("bash run.sh");
    assert_eq!(code, 0);
    assert_eq!(out, "frombash\n");
}

#[test]
fn bash_missing_script() {
    let mut s = shell();
    let (out, code, _) = s.run_line("bash nowhere.sh");
    assert_eq!(code, 1);
    assert!(out.contains("No such file"), "got {:?}", out);
}

#[test]
fn bash_rejects_directory() {
    let mut s = shell();
    assert_eq!(s.run_line("mkdir adir").1, 0);
    let (out, code, _) = s.run_line("bash adir");
    assert_eq!(code, 1);
    assert!(out.contains("Is a directory"), "got {:?}", out);
}

#[test]
fn bash_rejects_unreadable_script() {
    let mut s = shell_with_files(serde_json::json!({
        "secret.sh": { "content": "echo no\n", "mode": 0 }
    }));
    let (out, code, _) = s.run_line("bash secret.sh");
    assert_eq!(code, 1);
    assert!(out.contains("Permission denied"), "got {:?}", out);
}

#[test]
fn sh_runs_script_like_bash() {
    let mut s = shell_with_files(serde_json::json!({
        "run.sh": { "content": "echo fromsh\n", "mode": 755 }
    }));
    let (out, code, _) = s.run_line("sh run.sh");
    assert_eq!(code, 0);
    assert_eq!(out, "fromsh\n");
}

#[test]
fn sh_missing_script() {
    let mut s = shell();
    let (out, code, _) = s.run_line("sh nope.sh");
    assert_eq!(code, 1);
    assert!(out.contains("No such file"), "got {:?}", out);
}

// ---------------------------------------------------------------------------
// bash -c
// ---------------------------------------------------------------------------

#[test]
fn bash_c_executes_string() {
    let mut s = shell();
    let (out, code, _) = s.run_line("bash -c 'echo hello from bash -c'");
    assert_eq!(code, 0);
    assert_eq!(out, "hello from bash -c\n");
}

#[test]
fn bash_c_isolated() {
    let mut s = shell();
    s.run_line("bash -c 'export LEAK=yes'");
    let (out, _, _) = s.run_line("echo ${LEAK:-unset}");
    assert_eq!(out, "unset\n", "bash -c should be isolated");
}

#[test]
fn bash_c_missing_arg() {
    let mut s = shell();
    let (out, code, _) = s.run_line("bash -c");
    assert_eq!(code, 2);
    assert!(out.contains("requires an argument"), "got: {}", out);
}

// ---------------------------------------------------------------------------
// ./script (direct execution)
// ---------------------------------------------------------------------------

#[test]
fn exec_dot_slash_runs_with_execute_bit() {
    let mut s = shell_with_files(serde_json::json!({
        "hello.sh": { "content": "echo from_exec\n", "mode": 755 }
    }));
    let (out, code, _) = s.run_line("./hello.sh");
    assert_eq!(code, 0);
    assert_eq!(out, "from_exec\n");
}

#[test]
fn exec_rejects_non_executable_file() {
    let mut s = shell_with_files(serde_json::json!({
        "no_x.sh": { "content": "echo x\n", "mode": 644 }
    }));
    let (out, code, _) = s.run_line("./no_x.sh");
    assert_eq!(code, 126);
    assert!(out.contains("Permission denied"), "got {:?}", out);
}

#[test]
fn exec_rejects_unreadable_script() {
    let mut s = shell_with_files(serde_json::json!({
        "locked.sh": { "content": "x", "mode": 0 }
    }));
    let (out, code, _) = s.run_line("./locked.sh");
    assert_eq!(code, 126);
    assert!(out.contains("Permission denied"), "got {:?}", out);
}

#[test]
fn exec_missing_script() {
    let mut s = shell();
    let (out, code, _) = s.run_line("./missing.sh");
    assert_eq!(code, 127);
    assert!(out.contains("No such file"), "got {:?}", out);
}

// ---------------------------------------------------------------------------
// source / .
// ---------------------------------------------------------------------------

#[test]
fn source_runs_script_in_current_context() {
    let mut s = shell();
    s.run_line("echo 'export MSG=hello' > setup.sh");
    s.run_line("source setup.sh");
    let (out, _, _) = s.run_line("echo $MSG");
    assert_eq!(out, "hello\n");
}

#[test]
fn dot_is_alias_for_source() {
    let mut s = shell();
    s.run_line("echo 'export X=42' > env.sh");
    s.run_line(". env.sh");
    let (out, _, _) = s.run_line("echo $X");
    assert_eq!(out, "42\n");
}

#[test]
fn source_missing_file_fails() {
    let mut s = shell();
    let (out, code, _) = s.run_line("source nope.sh");
    assert_eq!(code, 1);
    assert!(out.contains("nope.sh"), "got {:?}", out);
}

#[test]
fn source_no_args_fails() {
    let mut s = shell();
    let (out, code, _) = s.run_line("source");
    assert_eq!(code, 2);
    assert!(out.contains("filename"), "got {:?}", out);
}

#[test]
fn source_searches_path() {
    let mut s = shell();
    s.fs.set_current_user(0, 0);
    let _ = s.fs.create_dir_all("/usr/local/bin");
    s.fs.write("/usr/local/bin/helpers.sh", b"export HELPER=loaded")
        .ok();
    s.fs.set_current_user(1000, 1000);
    s.run_line("source helpers.sh");
    let (out, _, _) = s.run_line("echo $HELPER");
    assert_eq!(out, "loaded\n", "source should find file via PATH");
}

#[test]
fn source_with_args() {
    let mut s = shell_with_files(serde_json::json!({
        "args.sh": { "content": "echo $# $1 $2", "mode": 644 }
    }));
    let (out, _, _) = s.run_line("source args.sh x y");
    assert_eq!(out, "2 x y\n");
}

#[test]
fn source_restores_params_after() {
    let mut s = shell();
    s.set_positional_params(&["outer".to_string()]);
    s.run_line("echo 'echo $1' > tmp.sh");
    let (out, _, _) = s.run_line("source tmp.sh inner");
    assert_eq!(out, "inner\n");
    let (out2, _, _) = s.run_line("echo $1");
    assert_eq!(out2, "outer\n");
}

// ---------------------------------------------------------------------------
// Isolation: bash/exec should NOT leak env, cwd, or functions
// ---------------------------------------------------------------------------

#[test]
fn bash_does_not_leak_env() {
    let mut s = shell_with_files(serde_json::json!({
        "mutate.sh": { "content": "export X=fromscript\ncd /tmp", "mode": 755 }
    }));
    s.run_line("bash mutate.sh");
    let (out, _, _) = s.run_line("echo ${X:-unset}");
    assert_eq!(out, "unset\n", "env should not leak from bash");
    let (cwd, _, _) = s.run_line("pwd");
    assert!(
        !cwd.contains("/tmp"),
        "cwd should not leak from bash: {}",
        cwd
    );
}

#[test]
fn exec_does_not_leak_env() {
    let mut s = shell_with_files(serde_json::json!({
        "mutate.sh": { "content": "export Y=leaked\nhelper() { echo bad; }", "mode": 755 }
    }));
    s.run_line("./mutate.sh");
    let (out, _, _) = s.run_line("echo ${Y:-unset}");
    assert_eq!(out, "unset\n", "env should not leak from ./exec");
    let (out2, code, _) = s.run_line("helper");
    assert_eq!(code, 127, "functions should not leak from ./exec");
    assert!(out2.contains("command not found"), "got: {}", out2);
}

#[test]
fn source_does_leak_env() {
    let mut s = shell_with_files(serde_json::json!({
        "setup.sh": { "content": "export Z=sourced\ngreet() { echo hi; }", "mode": 644 }
    }));
    s.run_line("source setup.sh");
    let (out, _, _) = s.run_line("echo $Z");
    assert_eq!(out, "sourced\n", "env should leak from source");
    let (out2, code, _) = s.run_line("greet");
    assert_eq!(code, 0);
    assert_eq!(out2, "hi\n", "functions should leak from source");
}

#[test]
fn path_script_does_not_leak_env() {
    let mut s = shell();
    s.fs.set_current_user(0, 0);
    let _ = s.fs.create_dir_all("/usr/local/bin");
    s.fs.write_with_mode("/usr/local/bin/pathmut", b"export PLEAK=1\ncd /tmp", 0o755)
        .ok();
    s.fs.set_current_user(1000, 1000);
    s.run_line("pathmut");
    let (out, _, _) = s.run_line("echo ${PLEAK:-unset}");
    assert_eq!(out, "unset\n", "PATH script should not leak env");
    let (cwd, _, _) = s.run_line("pwd");
    assert!(
        !cwd.contains("/tmp"),
        "PATH script should not leak cwd: {}",
        cwd
    );
}

#[test]
fn cmd_subst_does_not_leak_cwd() {
    let mut s = shell();
    let (out, _, _) = s.run_line("echo $(cd /tmp; echo hi)");
    assert_eq!(out, "hi\n");
    let (cwd, _, _) = s.run_line("pwd");
    assert!(
        !cwd.contains("/tmp"),
        "cmdsubst should not leak cwd: {}",
        cwd
    );
}

#[test]
fn cmd_subst_does_not_leak_env() {
    let mut s = shell();
    let (_, _, _) = s.run_line("echo $(export LEAK=yes)");
    let (out, _, _) = s.run_line("echo ${LEAK:-unset}");
    assert_eq!(out, "unset\n", "cmdsubst should not leak env");
}

#[test]
fn bash_does_not_leak_readonly() {
    let mut s = shell_with_files(serde_json::json!({
        "ro.sh": { "content": "readonly RO=locked", "mode": 755 }
    }));
    s.run_line("bash ro.sh");
    s.run_line("export RO=free");
    let (out, _, _) = s.run_line("echo $RO");
    assert_eq!(out, "free\n", "bash should not leak readonly");
}

#[test]
fn subshell_does_not_leak_readonly() {
    let mut s = shell();
    let (_, _) = s.run_script("(readonly SUB=1)\nexport SUB=2");
    let (out, _, _) = s.run_line("echo $SUB");
    assert_eq!(out, "2\n", "subshell readonly should not leak");
}

#[test]
fn bash_does_not_leak_shell_opts() {
    let mut s = shell_with_files(serde_json::json!({
        "opts.sh": { "content": "set -e\nset -x", "mode": 755 }
    }));
    s.run_line("bash opts.sh");
    assert!(!s.exec.opts.errexit, "set -e should not leak from bash");
    assert!(!s.exec.opts.xtrace, "set -x should not leak from bash");
}

#[test]
fn subshell_does_not_leak_shell_opts() {
    let mut s = shell();
    s.run_script("(set -eu)");
    assert!(!s.exec.opts.errexit, "set -e should not leak from subshell");
    assert!(!s.exec.opts.nounset, "set -u should not leak from subshell");
}

#[test]
fn cmd_subst_does_not_leak_shell_opts() {
    let mut s = shell();
    s.run_line("echo $(set -x; echo hi)");
    assert!(!s.exec.opts.xtrace, "set -x should not leak from $()");
}

#[test]
fn cmd_subst_does_not_leak_functions() {
    let mut s = shell();
    s.run_line("echo $(f() { echo x; }; f)");
    assert!(
        !s.defs.has_function("f"),
        "function should not leak from $()"
    );
}

#[test]
fn cmd_subst_does_not_leak_aliases() {
    let mut s = shell();
    s.run_line("echo $(alias ll='ls -la')");
    assert!(
        s.defs.get_alias("ll").is_none(),
        "alias should not leak from $()"
    );
}

// ---------------------------------------------------------------------------
// Positional parameters ($#, $1, $2, ...)
// ---------------------------------------------------------------------------

#[test]
fn bash_receives_positional_params() {
    let mut s = shell_with_files(serde_json::json!({
        "args.sh": { "content": "echo $# $1 $2", "mode": 755 }
    }));
    let (out, _, _) = s.run_line("bash args.sh one two");
    assert_eq!(out, "2 one two\n");
}

#[test]
fn exec_receives_positional_params() {
    let mut s = shell_with_files(serde_json::json!({
        "args.sh": { "content": "echo $# $1 $2", "mode": 755 }
    }));
    let (out, _, _) = s.run_line("./args.sh alpha beta");
    assert_eq!(out, "2 alpha beta\n");
}

// ---------------------------------------------------------------------------
// $0 in various contexts
// ---------------------------------------------------------------------------

#[test]
fn dollar_zero_in_bash_script() {
    let mut s = shell_with_files(serde_json::json!({
        "zero.sh": { "content": "echo $0", "mode": 755 }
    }));
    let (out, _, _) = s.run_line("bash zero.sh");
    assert_eq!(out, "zero.sh\n");
}

#[test]
fn dollar_zero_in_bash_c() {
    let mut s = shell();
    let (out, _, _) = s.run_line("bash -c 'echo $0' myname");
    assert_eq!(out, "myname\n");
}

#[test]
fn dollar_zero_in_bash_c_default() {
    let mut s = shell();
    let (out, _, _) = s.run_line("bash -c 'echo $0'");
    assert_eq!(out, "bash\n");
}

#[test]
fn dollar_zero_in_exec_script() {
    let mut s = shell_with_files(serde_json::json!({
        "zero.sh": { "content": "echo $0", "mode": 755 }
    }));
    let (out, _, _) = s.run_line("./zero.sh");
    assert_eq!(out, "./zero.sh\n");
}

// ---------------------------------------------------------------------------
// exec builtin
// ---------------------------------------------------------------------------

#[test]
fn exec_builtin_runs_command() {
    let mut s = shell();
    let (out, _, _) = s.run_line("exec echo hello");
    assert_eq!(out, "hello\n");
}

#[test]
fn exec_stops_script() {
    let mut s = shell();
    let (out, code) = s.run_script("exec echo hello\necho should_not_run");
    assert_eq!(out, "hello\n");
    assert_eq!(code, 0);
    assert!(
        !out.contains("should_not_run"),
        "exec should terminate script"
    );
}

#[test]
fn exec_no_args_noop() {
    let mut s = shell();
    let (out, code) = s.run_script("exec\necho still_here");
    assert!(
        out.contains("still_here"),
        "exec with no args should be no-op"
    );
    assert_eq!(code, 0);
}

// ---------------------------------------------------------------------------
// builtin builtin
// ---------------------------------------------------------------------------

#[test]
fn builtin_builtin_runs_builtin() {
    let mut s = shell();
    let (out, _, _) = s.run_line("builtin echo hi");
    assert_eq!(out, "hi\n");
}

#[test]
fn builtin_rejects_non_builtin() {
    let mut s = shell();
    let (out, code, _) = s.run_line("builtin nosuchcmd");
    assert_eq!(code, 1);
    assert!(out.contains("not a shell builtin"), "got: {}", out);
}

// ---------------------------------------------------------------------------
// Brace groups
// ---------------------------------------------------------------------------

#[test]
fn brace_group_basic() {
    let mut s = shell();
    let (out, _) = s.run_script("{ echo a; echo b; }");
    let lines: Vec<&str> = out.trim().lines().filter(|l| !l.is_empty()).collect();
    assert_eq!(lines, vec!["a", "b"]);
}

#[test]
fn brace_group_shares_env() {
    let mut s = shell();
    let (out, _) = s.run_script("{ export X=inside; }; echo $X");
    assert_eq!(out, "inside\n", "brace group should share env with parent");
}

// ---------------------------------------------------------------------------
// Process substitution <() and >()
// ---------------------------------------------------------------------------

#[test]
fn process_subst_input() {
    let mut s = shell();
    s.run_line("echo 'alpha' > /tmp/a.txt");
    let (out, _, _) = s.run_line("cat <(echo hello)");
    assert_eq!(out, "hello\n");
}

#[test]
fn process_subst_does_not_leak() {
    let mut s = shell();
    s.run_line("cat <(export PSUB=yes; echo test)");
    let (out, _, _) = s.run_line("echo ${PSUB:-unset}");
    assert_eq!(out, "unset\n", "process subst should be isolated");
}

#[test]
fn output_process_subst_runs_consumer() {
    let mut s = shell();
    let (_, _, _) = s.run_line("echo hello > >(cat > /tmp/captured)");
    let (out, _, _) = s.run_line("cat /tmp/captured");
    assert_eq!(out, "hello\n", ">(cmd) should pipe data to consumer");
}

#[test]
fn output_process_subst_isolated() {
    let mut s = shell();
    s.run_line("echo test > >(export PSUB=yes)");
    let (out, _, _) = s.run_line("echo ${PSUB:-unset}");
    assert_eq!(out, "unset\n", ">(cmd) should be isolated");
}

#[test]
fn output_process_subst_runs_on_empty() {
    let mut s = shell();
    s.run_line("true > >(echo ran > /tmp/empty_consumer)");
    let (out, _, _) = s.run_line("cat /tmp/empty_consumer");
    assert_eq!(out, "ran\n", ">(cmd) should run even with empty input");
}

#[test]
fn quoted_process_subst_is_literal() {
    let mut s = shell();
    let (out, _, _) = s.run_line("echo '<(echo hi)'");
    assert_eq!(out, "<(echo hi)\n", "single-quoted <() should be literal");
}

#[test]
fn quoted_output_subst_is_literal() {
    let mut s = shell();
    let (out, _, _) = s.run_line("echo '>(cat)'");
    assert_eq!(out, ">(cat)\n", "single-quoted >() should be literal");
}

#[test]
fn double_quoted_process_subst_is_literal() {
    let mut s = shell();
    let (out, _, _) = s.run_line(r#"echo "<(echo hi)""#);
    assert_eq!(out, "<(echo hi)\n", "double-quoted <() should be literal");
}

// ---------------------------------------------------------------------------
// Alias re-parsing
// ---------------------------------------------------------------------------

#[test]
fn alias_with_pipe_is_reparsed() {
    let mut s = shell();
    s.run_line("alias countlines='wc -l'");
    s.run_line("echo -e 'a\nb\nc' > /tmp/lines.txt");
    let (out, code, _) = s.run_line("cat /tmp/lines.txt | countlines");
    assert_eq!(code, 0);
    let n: usize = out
        .trim()
        .split_whitespace()
        .next()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    assert!(
        n > 0,
        "alias with pipe should produce a line count: {}",
        out
    );
}

#[test]
fn alias_body_with_pipe() {
    let mut s = shell();
    s.run_line("alias p='echo hi | wc -c'");
    let (out, code, _) = s.run_line("p");
    assert_eq!(code, 0);
    let n: usize = out
        .trim()
        .split_whitespace()
        .next()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    assert!(n > 0, "alias with pipe should produce char count: {}", out);
}

#[test]
fn alias_body_with_variable() {
    let mut s = shell();
    s.run_line("alias show='echo $HOME'");
    let (out, code, _) = s.run_line("show");
    assert_eq!(code, 0);
    assert_eq!(
        out,
        format!("{}\n", s.ident.home),
        "alias should expand $HOME"
    );
}

#[test]
fn alias_recursive_guard() {
    let mut s = shell();
    s.run_line("alias foo='foo bar'");
    let (_, code, _) = s.run_line("foo");
    assert_eq!(code, 127, "recursive alias should error or terminate");
}

// ---------------------------------------------------------------------------
// trap (EXIT, ERR)
// ---------------------------------------------------------------------------

#[test]
fn trap_exit_runs_on_script_end() {
    let mut s = shell();
    let (out, code) = s.run_script("trap 'echo goodbye' EXIT\necho hello");
    assert_eq!(code, 0);
    assert!(out.contains("hello"), "expected hello in output: {}", out);
    assert!(
        out.contains("goodbye"),
        "expected goodbye in output: {}",
        out
    );
}

#[test]
fn trap_exit_empty_command_ignored() {
    let mut s = shell();
    let (out, code) = s.run_script("trap '' EXIT\necho hello");
    assert_eq!(code, 0);
    assert!(out.contains("hello"));
}

#[test]
fn trap_reset() {
    let mut s = shell();
    let (out, _) = s.run_script("trap 'echo bye' EXIT\ntrap - EXIT\necho hello");
    assert!(out.contains("hello"));
    assert!(
        !out.contains("bye"),
        "expected no bye after trap reset: {}",
        out
    );
}

#[test]
fn trap_err_fires_on_failure() {
    let mut s = shell();
    let (out, _, _) = s.run_line("trap 'echo ERR_FIRED' ERR");
    assert!(out.is_empty());
    let (out2, code, _) = s.run_line("false");
    assert_eq!(code, 1);
    assert!(out2.contains("ERR_FIRED"), "expected ERR_FIRED: {}", out2);
}

#[test]
fn trap_err_not_on_success() {
    let mut s = shell();
    s.run_line("trap 'echo ERR_FIRED' ERR");
    let (out, code, _) = s.run_line("true");
    assert_eq!(code, 0);
    assert!(!out.contains("ERR_FIRED"));
}

#[test]
fn trap_display() {
    let mut s = shell();
    s.run_line("trap 'echo bye' EXIT");
    let (out, code, _) = s.run_line("trap");
    assert_eq!(code, 0);
    assert!(out.contains("trap -- 'echo bye' EXIT"), "got: {}", out);
}

#[test]
fn source_does_not_consume_exit_trap() {
    let mut s = shell();
    s.run_line("echo 'trap \"echo inner_exit\" EXIT' > trap.sh");
    s.run_line("source trap.sh");
    assert!(
        s.defs.traps.contains_key("EXIT"),
        "EXIT trap should persist after source, not be consumed"
    );
}

#[test]
fn subshell_trap_does_not_leak() {
    let mut s = shell();
    let (out, _) = s.run_script("(trap 'echo inner' EXIT; echo body)\necho after");
    let out_lines: Vec<&str> = out.trim().lines().filter(|l| !l.is_empty()).collect();
    assert_eq!(
        out_lines,
        vec!["body", "inner", "after"],
        "subshell EXIT should fire at subshell end, got: {}",
        out
    );
    assert!(
        !s.defs.traps.contains_key("EXIT"),
        "subshell trap should not leak to parent"
    );
}

// ---------------------------------------------------------------------------
// Control flow (break, continue, return)
// ---------------------------------------------------------------------------

#[test]
fn break_outside_loop_errors() {
    let mut s = shell();
    let (out, code) = s.run_script("break");
    assert!(out.contains("only meaningful in a loop"), "got: {}", out);
    assert_eq!(code, 1);
}

#[test]
fn continue_outside_loop_errors() {
    let mut s = shell();
    let (out, code) = s.run_script("continue");
    assert!(out.contains("only meaningful in a loop"), "got: {}", out);
    assert_eq!(code, 1);
}

#[test]
fn return_in_bash_script_errors() {
    let mut s = shell_with_files(serde_json::json!({
        "ret.sh": { "content": "return 7", "mode": 755 }
    }));
    let (out, code, _) = s.run_line("bash ret.sh");
    assert!(
        out.contains("can only") || out.contains("return"),
        "got: {}",
        out
    );
    assert_eq!(code, 1);
}

#[test]
fn return_in_source_is_valid() {
    let mut s = shell_with_files(serde_json::json!({
        "early.sh": { "content": "echo before\nreturn 0\necho after", "mode": 644 }
    }));
    let (out, code, _) = s.run_line("source early.sh");
    assert_eq!(out, "before\n", "return in source should exit early");
    assert_eq!(code, 0);
}

#[test]
fn break_in_function_without_loop_errors() {
    let mut s = shell();
    let (out, _) = s.run_script("f() { echo before; break; echo after; }\nf\necho done");
    assert!(out.contains("only meaningful in a loop"), "got: {}", out);
    assert!(
        out.contains("done"),
        "should continue after function: {}",
        out
    );
}

#[test]
fn heredoc_preserves_exit_code() {
    let mut s = shell();
    let (out, code) = s.run_script("grep -c foo <<EOF\nbar\nEOF");
    assert_eq!(out, "0\n", "grep -c with no matches should output 0");
    assert_eq!(code, 1, "grep with no matches should exit 1, got {}", code);
}

#[test]
fn heredoc_after_parse_error_aborts() {
    let mut s = shell();
    let (out, code) = s.run_script("if true\ncat <<EOF\nhello\nEOF");
    assert!(
        out.contains("parse error") || out.contains("expected"),
        "got: {}",
        out
    );
    assert_eq!(code, 2, "should exit 2 on parse error, got {}", code);
}

// ---------------------------------------------------------------------------
// C-style for (( )) loop
// ---------------------------------------------------------------------------

#[test]
fn for_arith_basic_count() {
    let mut s = shell();
    let (out, code) = s.run_script("for (( i=0; i<3; i++ )); do echo $i; done");
    assert_eq!(code, 0);
    let lines: Vec<&str> = out.trim().lines().filter(|l| !l.is_empty()).collect();
    assert_eq!(lines, vec!["0", "1", "2"]);
}

#[test]
fn for_arith_empty_body() {
    let mut s = shell();
    let (out, code) = s.run_script("for (( i=0; i<0; i++ )); do echo $i; done");
    assert_eq!(code, 0);
    assert_eq!(out, "");
}

#[test]
fn for_arith_sum() {
    let mut s = shell();
    let (out, code) = s.run_script("for (( i=0; i<5; i++ ))\ndo\necho $i\ndone");
    assert_eq!(code, 0, "script failed: {}", out);
    let sum: i32 = out
        .trim()
        .lines()
        .map(|l| l.trim().parse::<i32>().unwrap_or(0))
        .sum();
    assert_eq!(sum, 10, "expected sum of 0+1+2+3+4=10, got: {:?}", out);
}

#[test]
fn for_arith_break() {
    let mut s = shell();
    let (out, code) =
        s.run_script("for (( i=0; i<10; i++ )); do if [ $i -eq 3 ]; then break; fi; echo $i; done");
    assert_eq!(code, 0);
    let lines: Vec<&str> = out.trim().lines().filter(|l| !l.is_empty()).collect();
    assert_eq!(lines, vec!["0", "1", "2"]);
}

// ---------------------------------------------------------------------------
// $FUNCNAME and $BASH_SOURCE
// ---------------------------------------------------------------------------

#[test]
fn funcname_set_inside_function() {
    let mut s = shell();
    let (out, code) = s.run_script("myfunc() { echo $FUNCNAME; }; myfunc");
    assert_eq!(code, 0);
    assert_eq!(out, "myfunc\n");
}

#[test]
fn funcname_restored_after_function() {
    let mut s = shell();
    s.run_script("outer() { echo $FUNCNAME; }; outer");
    let (out, _) = s.run_script("echo \"[$FUNCNAME]\"");
    assert_eq!(out, "[]\n");
}

#[test]
fn bash_source_set_inside_function() {
    let mut s = shell();
    let (out, code) = s.run_script("myfunc() { echo \"${#BASH_SOURCE}\"; }; myfunc");
    assert_eq!(code, 0);
    assert_eq!(out, "0\n");
}

// ---------------------------------------------------------------------------
// 2C.5: declare -p and -i
// ---------------------------------------------------------------------------

#[test]
fn declare_p_prints_variable() {
    let mut s = shell();
    s.run_line("export FOO=bar");
    let (out, code, _) = s.run_line("declare -p FOO");
    assert_eq!(code, 0);
    assert!(
        out.contains("declare -- FOO=\"bar\""),
        "expected declare -p output: {}",
        out
    );
}

#[test]
fn declare_p_no_args_lists_all() {
    let mut s = shell();
    s.run_line("export XX=yy");
    let (out, code, _) = s.run_line("declare -p");
    assert_eq!(code, 0);
    assert!(out.contains("XX=\"yy\""), "expected XX in output: {}", out);
    assert!(out.contains("HOME="), "expected HOME in output: {}", out);
}

#[test]
fn declare_i_evaluates_arithmetic() {
    let mut s = shell();
    let (out, code) = s.run_script("declare -i x=2+3\necho $x");
    assert_eq!(code, 0);
    assert_eq!(out, "5\n", "declare -i should evaluate arithmetic");
}

#[test]
fn declare_i_with_variable_reference() {
    let mut s = shell();
    let (out, code) = s.run_script("a=10\ndeclare -i b=a+5\necho $b");
    assert_eq!(code, 0);
    assert_eq!(
        out.trim(),
        "15",
        "declare -i should resolve variable references"
    );
}

// ---------------------------------------------------------------------------
// 2C.6: read -a ARRAY
// ---------------------------------------------------------------------------

#[test]
fn read_a_splits_into_array() {
    let mut s = shell();
    s.run_line("echo 'a b c' | read -a arr");
    let (out, code, _) = s.run_line("echo ${arr[0]}");
    assert_eq!(code, 0);
    assert_eq!(out, "a\n");
}

#[test]
fn read_a_array_second_element() {
    let mut s = shell();
    s.run_line("echo 'x y z' | read -a items");
    let (out, _, _) = s.run_line("echo ${items[1]}");
    assert_eq!(out, "y\n");
}

#[test]
fn read_a_array_all_elements() {
    let mut s = shell();
    s.run_line("echo 'one two three' | read -a words");
    let (out, _, _) = s.run_line("echo ${words[@]}");
    assert!(out.contains("one"), "expected one: {}", out);
    assert!(out.contains("two"), "expected two: {}", out);
    assert!(out.contains("three"), "expected three: {}", out);
}
