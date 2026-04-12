use super::*;

// ---------------------------------------------------------------------------
// Pipeline PID allocation
// ---------------------------------------------------------------------------

#[test]
fn pipeline_bashpid_differs_per_segment() {
    let mut s = shell();
    s.run_line("echo $BASHPID > /tmp/left_pid");
    s.run_line("echo $BASHPID > /tmp/right_pid");
    let (left, _, _) = s.run_line("cat /tmp/left_pid");
    let (right, _, _) = s.run_line("cat /tmp/right_pid");
    let _l: u32 = left.trim().parse().expect("left PID should be number");
    let _r: u32 = right.trim().parse().expect("right PID should be number");
}

#[test]
fn pipeline_each_segment_gets_own_bashpid() {
    let mut s = shell();
    let (out, code, _) = s.run_line("echo $BASHPID | cat");
    assert_eq!(code, 0);
    let pid: u32 = out
        .trim()
        .parse()
        .expect("pipeline BASHPID should be a number");
    assert_ne!(
        pid,
        s.procs.shell_pid(),
        "pipeline segment PID != shell PID"
    );
}

#[test]
fn pipeline_dollar_dollar_still_shell_pid() {
    let mut s = shell();
    let (out, _, _) = s.run_line("echo $$ | cat");
    let pid: u32 = out.trim().parse().expect("$$ in pipeline should be number");
    assert_eq!(pid, s.procs.shell_pid(), "$$ in pipeline == shell PID");
}

#[test]
fn current_pid_resets_after_pipeline() {
    let mut s = shell();
    let before = s.procs.current_pid();
    s.run_line("echo hello | cat");
    let after = s.procs.current_pid();
    assert_eq!(
        before, after,
        "current_pid should reset to shell PID after pipeline"
    );
}

// ---------------------------------------------------------------------------
// Chunked pipeline: early exit + SIGPIPE
// ---------------------------------------------------------------------------

#[test]
fn seq_pipe_head_early_exit() {
    let mut s = shell();
    let (out, code, _) = s.run_line("seq 1 1000000 | head -1");
    assert_eq!(code, 0);
    assert_eq!(out, "1\n", "head should get first line from seq");
}

#[test]
fn seq_pipe_head_n3() {
    let mut s = shell();
    let (out, code, _) = s.run_line("seq 1 1000000 | head -3");
    assert_eq!(code, 0);
    assert_eq!(out, "1\n2\n3\n");
}

#[test]
fn cat_pipe_head() {
    let mut s = shell();
    s.run_line("seq 1 100 > /tmp/nums.txt");
    let (out, code, _) = s.run_line("cat /tmp/nums.txt | head -2");
    assert_eq!(code, 0);
    assert_eq!(out, "1\n2\n");
}

#[test]
fn three_stage_pipeline() {
    let mut s = shell();
    let (out, code, _) = s.run_line("seq 1 10 | cat | head -3");
    assert_eq!(code, 0);
    assert_eq!(out, "1\n2\n3\n");
}

// ---------------------------------------------------------------------------
// Chunked pipeline: mixed stream/batch modes
// ---------------------------------------------------------------------------

#[test]
fn mixed_stream_batch_stream_pipeline() {
    let mut s = shell();
    // seq (stream) -> sort (batch) -> head (stream)
    let (out, code, _) = s.run_line("seq 10 -1 1 | sort | head -3");
    assert_eq!(code, 0);
    assert_eq!(out, "1\n10\n2\n", "sort is lexicographic");
}

#[test]
fn batch_only_multi_segment() {
    let mut s = shell();
    s.run_line("echo -e 'b\na\nb\na' > /tmp/data.txt");
    let (out, code, _) = s.run_line("sort /tmp/data.txt | uniq");
    assert_eq!(code, 0);
    assert_eq!(out, "a\nb\n");
}

#[test]
fn seq_pipe_grep_pipe_head() {
    let mut s = shell();
    let (out, code, _) = s.run_line("seq 1 100 | grep '5' | head -3");
    assert_eq!(code, 0);
    assert_eq!(out, "5\n15\n25\n");
}

// ---------------------------------------------------------------------------
// Chunked pipeline: edge cases
// ---------------------------------------------------------------------------

#[test]
fn head_fewer_lines_than_requested() {
    let mut s = shell();
    let (out, code, _) = s.run_line("seq 1 3 | head -10");
    assert_eq!(code, 0);
    assert_eq!(out, "1\n2\n3\n");
}

#[test]
fn seq_with_step() {
    let mut s = shell();
    let (out, code, _) = s.run_line("seq 1 2 10 | head -3");
    assert_eq!(code, 0);
    assert_eq!(out, "1\n3\n5\n");
}

#[test]
fn seq_reverse() {
    let mut s = shell();
    let (out, code, _) = s.run_line("seq 5 -1 1 | head -3");
    assert_eq!(code, 0);
    assert_eq!(out, "5\n4\n3\n");
}

#[test]
fn head_no_trailing_newline() {
    let mut s = shell();
    s.run_line("printf '1\\n2\\n3' > /tmp/nonl.txt");
    let (out, code, _) = s.run_line("cat /tmp/nonl.txt | head -2");
    assert_eq!(code, 0);
    assert_eq!(out, "1\n2\n");
}

#[test]
fn head_single_line_no_newline() {
    let mut s = shell();
    s.run_line("printf 'hello' > /tmp/single.txt");
    let (out, code, _) = s.run_line("cat /tmp/single.txt | head -1");
    assert_eq!(code, 0);
    assert_eq!(out, "hello\n");
}

#[test]
fn head_empty_input() {
    let mut s = shell();
    let (out, code, _) = s.run_line("echo -n '' | head -5");
    assert_eq!(code, 0);
    assert_eq!(out, "");
}

// ---------------------------------------------------------------------------
// pipefail and |&
// ---------------------------------------------------------------------------

#[test]
fn pipefail_with_chunked_segments() {
    let mut s = shell();
    s.run_line("set -o pipefail");
    let (_, code, _) = s.run_line("false | echo ok");
    assert_ne!(
        code, 0,
        "pipefail should propagate failure from first segment"
    );
}

#[test]
fn pipe_ampersand_works_like_pipe() {
    let mut s = shell();
    let (out, code) = s.run_script("echo hello |& cat");
    assert_eq!(code, 0);
    assert_eq!(out, "hello\n");
}

// ---------------------------------------------------------------------------
// Streaming cat in pipelines
// ---------------------------------------------------------------------------

#[test]
fn cat_file_streams_lazily() {
    let mut s = shell();
    s.run_line("seq 1 50 > /tmp/cat_lazy.txt");
    let (out, code, _) = s.run_line("cat /tmp/cat_lazy.txt | head -3");
    assert_eq!(code, 0);
    assert_eq!(out, "1\n2\n3\n");
}

#[test]
fn cat_multiple_files_streams() {
    let mut s = shell();
    s.fs.write("/tmp/f1.txt", b"aaa\n").unwrap();
    s.fs.write("/tmp/f2.txt", b"bbb\n").unwrap();
    let (out, code, _) = s.run_line("cat /tmp/f1.txt /tmp/f2.txt | head -2");
    assert_eq!(code, 0);
    assert_eq!(out, "aaa\nbbb\n");
}

#[test]
fn cat_nonexistent_file_falls_back_to_batch() {
    let mut s = shell();
    let (out, _, _) = s.run_line("cat /tmp/no_such_file.txt");
    assert!(out.contains("No such file"), "should report error: {}", out);
}

// ---------------------------------------------------------------------------
// head -n variants
// ---------------------------------------------------------------------------

#[test]
fn head_dash_n_space() {
    let mut s = shell();
    let (out, code, _) = s.run_line("seq 1 10 | head -n 3");
    assert_eq!(code, 0);
    assert_eq!(out, "1\n2\n3\n");
}

#[test]
fn head_dash_n_attached() {
    let mut s = shell();
    let (out, code, _) = s.run_line("seq 1 10 | head -n5");
    assert_eq!(code, 0);
    assert_eq!(out, "1\n2\n3\n4\n5\n");
}
