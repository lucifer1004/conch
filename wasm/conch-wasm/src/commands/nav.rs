use crate::shell::Shell;

impl Shell {
    /// `cd [dir]` — change the current working directory; `-` swaps to `$OLDPWD`.
    pub fn cmd_cd(&mut self, args: &[String]) -> (String, i32) {
        let raw = args.first().map(|s| s.as_str()).unwrap_or("");

        // cd - : swap to OLDPWD
        if raw == "-" {
            let oldpwd = self.vars.env.get("OLDPWD").cloned().unwrap_or_default();
            if oldpwd.is_empty() {
                return ("cd: OLDPWD not set".to_string(), 1);
            }
            let prev = self.cwd.to_string();
            match self.fs.get(&oldpwd) {
                Some(e) if e.is_dir() => {
                    self.cwd = oldpwd.clone().into();
                    self.vars.env.insert("OLDPWD".into(), prev);
                    self.vars.env.insert("PWD".into(), oldpwd.clone());
                    return (oldpwd, 0);
                }
                Some(_) => return (format!("cd: not a directory: {}", oldpwd), 1),
                None => return (format!("cd: no such file or directory: {}", oldpwd), 1),
            }
        }

        let target = if args.is_empty() {
            self.ident.home.clone()
        } else {
            self.resolve(&args[0]).into()
        };

        match self.fs.get(&target) {
            Some(e) if e.is_dir() => {
                let prev = self.cwd.to_string();
                self.cwd = target.clone();
                self.vars.env.insert("OLDPWD".into(), prev);
                self.vars.env.insert("PWD".into(), target.to_string());
                (String::new(), 0)
            }
            Some(_) => (format!("cd: not a directory: {}", raw), 1),
            None => (format!("cd: no such file or directory: {}", raw), 1),
        }
    }

    /// `pushd [dir]` — push `dir` onto the directory stack and cd into it; with no args, swap the top two entries.
    pub fn cmd_pushd(&mut self, args: &[String]) -> (String, i32) {
        let target = if args.is_empty() {
            // pushd with no args swaps top two entries
            if self.dir_stack.is_empty() {
                return ("pushd: no other directory".to_string(), 1);
            }
            self.dir_stack.remove(0)
        } else {
            self.resolve(&args[0]).to_string()
        };

        match self.fs.get(&target) {
            Some(e) if e.is_dir() => {
                let prev = self.cwd.to_string();
                self.dir_stack.insert(0, prev);
                self.cwd = target.into();
                self.vars.env.insert("PWD".into(), self.cwd.to_string());
                (self.format_dir_stack(), 0)
            }
            Some(_) => (
                format!(
                    "pushd: {}: Not a directory",
                    args.first().map(|s| s.as_str()).unwrap_or("")
                ),
                1,
            ),
            None => (
                format!(
                    "pushd: {}: No such file or directory",
                    args.first().map(|s| s.as_str()).unwrap_or("")
                ),
                1,
            ),
        }
    }

    /// `popd` — pop the top directory off the stack and cd into it.
    pub fn cmd_popd(&mut self, _args: &[String]) -> (String, i32) {
        if self.dir_stack.is_empty() {
            return ("popd: directory stack empty".to_string(), 1);
        }
        let dir = self.dir_stack.remove(0);
        self.cwd = dir.into();
        self.vars.env.insert("PWD".into(), self.cwd.to_string());
        (self.format_dir_stack(), 0)
    }

    /// `dirs` — print the current directory stack.
    pub fn cmd_dirs(&self, _args: &[String]) -> (String, i32) {
        (self.format_dir_stack(), 0)
    }

    fn format_dir_stack(&self) -> String {
        let mut parts = vec![self.cwd.to_string()];
        for d in &self.dir_stack {
            parts.push(d.clone());
        }
        parts.join(" ")
    }

    /// `printenv [VAR...]` — print the value of each named variable, or all environment variables if none given.
    pub fn cmd_printenv(&self, args: &[String]) -> (String, i32) {
        if args.is_empty() {
            return self.cmd_env();
        }
        // printenv VAR: print single variable value
        let name = &args[0];
        match self.vars.env.get(name.as_str()) {
            Some(val) => (val.clone(), 0),
            None => (String::new(), 1),
        }
    }

    /// `env` — print all environment variables as `KEY=value` lines.
    pub fn cmd_env(&self) -> (String, i32) {
        let mut lines: Vec<String> = self
            .vars
            .env
            .iter()
            .filter(|(k, _)| k.as_str() != "?")
            .map(|(k, v)| format!("{}={}", k, v))
            .collect();
        lines.sort();
        (lines.join("\n"), 0)
    }

    /// `export [NAME=VALUE...]` — set and export variables; with no args, list all exported variables in `declare -x` format.
    pub fn cmd_export(&mut self, args: &[String]) -> (String, i32) {
        if args.is_empty() {
            // List all variables in declare -x format
            let mut lines: Vec<String> = self
                .vars
                .env
                .iter()
                .filter(|(k, _)| k.as_str() != "?")
                .map(|(k, v)| format!("declare -x {}=\"{}\"", k, v))
                .collect();
            lines.sort();
            return (lines.join("\n"), 0);
        }
        for arg in args {
            if let Some((k, v)) = arg.split_once('=') {
                let expanded = self.expand(v);
                if let Err(e) = self.vars.set(k, expanded) {
                    return (e, 1);
                }
            }
        }
        (String::new(), 0)
    }

    /// `date [+FORMAT]` — print the current VFS clock time; accepts a `+FORMAT` strftime-style argument.
    pub fn cmd_date(&self, args: &[String]) -> (String, i32) {
        use crate::shell::{format_date_str, ticks_to_datetime};
        let ticks = self.fs.time();

        // Check for +FORMAT argument
        if let Some(fmt_arg) = args.first() {
            if let Some(fmt) = fmt_arg.strip_prefix('+') {
                return (format_date_str(ticks, fmt), 0);
            }
        }

        // Default format: "Sat Apr 12 00:00:00 UTC 2026"
        let (y, mo, d, h, mi, s) = ticks_to_datetime(ticks);
        let dow = {
            // Tomohiko Sakamoto
            let adj_y = if mo < 3 { y - 1 } else { y };
            let t: [u64; 12] = [0, 3, 2, 5, 0, 3, 5, 1, 4, 6, 2, 4];
            let idx = (mo as usize).wrapping_sub(1);
            ((adj_y + adj_y / 4 - adj_y / 100 + adj_y / 400 + t[idx] + d as u64) % 7) as usize
        };
        const WD: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
        const MO: [&str; 12] = [
            "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
        ];
        let date_str = format!(
            "{} {} {:>2} {:02}:{:02}:{:02} UTC {:04}",
            WD[dow],
            MO[(mo as usize).wrapping_sub(1)],
            d,
            h,
            mi,
            s,
            y,
        );
        (date_str, 0)
    }

    /// `which NAME...` — locate each command on `$PATH` or among builtins, printing its resolved path.
    pub fn cmd_which(&self, args: &[String]) -> (String, i32) {
        if args.is_empty() {
            return (String::new(), 1);
        }
        let mut out = Vec::new();
        let mut any_failed = false;
        for arg in args {
            if crate::commands::BUILTINS.contains(&arg.as_str())
                || self.defs.has_function(arg.as_str())
            {
                out.push(format!("/bin/{}", arg));
            } else if let Some(path) = self.which_path(arg) {
                out.push(path);
            } else {
                out.push(format!("which: no {} in (/bin)", arg));
                any_failed = true;
            }
        }
        (out.join("\n"), if any_failed { 1 } else { 0 })
    }

    /// `type [-t] NAME...` — describe how each name would be interpreted; `-t` prints a single classification word.
    pub fn cmd_type(&self, args: &[String]) -> (String, i32) {
        if args.is_empty() {
            return (String::new(), 1);
        }

        // Check for -t flag
        let type_flag = args.iter().any(|a| a == "-t");
        let real_args: Vec<&String> = args.iter().filter(|a| a.as_str() != "-t").collect();

        if real_args.is_empty() {
            return (String::new(), 1);
        }

        let mut out = Vec::new();
        let mut any_failed = false;
        for arg in &real_args {
            if type_flag {
                // -t: output single word
                if self.defs.get_alias(arg.as_str()).is_some() {
                    out.push("alias".to_string());
                } else if crate::commands::BUILTINS.contains(&arg.as_str()) {
                    out.push("builtin".to_string());
                } else if self.defs.has_function(arg.as_str()) {
                    out.push("function".to_string());
                } else if self.which_path(arg).is_some() {
                    out.push("file".to_string());
                } else {
                    any_failed = true;
                }
            } else {
                // Normal output
                if self.defs.get_alias(arg.as_str()).is_some() {
                    let val = self.defs.get_alias(arg.as_str()).unwrap_or_default();
                    out.push(format!("{} is aliased to `{}'", arg, val));
                } else if crate::commands::BUILTINS.contains(&arg.as_str()) {
                    out.push(format!("{} is a shell builtin", arg));
                } else if self.defs.has_function(arg.as_str()) {
                    out.push(format!("{} is a function", arg));
                } else if let Some(path) = self.which_path(arg) {
                    out.push(format!("{} is {}", arg, path));
                } else {
                    out.push(format!("type: {}: not found", arg));
                    any_failed = true;
                }
            }
        }
        (out.join("\n"), if any_failed { 1 } else { 0 })
    }

    /// `basename PATH [SUFFIX]` — strip directory components (and optional suffix) from a path.
    pub fn cmd_basename(&self, args: &[String]) -> (String, i32) {
        if args.is_empty() {
            return ("basename: missing operand".into(), 1);
        }
        let name = args[0].rsplit('/').next().unwrap_or(&args[0]);
        if args.len() > 1 {
            if let Some(stripped) = name.strip_suffix(args[1].as_str()) {
                return (stripped.to_string(), 0);
            }
        }
        (name.to_string(), 0)
    }

    /// `dirname PATH` — strip the last component from a path, printing the directory portion.
    pub fn cmd_dirname(&self, args: &[String]) -> (String, i32) {
        if args.is_empty() {
            return ("dirname: missing operand".into(), 1);
        }
        let dir = if let Some((d, _)) = args[0].rsplit_once('/') {
            if d.is_empty() {
                "/"
            } else {
                d
            }
        } else {
            "."
        };
        (dir.to_string(), 0)
    }

    /// `unset [-f] [-v] NAME...` — remove shell variables (`-v`, default) or functions (`-f`); supports array-element syntax `arr[N]`.
    pub fn cmd_unset(&mut self, args: &[String]) -> (String, i32) {
        // Handle -f flag: unset functions
        if !args.is_empty() && args[0] == "-f" {
            for name in &args[1..] {
                self.defs.functions.remove(name.as_str());
            }
            return (String::new(), 0);
        }
        // Handle -v flag: unset variables (default behavior, just skip flag)
        let effective_args: &[String] = if !args.is_empty() && args[0] == "-v" {
            &args[1..]
        } else {
            args
        };
        for arg in effective_args {
            // Detect arr[N] pattern for array element removal
            if let Some(bracket_pos) = arg.find('[') {
                let name = &arg[..bracket_pos];
                if self.vars.readonly.contains(name) {
                    return (format!("conch: unset: {}: readonly variable", name), 1);
                }
                let rest = &arg[bracket_pos + 1..];
                if let Some(idx_str) = rest.strip_suffix(']') {
                    // Associative array element
                    if let Some(assoc) = self.vars.assoc_arrays.get_mut(name) {
                        assoc.remove(idx_str);
                        continue;
                    }
                    // Indexed array element
                    if let Some(arr) = self.vars.arrays.get_mut(name) {
                        if let Ok(idx) = idx_str.parse::<usize>() {
                            if idx < arr.len() {
                                arr[idx] = String::new();
                            }
                        }
                        continue;
                    }
                }
            } else if let Err(e) = self.vars.unset(arg.as_str()) {
                return (e, 1);
            }
        }
        (String::new(), 0)
    }

    /// `sleep N[smhd]` — advance the VFS logical clock by N seconds (or minutes/hours/days with suffix).
    pub fn cmd_sleep(&mut self, args: &[String]) -> (String, i32) {
        // Advance VFS logical clock to simulate time passage.
        use crate::shell::pipeline::TICKS_PER_SECOND;
        let seconds: f64 = args
            .first()
            .and_then(|a| parse_sleep_duration(a))
            .unwrap_or(0.0);
        let delta = (seconds * TICKS_PER_SECOND as f64) as u64;
        self.fs.set_time(self.fs.time() + delta);
        (String::new(), 0)
    }

    /// `history` — print the command history with line numbers.
    pub fn cmd_history(&self, _args: &[String]) -> (String, i32) {
        let out: Vec<String> = self
            .history
            .iter()
            .enumerate()
            .map(|(i, cmd)| format!("{:>5}  {}", i + 1, cmd))
            .collect();
        (out.join("\n"), 0)
    }

    /// `realpath PATH` — resolve symlinks and canonicalize PATH, returning an absolute path.
    pub fn cmd_realpath(&mut self, args: &[String]) -> (String, i32) {
        if args.is_empty() {
            return ("realpath: missing operand".into(), 1);
        }
        let path = self.resolve(&args[0]);
        match self.fs.canonical_path(&path) {
            Ok(canonical) => (canonical, 0),
            Err(_) => (
                format!("realpath: {}: No such file or directory", args[0]),
                1,
            ),
        }
    }
}

/// Parse a sleep duration string with optional suffix: s (seconds), m (minutes),
/// h (hours), d (days). No suffix defaults to seconds.
fn parse_sleep_duration(s: &str) -> Option<f64> {
    let s = s.trim();
    if s.is_empty() {
        return Some(0.0);
    }
    let last = s.as_bytes()[s.len() - 1];
    let (num_part, multiplier) = match last {
        b's' => (&s[..s.len() - 1], 1.0),
        b'm' => (&s[..s.len() - 1], 60.0),
        b'h' => (&s[..s.len() - 1], 3600.0),
        b'd' => (&s[..s.len() - 1], 86400.0),
        _ => (s, 1.0),
    };
    num_part.parse::<f64>().ok().map(|v| v * multiplier)
}
