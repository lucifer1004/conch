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
        let mut any_failed = false;
        for arg in args {
            if crate::commands::BUILTINS.contains(&arg.as_str()) {
                out.push(format!("/bin/{}", arg));
            } else {
                out.push(format!("which: no {} in (/bin)", arg));
                any_failed = true;
            }
        }
        (out.join("\n"), if any_failed { 1 } else { 0 })
    }

    pub fn cmd_type(&self, args: &[String]) -> (String, i32) {
        if args.is_empty() {
            return (String::new(), 1);
        }
        let mut out = Vec::new();
        let mut any_failed = false;
        for arg in args {
            if crate::commands::BUILTINS.contains(&arg.as_str()) {
                out.push(format!("{} is a shell builtin", arg));
            } else {
                out.push(format!("type: {}: not found", arg));
                any_failed = true;
            }
        }
        (out.join("\n"), if any_failed { 1 } else { 0 })
    }

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

    pub fn cmd_unset(&mut self, args: &[String]) -> (String, i32) {
        for arg in args {
            self.env.remove(arg);
        }
        (String::new(), 0)
    }

    pub fn cmd_sleep(&self, _args: &[String]) -> (String, i32) {
        // No-op in WASM — cannot actually sleep
        (String::new(), 0)
    }

    pub fn cmd_history(&self, _args: &[String]) -> (String, i32) {
        let out: Vec<String> = self
            .history
            .iter()
            .enumerate()
            .map(|(i, cmd)| format!("{:>5}  {}", i + 1, cmd))
            .collect();
        (out.join("\n"), 0)
    }

    pub fn cmd_realpath(&self, args: &[String]) -> (String, i32) {
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
