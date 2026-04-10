use crate::ansi;
use crate::shell::Shell;
use crate::types::FsEntry;

impl Shell {
    pub fn cmd_stat(&self, args: &[String]) -> (String, i32) {
        let file_args: Vec<&String> = args.iter().filter(|a| !a.starts_with('-')).collect();
        if file_args.is_empty() {
            return ("stat: missing operand".into(), 1);
        }

        let mut out = Vec::new();
        for arg in &file_args {
            let path = self.resolve(arg);
            let meta = match self.fs.metadata(&path) {
                Ok(m) => m,
                Err(_) => {
                    return (
                        format!("stat: cannot stat '{}': No such file or directory", arg),
                        1,
                    )
                }
            };

            let name = path.rsplit('/').next().unwrap_or(&path);
            let type_str = if meta.is_dir() {
                "directory"
            } else {
                "regular file"
            };
            let size = meta.len();
            let mode = meta.mode();
            let mode_str = FsEntry::format_mode(mode);
            let mode_octal = format!("{:04o}", mode);

            out.push(format!(
                "  File: {}\n  Size: {:<12}Type: {}\n  Mode: ({}/{}{})\n  Uid: {:<8} Gid: {}",
                name,
                size,
                type_str,
                mode_octal,
                if meta.is_dir() { "d" } else { "-" },
                mode_str,
                meta.uid(),
                meta.gid(),
            ));
        }

        (out.join("\n"), 0)
    }

    pub fn cmd_test(&self, args: &[String]) -> (String, i32) {
        // Strip trailing ] for the `[` command
        let args: Vec<&String> = if args.last().map(|s| s.as_str()) == Some("]") {
            &args[..args.len() - 1]
        } else {
            args
        }
        .iter()
        .collect();

        let result = self.evaluate_test(&args);
        (String::new(), if result { 0 } else { 1 })
    }

    fn evaluate_test(&self, args: &[&String]) -> bool {
        match args {
            [] => false,
            [flag, path] if flag.as_str() == "-e" => {
                let p = self.resolve(path.as_str());
                self.fs.exists(&p)
            }
            [flag, path] if flag.as_str() == "-f" => {
                let p = self.resolve(path.as_str());
                self.fs.is_file(&p)
            }
            [flag, path] if flag.as_str() == "-d" => {
                let p = self.resolve(path.as_str());
                self.fs.is_dir(&p)
            }
            [flag, path] if flag.as_str() == "-r" => {
                let p = self.resolve(path.as_str());
                self.fs
                    .metadata(&p)
                    .map(|m| m.is_readable())
                    .unwrap_or(false)
            }
            [flag, path] if flag.as_str() == "-w" => {
                let p = self.resolve(path.as_str());
                self.fs
                    .metadata(&p)
                    .map(|m| m.is_writable())
                    .unwrap_or(false)
            }
            [flag, path] if flag.as_str() == "-x" => {
                let p = self.resolve(path.as_str());
                self.fs
                    .metadata(&p)
                    .map(|m| m.is_executable())
                    .unwrap_or(false)
            }
            [flag, path] if flag.as_str() == "-s" => {
                let p = self.resolve(path.as_str());
                self.fs.metadata(&p).map(|m| !m.is_empty()).unwrap_or(false)
            }
            [flag, s] if flag.as_str() == "-z" => s.is_empty(),
            [flag, s] if flag.as_str() == "-n" => !s.is_empty(),
            [a, op, b] if op.as_str() == "=" => a == b,
            [a, op, b] if op.as_str() == "!=" => a != b,
            [a, op, b] if op.as_str() == "-eq" => {
                let na: i64 = a.parse().unwrap_or(0);
                let nb: i64 = b.parse().unwrap_or(0);
                na == nb
            }
            [a, op, b] if op.as_str() == "-ne" => {
                let na: i64 = a.parse().unwrap_or(0);
                let nb: i64 = b.parse().unwrap_or(0);
                na != nb
            }
            [a, op, b] if op.as_str() == "-lt" => {
                let na: i64 = a.parse().unwrap_or(0);
                let nb: i64 = b.parse().unwrap_or(0);
                na < nb
            }
            [a, op, b] if op.as_str() == "-le" => {
                let na: i64 = a.parse().unwrap_or(0);
                let nb: i64 = b.parse().unwrap_or(0);
                na <= nb
            }
            [a, op, b] if op.as_str() == "-gt" => {
                let na: i64 = a.parse().unwrap_or(0);
                let nb: i64 = b.parse().unwrap_or(0);
                na > nb
            }
            [a, op, b] if op.as_str() == "-ge" => {
                let na: i64 = a.parse().unwrap_or(0);
                let nb: i64 = b.parse().unwrap_or(0);
                na >= nb
            }
            _ => false,
        }
    }

    pub fn cmd_du(&self, args: &[String]) -> (String, i32) {
        let mut summary = false;
        let mut human = false;
        let mut paths = Vec::new();

        let mut parser = lexopt::Parser::from_args(args.iter().cloned());
        loop {
            match parser.next() {
                Ok(Some(lexopt::Arg::Short('s'))) => summary = true,
                Ok(Some(lexopt::Arg::Short('h'))) => human = true,
                Ok(Some(lexopt::Arg::Value(val))) => paths.push(val.to_string_lossy().to_string()),
                Ok(Some(_)) => {}
                Ok(None) | Err(_) => break,
            }
        }

        if paths.is_empty() {
            paths.push(".".to_string());
        }

        let format_size = |bytes: usize| -> String {
            if !human {
                // Report in 1K blocks (ceiling division)
                let kb = bytes.div_ceil(1024);
                return kb.to_string();
            }
            if bytes >= 1024 * 1024 {
                format!("{:.1}M", bytes as f64 / (1024.0 * 1024.0))
            } else if bytes >= 1024 {
                format!("{:.1}K", bytes as f64 / 1024.0)
            } else {
                format!("{}B", bytes)
            }
        };

        let mut out = Vec::new();

        for target_arg in &paths {
            let root = self.resolve(target_arg);

            match self.fs.get(&root) {
                None => {
                    return (
                        format!(
                            "du: cannot access '{}': No such file or directory",
                            target_arg
                        ),
                        1,
                    )
                }
                Some(e) if e.is_file() => {
                    let size = e.len();
                    out.push(format!("{}\t{}", format_size(size), target_arg));
                    continue;
                }
                _ => {}
            }

            // It's a directory — collect all entries under it
            let prefix = if root == "/" {
                "/".to_string()
            } else {
                format!("{}/", root)
            };

            let mut total: usize = 0;
            let mut entries_out: Vec<(String, usize)> = Vec::new();

            for (path, entry) in self.fs.iter() {
                if path != root && !path.starts_with(&prefix) {
                    continue;
                }
                let size = entry.len();
                total += size;
                if !summary {
                    // Display relative path
                    let display = if path == root {
                        target_arg.clone()
                    } else {
                        let rel = &path[root.len()..];
                        let rel = rel.strip_prefix('/').unwrap_or(rel);
                        format!("{}/{}", target_arg, rel)
                    };
                    entries_out.push((display, size));
                }
            }

            if summary {
                out.push(format!("{}\t{}", format_size(total), target_arg));
            } else {
                entries_out.sort_by(|a, b| a.0.cmp(&b.0));
                for (display, size) in entries_out {
                    out.push(format!("{}\t{}", format_size(size), display));
                }
                out.push(format!("{}\t{}", format_size(total), target_arg));
            }
        }

        (out.join("\n"), 0)
    }

    pub fn cmd_tree(&self, args: &[String]) -> (String, i32) {
        let display_arg = args.first().map(|s| s.as_str()).unwrap_or(".");
        let root_path = if args.is_empty() {
            self.cwd.clone()
        } else {
            self.resolve(&args[0])
        };

        if !self.fs.get(&root_path).is_some_and(|e| e.is_dir()) {
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
}
