use crate::ansi;
use crate::shell::Shell;

impl Shell {
    pub fn cmd_cd(&mut self, args: &[String]) -> (String, i32) {
        let raw = args.first().map(|s| s.as_str()).unwrap_or("");
        let target = if args.is_empty() {
            self.home.clone()
        } else {
            self.resolve(&args[0])
        };

        match self.fs.get(&target) {
            Some(e) if e.is_dir() => {
                self.cwd = target.clone();
                self.env.insert("PWD".to_string(), target);
                (String::new(), 0)
            }
            Some(_) => (format!("cd: not a directory: {}", raw), 1),
            None => (format!("cd: no such file or directory: {}", raw), 1),
        }
    }

    pub fn cmd_tree(&self, args: &[String]) -> (String, i32) {
        let display_arg = args.first().map(|s| s.as_str()).unwrap_or(".");
        let root_path = if args.is_empty() {
            self.cwd.clone()
        } else {
            self.resolve(&args[0])
        };

        if !self.fs.get(&root_path).map_or(false, |e| e.is_dir()) {
            return (format!("tree: '{}': No such directory", display_arg), 1);
        }

        let root_display = if root_path == self.cwd {
            ".".into()
        } else {
            root_path
                .rsplit('/')
                .next()
                .unwrap_or(&root_path)
                .to_string()
        };

        let mut lines = vec![root_display];
        self.tree_recurse(&root_path, "", &mut lines);
        (lines.join("\n"), 0)
    }

    fn tree_recurse(&self, dir: &str, prefix: &str, lines: &mut Vec<String>) {
        let children = self.list_dir(dir);
        for (i, (name, is_dir, _mode)) in children.iter().enumerate() {
            let is_last = i == children.len() - 1;
            let connector = if is_last {
                "\u{2514}\u{2500}\u{2500} "
            } else {
                "\u{251c}\u{2500}\u{2500} "
            };
            let display = if *is_dir {
                format!("{}{}/{}", ansi::BOLD_BLUE, name, ansi::RESET)
            } else {
                name.clone()
            };
            lines.push(format!("{}{}{}", prefix, connector, display));

            if *is_dir {
                let child_prefix = if is_last {
                    format!("{}    ", prefix)
                } else {
                    format!("{}\u{2502}   ", prefix)
                };
                self.tree_recurse(&format!("{}/{}", dir, name), &child_prefix, lines);
            }
        }
    }

    pub fn cmd_env(&self) -> (String, i32) {
        let mut lines: Vec<String> = self
            .env
            .iter()
            .filter(|(k, _)| k.as_str() != "?")
            .map(|(k, v)| format!("{}={}", k, v))
            .collect();
        lines.sort();
        (lines.join("\n"), 0)
    }

    pub fn cmd_export(&mut self, args: &[String]) -> (String, i32) {
        for arg in args {
            if let Some((k, v)) = arg.split_once('=') {
                let expanded = self.expand(v);
                self.env.insert(k.to_string(), expanded);
            }
        }
        (String::new(), 0)
    }

    pub fn cmd_bash(&mut self, args: &[String]) -> (String, i32) {
        if args.is_empty() {
            return ("bash: missing script file".into(), 1);
        }
        let path = self.resolve(&args[0]);
        let script = match self.fs.get(&path) {
            Some(e) if e.is_file() && !e.is_readable() => {
                return (format!("bash: {}: Permission denied", args[0]), 1)
            }
            Some(e) if e.is_file() => e.content().unwrap().to_string(),
            Some(_) => return (format!("bash: {}: Is a directory", args[0]), 1),
            None => return (format!("bash: {}: No such file or directory", args[0]), 1),
        };
        self.run_script(&script)
    }

    /// Execute a file as a script (for `./script.sh` invocation).
    /// Requires the file to have execute permission.
    pub fn cmd_exec(&mut self, cmd: &str, _args: &[String]) -> (String, i32) {
        let path = self.resolve(cmd);
        let (script, mode) = match self.fs.get(&path) {
            Some(e) if e.is_file() && !e.is_readable() => {
                return (format!("conch: {}: Permission denied", cmd), 126)
            }
            Some(e) if e.is_file() => (e.content().unwrap().to_string(), e.mode()),
            Some(_) => return (format!("conch: {}: Is a directory", cmd), 126),
            None => return (format!("conch: {}: No such file or directory", cmd), 127),
        };
        // Check execute permission (any execute bit)
        if mode & 0o111 == 0 {
            return (format!("conch: {}: Permission denied", cmd), 126);
        }
        self.run_script(&script)
    }

    pub fn cmd_date(&self) -> (String, i32) {
        let date = self
            .env
            .get("DATE")
            .cloned()
            .unwrap_or_else(|| "Mon Jan  1 00:00:00 UTC 2024".to_string());
        (date, 0)
    }

    pub fn cmd_which(&self, args: &[String]) -> (String, i32) {
        if args.is_empty() {
            return (String::new(), 1);
        }
        let mut out = Vec::new();
        for arg in args {
            if crate::commands::BUILTINS.contains(&arg.as_str()) {
                out.push(format!("/bin/{}", arg));
            } else {
                return (format!("which: no {} in (/bin)", arg), 1);
            }
        }
        (out.join("\n"), 0)
    }

    pub fn cmd_type(&self, args: &[String]) -> (String, i32) {
        if args.is_empty() {
            return (String::new(), 1);
        }
        let mut out = Vec::new();
        for arg in args {
            if crate::commands::BUILTINS.contains(&arg.as_str()) {
                out.push(format!("{} is a shell builtin", arg));
            } else {
                out.push(format!("type: {}: not found", arg));
            }
        }
        (out.join("\n"), 0)
    }
}
