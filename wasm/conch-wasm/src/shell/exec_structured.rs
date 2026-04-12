/// Structured command executor.
///
/// Walks `CommandList` / `StructuredPipeline` / `SimpleCommand` AST nodes
/// directly, using the word-level expansion engine (`expand_word`) instead
/// of the old string-based parsing pipeline.
use crate::script::word::*;

impl super::Shell {
    /// Execute a CommandList (chain of pipelines with &&/||/; operators).
    pub fn exec_command_list(&mut self, list: &CommandList) -> (String, i32, Option<String>) {
        self.exec_command_list_with_stdin(list, None)
    }

    /// Execute a CommandList with optional initial stdin for the first pipeline.
    pub fn exec_command_list_with_stdin(
        &mut self,
        list: &CommandList,
        initial_stdin: Option<&str>,
    ) -> (String, i32, Option<String>) {
        let mut all_output = String::new();
        let mut final_code: i32 = 0;
        let mut final_lang: Option<String> = None;

        for (i, (pipeline, op)) in list.items.iter().enumerate() {
            // Determine if we're inside an && / || chain (for errexit suppression)
            let in_logic_chain = if i > 0 {
                matches!(list.items[i - 1].1, Some(ChainOp::And) | Some(ChainOp::Or))
            } else {
                false
            };

            // Chain operator logic
            if i > 0 {
                if let Some(ref prev_op) = list.items[i - 1].1 {
                    match prev_op {
                        ChainOp::And if final_code != 0 => continue,
                        ChainOp::Or if final_code == 0 => continue,
                        _ => {}
                    }
                }
            }

            // Only pass initial stdin to the first pipeline in the chain
            let pipe_stdin = if i == 0 { initial_stdin } else { None };
            let is_background = matches!(op, Some(ChainOp::Background));

            // For background jobs, pre-allocate PID so $BASHPID inside == $! after
            let bg_pid = if is_background {
                let pid = self.procs.alloc_pid();
                self.procs.set_current(pid);
                self.procs.last_bg_pid = Some(pid);
                Some(pid)
            } else {
                None
            };

            if let Some(pid) = bg_pid {
                use crate::types::BackgroundMode;
                let cmd_source = pipeline.to_source();

                match self.bg_mode {
                    BackgroundMode::Sync => {
                        // Run immediately to completion (current behavior)
                        let (_, code, _) =
                            self.exec_structured_pipeline(pipeline, pipe_stdin, Some(pid));
                        self.procs.record_bg(pid, &cmd_source, code);
                    }
                    BackgroundMode::Deferred | BackgroundMode::Interleaved => {
                        // Build segments but don't execute — defer to wait/fg/interleave
                        use crate::shell::pipeline::{
                            try_make_stream, BackgroundJob, BatchState, PipeBuffer,
                            PipelineSegment, SegmentExec,
                        };
                        let segment_count = pipeline.commands.len();
                        let mut segments: Vec<PipelineSegment> = Vec::with_capacity(segment_count);
                        for (seg_idx, cmd) in pipeline.commands.iter().enumerate() {
                            let seg_pid = if seg_idx == 0 {
                                pid
                            } else {
                                self.procs.spawn(&cmd_source)
                            };
                            let exec = if let Some(stream) = try_make_stream(self, cmd) {
                                SegmentExec::Stream(stream)
                            } else {
                                SegmentExec::Batch(BatchState::new(cmd.clone()))
                            };
                            segments.push(PipelineSegment {
                                pid: seg_pid,
                                exec,
                                exited: None,
                            });
                        }
                        let buffers: Vec<PipeBuffer> = (0..segment_count.saturating_sub(1))
                            .map(|_| PipeBuffer::new())
                            .collect();
                        self.procs.record_bg_running(pid, &cmd_source);
                        self.bg_jobs.push(BackgroundJob {
                            pid,
                            cmd: cmd_source.clone(),
                            segments,
                            buffers,
                        });
                    }
                }

                let job_num = self.procs.jobs.len();
                let notification = format!("[{}] {}", job_num, pid);
                if !all_output.is_empty() {
                    all_output.push('\n');
                }
                all_output.push_str(&notification);
                self.procs.reset_current();
                final_code = 0;
                self.exec.last_exit_code = 0;
                self.vars.env.insert("?".into(), "0".to_string());
                continue;
            }

            let (output, code, lang) = self.exec_structured_pipeline(pipeline, pipe_stdin, None);
            final_lang = lang;

            // exec builtin: stop script execution (don't reset flag — let interpret_stmts see it)
            if self.exec.exec_pending {
                if !output.is_empty() {
                    if !all_output.is_empty() && !all_output.ends_with('\n') {
                        all_output.push('\n');
                    }
                    all_output.push_str(&output);
                }
                final_code = code;
                break;
            }

            if !output.is_empty() {
                if !all_output.is_empty() && !all_output.ends_with('\n') {
                    all_output.push('\n');
                }
                all_output.push_str(&output);
            }
            final_code = code;
            self.exec.last_exit_code = code;
            self.vars.env.insert("?".into(), code.to_string());

            // Interleaved mode: step background jobs after each foreground pipeline
            if self.bg_mode == crate::types::BackgroundMode::Interleaved && !self.bg_jobs.is_empty()
            {
                let completions = self.step_bg_jobs();
                self.pending_bg_completions.extend(completions);
            }

            // Handle ERR trap
            if code != 0 && self.exec.in_condition == 0 {
                if let Some(trap_cmd) = self.defs.traps.get("ERR").cloned() {
                    if !trap_cmd.is_empty() {
                        let (trap_out, _, _) = self.run_line(&trap_cmd);
                        if !trap_out.is_empty() {
                            if !all_output.is_empty() {
                                all_output.push('\n');
                            }
                            all_output.push_str(&trap_out);
                        }
                    }
                }
            }

            // Handle errexit
            if self.exec.opts.errexit
                && final_code != 0
                && self.exec.in_condition == 0
                && !in_logic_chain
            {
                let next_is_logic = matches!(op, Some(ChainOp::And) | Some(ChainOp::Or));
                if !next_is_logic {
                    break;
                }
            }
        }

        // Execute deferred >(cmd) process substitutions
        let deferred = std::mem::take(&mut self.deferred_process_substs);
        for (tmp_path, cmd) in deferred {
            let content = self
                .fs
                .read_to_string(&tmp_path)
                .map(|s| s.to_string())
                .unwrap_or_default();
            let snap = self.snapshot_subshell();
            self.run_line_with_stdin(&cmd, Some(&content));
            self.restore_subshell(snap);
            // Clean up temp file
            let _ = self.fs.remove_file(&tmp_path);
        }

        // Prune old completed jobs to prevent unbounded growth.
        self.procs.prune_done_jobs(10);

        (all_output, final_code, final_lang)
    }

    /// Execute a StructuredPipeline (commands connected by |).
    /// `preallocated_pid`: if Some, use this PID for the first segment (for background &).
    fn exec_structured_pipeline(
        &mut self,
        pipeline: &StructuredPipeline,
        initial_stdin: Option<&str>,
        preallocated_pid: Option<u32>,
    ) -> (String, i32, Option<String>) {
        use crate::shell::pipeline::{
            try_make_stream, BatchState, PipeBuffer, PipelineSegment, SegmentExec,
        };

        let segment_count = pipeline.commands.len();

        // Single command — fast path, no scheduler overhead.
        if segment_count == 1 {
            let cmd = &pipeline.commands[0];
            if preallocated_pid.is_none() {
                let mut src = String::new();
                cmd.write_source(&mut src);
                self.procs.spawn(&src);
            }
            let mut xtrace_lines = Vec::new();
            let (output, code, lang) =
                self.exec_simple_command(cmd, initial_stdin, &mut xtrace_lines);
            self.procs.reset_current();

            let output = if xtrace_lines.is_empty() {
                output
            } else {
                let trace = xtrace_lines.join("\n");
                if output.is_empty() {
                    trace
                } else {
                    format!("{}\n{}", trace, output)
                }
            };

            let final_code = if pipeline.bang {
                if code == 0 {
                    1
                } else {
                    0
                }
            } else {
                code
            };
            return (output, final_code, lang);
        }

        // Multi-segment pipeline — use the chunked scheduler.
        let mut segments: Vec<PipelineSegment> = Vec::with_capacity(segment_count);
        for (seg_idx, cmd) in pipeline.commands.iter().enumerate() {
            let pid = if seg_idx == 0 && preallocated_pid.is_some() {
                preallocated_pid.unwrap_or(0)
            } else {
                let mut src = String::new();
                cmd.write_source(&mut src);
                self.procs.spawn(&src)
            };
            let exec = if let Some(stream) = try_make_stream(self, cmd) {
                SegmentExec::Stream(stream)
            } else {
                SegmentExec::Batch(BatchState::new(cmd.clone()))
            };
            segments.push(PipelineSegment {
                pid,
                exec,
                exited: None,
            });
        }

        let mut buffers: Vec<PipeBuffer> =
            (0..segment_count - 1).map(|_| PipeBuffer::new()).collect();

        let (output, codes) = self.run_pipeline_chunked(&mut segments, &mut buffers, initial_stdin);

        self.procs.reset_current();

        // Determine final exit code.
        let mut last_code = *codes.last().unwrap_or(&0);

        // Pipefail: use the rightmost non-zero code.
        if self.exec.opts.pipefail && codes.len() > 1 {
            if let Some(&nonzero) = codes.iter().rev().find(|&&c| c != 0) {
                last_code = nonzero;
            }
        }

        // `! pipeline` — negate the exit code.
        if pipeline.bang {
            last_code = if last_code == 0 { 1 } else { 0 };
        }

        (output, last_code, None)
    }

    /// Execute a single SimpleCommand.
    pub(crate) fn exec_simple_command(
        &mut self,
        cmd: &SimpleCommand,
        stdin: Option<&str>,
        xtrace_lines: &mut Vec<String>,
    ) -> (String, i32, Option<String>) {
        // 0a. Detect (( expr )) arithmetic command
        //     The word parser produces words like ["((", "expr", "parts", "))"]
        //     or possibly ["((expr))"] when there are no spaces.
        if cmd.assignments.is_empty() && !cmd.words.is_empty() {
            let first_src = cmd.words[0].to_source();
            if first_src == "((" || first_src.starts_with("((") {
                // Reconstruct the full source and check for arithmetic command
                let full_source: String = cmd
                    .words
                    .iter()
                    .map(|w| w.to_source())
                    .collect::<Vec<_>>()
                    .join(" ");
                if let Some(inner) = super::Shell::extract_arith_command(&full_source) {
                    let (out, code) = self.cmd_arith(inner);
                    self.exec.last_exit_code = code;
                    self.vars.env.insert("?".into(), code.to_string());
                    return (out, code, None);
                }
            }
        }

        // 0b. Detect array assignment in raw source: name=(a b c)
        //     The word parser may not correctly parse these as assignments
        //     when the value contains parentheses.
        if cmd.assignments.is_empty() && !cmd.words.is_empty() {
            let full_source: String = cmd
                .words
                .iter()
                .map(|w| w.to_source())
                .collect::<Vec<_>>()
                .join(" ");
            if let Some((arr_name, arr_append, arr_body)) =
                super::Shell::detect_array_assignment(&full_source)
            {
                let elements = super::Shell::parse_array_elements(&arr_body, self);
                if arr_append {
                    self.vars
                        .arrays
                        .entry(arr_name.into())
                        .or_default()
                        .extend(elements);
                } else {
                    self.vars.arrays.insert(arr_name.into(), elements);
                }
                return (String::new(), 0, None);
            }

            // 0c. Detect associative/indexed array element assignment: name[key]=value
            if let Some((arr_name, key, raw_val)) =
                super::Shell::detect_assoc_elem_assignment(&full_source)
            {
                let val = self.expand_full(&raw_val);
                if self.vars.assoc_arrays.contains_key(arr_name.as_str()) {
                    if let Some(map) = self.vars.assoc_arrays.get_mut(arr_name.as_str()) {
                        map.insert(key, val);
                    }
                } else {
                    if let Ok(idx) = key.parse::<usize>() {
                        let arr = self.vars.arrays.entry(arr_name.into()).or_default();
                        while arr.len() <= idx {
                            arr.push(String::new());
                        }
                        arr[idx] = val;
                    }
                }
                return (String::new(), 0, None);
            }
        }

        // 1. Handle input redirects
        let effective_stdin = match self.resolve_structured_input_redirect(cmd, stdin) {
            Ok(s) => s,
            Err((msg, code)) => return (msg, code, None),
        };

        // 2. nounset: check for unbound variables before proceeding
        //    We reconstruct the source and use the existing check_nounset
        if self.exec.opts.nounset {
            let mut source_parts = String::new();
            for w in &cmd.words {
                if !source_parts.is_empty() {
                    source_parts.push(' ');
                }
                source_parts.push_str(&w.to_source());
            }
            for a in &cmd.assignments {
                match &a.value {
                    AssignValue::Scalar(w) => {
                        source_parts.push(' ');
                        source_parts.push_str(&w.to_source());
                    }
                    AssignValue::Array(words) => {
                        for w in words {
                            source_parts.push(' ');
                            source_parts.push_str(&w.to_source());
                        }
                    }
                }
            }
            if let Some(unbound) = self.check_nounset(&source_parts) {
                return (format!("conch: {}: unbound variable", unbound), 1, None);
            }
        }

        // 3. Process assignments
        for assignment in &cmd.assignments {
            match &assignment.value {
                AssignValue::Array(words) => {
                    let elements: Vec<String> =
                        words.iter().map(|w| self.expand_word_nosplit(w)).collect();
                    match assignment.op {
                        AssignOp::Assign => {
                            self.vars.arrays.insert(assignment.name.clone(), elements);
                        }
                        AssignOp::PlusAssign => {
                            self.vars
                                .arrays
                                .entry(assignment.name.clone())
                                .or_default()
                                .extend(elements);
                        }
                    }
                    continue;
                }
                AssignValue::Scalar(word) => {
                    let value = self.expand_word_nosplit(word);
                    match assignment.op {
                        AssignOp::Assign => {
                            if let Err(e) = self.vars.set(&assignment.name, value) {
                                return (e, 1, None);
                            }
                        }
                        AssignOp::PlusAssign => {
                            let existing = self
                                .vars
                                .get(&assignment.name)
                                .unwrap_or_default()
                                .to_string();
                            if let Err(e) = self.vars.set(&assignment.name, existing + &value) {
                                return (e, 1, None);
                            }
                        }
                    }
                }
            }
        }

        // 4. If no command words, just assignments — return success
        if cmd.words.is_empty() {
            return (String::new(), 0, None);
        }

        // 5. Expand command name and args
        let name_expanded = self.expand_word(&cmd.words[0]);
        if name_expanded.is_empty() {
            return (String::new(), 0, None);
        }
        let cmd_name = name_expanded[0].clone();

        let mut args: Vec<String> = Vec::new();
        for word in &cmd.words[1..] {
            args.extend(self.expand_word(word));
        }

        // 6. Alias expansion — re-parse through the full command line parser
        //    so pipes, redirects, variables, and operators in the alias body work.
        if let Some(alias_val) = self
            .defs
            .get_alias(cmd_name.as_str())
            .map(|s| s.to_string())
        {
            // Guard: alias depth limit (prevents indirect recursion a→b→a)
            if self.exec.alias_depth >= 10 {
                return (
                    format!(
                        "conch: alias expansion too deep (recursion on '{}')",
                        cmd_name
                    ),
                    1,
                    None,
                );
            }
            // Guard against self-referencing aliases (e.g. alias ls='ls --color')
            // by checking if the first word of the alias body equals the alias name.
            let first_word = alias_val.split_whitespace().next().unwrap_or("");
            if first_word != cmd_name {
                // Reconstruct the full command line: alias body + remaining args
                let mut full_line = alias_val;
                for a in &args {
                    full_line.push(' ');
                    full_line.push_str(a);
                }

                // xtrace
                if self.exec.opts.xtrace {
                    xtrace_lines.push(format!("+ {}", full_line));
                }

                // Re-parse and execute through the full pipeline
                self.exec.alias_depth += 1;
                let (output, code, _) =
                    self.run_line_with_stdin(&full_line, effective_stdin.as_deref());
                self.exec.alias_depth -= 1;
                return self.apply_structured_output_redirects(cmd, output, code, None);
            }
        }

        // 7. xtrace
        if self.exec.opts.xtrace {
            let mut trace = format!("+ {}", cmd_name);
            for a in &args {
                trace.push(' ');
                trace.push_str(a);
            }
            xtrace_lines.push(trace);
        }

        // 8. Dispatch
        let (output, code, lang) =
            crate::commands::dispatch(self, &cmd_name, &args, effective_stdin.as_deref());

        // 9. Apply output redirects
        self.apply_structured_output_redirects(cmd, output, code, lang)
    }

    /// Resolve input redirects for a structured command.
    /// Returns Ok(stdin) on success, Err((message, code)) on failure.
    fn resolve_structured_input_redirect(
        &mut self,
        cmd: &SimpleCommand,
        stdin: Option<&str>,
    ) -> Result<Option<String>, (String, i32)> {
        for redir in &cmd.redirects {
            match redir.op {
                RedirectOp::Read => {
                    if let RedirectTarget::File(ref word) = redir.target {
                        let path = self.expand_word_nosplit(word);
                        let resolved = self.resolve(&path);
                        match self.fs.read_to_string(&resolved) {
                            Ok(content) => return Ok(Some(content.to_string())),
                            Err(_) => {
                                return Err((
                                    format!("conch: {}: No such file or directory", path),
                                    1,
                                ));
                            }
                        }
                    }
                }
                RedirectOp::HereString => {
                    if let RedirectTarget::File(ref word) = redir.target {
                        let expanded = self.expand_word_nosplit(word);
                        return Ok(Some(format!("{}\n", expanded)));
                    }
                }
                _ => {}
            }
        }
        Ok(stdin.map(|s| s.to_string()))
    }

    /// Apply output redirects to command output (structured path).
    fn apply_structured_output_redirects(
        &mut self,
        cmd: &SimpleCommand,
        output: String,
        code: i32,
        lang: Option<String>,
    ) -> (String, i32, Option<String>) {
        for redir in &cmd.redirects {
            match redir.op {
                RedirectOp::Write | RedirectOp::Append => {
                    // Handle FD duplication (e.g. 2>&1) — no-op in single-stream model
                    if let RedirectTarget::FdDup(_) = &redir.target {
                        continue;
                    }
                    if let RedirectTarget::File(ref word) = redir.target {
                        let path = self.expand_word_nosplit(word);

                        // Handle fd=2 redirects: strip error lines
                        if redir.fd == Some(2) {
                            let resolved = self.resolve(&path);
                            let (errors, non_errors): (Vec<&str>, Vec<&str>) =
                                output.lines().partition(|l| l.starts_with("conch:"));
                            if resolved != "/dev/null" && !errors.is_empty() {
                                let error_text = errors.join("\n");
                                let _ = self.fs.write(&resolved, error_text.as_bytes());
                            }
                            return (non_errors.join("\n"), code, None);
                        }

                        let resolved = self.resolve(&path);
                        // Check permissions
                        if self.fs.exists(&resolved) {
                            if let Some(e) = self.fs.get(&resolved) {
                                if !e.is_writable() {
                                    return (
                                        format!("conch: {}: Permission denied", path),
                                        1,
                                        None,
                                    );
                                }
                            }
                        } else {
                            // Check parent dir write permission
                            let parent = resolved
                                .rsplit_once('/')
                                .map(|(p, _)| if p.is_empty() { "/" } else { p })
                                .unwrap_or("/");
                            if self.fs.access(parent, bare_vfs::AccessMode::W_OK).is_err() {
                                return (format!("conch: {}: Permission denied", path), 1, None);
                            }
                        }
                        match redir.op {
                            RedirectOp::Write => {
                                // noclobber (-C): refuse to overwrite existing files
                                if self.exec.opts.noclobber && self.fs.exists(&resolved) {
                                    return (
                                        format!("conch: {}: cannot overwrite existing file", path),
                                        1,
                                        None,
                                    );
                                }
                                let _ = self.fs.write(&resolved, output.as_bytes());
                            }
                            RedirectOp::Append => {
                                if !self.fs.exists(&resolved) {
                                    let _ = self.fs.write(&resolved, output.as_bytes());
                                } else {
                                    let needs_nl =
                                        self.fs.read(&resolved).is_ok_and(|b| !b.is_empty());
                                    if needs_nl {
                                        let _ = self.fs.append(&resolved, b"\n");
                                    }
                                    let _ = self.fs.append(&resolved, output.as_bytes());
                                }
                            }
                            _ => {}
                        }
                        // Truncate /dev/null after write
                        if resolved == "/dev/null" {
                            let _ = self.fs.write("/dev/null", b"");
                        }
                        return (String::new(), code, None);
                    }
                }
                _ => {} // Input redirects handled above
            }
        }
        (output, code, lang)
    }
}
