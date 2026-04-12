/// Chunked pipeline execution engine.
///
/// Simulates Linux pipe semantics (bounded buffer, SIGPIPE, demand-driven
/// scheduling) in a single-threaded WASM environment via time-division
/// multiplexing.
use crate::script::word::SimpleCommand;
use crate::shell::Shell;

// ---------------------------------------------------------------------------
// Pipe buffer
// ---------------------------------------------------------------------------

/// Soft cap for pipe buffer size (64 KB, matching Linux default).
/// Streaming commands respect this limit per chunk. Batch commands may exceed
/// it when producing output atomically — this is intentional and matches the
/// Linux behaviour where a single `write()` larger than PIPE_BUF is allowed.
const PIPE_BUF_SOFT_CAP: usize = 65536;

/// VFS ticks per simulated second.  `sleep 1` advances the VFS clock by this
/// amount; scheduler quantum boundaries advance by `QUANTUM_TICKS`.
pub(crate) const TICKS_PER_SECOND: u64 = 1000;

/// VFS ticks per scheduler quantum (~10 ms simulated wall time).
pub(crate) const QUANTUM_TICKS: u64 = 10;

/// Bounded byte buffer between two pipeline segments.
pub(crate) struct PipeBuffer {
    data: Vec<u8>,
    consumed: usize,
    pub producer_done: bool,
}

impl PipeBuffer {
    pub fn new() -> Self {
        Self {
            data: Vec::with_capacity(PIPE_BUF_SOFT_CAP),
            consumed: 0,
            producer_done: false,
        }
    }

    /// Bytes available for reading.
    pub fn readable(&self) -> &[u8] {
        &self.data[self.consumed..]
    }

    /// How many bytes the producer can still write before hitting the cap.
    #[cfg(test)]
    pub fn writable_len(&self) -> usize {
        PIPE_BUF_SOFT_CAP.saturating_sub(self.data.len() - self.consumed)
    }

    /// Append data (producer side).
    pub fn push(&mut self, data: &[u8]) {
        self.data.extend_from_slice(data);
    }

    /// Mark `n` bytes as consumed (consumer side).
    pub fn consume(&mut self, n: usize) {
        self.consumed += n;
        // Compact when the consumed prefix exceeds the capacity to avoid
        // unbounded growth of the underlying Vec.
        if self.consumed > PIPE_BUF_SOFT_CAP {
            self.data.drain(..self.consumed);
            self.consumed = 0;
        }
    }

    pub fn is_empty(&self) -> bool {
        self.consumed >= self.data.len()
    }

    /// True when upstream is done AND all data has been consumed.
    pub fn eof(&self) -> bool {
        self.producer_done && self.is_empty()
    }
}

// ---------------------------------------------------------------------------
// ChunkIter trait
// ---------------------------------------------------------------------------

/// Result of a single step of a streaming command.
pub(crate) enum StepResult {
    /// Produced output and consumed `consumed` bytes of input.  Continue.
    Continue { consumed: usize },
    /// Cannot make progress without more input.
    NeedInput,
    /// Command finished with the given exit code.
    Done(i32),
}

/// A command that can execute incrementally, one chunk at a time.
///
/// The scheduler guarantees that only one segment calls `step()` at a time,
/// so the `&mut Shell` borrow is safe.
pub(crate) trait ChunkIter {
    /// Execute one chunk of work.
    ///
    /// * `shell`  — mutable shell state (filesystem, variables, etc.)
    /// * `input`  — bytes available from the upstream pipe buffer
    /// * `eof`    — upstream has finished and buffer is drained
    /// * `output` — write output bytes here (goes into downstream buffer)
    fn step(
        &mut self,
        shell: &mut Shell,
        input: &[u8],
        eof: bool,
        output: &mut Vec<u8>,
    ) -> StepResult;
}

// ---------------------------------------------------------------------------
// Streaming command implementations
// ---------------------------------------------------------------------------

/// `seq START STEP END` — lazy producer.
pub(crate) struct SeqIter {
    current: i64,
    end: i64,
    step: i64,
}

impl SeqIter {
    pub fn new(start: i64, step: i64, end: i64) -> Self {
        Self {
            current: start,
            end,
            step,
        }
    }
}

impl ChunkIter for SeqIter {
    fn step(
        &mut self,
        _shell: &mut Shell,
        _input: &[u8],
        _eof: bool,
        output: &mut Vec<u8>,
    ) -> StepResult {
        let mut written = 0;
        if self.step > 0 {
            while self.current <= self.end && written < PIPE_BUF_SOFT_CAP {
                let s = self.current.to_string();
                output.extend_from_slice(s.as_bytes());
                output.push(b'\n');
                written += s.len() + 1;
                self.current += self.step;
            }
        } else if self.step < 0 {
            while self.current >= self.end && written < PIPE_BUF_SOFT_CAP {
                let s = self.current.to_string();
                output.extend_from_slice(s.as_bytes());
                output.push(b'\n');
                written += s.len() + 1;
                self.current += self.step;
            }
        }
        if (self.step > 0 && self.current > self.end)
            || (self.step < 0 && self.current < self.end)
            || self.step == 0
        {
            StepResult::Done(0)
        } else {
            StepResult::Continue { consumed: 0 }
        }
    }
}

/// `head -N` — early-exit consumer.
pub(crate) struct HeadIter {
    remaining: usize,
    /// Partial line leftover from a previous chunk.
    partial: Vec<u8>,
}

impl HeadIter {
    pub fn new(n: usize) -> Self {
        Self {
            remaining: n,
            partial: Vec::new(),
        }
    }
}

impl ChunkIter for HeadIter {
    fn step(
        &mut self,
        _shell: &mut Shell,
        input: &[u8],
        eof: bool,
        output: &mut Vec<u8>,
    ) -> StepResult {
        if self.remaining == 0 {
            return StepResult::Done(0);
        }

        if input.is_empty() && self.partial.is_empty() {
            return if eof {
                StepResult::Done(0)
            } else {
                StepResult::NeedInput
            };
        }

        // Combine any leftover partial line with new input.
        let had_partial = !self.partial.is_empty();
        let work = if had_partial {
            let mut combined = std::mem::take(&mut self.partial);
            combined.extend_from_slice(input);
            combined
        } else {
            input.to_vec()
        };

        let mut pos = 0;
        while pos < work.len() && self.remaining > 0 {
            if let Some(nl) = work[pos..].iter().position(|&b| b == b'\n') {
                // Full line found.
                output.extend_from_slice(&work[pos..pos + nl]);
                output.push(b'\n');
                self.remaining -= 1;
                pos += nl + 1;
            } else {
                // No newline — partial line at end of buffer.
                if eof {
                    let tail = &work[pos..];
                    if !tail.is_empty() {
                        output.extend_from_slice(tail);
                        output.push(b'\n');
                        self.remaining -= 1;
                    }
                    pos = work.len();
                } else {
                    // Save partial for next step.
                    self.partial = work[pos..].to_vec();
                    // We consumed all of `input` (it's been merged into work).
                    return StepResult::Continue {
                        consumed: input.len(),
                    };
                }
                break;
            }
        }

        // Calculate how many bytes of the original `input` were consumed.
        // If we had a partial prefix, subtract its length from pos.
        let consumed = if had_partial {
            pos.saturating_sub(work.len() - input.len())
        } else {
            pos
        };

        if self.remaining == 0 || eof {
            StepResult::Done(0)
        } else {
            StepResult::Continue { consumed }
        }
    }
}

/// `cat` — streaming passthrough (reads file or passes stdin).
pub(crate) struct CatIter {
    /// File paths to read sequentially (empty = stdin mode).
    files: Vec<String>,
    /// Index into `files` for the current file being read.
    file_idx: usize,
    /// Byte offset within the current file.
    file_pos: usize,
}

impl CatIter {
    /// Cat from stdin (no file args).
    pub fn stdin() -> Self {
        Self {
            files: Vec::new(),
            file_idx: 0,
            file_pos: 0,
        }
    }

    /// Cat from file path(s) — reads lazily from VFS on each step.
    pub fn from_files(files: Vec<String>) -> Self {
        Self {
            files,
            file_idx: 0,
            file_pos: 0,
        }
    }
}

impl ChunkIter for CatIter {
    fn step(
        &mut self,
        shell: &mut Shell,
        input: &[u8],
        eof: bool,
        output: &mut Vec<u8>,
    ) -> StepResult {
        if self.files.is_empty() {
            // Stdin passthrough mode.
            if !input.is_empty() {
                output.extend_from_slice(input);
                StepResult::Continue {
                    consumed: input.len(),
                }
            } else if eof {
                StepResult::Done(0)
            } else {
                StepResult::NeedInput
            }
        } else {
            // File mode: read one chunk per step from the current file.
            if self.file_idx >= self.files.len() {
                return StepResult::Done(0);
            }
            let path = &self.files[self.file_idx];
            match shell.fs.read(path) {
                Ok(data) => {
                    let remaining = &data[self.file_pos..];
                    let chunk = remaining.len().min(PIPE_BUF_SOFT_CAP);
                    if chunk > 0 {
                        output.extend_from_slice(&remaining[..chunk]);
                        self.file_pos += chunk;
                    }
                    if self.file_pos >= data.len() {
                        // Move to next file.
                        self.file_idx += 1;
                        self.file_pos = 0;
                    }
                    if self.file_idx >= self.files.len() {
                        StepResult::Done(0)
                    } else {
                        StepResult::Continue { consumed: 0 }
                    }
                }
                Err(_) => StepResult::Done(1),
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Segment builder — decide Stream vs Batch for a command
// ---------------------------------------------------------------------------

/// Try to create a streaming ChunkIter for a command.
/// Returns None if the command should use batch mode.
pub(crate) fn try_make_stream(
    shell: &mut Shell,
    cmd: &SimpleCommand,
) -> Option<Box<dyn ChunkIter>> {
    // Only pure commands (no assignments, no redirects) are candidates.
    if !cmd.assignments.is_empty() || !cmd.redirects.is_empty() {
        return None;
    }
    if cmd.words.is_empty() {
        return None;
    }

    // Expand the command name to determine which command this is.
    let name_parts = shell.expand_word(&cmd.words[0]);
    let cmd_name = name_parts.first()?;

    match cmd_name.as_str() {
        "seq" => {
            // Expand args to get numeric values.
            let mut nums: Vec<i64> = Vec::new();
            for w in &cmd.words[1..] {
                for expanded in shell.expand_word(w) {
                    if let Ok(n) = expanded.parse::<i64>() {
                        nums.push(n);
                    }
                }
            }
            let (start, step, end) = match nums.len() {
                1 => (1, 1, nums[0]),
                2 => (nums[0], 1, nums[1]),
                3 => (nums[0], nums[1], nums[2]),
                _ => return None,
            };
            if step == 0 {
                return None;
            }
            Some(Box::new(SeqIter::new(start, step, end)))
        }
        "head" => {
            let mut n: usize = 10;
            let mut has_file = false;
            let mut expect_n = false;
            let mut all_args: Vec<String> = Vec::new();
            for w in &cmd.words[1..] {
                all_args.extend(shell.expand_word(w));
            }
            for arg in &all_args {
                if expect_n {
                    if let Ok(num) = arg.parse::<usize>() {
                        n = num;
                    }
                    expect_n = false;
                } else if arg == "-n" {
                    expect_n = true;
                } else if let Some(num_str) = arg.strip_prefix('-') {
                    if let Ok(num) = num_str.parse::<usize>() {
                        n = num;
                    } else if let Some(n_rest) = num_str.strip_prefix('n') {
                        // -n5 form
                        if let Ok(num) = n_rest.parse::<usize>() {
                            n = num;
                        } else {
                            return None; // unrecognized flag, fall back to batch
                        }
                    } else {
                        return None; // -c or other unsupported flag
                    }
                } else {
                    has_file = true;
                }
            }
            // Only stream when reading from stdin (no file args).
            if has_file {
                return None;
            }
            Some(Box::new(HeadIter::new(n)))
        }
        "cat" => {
            // Check for file args.
            let mut file_args: Vec<String> = Vec::new();
            let mut has_flags = false;
            for w in &cmd.words[1..] {
                for arg in shell.expand_word(w) {
                    if arg.starts_with('-') {
                        has_flags = true;
                    } else {
                        file_args.push(arg);
                    }
                }
            }
            if has_flags {
                return None; // cat -n etc. not handled in stream mode
            }
            if file_args.is_empty() {
                // cat from stdin — stream passthrough
                Some(Box::new(CatIter::stdin()))
            } else {
                // cat with files — resolve paths, read lazily per step
                let resolved: Vec<String> = file_args.iter().map(|f| shell.resolve(f)).collect();
                // Verify all files exist before committing to stream mode
                for path in &resolved {
                    if !shell.fs.is_file(path) {
                        return None; // fall back to batch for error handling
                    }
                }
                Some(Box::new(CatIter::from_files(resolved)))
            }
        }
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Background job
// ---------------------------------------------------------------------------

/// A deferred background job that can be stepped incrementally.
pub(crate) struct BackgroundJob {
    pub pid: u32,
    pub cmd: String,
    pub segments: Vec<PipelineSegment>,
    pub buffers: Vec<PipeBuffer>,
}

// ---------------------------------------------------------------------------
// Pipeline segment
// ---------------------------------------------------------------------------

/// Batch-mode state: accumulates all stdin, then runs exec_simple_command once.
pub(crate) struct BatchState {
    pub cmd: SimpleCommand,
    pub input_buf: Vec<u8>,
    pub input_limit: usize,
}

const BATCH_INPUT_LIMIT: usize = 16 * 1024 * 1024; // 16 MB

impl BatchState {
    pub fn new(cmd: SimpleCommand) -> Self {
        Self {
            cmd,
            input_buf: Vec::new(),
            input_limit: BATCH_INPUT_LIMIT,
        }
    }
}

/// How a pipeline segment executes.
pub(crate) enum SegmentExec {
    /// Streaming — can produce/consume incrementally.
    Stream(Box<dyn ChunkIter>),
    /// Batch — must collect all input before running.
    Batch(BatchState),
}

/// A single segment in a chunked pipeline.
pub(crate) struct PipelineSegment {
    #[allow(dead_code)]
    pub pid: u32,
    pub exec: SegmentExec,
    /// None while running, Some(code) once exited.
    pub exited: Option<i32>,
}

// ---------------------------------------------------------------------------
// Scheduler
// ---------------------------------------------------------------------------

/// SIGPIPE exit code (128 + 13).
const SIGPIPE_EXIT: i32 = 141;

/// Result of a single scheduler quantum.
pub(crate) enum PipelineStatus {
    /// Pipeline still running — call again to continue.
    InProgress,
    /// All segments have exited.
    Done,
}

impl Shell {
    /// Run a multi-segment pipeline using the chunked scheduler.
    ///
    /// When `single_quantum` is false, runs to completion (foreground).
    /// When `single_quantum` is true, runs one scheduler iteration and returns
    /// `PipelineStatus::InProgress` or `PipelineStatus::Done`.
    ///
    /// Returns (final_output, per-segment exit codes, status).
    pub(crate) fn run_pipeline_chunked(
        &mut self,
        segments: &mut [PipelineSegment],
        buffers: &mut [PipeBuffer],
        initial_stdin: Option<&str>,
    ) -> (String, Vec<i32>) {
        self.run_pipeline_inner(segments, buffers, initial_stdin, false)
            .0
    }

    /// Run one quantum of a pipeline. Returns (accumulated_output, status).
    pub(crate) fn run_pipeline_quantum(
        &mut self,
        segments: &mut [PipelineSegment],
        buffers: &mut [PipeBuffer],
    ) -> PipelineStatus {
        self.run_pipeline_inner(segments, buffers, None, true).1
    }

    fn run_pipeline_inner(
        &mut self,
        segments: &mut [PipelineSegment],
        buffers: &mut [PipeBuffer],
        initial_stdin: Option<&str>,
        single_quantum: bool,
    ) -> ((String, Vec<i32>), PipelineStatus) {
        // initial_stdin feeds segment 0 via seg0_stdin (NOT buffers[0],
        // which connects segment 0's output to segment 1's input).
        let mut seg0_stdin: Option<Vec<u8>> = initial_stdin.map(|s| s.as_bytes().to_vec());

        let mut final_output: Vec<u8> = Vec::new();

        loop {
            let mut progress = false;

            // Demand-driven: iterate right to left.
            for i in (0..segments.len()).rev() {
                if segments[i].exited.is_some() {
                    continue;
                }

                // Determine input source and eof for this segment.
                let (input_slice, input_eof) = if i == 0 {
                    if let Some(ref buf) = seg0_stdin {
                        (buf.as_slice(), true)
                    } else {
                        (&[][..], true)
                    }
                } else {
                    let buf = &buffers[i - 1];
                    (buf.readable(), buf.eof())
                };

                let is_last = i == segments.len() - 1;

                match &mut segments[i].exec {
                    SegmentExec::Stream(iter) => {
                        let mut chunk_out = Vec::new();
                        let result = iter.step(self, input_slice, input_eof, &mut chunk_out);

                        let consumed = match &result {
                            StepResult::Continue { consumed } => *consumed,
                            StepResult::NeedInput => 0,
                            StepResult::Done(_) => 0,
                        };

                        if i == 0 {
                            if consumed > 0 {
                                if let Some(ref mut buf) = seg0_stdin {
                                    buf.drain(..consumed);
                                }
                                progress = true;
                            }
                        } else if consumed > 0 {
                            buffers[i - 1].consume(consumed);
                            progress = true;
                        }

                        if !chunk_out.is_empty() {
                            if is_last {
                                final_output.extend_from_slice(&chunk_out);
                            } else {
                                buffers[i].push(&chunk_out);
                            }
                            progress = true;
                        }

                        if let StepResult::Done(code) = result {
                            segments[i].exited = Some(code);
                            if !is_last {
                                buffers[i].producer_done = true;
                            }
                            sigpipe_upstream(segments, buffers, i);
                            progress = true;
                        }
                    }

                    SegmentExec::Batch(state) => {
                        if !input_eof {
                            // Accumulate input.
                            if state.input_buf.len() + input_slice.len() > state.input_limit {
                                segments[i].exited = Some(1);
                                progress = true;
                                continue;
                            }
                            state.input_buf.extend_from_slice(input_slice);
                            let len = input_slice.len();
                            if i == 0 {
                                if let Some(ref mut buf) = seg0_stdin {
                                    buf.drain(..len);
                                }
                            } else if len > 0 {
                                buffers[i - 1].consume(len);
                            }
                            if len > 0 {
                                progress = true;
                            }
                        } else {
                            // All input arrived — run via exec_simple_command.
                            let stdin_str = if state.input_buf.is_empty() {
                                None
                            } else {
                                Some(String::from_utf8_lossy(&state.input_buf).to_string())
                            };
                            let mut xtrace_lines: Vec<String> = Vec::new();
                            let (output, code, _) = self.exec_simple_command(
                                &state.cmd,
                                stdin_str.as_deref(),
                                &mut xtrace_lines,
                            );
                            // Prepend any xtrace output
                            if !xtrace_lines.is_empty() {
                                let trace = xtrace_lines.join("\n");
                                let combined = if output.is_empty() {
                                    trace
                                } else {
                                    format!("{}\n{}", trace, output)
                                };
                                if is_last {
                                    final_output.extend_from_slice(combined.as_bytes());
                                } else {
                                    buffers[i].push(combined.as_bytes());
                                    buffers[i].producer_done = true;
                                }
                            } else {
                                if is_last {
                                    final_output.extend_from_slice(output.as_bytes());
                                } else {
                                    buffers[i].push(output.as_bytes());
                                    buffers[i].producer_done = true;
                                }
                            }
                            segments[i].exited = Some(code);
                            progress = true;
                        }
                    }
                }
            }

            // Quantum boundary: advance VFS clock.
            self.fs.set_time(self.fs.time() + QUANTUM_TICKS);

            if !progress {
                break;
            }
            if segments.iter().all(|s| s.exited.is_some()) {
                break;
            }
            if single_quantum {
                // Single-quantum mode: return after one iteration.
                let codes: Vec<i32> = segments.iter().map(|s| s.exited.unwrap_or(0)).collect();
                let output = String::from_utf8_lossy(&final_output).to_string();
                return ((output, codes), PipelineStatus::InProgress);
            }
        }

        let status = if segments.iter().all(|s| s.exited.is_some()) {
            PipelineStatus::Done
        } else {
            PipelineStatus::InProgress
        };
        let codes: Vec<i32> = segments.iter().map(|s| s.exited.unwrap_or(0)).collect();
        let output = String::from_utf8_lossy(&final_output).to_string();
        ((output, codes), status)
    }

    /// Step all background jobs one round. Returns completion notifications
    /// for jobs that finished (e.g. "[1]+ Done  cmd").
    pub(crate) fn step_bg_jobs(&mut self) -> Vec<String> {
        // Take bg_jobs out to satisfy the borrow checker — run_pipeline_quantum
        // needs &mut self, which conflicts with borrowing self.bg_jobs.
        // We reuse the Vec's allocation by putting remaining jobs back.
        let mut jobs = std::mem::take(&mut self.bg_jobs);
        let mut completions = Vec::new();

        for mut job in jobs.drain(..) {
            if !job.segments.iter().all(|s| s.exited.is_some()) {
                self.run_pipeline_quantum(&mut job.segments, &mut job.buffers);
            }

            if job.segments.iter().all(|s| s.exited.is_some()) {
                let code = job.segments.last().and_then(|s| s.exited).unwrap_or(0);
                self.procs.finish_job(job.pid, code);
                let job_num = self
                    .procs
                    .jobs
                    .iter()
                    .position(|p| p.pid == job.pid)
                    .map(|j| j + 1)
                    .unwrap_or(0);
                let status = if code == 0 { "Done" } else { "Exit" };
                let exit_suffix = if code != 0 {
                    format!(" {}", code)
                } else {
                    String::new()
                };
                let status_field = format!("{}{}", status, exit_suffix);
                completions.push(format!("[{}]+  {:<24}{}", job_num, status_field, job.cmd));
                // Job is dropped — not put back.
            } else {
                self.bg_jobs.push(job);
            }
        }

        completions
    }

    /// Run a specific background job to completion (used by `wait PID`).
    pub(crate) fn run_bg_job_to_completion(&mut self, pid: u32) -> Option<i32> {
        let idx = self.bg_jobs.iter().position(|j| j.pid == pid)?;
        let mut job = self.bg_jobs.remove(idx);

        // Step until all segments exit
        while !job.segments.iter().all(|s| s.exited.is_some()) {
            self.run_pipeline_quantum(&mut job.segments, &mut job.buffers);
        }

        let code = job.segments.last().and_then(|s| s.exited).unwrap_or(0);
        self.procs.finish_job(job.pid, code);
        Some(code)
    }

    /// Run ALL background jobs to completion (used by bare `wait`).
    pub(crate) fn run_all_bg_jobs(&mut self) -> i32 {
        let mut last_code = 0;
        while !self.bg_jobs.is_empty() {
            let pid = self.bg_jobs[0].pid;
            if let Some(code) = self.run_bg_job_to_completion(pid) {
                last_code = code;
            }
        }
        last_code
    }
}

/// SIGPIPE: when a consumer exits, kill all upstream segments.
fn sigpipe_upstream(
    segments: &mut [PipelineSegment],
    buffers: &mut [PipeBuffer],
    consumer_idx: usize,
) {
    for j in (0..consumer_idx).rev() {
        if segments[j].exited.is_none() {
            segments[j].exited = Some(SIGPIPE_EXIT);
        }
        if j > 0 {
            buffers[j - 1].producer_done = true;
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pipe_buffer_push_and_read() {
        let mut buf = PipeBuffer::new();
        assert!(buf.is_empty());
        buf.push(b"hello\n");
        assert_eq!(buf.readable(), b"hello\n");
        assert!(!buf.is_empty());
    }

    #[test]
    fn pipe_buffer_consume() {
        let mut buf = PipeBuffer::new();
        buf.push(b"line1\nline2\n");
        buf.consume(6); // consume "line1\n"
        assert_eq!(buf.readable(), b"line2\n");
    }

    #[test]
    fn pipe_buffer_eof() {
        let mut buf = PipeBuffer::new();
        buf.push(b"data");
        buf.producer_done = true;
        assert!(!buf.eof()); // data still unread
        buf.consume(4);
        assert!(buf.eof()); // now truly done
    }

    #[test]
    fn pipe_buffer_writable_len() {
        let buf = PipeBuffer::new();
        assert_eq!(buf.writable_len(), PIPE_BUF_SOFT_CAP);
    }

    #[test]
    fn pipe_buffer_compact() {
        let mut buf = PipeBuffer::new();
        // Fill beyond capacity then consume to trigger compact.
        let chunk = vec![b'x'; PIPE_BUF_SOFT_CAP + 1];
        buf.push(&chunk);
        buf.consume(PIPE_BUF_SOFT_CAP + 1); // triggers compact
        assert!(buf.is_empty());
        assert_eq!(buf.data.len(), 0);
    }
}
