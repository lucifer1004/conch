use super::*;

// ---------------------------------------------------------------------------
// ps
// ---------------------------------------------------------------------------

#[test]
fn ps_shows_shell_process() {
    let mut s = shell();
    let (out, code, _) = s.run_line("ps");
    assert_eq!(code, 0);
    assert!(out.contains("PID"), "ps should show PID header: {}", out);
    assert!(out.contains("STAT"), "ps should show STAT header: {}", out);
    assert!(
        out.contains("COMMAND"),
        "ps should show COMMAND header: {}",
        out
    );
    assert!(
        out.contains("conch"),
        "ps should show shell process: {}",
        out
    );
    assert!(out.contains('S'), "shell STAT should be S: {}", out);
}

#[test]
fn ps_shows_background_job() {
    let mut s = shell();
    s.run_line("echo hi &");
    let (out, code, _) = s.run_line("ps");
    assert_eq!(code, 0);
    assert!(
        out.contains("echo hi"),
        "ps should show background job: {}",
        out
    );
    // Background job is Exited -> STAT = Z
    assert!(out.contains('Z'), "bg job STAT should be Z: {}", out);
}

// ---------------------------------------------------------------------------
// umask
// ---------------------------------------------------------------------------

#[test]
fn umask_displays_default() {
    let mut s = shell();
    let (out, code, _) = s.run_line("umask");
    assert_eq!(code, 0);
    assert_eq!(
        out, "0022\n",
        "default umask should be 0022, got: {:?}",
        out
    );
}

#[test]
fn umask_set_changes_mask() {
    let mut s = shell();
    let (_, code, _) = s.run_line("umask 077");
    assert_eq!(code, 0);
    let (out, code2, _) = s.run_line("umask");
    assert_eq!(code2, 0);
    assert_eq!(
        out, "0077\n",
        "umask should be 0077 after set, got: {:?}",
        out
    );
}

#[test]
fn umask_invalid_gives_error() {
    let mut s = shell();
    let (out, code, _) = s.run_line("umask xyz");
    assert_eq!(code, 1);
    assert!(
        out.contains("invalid"),
        "error should mention invalid: {}",
        out
    );
}

// ---------------------------------------------------------------------------
// time
// ---------------------------------------------------------------------------

#[test]
fn time_echo_outputs_command_and_timing() {
    let mut s = shell();
    let (out, code, _) = s.run_line("time echo hello");
    assert_eq!(code, 0);
    assert!(
        out.contains("hello"),
        "time should include command output: {}",
        out
    );
    assert!(
        out.contains("real"),
        "time should include timing line: {}",
        out
    );
    assert!(
        out.contains('m'),
        "time should include minutes field: {}",
        out
    );
    assert!(
        out.contains('s'),
        "time should include seconds field: {}",
        out
    );
}

#[test]
fn time_no_args_succeeds() {
    let mut s = shell();
    let (out, code, _) = s.run_line("time");
    assert_eq!(code, 0);
    assert!(
        out.contains("real"),
        "time with no args should still show timing: {}",
        out
    );
}

// ---------------------------------------------------------------------------
// shopt
// ---------------------------------------------------------------------------

#[test]
fn shopt_lists_all_options() {
    let mut s = shell();
    let (out, code, _) = s.run_line("shopt");
    assert_eq!(code, 0);
    assert!(
        out.contains("nullglob"),
        "shopt should list nullglob: {}",
        out
    );
    assert!(
        out.contains("failglob"),
        "shopt should list failglob: {}",
        out
    );
    assert!(
        out.contains("dotglob"),
        "shopt should list dotglob: {}",
        out
    );
    assert!(
        out.contains("off"),
        "options should default to off: {}",
        out
    );
}

#[test]
fn shopt_s_enables_option() {
    let mut s = shell();
    let (_, code, _) = s.run_line("shopt -s nullglob");
    assert_eq!(code, 0);
    let (out, code2, _) = s.run_line("shopt nullglob");
    assert_eq!(code2, 0);
    assert!(
        out.contains("on"),
        "nullglob should be on after -s: {}",
        out
    );
}

#[test]
fn shopt_u_disables_option() {
    let mut s = shell();
    s.run_line("shopt -s dotglob");
    let (_, code, _) = s.run_line("shopt -u dotglob");
    assert_eq!(code, 0);
    let (out, _, _) = s.run_line("shopt dotglob");
    assert!(
        out.contains("off"),
        "dotglob should be off after -u: {}",
        out
    );
}

#[test]
fn shopt_query_single_option_off() {
    let mut s = shell();
    let (out, code, _) = s.run_line("shopt failglob");
    assert_eq!(code, 0);
    assert!(
        out.contains("failglob"),
        "output should include option name: {}",
        out
    );
    assert!(
        out.contains("off"),
        "failglob should default to off: {}",
        out
    );
}

#[test]
fn shopt_invalid_option_gives_error() {
    let mut s = shell();
    let (out, code, _) = s.run_line("shopt bogus_opt");
    assert_eq!(code, 1);
    assert!(out.contains("invalid"), "error should say invalid: {}", out);
}

// ---------------------------------------------------------------------------
// timeout
// ---------------------------------------------------------------------------

#[test]
fn timeout_sleep_exceeds_limit_returns_124() {
    let mut s = shell();
    let (_, code, _) = s.run_line("timeout 1 sleep 5");
    assert_eq!(
        code, 124,
        "timeout should return 124 when command exceeds duration"
    );
}

#[test]
fn timeout_echo_within_limit_returns_0() {
    let mut s = shell();
    let (out, code, _) = s.run_line("timeout 10 echo hello");
    assert_eq!(
        code, 0,
        "timeout should return 0 when command finishes in time"
    );
    assert_eq!(out, "hello\n");
}

#[test]
fn timeout_sleep_within_limit_returns_0() {
    let mut s = shell();
    let (_, code, _) = s.run_line("timeout 0.5 sleep 0.1");
    assert_eq!(code, 0, "sleep 0.1 should finish within 0.5s timeout");
}

#[test]
fn timeout_no_args_succeeds() {
    let mut s = shell();
    let (_, code, _) = s.run_line("timeout 5");
    // no command — just succeed (no-op)
    assert_eq!(code, 0);
}

// ---------------------------------------------------------------------------
// wait -n
// ---------------------------------------------------------------------------

#[test]
fn wait_n_no_bg_jobs_returns_0() {
    let mut s = shell();
    let (_, code, _) = s.run_line("wait -n");
    assert_eq!(code, 0, "wait -n with no bg jobs should return 0");
}

#[test]
fn wait_n_sync_returns_last_job_exit_code() {
    let mut s = shell();
    s.run_line("true &");
    let (_, code, _) = s.run_line("wait -n");
    assert_eq!(code, 0, "wait -n should return 0 for true &");
}

#[test]
fn wait_n_sync_false_returns_nonzero() {
    let mut s = shell();
    s.run_line("false &");
    let (_, code, _) = s.run_line("wait -n");
    assert_eq!(code, 1, "wait -n should return 1 for false &");
}

#[test]
fn wait_n_deferred_returns_job_exit_code() {
    let mut s = shell_with_bg_mode("deferred");
    s.run_line("false &");
    let (_, code, _) = s.run_line("wait -n");
    assert_eq!(
        code, 1,
        "wait -n in deferred mode should return bg job's exit code"
    );
}
