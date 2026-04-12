use super::*;
use crate::types::Config;

// ---------------------------------------------------------------------------
// Environment variables (export, env, printenv, unset)
// ---------------------------------------------------------------------------

#[test]
fn export_semicolon_echo_expands_var() {
    let mut s = shell();
    let (out, code, _) = s.run_line("export ZZ=hello; echo $ZZ");
    assert_eq!(code, 0);
    assert_eq!(out, "hello\n");
}

#[test]
fn export_adds_variable_to_env() {
    let mut s = shell();
    let (out, code, _) = s.run_line("export MYVAR=testval; env");
    assert_eq!(code, 0);
    assert!(out.contains("MYVAR=testval"), "got {:?}", out);
}

#[test]
fn env_and_printenv_list_variables() {
    let mut s = shell();
    let (eout, c1, _) = s.run_line("env");
    assert_eq!(c1, 0);
    assert!(eout.contains("HOME=/home/u"), "got {:?}", eout);
    assert!(eout.contains("USER=u"));
    let (pout, c2, _) = s.run_line("printenv");
    assert_eq!(c2, 0);
    assert_eq!(eout, pout);
}

#[test]
fn unset_removes_variable() {
    let mut s = shell();
    s.run_line("export FOO=bar");
    let (out, _, _) = s.run_line("echo $FOO");
    assert_eq!(out, "bar\n");
    s.run_line("unset FOO");
    let (out2, _, _) = s.run_line("echo $FOO");
    assert!(
        out2.is_empty() || out2 == "\n" || out2 == "$FOO",
        "got {:?}",
        out2
    );
}

// ---------------------------------------------------------------------------
// Command lookup (which, type)
// ---------------------------------------------------------------------------

#[test]
fn which_and_type_builtin() {
    let mut s = shell();
    let (w, c1, _) = s.run_line("which echo");
    assert_eq!(c1, 0);
    assert!(w.contains("/bin/echo"), "got {:?}", w);
    let (t, c2, _) = s.run_line("type cd");
    assert_eq!(c2, 0);
    assert!(t.contains("builtin"), "got {:?}", t);
}

#[test]
fn which_only_finds_builtins() {
    let mut s = shell();
    let (out, code, _) = s.run_line("which /bin/ls");
    assert_eq!(code, 1);
    assert!(
        out.contains("no /bin/ls") || out.contains("no "),
        "got {:?}",
        out
    );
}

#[test]
fn type_unknown_command() {
    let mut s = shell();
    let (out, code, _) = s.run_line("type nosuchcmd");
    assert_eq!(code, 1);
    assert!(out.contains("not found"), "got {:?}", out);
}

#[test]
fn type_unknown_exits_1() {
    let mut s = shell();
    let (_, code, _) = s.run_line("type nonexistent_cmd");
    assert_eq!(code, 1);
}

#[test]
fn which_reports_all_args() {
    let mut s = shell();
    let (out, code, _) = s.run_line("which echo nonexistent_cmd cat");
    assert_eq!(code, 1);
    assert!(out.contains("no nonexistent_cmd"), "got: {out}");
}

// ---------------------------------------------------------------------------
// System info (hostname, date, id, groups)
// ---------------------------------------------------------------------------

#[test]
fn hostname_returns_configured_name() {
    let mut s = shell();
    let (out, code, _) = s.run_line("hostname");
    assert_eq!(code, 0);
    assert_eq!(out, "h\n");
}

#[test]
fn date_uses_config_env_date() {
    let val = serde_json::json!({
        "user": "u",
        "system": {
            "hostname": "h",
            "users": [{"name": "u", "home": "/home/u"}],
        },
        "commands": [],
        "date": "Wed Apr  8 12:00:00 UTC 2026",
    });
    let parsed: Result<Config, _> = serde_json::from_value(val);
    assert!(parsed.is_ok(), "config parse failed");
    let c = match parsed {
        Ok(c) => c,
        Err(_) => return,
    };
    let mut s = Shell::new(&c);
    let (out, code, _) = s.run_line("date");
    assert_eq!(code, 0);
    assert_eq!(out, "Wed Apr  8 12:00:00 UTC 2026\n");
}

#[test]
fn id_shows_user_info() {
    let mut s = shell();
    let (out, code, _) = s.run_line("id");
    assert_eq!(code, 0);
    assert!(out.contains("uid="), "expected uid= in output: {:?}", out);
    assert!(out.contains("gid="), "expected gid= in output: {:?}", out);
    assert!(
        out.contains("(u)"),
        "expected username in output: {:?}",
        out
    );
}

#[test]
fn groups_shows_groups() {
    let mut s = shell();
    let (out, code, _) = s.run_line("groups");
    assert_eq!(code, 0);
    assert_eq!(out, "u\n");
}

// ---------------------------------------------------------------------------
// History
// ---------------------------------------------------------------------------

#[test]
fn history_records_commands() {
    let mut s = shell();
    s.run_line("echo hello");
    s.run_line("ls");
    s.run_line("pwd");
    let (out, code, _) = s.run_line("history");
    assert_eq!(code, 0);
    assert!(out.contains("echo hello"), "got: {out}");
    assert!(out.contains("ls"), "got: {out}");
    assert!(out.contains("pwd"), "got: {out}");
    assert!(
        out.contains("history"),
        "history itself should be recorded: {out}"
    );
    assert!(
        out.contains("    1  echo hello"),
        "expected numbered output: {out}"
    );
}

#[test]
fn history_via_execute() {
    let mut s = shell();
    s.execute("echo first");
    s.execute("echo second");
    let entry = s.execute("history");
    assert_eq!(entry.exit_code, 0);
    assert!(entry.output.contains("echo first"), "got: {}", entry.output);
    assert!(
        entry.output.contains("echo second"),
        "got: {}",
        entry.output
    );
    assert!(entry.output.contains("history"), "got: {}", entry.output);
}

// ---------------------------------------------------------------------------
// set -C (noclobber)
// ---------------------------------------------------------------------------

#[test]
fn set_noclobber_prevents_overwrite() {
    let mut s = shell();
    s.run_line("echo original > /tmp/protected.txt");
    s.run_line("set -C");
    let (out, code) = s.run_script("echo new > /tmp/protected.txt");
    assert_eq!(code, 1, "expected error with noclobber");
    assert!(out.contains("cannot overwrite"), "got: {:?}", out);
    let (content, _) = s.run_script("cat /tmp/protected.txt");
    assert_eq!(content, "original\n");
}

#[test]
fn set_noclobber_allows_new_file() {
    let mut s = shell();
    s.run_line("set -C");
    let (_, code) = s.run_script("echo hello > /tmp/newfile.txt");
    assert_eq!(code, 0);
    let (content, _) = s.run_script("cat /tmp/newfile.txt");
    assert_eq!(content, "hello\n");
}

#[test]
fn set_noclobber_flag() {
    let mut s = shell();
    assert!(!s.exec.opts.noclobber);
    s.run_line("set -C");
    assert!(s.exec.opts.noclobber);
    s.run_line("set +C");
    assert!(!s.exec.opts.noclobber);
}

// ---------------------------------------------------------------------------
// Identifier validation
// ---------------------------------------------------------------------------

#[test]
fn readonly_rejects_invalid_identifier() {
    let mut s = shell();
    let (out, code, _) = s.run_line("readonly 1=foo");
    assert!(out.contains("not a valid identifier"), "got: {}", out);
    assert_eq!(code, 1);
}

#[test]
fn declare_rejects_invalid_identifier() {
    let mut s = shell();
    let (out, _) = s.run_script("f() { local 1bad=x; }\nf");
    assert!(out.contains("not a valid identifier"), "got: {}", out);
}

#[test]
fn export_rejects_invalid_identifier() {
    let mut s = shell();
    let (out, code, _) = s.run_line("export 1bad=foo");
    assert!(out.contains("not a valid identifier"), "got: {}", out);
    assert_eq!(code, 1);
}

// ---------------------------------------------------------------------------
// Builtin error diagnostics
// ---------------------------------------------------------------------------

#[test]
fn unalias_errors_on_missing() {
    let mut s = shell();
    let (out, code, _) = s.run_line("unalias nosuch");
    assert!(out.contains("not found"), "got: {}", out);
    assert_eq!(code, 1);
}

#[test]
fn trap_errors_on_unsupported_signal() {
    let mut s = shell();
    let (out, code, _) = s.run_line("trap 'echo hi' FAKESIG");
    assert!(out.contains("not supported"), "got: {}", out);
    assert_eq!(code, 1);
}

#[test]
fn set_errors_on_unknown_flag() {
    let mut s = shell();
    let (out, code, _) = s.run_line("set -z");
    assert!(out.contains("invalid option"), "got: {}", out);
    assert_eq!(code, 2);
}

#[test]
fn set_errors_on_compound_unknown_flag() {
    let mut s = shell();
    let (out, code, _) = s.run_line("set -ez");
    assert!(out.contains("invalid option"), "got: {}", out);
    assert_eq!(code, 2);
}

// ---------------------------------------------------------------------------
// Alias management
// ---------------------------------------------------------------------------

#[test]
fn alias_indirect_recursion_does_not_hang() {
    let mut s = shell();
    s.run_line("alias a='b'");
    s.run_line("alias b='a'");
    let (out, code, _) = s.run_line("a");
    assert_eq!(code, 1, "indirect alias recursion should error");
    assert!(
        out.contains("alias") || out.contains("recursion") || out.contains("loop"),
        "should mention alias/recursion: {}",
        out
    );
}

#[test]
fn alias_depth_limit_three_levels() {
    let mut s = shell();
    s.run_line("alias a='b'");
    s.run_line("alias b='c'");
    s.run_line("alias c='echo ok'");
    let (out, code, _) = s.run_line("a");
    assert_eq!(code, 0);
    assert_eq!(out, "ok\n", "3-level alias chain should work");
}

// ---------------------------------------------------------------------------
// 2C.3: export no-args lists variables in declare -x format
// ---------------------------------------------------------------------------

#[test]
fn export_no_args_lists_declare_x() {
    let mut s = shell();
    s.run_line("export FOO=bar");
    let (out, code, _) = s.run_line("export");
    assert_eq!(code, 0);
    assert!(
        out.contains("declare -x FOO=\"bar\""),
        "expected declare -x format: {}",
        out
    );
    assert!(
        out.contains("declare -x HOME="),
        "expected HOME in export list: {}",
        out
    );
}

// ---------------------------------------------------------------------------
// 2C.4: set no-args lists all variables
// ---------------------------------------------------------------------------

#[test]
fn set_no_args_lists_variables() {
    let mut s = shell();
    s.run_line("export MYVAR=hello");
    let (out, code, _) = s.run_line("set");
    assert_eq!(code, 0);
    assert!(
        out.contains("MYVAR=hello"),
        "expected MYVAR=hello in set output: {}",
        out
    );
    assert!(
        out.contains("HOME="),
        "expected HOME= in set output: {}",
        out
    );
}

// ---------------------------------------------------------------------------
// 2C.2: date +FORMAT
// ---------------------------------------------------------------------------

#[test]
fn date_format_year_month_day() {
    let val = serde_json::json!({
        "user": "u",
        "system": {
            "hostname": "h",
            "users": [{"name": "u", "home": "/home/u"}],
        },
        "commands": [],
        "date": "Wed Apr  8 12:00:00 UTC 2026",
    });
    let c: Config = serde_json::from_value(val).unwrap();
    let mut s = Shell::new(&c);
    let (out, code, _) = s.run_line("date '+%Y-%m-%d'");
    assert_eq!(code, 0);
    assert_eq!(out, "2026-04-08\n");
}

#[test]
fn date_format_time() {
    let val = serde_json::json!({
        "user": "u",
        "system": {
            "hostname": "h",
            "users": [{"name": "u", "home": "/home/u"}],
        },
        "commands": [],
        "date": "Wed Apr  8 14:30:45 UTC 2026",
    });
    let c: Config = serde_json::from_value(val).unwrap();
    let mut s = Shell::new(&c);
    let (out, code, _) = s.run_line("date '+%H:%M:%S'");
    assert_eq!(code, 0);
    assert_eq!(out, "14:30:45\n");
}

// ---------------------------------------------------------------------------
// 2C.7: type -t
// ---------------------------------------------------------------------------

#[test]
fn type_t_builtin() {
    let mut s = shell();
    let (out, code, _) = s.run_line("type -t echo");
    assert_eq!(code, 0);
    assert_eq!(out, "builtin\n");
}

#[test]
fn type_t_alias() {
    let mut s = shell();
    s.run_line("alias ll='ls -la'");
    let (out, code, _) = s.run_line("type -t ll");
    assert_eq!(code, 0);
    assert_eq!(out, "alias\n");
}

#[test]
fn type_alias_shows_aliased() {
    let mut s = shell();
    s.run_line("alias ll='ls -la'");
    let (out, code, _) = s.run_line("type ll");
    assert_eq!(code, 0);
    assert!(out.contains("aliased to"), "expected alias info: {}", out);
}

// ---------------------------------------------------------------------------
// 2C.8: trap expanded signals + trap -p
// ---------------------------------------------------------------------------

#[test]
fn trap_accepts_int_signal() {
    let mut s = shell();
    let (out, code, _) = s.run_line("trap 'echo caught' INT");
    assert_eq!(code, 0, "trap INT should succeed: {}", out);
    assert!(out.is_empty(), "expected no output: {}", out);
}

#[test]
fn trap_accepts_term_signal() {
    let mut s = shell();
    let (out, code, _) = s.run_line("trap 'echo caught' TERM");
    assert_eq!(code, 0, "trap TERM should succeed: {}", out);
}

#[test]
fn trap_accepts_hup_debug_return() {
    let mut s = shell();
    let (_, c1, _) = s.run_line("trap 'echo hup' HUP");
    assert_eq!(c1, 0);
    let (_, c2, _) = s.run_line("trap 'echo debug' DEBUG");
    assert_eq!(c2, 0);
    let (_, c3, _) = s.run_line("trap 'echo ret' RETURN");
    assert_eq!(c3, 0);
}

#[test]
fn trap_p_prints_traps() {
    let mut s = shell();
    s.run_line("trap 'echo bye' EXIT");
    s.run_line("trap 'echo ouch' INT");
    let (out, code, _) = s.run_line("trap -p");
    assert_eq!(code, 0);
    assert!(out.contains("trap -- 'echo bye' EXIT"), "got: {}", out);
    assert!(out.contains("trap -- 'echo ouch' INT"), "got: {}", out);
}

#[test]
fn trap_p_specific_signal() {
    let mut s = shell();
    s.run_line("trap 'echo bye' EXIT");
    s.run_line("trap 'echo ouch' INT");
    let (out, code, _) = s.run_line("trap -p EXIT");
    assert_eq!(code, 0);
    assert!(out.contains("EXIT"), "got: {}", out);
    assert!(!out.contains("INT"), "should only show EXIT: {}", out);
}

// ---------------------------------------------------------------------------
// Feature 7: unset -f (unset function)
// ---------------------------------------------------------------------------

#[test]
fn unset_f_removes_function() {
    let mut s = shell();
    s.run_script("myfn() { echo hi; }");
    let (out, code, _) = s.run_line("myfn");
    assert_eq!(code, 0);
    assert_eq!(out, "hi\n");
    s.run_line("unset -f myfn");
    let (out2, code2, _) = s.run_line("myfn");
    assert_eq!(code2, 127, "unset -f should remove function");
    assert!(out2.contains("command not found"), "got: {}", out2);
}

#[test]
fn unset_f_nonexistent_is_ok() {
    let mut s = shell();
    let (_, code, _) = s.run_line("unset -f nosuchfn");
    assert_eq!(code, 0);
}

// ---------------------------------------------------------------------------
// Feature 9: declare -x (export), -r (readonly), -f (list functions), -F (list function names)
// ---------------------------------------------------------------------------

#[test]
fn declare_x_exports_variable() {
    let mut s = shell();
    s.run_line("declare -x MYVAR=exported");
    let (out, code, _) = s.run_line("env");
    assert_eq!(code, 0);
    assert!(
        out.contains("MYVAR=exported"),
        "declare -x should export: {}",
        out
    );
}

#[test]
fn declare_r_makes_readonly() {
    let mut s = shell();
    s.run_line("declare -r RO=1");
    let (out, code) = s.run_script("RO=2");
    assert_eq!(code, 1);
    assert!(out.contains("readonly"), "expected readonly error: {}", out);
}

#[test]
fn declare_f_shows_function_body() {
    let mut s = shell();
    s.run_script("f() { echo hi; }");
    let (out, code, _) = s.run_line("declare -f f");
    assert_eq!(code, 0);
    assert!(
        out.contains("echo hi"),
        "declare -f should show body: {}",
        out
    );
}

#[test]
fn declare_big_f_shows_function_names() {
    let mut s = shell();
    s.run_script("myfn() { echo hi; }");
    s.run_script("other() { echo bye; }");
    let (out, code, _) = s.run_line("declare -F");
    assert_eq!(code, 0);
    assert!(out.contains("myfn"), "declare -F should list myfn: {}", out);
    assert!(
        out.contains("other"),
        "declare -F should list other: {}",
        out
    );
}

// ---------------------------------------------------------------------------
// Feature 10: read -d DELIM (custom delimiter) and -n NCHARS (read N chars)
// ---------------------------------------------------------------------------

#[test]
fn read_d_custom_delimiter() {
    let mut s = shell();
    // Test read -d directly: read up to ':' delimiter
    let (_, code) = s.cmd_read(&["-d".into(), ":".into(), "var".into()], Some("a:b:c"));
    assert_eq!(code, 0);
    assert_eq!(s.vars.env.get("var").map(|s| s.as_str()), Some("a"));
}

#[test]
fn read_n_nchars() {
    let mut s = shell();
    // Test read -n directly: read exactly 3 characters
    let (_, code) = s.cmd_read(&["-n".into(), "3".into(), "var".into()], Some("hello"));
    assert_eq!(code, 0);
    assert_eq!(s.vars.env.get("var").map(|s| s.as_str()), Some("hel"));
}

// ---------------------------------------------------------------------------
// Feature 11: bash -e (errexit) and -x (xtrace)
// ---------------------------------------------------------------------------

#[test]
fn bash_e_stops_on_error() {
    let mut s = shell_with_files(serde_json::json!({
        "err.sh": { "content": "echo before\nfalse\necho after", "mode": 755 }
    }));
    let (out, code, _) = s.run_line("bash -e err.sh");
    assert!(out.contains("before"), "should run first command: {}", out);
    assert!(!out.contains("after"), "should stop after false: {}", out);
    assert_eq!(code, 1);
}

#[test]
fn bash_x_traces_commands() {
    let mut s = shell_with_files(serde_json::json!({
        "trace.sh": { "content": "echo hello\necho world", "mode": 755 }
    }));
    let (out, code, _) = s.run_line("bash -x trace.sh");
    assert_eq!(code, 0);
    assert!(
        out.contains("+ echo hello"),
        "should trace commands: {}",
        out
    );
    assert!(out.contains("hello"), "should show output: {}", out);
}

// ---------------------------------------------------------------------------
// Special shell variables: $SECONDS, $UID, $EUID, $GROUPS, $HOSTTYPE, $OSTYPE
// ---------------------------------------------------------------------------

#[test]
fn seconds_is_zero_for_fresh_shell() {
    let mut s = shell();
    let (out, code, _) = s.run_line("echo $SECONDS");
    assert_eq!(code, 0);
    let val: u64 = out.trim().parse().expect("$SECONDS should be a number");
    assert!(
        val <= 1,
        "$SECONDS should be 0 or 1 for fresh shell, got {val}"
    );
}

#[test]
fn seconds_increases_after_sleep() {
    let mut s = shell();
    let (out0, _, _) = s.run_line("echo $SECONDS");
    let before: u64 = out0.trim().parse().unwrap();
    s.run_line("sleep 2");
    let (out1, _, _) = s.run_line("echo $SECONDS");
    let after: u64 = out1.trim().parse().unwrap();
    assert!(
        after >= before + 2,
        "$SECONDS should increase after sleep 2: before={before} after={after}"
    );
}

#[test]
fn uid_returns_1000() {
    let mut s = shell();
    let (out, code, _) = s.run_line("echo $UID");
    assert_eq!(code, 0);
    assert_eq!(out, "1000\n");
}

#[test]
fn euid_same_as_uid() {
    let mut s = shell();
    let (uid_out, _, _) = s.run_line("echo $UID");
    let (euid_out, _, _) = s.run_line("echo $EUID");
    assert_eq!(uid_out, euid_out);
}

#[test]
fn hosttype_returns_wasm32() {
    let mut s = shell();
    let (out, code, _) = s.run_line("echo $HOSTTYPE");
    assert_eq!(code, 0);
    assert_eq!(out, "wasm32\n");
}

#[test]
fn ostype_returns_linux_wasm() {
    let mut s = shell();
    let (out, code, _) = s.run_line("echo $OSTYPE");
    assert_eq!(code, 0);
    assert_eq!(out, "linux-wasm\n");
}

#[test]
fn groups_var_returns_space_separated_numbers() {
    let mut s = shell();
    let (out, code, _) = s.run_line("echo $GROUPS");
    assert_eq!(code, 0);
    // Output must end with newline
    assert!(
        out.ends_with('\n'),
        "$GROUPS output should end with newline"
    );
    // Each token must be a number
    let trimmed = out.trim();
    if !trimmed.is_empty() {
        for tok in trimmed.split_whitespace() {
            tok.parse::<u32>()
                .unwrap_or_else(|_| panic!("$GROUPS token {tok:?} is not a number"));
        }
    }
}
