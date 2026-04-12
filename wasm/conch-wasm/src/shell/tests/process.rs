use super::*;

// ---------------------------------------------------------------------------
// ProcessTable basics
// ---------------------------------------------------------------------------

#[test]
fn shell_has_nonzero_pid() {
    let s = shell();
    assert!(s.procs.shell_pid() > 0, "shell PID should be > 0");
}

#[test]
fn shell_pid_is_stable() {
    let s = shell();
    let pid1 = s.procs.shell_pid();
    let pid2 = s.procs.shell_pid();
    assert_eq!(pid1, pid2, "shell PID should not change");
}

#[test]
fn spawn_returns_incrementing_pids() {
    let mut s = shell();
    let p1 = s.procs.spawn("cmd1");
    let p2 = s.procs.spawn("cmd2");
    assert!(p2 > p1, "PIDs should increment: {} > {}", p2, p1);
}

#[test]
fn spawn_pid_differs_from_shell_pid() {
    let mut s = shell();
    let child = s.procs.spawn("child");
    assert_ne!(child, s.procs.shell_pid());
}

// ---------------------------------------------------------------------------
// PID variable expansion ($$, $BASHPID, $PPID, $!, $0)
// ---------------------------------------------------------------------------

#[test]
fn dollar_dollar_expands_to_shell_pid() {
    let mut s = shell();
    let (out, code, _) = s.run_line("echo $$");
    assert_eq!(code, 0);
    let pid: u32 = out
        .trim_end_matches('\n')
        .parse()
        .expect("$$ should be a number");
    assert_eq!(pid, s.procs.shell_pid());
}

#[test]
fn dollar_dollar_stable_across_commands() {
    let mut s = shell();
    let (out1, _, _) = s.run_line("echo $$");
    let (out2, _, _) = s.run_line("echo $$");
    assert_eq!(out1, out2, "$$ should be stable");
}

#[test]
fn bashpid_expands_to_number() {
    let mut s = shell();
    let (out, code, _) = s.run_line("echo $BASHPID");
    assert_eq!(code, 0);
    let _pid: u32 = out
        .trim_end_matches('\n')
        .parse()
        .expect("$BASHPID should be a number");
}

#[test]
fn ppid_expands_to_number() {
    let mut s = shell();
    let (out, code, _) = s.run_line("echo $PPID");
    assert_eq!(code, 0);
    let ppid: u32 = out
        .trim_end_matches('\n')
        .parse()
        .expect("$PPID should be a number");
    assert_ne!(
        ppid,
        s.procs.shell_pid(),
        "$PPID should differ from $$ (parent != self)"
    );
}

#[test]
fn dollar_dollar_unchanged_in_subshell() {
    let mut s = shell();
    let (outer, _, _) = s.run_line("echo $$");
    let (inner, _, _) = s.run_line("echo $(echo $$)");
    assert_eq!(outer, inner, "$$ should be same in subshell");
}

#[test]
fn bashpid_differs_in_subshell() {
    let mut s = shell();
    let (out, _, _) = s.run_line("echo $BASHPID $(echo $BASHPID)");
    let parts: Vec<&str> = out.trim_end_matches('\n').split_whitespace().collect();
    assert_eq!(parts.len(), 2, "should have two values: {}", out);
    let p1: u32 = parts[0].parse().expect("first $BASHPID should be number");
    let p2: u32 = parts[1].parse().expect("second $BASHPID should be number");
    assert_ne!(p1, p2, "$BASHPID should differ in command substitution");
}

#[test]
fn ppid_in_subshell_equals_parent_shell_pid() {
    let mut s = shell();
    let (out, _, _) = s.run_line("echo $(echo $PPID)");
    let ppid: u32 = out
        .trim_end_matches('\n')
        .parse()
        .expect("$PPID in subshell should be number");
    assert_eq!(ppid, s.procs.shell_pid(), "$PPID in subshell == parent $$");
}

#[test]
fn dollar_bang_empty_without_background() {
    let mut s = shell();
    let (out, _, _) = s.run_line("echo \"[$!]\"");
    assert_eq!(out, "[]\n", "$! should be empty with no bg jobs");
}

#[test]
fn dollar_zero_is_set() {
    let mut s = shell();
    let (out, code, _) = s.run_line("echo $0");
    assert_eq!(code, 0);
    assert_eq!(out, "conch\n", "$0 should default to conch");
}

// ---------------------------------------------------------------------------
// Job control (jobs, wait, kill)
// ---------------------------------------------------------------------------

#[test]
fn jobs_no_background_gives_empty() {
    let mut s = shell();
    let (out, code, _) = s.run_line("jobs");
    assert_eq!(code, 0);
    assert_eq!(out, "", "jobs should be empty with no bg jobs");
}

#[test]
fn wait_no_background_succeeds() {
    let mut s = shell();
    let (_, code, _) = s.run_line("wait");
    assert_eq!(code, 0, "wait with no bg jobs should succeed");
}

#[test]
fn wait_invalid_pid_fails() {
    let mut s = shell();
    let (out, code, _) = s.run_line("wait 99999");
    assert_eq!(code, 127, "wait with unknown PID should fail");
    assert!(
        out.contains("99999"),
        "error should mention the PID: {}",
        out
    );
}

#[test]
fn kill_is_recognized() {
    let mut s = shell();
    let (out, _, _) = s.run_line("kill");
    assert!(
        !out.contains("command not found"),
        "kill should be a builtin"
    );
}

#[test]
fn kill_valid_pid_changes_status() {
    let mut s = shell();
    s.run_line("echo hello &");
    let (bang, _, _) = s.run_line("echo $!");
    let pid = bang.trim_end_matches('\n');
    let (_, code, _) = s.run_line(&format!("kill -9 {}", pid));
    assert_eq!(code, 0, "kill valid PID should succeed");
    let (jobs_out, _, _) = s.run_line("jobs");
    assert!(
        jobs_out.contains("137"),
        "killed job should show exit 137: {}",
        jobs_out
    );
}

#[test]
fn kill_signal_zero_checks_existence() {
    let mut s = shell();
    s.run_line("echo hi &");
    let (bang, _, _) = s.run_line("echo $!");
    let pid = bang.trim_end_matches('\n');
    let (_, code, _) = s.run_line(&format!("kill -0 {}", pid));
    assert_eq!(code, 0, "kill -0 should succeed for existing PID");
    let (_, code2, _) = s.run_line("kill -0 99999");
    assert_eq!(code2, 1, "kill -0 should fail for nonexistent PID");
}

#[test]
fn kill_with_signal_name() {
    let mut s = shell();
    s.run_line("echo hi &");
    let (bang, _, _) = s.run_line("echo $!");
    let pid = bang.trim_end_matches('\n');
    let (_, code, _) = s.run_line(&format!("kill -TERM {}", pid));
    assert_eq!(code, 0);
    let (_, code2, _) = s.run_line(&format!("kill -USR1 {}", pid));
    assert_eq!(code2, 0);
}

#[test]
fn jobs_pruning_keeps_bounded() {
    let mut s = shell();
    for i in 0..15 {
        s.run_line(&format!("echo job{} &", i));
    }
    assert!(
        s.procs.jobs.len() <= 10,
        "jobs should be pruned to <=10, got {}",
        s.procs.jobs.len()
    );
}

// ---------------------------------------------------------------------------
// Background execution (&)
// ---------------------------------------------------------------------------

#[test]
fn background_sets_dollar_bang() {
    let mut s = shell();
    s.run_line("echo hello &");
    let (out, _, _) = s.run_line("echo $!");
    let pid: u32 = out
        .trim_end_matches('\n')
        .parse()
        .expect("$! should be a number after &");
    assert!(pid > 0);
}

#[test]
fn background_bashpid_matches_dollar_bang() {
    let mut s = shell();
    s.run_line("echo $BASHPID > /tmp/bg_pid &");
    let (bang, _, _) = s.run_line("echo $!");
    let (file_pid, _, _) = s.run_line("cat /tmp/bg_pid");
    assert_eq!(
        bang, file_pid,
        "$BASHPID inside & should equal $! (got BASHPID={}, $!={})",
        file_pid, bang,
    );
}

#[test]
fn background_job_appears_in_jobs() {
    let mut s = shell();
    s.run_line("echo hello &");
    let (out, code, _) = s.run_line("jobs");
    assert_eq!(code, 0);
    assert!(
        out.contains("echo hello"),
        "jobs should show the command: {}",
        out
    );
    assert!(out.contains("Done"), "job should be Done: {}", out);
}

#[test]
fn background_wait_returns_exit_code() {
    let mut s = shell();
    s.run_line("true &");
    let (bang, _, _) = s.run_line("echo $!");
    let pid = bang.trim_end_matches('\n');
    let (_, code, _) = s.run_line(&format!("wait {}", pid));
    assert_eq!(code, 0, "wait for true should return 0");
}

#[test]
fn background_multiple_jobs() {
    let mut s = shell();
    s.run_line("echo first &");
    let (out1, _, _) = s.run_line("echo $!");
    s.run_line("echo second &");
    let (out2, _, _) = s.run_line("echo $!");
    assert_ne!(out1, out2, "each & should get a unique PID");
    let (jobs_out, _, _) = s.run_line("jobs");
    assert!(
        jobs_out.contains("first"),
        "jobs should list first: {}",
        jobs_out
    );
    assert!(
        jobs_out.contains("second"),
        "jobs should list second: {}",
        jobs_out
    );
}

#[test]
fn background_does_not_print_output() {
    let mut s = shell();
    let (out, code, _) = s.run_line("echo hidden &");
    assert_eq!(code, 0);
    assert!(out.contains('['), "should have job notification: {}", out);
}

#[test]
fn false_background_does_not_affect_foreground_status() {
    let mut s = shell();
    let (_, code, _) = s.run_line("false &");
    assert_eq!(code, 0, "foreground $? should be 0 after false &");
}

#[test]
fn multiple_background_in_one_line() {
    let mut s = shell();
    let (out, code) = s.run_script("echo a & echo b &");
    assert_eq!(code, 0);
    let brackets: Vec<&str> = out.matches('[').collect();
    assert_eq!(
        brackets.len(),
        2,
        "should have exactly 2 job notifications: {}",
        out
    );
}

#[test]
fn background_and_then_foreground() {
    let mut s = shell();
    let (out, code) = s.run_script("echo bg & echo fg");
    assert_eq!(code, 0);
    assert!(out.contains('['), "should have job notification: {}", out);
    assert!(
        out.ends_with("fg\n"),
        "should end with foreground output: {}",
        out
    );
}

#[test]
fn wait_returns_bg_exit_code() {
    let mut s = shell();
    s.run_line("false &");
    let (bang, _, _) = s.run_line("echo $!");
    let pid = bang.trim_end_matches('\n');
    let (_, code, _) = s.run_line(&format!("wait {}", pid));
    assert_eq!(code, 1, "wait should return the bg job's exit code");
}

// ---------------------------------------------------------------------------
// Deferred background mode
// ---------------------------------------------------------------------------

#[test]
fn deferred_bg_does_not_execute_immediately() {
    let mut s = shell_with_bg_mode("deferred");
    s.run_line("echo hello > /tmp/bg_out.txt &");
    assert!(
        !s.fs.exists("/tmp/bg_out.txt"),
        "deferred bg should not create file until wait"
    );
}

#[test]
fn deferred_bg_sets_dollar_bang() {
    let mut s = shell_with_bg_mode("deferred");
    s.run_line("echo hello &");
    let (out, _, _) = s.run_line("echo $!");
    let _pid: u32 = out
        .trim_end_matches('\n')
        .parse()
        .expect("$! should be set in deferred mode");
}

#[test]
fn deferred_jobs_shows_running() {
    let mut s = shell_with_bg_mode("deferred");
    s.run_line("echo hello &");
    let (out, _, _) = s.run_line("jobs");
    assert!(
        out.contains("Running"),
        "deferred job should show Running: {}",
        out
    );
}

#[test]
fn deferred_wait_executes_and_returns_output() {
    let mut s = shell_with_bg_mode("deferred");
    s.run_line("echo hello > /tmp/bg_out.txt &");
    let (_, code, _) = s.run_line("wait");
    assert_eq!(code, 0);
    assert!(
        s.fs.exists("/tmp/bg_out.txt"),
        "wait should have executed the bg job"
    );
    let (content, _, _) = s.run_line("cat /tmp/bg_out.txt");
    assert_eq!(content, "hello\n");
}

#[test]
fn deferred_wait_specific_pid() {
    let mut s = shell_with_bg_mode("deferred");
    s.run_line("echo first > /tmp/f1.txt &");
    let (bang, _, _) = s.run_line("echo $!");
    let pid = bang.trim_end_matches('\n');
    s.run_line("echo second > /tmp/f2.txt &");
    let (_, code, _) = s.run_line(&format!("wait {}", pid));
    assert_eq!(code, 0);
    assert!(s.fs.exists("/tmp/f1.txt"), "waited job should be done");
}

#[test]
fn deferred_jobs_shows_done_after_wait() {
    let mut s = shell_with_bg_mode("deferred");
    s.run_line("echo hello &");
    s.run_line("wait");
    let (out, _, _) = s.run_line("jobs");
    assert!(
        out.contains("Done"),
        "job should be Done after wait: {}",
        out
    );
}

#[test]
fn deferred_bg_with_stdin() {
    let mut s = shell_with_bg_mode("deferred");
    s.run_line("echo input_data | cat > /tmp/piped.txt &");
    s.run_line("wait");
    let (out, _, _) = s.run_line("cat /tmp/piped.txt");
    assert_eq!(out, "input_data\n", "deferred bg should handle piped stdin");
}

// ---------------------------------------------------------------------------
// Interleaved background mode
// ---------------------------------------------------------------------------

#[test]
fn interleaved_bg_runs_during_foreground() {
    let mut s = shell_with_bg_mode("interleaved");
    s.run_line("echo bg_data > /tmp/il_out.txt &");
    s.run_line("echo foreground");
    assert!(
        s.fs.exists("/tmp/il_out.txt"),
        "interleaved bg should run during foreground"
    );
}

#[test]
fn interleaved_completion_notification() {
    let mut s = shell_with_bg_mode("interleaved");
    s.run_line("echo bg_result &");
    s.run_line("echo fg");
    let (out, _, _) = s.run_line("jobs");
    assert!(
        out.contains("Done"),
        "interleaved job should be Done: {}",
        out
    );
}

#[test]
fn interleaved_wait_forces_completion() {
    let mut s = shell_with_bg_mode("interleaved");
    s.run_line("echo data > /tmp/il_wait.txt &");
    let (_, code, _) = s.run_line("wait");
    assert_eq!(code, 0);
    assert!(s.fs.exists("/tmp/il_wait.txt"));
}

#[test]
fn interleaved_multiple_bg_jobs_interleave() {
    let mut s = shell_with_bg_mode("interleaved");
    s.run_line("echo first > /tmp/il1.txt &");
    s.run_line("echo second > /tmp/il2.txt &");
    s.run_line("jobs");
    s.run_line("echo fg");
    assert!(s.fs.exists("/tmp/il1.txt"), "first bg should complete");
    assert!(s.fs.exists("/tmp/il2.txt"), "second bg should complete");
}

// ---------------------------------------------------------------------------
// sleep and VFS time
// ---------------------------------------------------------------------------

#[test]
fn sleep_is_noop() {
    let mut s = shell();
    let (_, code, _) = s.run_line("sleep 1");
    assert_eq!(code, 0);
}

#[test]
fn sleep_advances_vfs_time() {
    let mut s = shell();
    s.run_line("touch /tmp/before");
    let mtime_before = s.fs.metadata("/tmp/before").unwrap().mtime();
    s.run_line("sleep 2");
    s.run_line("touch /tmp/after");
    let mtime_after = s.fs.metadata("/tmp/after").unwrap().mtime();
    let gap = mtime_after - mtime_before;
    assert!(
        gap >= 2000,
        "mtime gap should be >= 2000 after sleep 2, got {}",
        gap
    );
}

#[test]
fn sleep_zero_does_not_advance() {
    let mut s = shell();
    let time_before = s.fs.time();
    s.run_line("sleep 0");
    let time_after = s.fs.time();
    assert!(
        time_after - time_before < 100,
        "sleep 0 should not advance much"
    );
}

#[test]
fn sleep_fractional_seconds() {
    let mut s = shell();
    let before = s.fs.time();
    s.run_line("sleep 0.5");
    let after = s.fs.time();
    let gap = after - before;
    assert!(
        gap >= 500,
        "sleep 0.5 should advance by ~500 ticks, got {}",
        gap
    );
    assert!(
        gap < 1000,
        "sleep 0.5 should not advance by 1000+, got {}",
        gap
    );
}

// ---------------------------------------------------------------------------
// step_bg_jobs: ordering preserved with swap_remove
// ---------------------------------------------------------------------------

#[test]
fn step_bg_jobs_completes_all() {
    let mut s = shell_with_bg_mode("interleaved");
    s.run_line("echo a > /tmp/o1 &");
    s.run_line("echo b > /tmp/o2 &");
    s.run_line("echo c > /tmp/o3 &");
    assert_eq!(s.bg_jobs.len(), 3, "should have 3 pending bg jobs");
    // Trigger stepping
    s.run_line("echo trigger");
    // All three should have completed — none lost
    assert!(s.fs.exists("/tmp/o1"), "job 1 should complete");
    assert!(s.fs.exists("/tmp/o2"), "job 2 should complete");
    assert!(s.fs.exists("/tmp/o3"), "job 3 should complete");
    assert_eq!(s.bg_jobs.len(), 0, "no pending bg jobs should remain");
    // All should be recorded as Exited in the process table
    let done_count = s
        .procs
        .jobs
        .iter()
        .filter(|p| matches!(p.status, crate::shell::ProcessStatus::Exited(_)))
        .count();
    assert_eq!(
        done_count, 3,
        "exactly 3 jobs should be Exited: {}",
        done_count
    );
}

// ---------------------------------------------------------------------------
// wait with already-completed PID returns exit code
// ---------------------------------------------------------------------------

#[test]
fn wait_completed_pid_returns_exit_code() {
    let mut s = shell();
    s.run_line("false &");
    let (bang, _, _) = s.run_line("echo $!");
    let pid = bang.trim_end_matches('\n');
    // In Sync mode, job is already Exited(1)
    let (_, code, _) = s.run_line(&format!("wait {}", pid));
    assert_eq!(code, 1, "wait for false should return 1");
}

// ---------------------------------------------------------------------------
// Feature 3: sleep suffix support (s, m, h, d)
// ---------------------------------------------------------------------------

#[test]
fn sleep_suffix_s() {
    let mut s = shell();
    let (_, code, _) = s.run_line("sleep 1s");
    assert_eq!(code, 0);
    // Should behave same as sleep 1
}

#[test]
fn sleep_suffix_m_advances_time() {
    let mut s = shell();
    let before = s.fs.time();
    s.run_line("sleep 0.1m");
    let after = s.fs.time();
    let gap = after - before;
    // 0.1m = 6 seconds = 6000 ticks
    assert!(
        gap >= 5500 && gap <= 6500,
        "sleep 0.1m should advance ~6000 ticks, got {}",
        gap
    );
}

#[test]
fn sleep_suffix_h() {
    let mut s = shell();
    let before = s.fs.time();
    // Use a very small fraction to keep test fast
    s.run_line("sleep 0.001h");
    let after = s.fs.time();
    let gap = after - before;
    // 0.001h = 3.6 seconds = 3600 ticks
    assert!(
        gap >= 3000 && gap <= 4200,
        "sleep 0.001h should advance ~3600 ticks, got {}",
        gap
    );
}

// ---------------------------------------------------------------------------
// Feature 8: kill -l (list signals)
// ---------------------------------------------------------------------------

#[test]
fn kill_l_lists_signals() {
    let mut s = shell();
    let (out, code, _) = s.run_line("kill -l");
    assert_eq!(code, 0);
    assert!(out.contains("TERM"), "kill -l should contain TERM: {}", out);
    assert!(out.contains("INT"), "kill -l should contain INT: {}", out);
    assert!(out.contains("HUP"), "kill -l should contain HUP: {}", out);
}

#[test]
fn wait_deferred_completed_pid() {
    let mut s = shell_with_bg_mode("deferred");
    s.run_line("true &");
    let (bang, _, _) = s.run_line("echo $!");
    let pid = bang.trim_end_matches('\n');
    let (_, code, _) = s.run_line(&format!("wait {}", pid));
    assert_eq!(code, 0, "wait for true in deferred should return 0");
}

#[test]
fn wait_deferred_failing_pid() {
    let mut s = shell_with_bg_mode("deferred");
    s.run_line("false &");
    let (bang, _, _) = s.run_line("echo $!");
    let pid = bang.trim_end_matches('\n');
    let (_, code, _) = s.run_line(&format!("wait {}", pid));
    assert_eq!(code, 1, "wait for false in deferred should return 1");
}
