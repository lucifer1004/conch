// ---------------------------------------------------------------------------
// Shell builtin commands defined as impl Shell methods.
// ---------------------------------------------------------------------------

use crate::shell::Shell;

/// Signal name/number mapping for `kill`.
const SIGNALS: &[(&str, &str, i32)] = &[
    ("0", "", 0),
    ("1", "HUP", 1),
    ("2", "INT", 2),
    ("3", "QUIT", 3),
    ("6", "ABRT", 6),
    ("9", "KILL", 9),
    ("10", "USR1", 10),
    ("12", "USR2", 12),
    ("13", "PIPE", 13),
    ("14", "ALRM", 14),
    ("15", "TERM", 15),
    ("17", "STOP", 17),
    ("18", "CONT", 18),
    ("19", "TSTP", 19),
];

impl Shell {
    /// `set` — control shell options (-e, -x, -u, -o pipefail, etc.)
    /// With no arguments, lists all shell variables in `VAR=value` format.
    pub fn cmd_set(&mut self, args: &[String]) -> (String, i32) {
        if args.is_empty() {
            let mut lines: Vec<String> = self
                .vars
                .env
                .iter()
                .filter(|(k, _)| k.as_str() != "?")
                .map(|(k, v)| format!("{}={}", k, v))
                .collect();
            lines.sort();
            return (lines.join("\n"), 0);
        }
        let mut i = 0;
        while i < args.len() {
            let arg = &args[i];
            match arg.as_str() {
                "-e" => self.exec.opts.errexit = true,
                "+e" => self.exec.opts.errexit = false,
                "-x" => self.exec.opts.xtrace = true,
                "+x" => self.exec.opts.xtrace = false,
                "-u" => self.exec.opts.nounset = true,
                "+u" => self.exec.opts.nounset = false,
                "-f" => self.exec.opts.noglob = true,
                "+f" => self.exec.opts.noglob = false,
                "-C" => self.exec.opts.noclobber = true,
                "+C" => self.exec.opts.noclobber = false,
                "-o" => {
                    i += 1;
                    if i < args.len() {
                        match args[i].as_str() {
                            "pipefail" => self.exec.opts.pipefail = true,
                            "errexit" => self.exec.opts.errexit = true,
                            "xtrace" => self.exec.opts.xtrace = true,
                            "nounset" => self.exec.opts.nounset = true,
                            "noglob" => self.exec.opts.noglob = true,
                            "noclobber" => self.exec.opts.noclobber = true,
                            other => {
                                return (format!("conch: set: {}: invalid option name", other), 2);
                            }
                        }
                    }
                }
                "+o" => {
                    i += 1;
                    if i < args.len() {
                        match args[i].as_str() {
                            "pipefail" => self.exec.opts.pipefail = false,
                            "errexit" => self.exec.opts.errexit = false,
                            "xtrace" => self.exec.opts.xtrace = false,
                            "nounset" => self.exec.opts.nounset = false,
                            "noglob" => self.exec.opts.noglob = false,
                            "noclobber" => self.exec.opts.noclobber = false,
                            other => {
                                return (format!("conch: set: {}: invalid option name", other), 2);
                            }
                        }
                    }
                }
                // Compound flags like -ex, -eu, -ef, etc.
                s if s.starts_with('-') && s.len() > 2 && !s.starts_with("--") => {
                    for ch in s[1..].chars() {
                        match ch {
                            'e' => self.exec.opts.errexit = true,
                            'x' => self.exec.opts.xtrace = true,
                            'u' => self.exec.opts.nounset = true,
                            'f' => self.exec.opts.noglob = true,
                            'C' => self.exec.opts.noclobber = true,
                            other => {
                                return (format!("conch: set: -{}: invalid option", other), 2);
                            }
                        }
                    }
                }
                s if s.starts_with('+') && s.len() > 2 => {
                    for ch in s[1..].chars() {
                        match ch {
                            'e' => self.exec.opts.errexit = false,
                            'x' => self.exec.opts.xtrace = false,
                            'u' => self.exec.opts.nounset = false,
                            'f' => self.exec.opts.noglob = false,
                            'C' => self.exec.opts.noclobber = false,
                            other => {
                                return (format!("conch: set: +{}: invalid option", other), 2);
                            }
                        }
                    }
                }
                "--" => {
                    // Everything after -- becomes positional parameters
                    i += 1;
                    let positional: Vec<String> = args[i..].to_vec();
                    self.set_positional_params(&positional);
                    return (String::new(), 0);
                }
                other if other.starts_with('-') || other.starts_with('+') => {
                    return (format!("conch: set: {}: invalid option", other), 2);
                }
                _ => {} // non-flag args silently ignored (bash behavior)
            }
            i += 1;
        }
        (String::new(), 0)
    }

    /// `alias` — define or list aliases.
    pub fn cmd_alias(&mut self, args: &[String]) -> (String, i32) {
        if args.is_empty() {
            // List all aliases
            let lines: Vec<String> = self
                .defs
                .aliases
                .iter()
                .map(|(k, v)| format!("alias {}='{}'", k, v))
                .collect();
            return (lines.join("\n"), 0);
        }
        let mut out = Vec::new();
        let mut code = 0;
        for arg in args {
            if let Some((name, value)) = arg.split_once('=') {
                self.defs.aliases.insert(name.into(), value.to_string());
            } else {
                // Show single alias
                if let Some(v) = self.defs.get_alias(arg.as_str()) {
                    out.push(format!("alias {}='{}'", arg, v));
                } else {
                    out.push(format!("conch: alias: {}: not found", arg));
                    code = 1;
                }
            }
        }
        (out.join("\n"), code)
    }

    /// `unalias` — remove aliases.
    pub fn cmd_unalias(&mut self, args: &[String]) -> (String, i32) {
        let mut code = 0;
        let mut errors = Vec::new();
        for arg in args {
            if arg == "-a" {
                self.defs.aliases.clear();
                return (String::new(), 0);
            }
            if self.defs.aliases.remove(arg.as_str()).is_none() {
                errors.push(format!("conch: unalias: {}: not found", arg));
                code = 1;
            }
        }
        (errors.join("\n"), code)
    }

    /// `readonly` — mark variables as read-only.
    pub fn cmd_readonly(&mut self, args: &[String]) -> (String, i32) {
        if args.is_empty() || (args.len() == 1 && args[0] == "-p") {
            // List readonly vars
            let lines: Vec<String> = self
                .vars
                .readonly
                .iter()
                .map(|name| {
                    let val = self.vars.env.get(name).cloned().unwrap_or_default();
                    format!("declare -r {}=\"{}\"", name, val)
                })
                .collect();
            return (lines.join("\n"), 0);
        }
        for arg in args {
            if arg == "-p" {
                continue;
            }
            let (name, value) = if let Some((n, v)) = arg.split_once('=') {
                (n, Some(v))
            } else {
                (arg.as_str(), None)
            };
            if !super::is_valid_identifier(name) {
                return (
                    format!("conch: readonly: `{}': not a valid identifier", name),
                    1,
                );
            }
            if let Some(val) = value {
                if self.vars.readonly.contains(name) {
                    return (format!("conch: {}: readonly variable", name), 1);
                }
                let expanded = self.expand(val);
                self.vars.env.insert(name.into(), expanded);
            }
            self.vars.readonly.insert(name.into());
        }
        (String::new(), 0)
    }

    /// `getopts optstring name [args...]` — parse positional options.
    pub fn cmd_getopts(&mut self, args: &[String]) -> (String, i32) {
        if args.len() < 2 {
            return (
                "conch: getopts: usage: getopts optstring name [arg ...]".into(),
                2,
            );
        }
        let optstring = &args[0];
        let varname = &args[1];

        // Determine the argument list to parse
        let opt_args: Vec<String> = if args.len() > 2 {
            args[2..].to_vec()
        } else {
            // Use positional parameters $1, $2, ...
            let count: usize = self
                .vars
                .env
                .get("#")
                .and_then(|s| s.parse().ok())
                .unwrap_or(0);
            (1..=count)
                .filter_map(|i| self.vars.env.get(i.to_string().as_str()).cloned())
                .collect()
        };

        // Read current OPTIND (1-based)
        let optind: usize = self
            .vars
            .env
            .get("OPTIND")
            .and_then(|s| s.parse().ok())
            .unwrap_or(1);

        // Convert to 0-based index into opt_args
        let idx = optind.saturating_sub(1);
        if idx >= opt_args.len() {
            self.vars
                .env
                .insert(varname.as_str().into(), "?".to_string());
            return (String::new(), 1);
        }

        let current = &opt_args[idx];
        if !current.starts_with('-') || current == "-" || current == "--" {
            self.vars
                .env
                .insert(varname.as_str().into(), "?".to_string());
            if current == "--" {
                self.vars
                    .env
                    .insert("OPTIND".into(), (optind + 1).to_string());
            }
            return (String::new(), 1);
        }

        // Parse the option character(s) — we handle one option per call
        // Use OPTPOS to track position within a bundled option group like -abc
        let optpos: usize = self
            .vars
            .env
            .get("_OPTPOS")
            .and_then(|s| s.parse().ok())
            .unwrap_or(1);

        let chars: Vec<char> = current.chars().collect();
        if optpos >= chars.len() {
            // Move to next arg
            self.vars
                .env
                .insert("OPTIND".into(), (optind + 1).to_string());
            self.vars.env.remove("_OPTPOS");
            self.vars
                .env
                .insert(varname.as_str().into(), "?".to_string());
            return (String::new(), 1);
        }

        let opt_char = chars[optpos];

        // Check if this option is in optstring
        let opt_idx = optstring.find(opt_char);
        match opt_idx {
            Some(oi) => {
                let takes_arg = optstring.as_bytes().get(oi + 1) == Some(&b':');
                if takes_arg {
                    // If more chars follow in this arg, they are the optarg
                    if optpos + 1 < chars.len() {
                        let optarg: String = chars[optpos + 1..].iter().collect();
                        self.vars.env.insert("OPTARG".into(), optarg);
                        self.vars
                            .env
                            .insert("OPTIND".into(), (optind + 1).to_string());
                        self.vars.env.remove("_OPTPOS");
                    } else {
                        // Next argument is the optarg
                        let next_idx = idx + 1;
                        if next_idx < opt_args.len() {
                            self.vars
                                .env
                                .insert("OPTARG".into(), opt_args[next_idx].clone());
                            self.vars
                                .env
                                .insert("OPTIND".into(), (optind + 2).to_string());
                        } else {
                            self.vars.env.insert("OPTARG".into(), String::new());
                            self.vars
                                .env
                                .insert("OPTIND".into(), (optind + 2).to_string());
                            self.vars
                                .env
                                .insert(varname.as_str().into(), "?".to_string());
                            return ("conch: getopts: option requires an argument".into(), 0);
                        }
                        self.vars.env.remove("_OPTPOS");
                    }
                } else {
                    self.vars.env.remove("OPTARG");
                    // Check if more option chars remain in this group
                    if optpos + 1 < chars.len() {
                        self.vars
                            .env
                            .insert("_OPTPOS".into(), (optpos + 1).to_string());
                    } else {
                        self.vars
                            .env
                            .insert("OPTIND".into(), (optind + 1).to_string());
                        self.vars.env.remove("_OPTPOS");
                    }
                }
                self.vars
                    .env
                    .insert(varname.as_str().into(), opt_char.to_string());
                (String::new(), 0)
            }
            None => {
                // Unknown option
                self.vars
                    .env
                    .insert(varname.as_str().into(), "?".to_string());
                self.vars.env.remove("OPTARG");
                if optpos + 1 < chars.len() {
                    self.vars
                        .env
                        .insert("_OPTPOS".into(), (optpos + 1).to_string());
                } else {
                    self.vars
                        .env
                        .insert("OPTIND".into(), (optind + 1).to_string());
                    self.vars.env.remove("_OPTPOS");
                }
                (String::new(), 0)
            }
        }
    }

    /// Signals accepted by `trap`.
    const TRAP_SIGNALS: &'static [&'static str] =
        &["EXIT", "ERR", "INT", "TERM", "HUP", "DEBUG", "RETURN"];

    /// `trap` — set signal handlers.
    /// Supports: EXIT, ERR, INT, TERM, HUP, DEBUG, RETURN.
    /// `trap -p [signal]` prints traps.
    pub fn cmd_trap(&mut self, args: &[String]) -> (String, i32) {
        if args.is_empty() {
            let mut out = Vec::new();
            for (sig, cmd) in &self.defs.traps {
                out.push(format!("trap -- '{}' {}", cmd, sig));
            }
            return (out.join("\n"), 0);
        }

        // trap -p [signal ...]
        if args[0] == "-p" {
            if args.len() == 1 {
                // Print all traps
                let mut out = Vec::new();
                for (sig, cmd) in &self.defs.traps {
                    out.push(format!("trap -- '{}' {}", cmd, sig));
                }
                return (out.join("\n"), 0);
            }
            let mut out = Vec::new();
            for sig_arg in &args[1..] {
                let sig = sig_arg.to_uppercase();
                if let Some(cmd) = self.defs.traps.get(sig.as_str()) {
                    out.push(format!("trap -- '{}' {}", cmd, sig));
                }
            }
            return (out.join("\n"), 0);
        }

        if args.len() == 1 {
            let sig = args[0].to_uppercase();
            if sig == "-" {
                self.defs.traps.clear();
                return (String::new(), 0);
            }
            if let Some(cmd) = self.defs.traps.get(sig.as_str()) {
                return (format!("trap -- '{}' {}", cmd, sig), 0);
            }
            return (String::new(), 0);
        }
        let command = &args[0];
        let mut errors = Vec::new();
        let mut code = 0;
        for sig_arg in &args[1..] {
            let sig = sig_arg.to_uppercase();
            if !Self::TRAP_SIGNALS.contains(&sig.as_str()) {
                errors.push(format!(
                    "conch: trap: {}: signal not supported in WASM",
                    sig_arg
                ));
                code = 1;
                continue;
            }
            if command == "-" {
                self.defs.traps.remove(sig.as_str());
            } else {
                self.defs.traps.insert(sig.as_str().into(), command.clone());
            }
        }
        (errors.join("\n"), code)
    }

    /// `mapfile`/`readarray` — read stdin lines into an array variable.
    /// Supported flags: -t (strip trailing newlines), array name (default: MAPFILE).
    /// Unsupported flags (-n, -O, -s, -u, -C, -c) are silently skipped.
    pub fn cmd_mapfile(&mut self, args: &[String], stdin: Option<&str>) -> (String, i32) {
        let input = match stdin {
            Some(s) => s,
            None => return (String::new(), 0),
        };

        let mut strip_newlines = false;
        let mut array_name = "MAPFILE".to_string();
        let mut args_iter = args.iter().peekable();

        while let Some(arg) = args_iter.next() {
            match arg.as_str() {
                "-t" => strip_newlines = true,
                // Flags that take an argument — consume the argument and skip both
                "-n" | "-O" | "-s" | "-u" | "-C" | "-c" => {
                    args_iter.next(); // skip the argument
                }
                s if s.starts_with('-') => {
                    // Unknown flag, ignore
                }
                name => {
                    array_name = name.to_string();
                    break;
                }
            }
        }

        let lines: Vec<String> = input
            .lines()
            .map(|l| {
                if strip_newlines {
                    l.to_string()
                } else {
                    format!("{}\n", l)
                }
            })
            .collect();

        self.vars.arrays.insert(array_name.into(), lines);
        (String::new(), 0)
    }

    // -----------------------------------------------------------------------
    // Process / job control builtins
    // -----------------------------------------------------------------------

    /// `jobs` — list background jobs.
    pub fn cmd_jobs(&self, _args: &[String]) -> (String, i32) {
        let mut out = String::new();
        let len = self.procs.jobs.len();
        for (i, proc) in self.procs.jobs.iter().enumerate() {
            let status = match &proc.status {
                super::ProcessStatus::Running => "Running",
                super::ProcessStatus::Exited(0) => "Done",
                super::ProcessStatus::Exited(_) => "Exit",
            };
            // Most recent job = "+", second most recent = "-"
            let marker = if i + 1 == len {
                "+"
            } else if i + 2 == len {
                "-"
            } else {
                " "
            };
            if !out.is_empty() {
                out.push('\n');
            }
            let exit_suffix = match &proc.status {
                super::ProcessStatus::Exited(c) if *c != 0 => format!(" {}", c),
                _ => String::new(),
            };
            let amp = if matches!(proc.status, super::ProcessStatus::Running) {
                " &"
            } else {
                ""
            };
            let status_field = format!("{}{}", status, exit_suffix);
            out.push_str(&format!(
                "[{}]{}  {:<24}{}{}",
                i + 1,
                marker,
                status_field,
                proc.cmd,
                amp,
            ));
        }
        (out, 0)
    }

    /// `wait` — wait for background jobs.
    pub fn cmd_wait(&mut self, args: &[String]) -> (String, i32) {
        use crate::types::BackgroundMode;

        // Handle `wait -n`: wait for any one background job to complete.
        if args.first().map(|a| a.as_str()) == Some("-n") {
            match self.bg_mode {
                BackgroundMode::Sync => {
                    // In Sync mode all jobs are already done; return the most recent job's exit code.
                    let code = self
                        .procs
                        .jobs
                        .last()
                        .map(|p| match p.status {
                            super::ProcessStatus::Exited(c) => c,
                            super::ProcessStatus::Running => 0,
                        })
                        .unwrap_or(0);
                    return (String::new(), code);
                }
                BackgroundMode::Deferred | BackgroundMode::Interleaved => {
                    // Step bg_jobs until any one completes, return its code.
                    if self.bg_jobs.is_empty() {
                        return (String::new(), 0);
                    }
                    // Run the first pending job to completion and return its code.
                    let pid = self.bg_jobs[0].pid;
                    let code = self.run_bg_job_to_completion(pid).unwrap_or(0);
                    return (String::new(), code);
                }
            }
        }

        if args.is_empty() {
            // Wait for all background jobs
            match self.bg_mode {
                BackgroundMode::Sync => return (String::new(), 0),
                BackgroundMode::Deferred | BackgroundMode::Interleaved => {
                    let code = self.run_all_bg_jobs();
                    return (String::new(), code);
                }
            }
        }

        // wait PID ...
        let mut last_code = 0;
        let mut output = Vec::new();
        for arg in args {
            if let Ok(pid) = arg.parse::<u32>() {
                // Try to run a deferred/interleaved bg job first
                if let Some(code) = self.run_bg_job_to_completion(pid) {
                    last_code = code;
                } else if let Some(proc) = self.procs.jobs.iter().find(|p| p.pid == pid) {
                    // Job exists in process table but not in bg_jobs — already completed.
                    last_code = match proc.status {
                        super::ProcessStatus::Exited(c) => c,
                        // Running here is unreachable: Sync mode always records Exited,
                        // Deferred/Interleaved are handled by run_bg_job_to_completion.
                        super::ProcessStatus::Running => 127,
                    };
                } else {
                    output.push(format!(
                        "conch: wait: pid {} is not a child of this shell",
                        pid
                    ));
                    last_code = 127;
                }
            } else {
                output.push(format!("conch: wait: {}: not a pid", arg));
                last_code = 2;
            }
        }
        (output.join("\n"), last_code)
    }

    /// `timeout DURATION COMMAND [ARGS...]` — run command with a VFS time limit.
    /// Returns exit code 124 if the command exceeded the duration (bash convention).
    pub fn cmd_timeout(&mut self, args: &[String], stdin: Option<&str>) -> (String, i32) {
        use crate::shell::pipeline::TICKS_PER_SECOND;

        if args.is_empty() {
            return ("timeout: missing operand".into(), 1);
        }

        // Parse duration (seconds, float).
        let seconds: f64 = args[0].parse().unwrap_or(0.0);
        let limit_ticks = (seconds * TICKS_PER_SECOND as f64) as u64;

        if args.len() < 2 {
            // No command — succeed immediately.
            return (String::new(), 0);
        }

        let before = self.fs.time();
        let cmd = &args[1].clone();
        let cmd_args: Vec<String> = args[2..].to_vec();
        let (out, code, _) = crate::commands::dispatch(self, cmd, &cmd_args, stdin);
        let elapsed = self.fs.time().saturating_sub(before);

        if elapsed > limit_ticks {
            // Reset any time overage and return 124.
            self.fs.set_time(before + limit_ticks);
            return (out, 124);
        }

        (out, code)
    }

    /// `ps` — display the process table.
    pub fn cmd_ps(&self, _args: &[String]) -> (String, i32) {
        let mut lines = vec!["PID  STAT  COMMAND".to_string()];
        // Show the shell itself first
        lines.push(format!(
            "{:<5}{:<6}{}",
            self.procs.shell_pid(),
            "S",
            "conch"
        ));
        for proc in &self.procs.jobs {
            let stat = match proc.status {
                super::ProcessStatus::Running => "R",
                super::ProcessStatus::Exited(_) => "Z",
            };
            lines.push(format!("{:<5}{:<6}{}", proc.pid, stat, proc.cmd));
        }
        (lines.join("\n"), 0)
    }

    /// `umask` — display or set the file creation mask.
    pub fn cmd_umask(&mut self, args: &[String]) -> (String, i32) {
        if args.is_empty() {
            let mask = self.fs.umask();
            return (format!("{:04o}", mask), 0);
        }
        let arg = &args[0];
        match u16::from_str_radix(arg.as_str(), 8) {
            Ok(val) => {
                self.fs.set_umask(val);
                (String::new(), 0)
            }
            Err(_) => (format!("conch: umask: {}: invalid octal value", arg), 1),
        }
    }

    /// `time cmd args...` — run a command and report VFS-clock timing.
    /// Returns (output, exit_code); caller appends timing line.
    pub fn cmd_time(
        &mut self,
        args: &[String],
        stdin: Option<&str>,
    ) -> (String, i32, Option<String>) {
        use crate::shell::pipeline::TICKS_PER_SECOND;
        let before = self.fs.time();
        let (mut out, code, lang) = if args.is_empty() {
            (String::new(), 0, None)
        } else {
            crate::commands::dispatch(self, &args[0], &args[1..], stdin)
        };
        let elapsed = self.fs.time().saturating_sub(before);
        let secs = elapsed as f64 / TICKS_PER_SECOND as f64;
        let mins = (secs / 60.0) as u64;
        let rem_secs = secs - mins as f64 * 60.0;
        let timing = format!("\nreal\t{}m{:.3}s\n", mins, rem_secs);
        out.push_str(&timing);
        (out, code, lang)
    }

    /// `shopt` — display or toggle extended shell options.
    pub fn cmd_shopt(&mut self, args: &[String]) -> (String, i32) {
        if args.is_empty() {
            // List all shopt options
            let lines = [
                format!(
                    "dotglob\t\t{}",
                    if self.shopt.dotglob { "on" } else { "off" }
                ),
                format!(
                    "failglob\t{}",
                    if self.shopt.failglob { "on" } else { "off" }
                ),
                format!(
                    "nullglob\t{}",
                    if self.shopt.nullglob { "on" } else { "off" }
                ),
            ];
            return (lines.join("\n"), 0);
        }

        // Parse flags: -s (set/enable), -u (unset/disable), or bare name (query)
        let mut set_flag: Option<bool> = None;
        let mut names: Vec<&str> = Vec::new();
        let mut i = 0;
        while i < args.len() {
            match args[i].as_str() {
                "-s" => set_flag = Some(true),
                "-u" => set_flag = Some(false),
                s if s.starts_with('-') => {
                    return (format!("conch: shopt: {}: invalid option", s), 2);
                }
                name => names.push(name),
            }
            i += 1;
        }

        if names.is_empty() {
            return ("conch: shopt: no option name given".into(), 2);
        }

        let mut out = Vec::new();
        let mut code = 0;
        for name in names {
            match name {
                "nullglob" => match set_flag {
                    Some(val) => self.shopt.nullglob = val,
                    None => out.push(format!(
                        "nullglob\t{}",
                        if self.shopt.nullglob { "on" } else { "off" }
                    )),
                },
                "failglob" => match set_flag {
                    Some(val) => self.shopt.failglob = val,
                    None => out.push(format!(
                        "failglob\t{}",
                        if self.shopt.failglob { "on" } else { "off" }
                    )),
                },
                "dotglob" => match set_flag {
                    Some(val) => self.shopt.dotglob = val,
                    None => out.push(format!(
                        "dotglob\t\t{}",
                        if self.shopt.dotglob { "on" } else { "off" }
                    )),
                },
                other => {
                    out.push(format!(
                        "conch: shopt: {}: invalid shell option name",
                        other
                    ));
                    code = 1;
                }
            }
        }
        (out.join("\n"), code)
    }

    /// `kill` — send signal to a process (simulated).
    pub fn cmd_kill(&mut self, args: &[String]) -> (String, i32) {
        if args.is_empty() {
            return ("kill: usage: kill pid ...".to_string(), 2);
        }
        // kill -l: list signal names
        if args.len() == 1 && args[0] == "-l" {
            let names: Vec<&str> = SIGNALS
                .iter()
                .filter(|(_, name, _)| !name.is_empty())
                .map(|(_, name, _num)| *name)
                .collect();
            return (names.join("\n"), 0);
        }
        let mut signal = 15; // default SIGTERM
        let mut pids = Vec::new();
        let mut i = 0;
        while i < args.len() {
            let arg = &args[i];
            if arg.starts_with('-') && arg.len() > 1 {
                let sig_str = &arg[1..];
                if let Some((_, _, num)) = SIGNALS
                    .iter()
                    .find(|(n, name, _)| *n == sig_str || (!name.is_empty() && *name == sig_str))
                {
                    signal = *num;
                } else if let Ok(num) = sig_str.parse::<i32>() {
                    signal = num;
                } else {
                    return (format!("conch: kill: {}: invalid signal", sig_str), 2);
                }
            } else if let Ok(pid) = arg.parse::<u32>() {
                pids.push(pid);
            } else {
                return (format!("conch: kill: {}: not a pid", arg), 2);
            }
            i += 1;
        }
        if pids.is_empty() {
            return ("kill: usage: kill pid ...".to_string(), 2);
        }
        let mut output = Vec::new();
        let mut code = 0;
        for pid in pids {
            if let Some(proc) = self.procs.jobs.iter_mut().find(|p| p.pid == pid) {
                if signal == 0 {
                    // Signal 0: just check existence — it exists
                } else {
                    proc.status = super::ProcessStatus::Exited(128 + signal);
                }
            } else {
                output.push(format!("conch: kill: ({}) - No such process", pid));
                code = 1;
            }
        }
        (output.join("\n"), code)
    }
}
