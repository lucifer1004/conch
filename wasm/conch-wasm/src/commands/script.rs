use crate::commands::CmdResult;
use crate::shell::Shell;

impl Shell {
    /// `source file [args...]` — execute in CURRENT shell context (no isolation).
    /// If file is not found relative to cwd, searches $PATH directories.
    pub fn cmd_source(&mut self, args: &[String]) -> CmdResult {
        if args.is_empty() {
            return ("source: filename argument required".into(), 2, None);
        }

        // Try resolve relative to cwd first
        let path = self.resolve(&args[0]);
        let content = match self.fs.read_to_string(&path) {
            Ok(s) => s.to_string(),
            Err(_) => {
                // Fallback: search PATH directories (bash behavior for source)
                match self.find_in_path(&args[0]) {
                    Some(c) => c,
                    None => {
                        return (
                            format!("source: {}: No such file or directory", args[0]),
                            1,
                            None,
                        )
                    }
                }
            }
        };

        let saved_params = if args.len() > 1 {
            let saved = self.save_positional_params();
            self.set_positional_params(&args[1..]);
            Some(saved)
        } else {
            None
        };

        let (out, code) = self.run_script_no_exit_trap(&content);

        if let Some(saved) = saved_params {
            self.restore_positional_params(saved);
        }
        (out, code, None)
    }

    /// `bash [-e] [-x] [-c cmd_string] [file [args...]]` / `sh` — execute in ISOLATED subshell.
    pub fn cmd_bash(&mut self, args: &[String]) -> (String, i32) {
        if args.is_empty() {
            return ("bash: missing script file or -c".into(), 1);
        }

        // Pre-parse option flags before -c or file argument
        let mut set_errexit = false;
        let mut set_xtrace = false;
        let mut skip = 0;
        for arg in args {
            match arg.as_str() {
                "-e" => {
                    set_errexit = true;
                    skip += 1;
                }
                "-x" => {
                    set_xtrace = true;
                    skip += 1;
                }
                // Compound flags like -ex, -xe
                s if s.starts_with('-') && s.len() > 2 && s != "-c" && !s.starts_with("--") => {
                    let mut known = true;
                    for ch in s[1..].chars() {
                        match ch {
                            'e' => set_errexit = true,
                            'x' => set_xtrace = true,
                            _ => {
                                known = false;
                                break;
                            }
                        }
                    }
                    if known {
                        skip += 1;
                    } else {
                        break;
                    }
                }
                _ => break,
            }
        }
        let args = &args[skip..];

        if args.is_empty() {
            return ("bash: missing script file or -c".into(), 1);
        }

        // Handle bash -c 'command string' [name [args...]]
        if args[0] == "-c" {
            if args.len() < 2 {
                return ("bash: -c: option requires an argument".into(), 2);
            }
            let cmd_string = &args[1];
            let snap = self.snapshot_subshell();
            if set_errexit {
                self.exec.opts.errexit = true;
            }
            if set_xtrace {
                self.exec.opts.xtrace = true;
            }
            // args[2] = $0, args[3..] = $1..
            if args.len() > 2 {
                self.set_zero(&args[2]);
            } else {
                self.set_zero("bash");
            }
            if args.len() > 3 {
                self.set_positional_params(&args[3..]);
            }
            let (out, code) = self.run_script(cmd_string);
            self.restore_subshell(snap);
            return (out, code);
        }

        let path = self.resolve(&args[0]);
        let script = match self.fs.read_to_string(&path) {
            Ok(s) => s.to_string(),
            Err(e) => return (format!("bash: {}: {}", args[0], e), 1),
        };

        let snap = self.snapshot_subshell();
        if set_errexit {
            self.exec.opts.errexit = true;
        }
        if set_xtrace {
            self.exec.opts.xtrace = true;
        }
        self.set_zero(&args[0]);
        self.set_positional_params(&args[1..]);
        let (out, code) = self.run_script(&script);
        self.restore_subshell(snap);
        (out, code)
    }

    /// Execute a file as a script (`./script.sh`). ISOLATED subshell.
    pub fn cmd_exec(&mut self, cmd: &str, args: &[String]) -> (String, i32) {
        let path = self.resolve(cmd);
        let meta = match self.fs.metadata(&path) {
            Ok(m) if m.is_dir() => return (format!("conch: {}: Is a directory", cmd), 126),
            Ok(m) => m,
            Err(_) => return (format!("conch: {}: No such file or directory", cmd), 127),
        };
        if !meta.is_readable() {
            return (format!("conch: {}: Permission denied", cmd), 126);
        }
        if !meta.is_executable() {
            return (format!("conch: {}: Permission denied", cmd), 126);
        }
        let script = match self.fs.read_to_string(&path) {
            Ok(s) => s.to_string(),
            Err(e) => return (format!("conch: {}: {}", cmd, e), 126),
        };

        let snap = self.snapshot_subshell();
        self.set_zero(cmd);
        self.set_positional_params(args);
        let (out, code) = self.run_script(&script);
        self.restore_subshell(snap);
        (out, code)
    }

    /// Search PATH directories for a readable file, returning its content.
    fn find_in_path(&mut self, name: &str) -> Option<String> {
        let path_var = self.var("PATH")?.to_string();
        for dir in path_var.split(':') {
            if dir.is_empty() {
                continue;
            }
            let full = format!("{}/{}", dir, name);
            if let Ok(content) = self.fs.read_to_string(&full) {
                return Some(content.to_string());
            }
        }
        None
    }
}
