use globset::Glob;

use crate::ansi;
use crate::commands::CmdResult;
use crate::shell::Shell;
use crate::types::FsEntry;

impl Shell {
    pub fn cmd_ls(&self, args: &[String]) -> (String, i32) {
        let mut show_all = false;
        let mut long = false;
        let mut paths = Vec::new();

        let mut parser = lexopt::Parser::from_args(args.iter().cloned());
        loop {
            match parser.next() {
                Ok(Some(lexopt::Arg::Short('a'))) => show_all = true,
                Ok(Some(lexopt::Arg::Short('l'))) => long = true,
                Ok(Some(lexopt::Arg::Value(val))) => paths.push(val.to_string_lossy().to_string()),
                Ok(Some(_)) => {}
                Ok(None) | Err(_) => break,
            }
        }
        if paths.is_empty() {
            paths.push(".".to_string());
        }

        let target = self.resolve(&paths[0]);

        match self.fs.get(&target) {
            Some(e) if e.is_file() => {
                return (target.rsplit('/').next().unwrap_or(&target).to_string(), 0);
            }
            None => {
                return (
                    format!(
                        "ls: cannot access '{}': No such file or directory",
                        paths[0]
                    ),
                    2,
                );
            }
            _ => {}
        }

        let children = self.list_dir(&target);
        let formatted: Vec<String> = children
            .iter()
            .filter(|(name, _, _)| show_all || !name.starts_with('.'))
            .map(|(name, is_dir, mode)| {
                if long {
                    let ty = if *is_dir { "d" } else { "-" };
                    let ms = FsEntry::format_mode(*mode);
                    let display = if *is_dir {
                        format!("{}{}/{}", ansi::BOLD_BLUE, name, ansi::RESET)
                    } else {
                        name.clone()
                    };
                    format!("{}{}  {} {}  {}", ty, ms, self.user, self.user, display)
                } else if *is_dir {
                    format!("{}{}/{}", ansi::BOLD_BLUE, name, ansi::RESET)
                } else {
                    name.clone()
                }
            })
            .collect();

        let sep = if long { "\n" } else { "  " };
        (formatted.join(sep), 0)
    }

    pub fn cmd_cat(&self, args: &[String], stdin: Option<&str>) -> CmdResult {
        let mut line_numbers = false;
        let mut file_args = Vec::new();

        for arg in args {
            match arg.as_str() {
                "-n" => line_numbers = true,
                s if s.starts_with('-') => {}
                _ => file_args.push(arg),
            }
        }

        let content = if file_args.is_empty() {
            stdin.unwrap_or("").to_string()
        } else {
            let mut parts = Vec::new();
            for arg in &file_args {
                let path = self.resolve(arg);
                match self.fs.get(&path) {
                    Some(e) if e.is_file() && !e.is_readable() => {
                        return (format!("cat: {}: Permission denied", arg), 1, None)
                    }
                    Some(e) if e.is_file() => parts.push(e.content().unwrap().to_string()),
                    Some(e) if e.is_dir() => {
                        return (format!("cat: {}: Is a directory", arg), 1, None)
                    }
                    _ => return (format!("cat: {}: No such file or directory", arg), 1, None),
                }
            }
            parts.join("\n")
        };

        let lang = if file_args.len() == 1 && stdin.is_none() && !line_numbers {
            detect_lang(file_args[0])
        } else {
            None
        };

        if line_numbers {
            let numbered: Vec<String> = content
                .lines()
                .enumerate()
                .map(|(i, line)| format!("{:>6}\t{}", i + 1, line))
                .collect();
            (numbered.join("\n"), 0, None)
        } else {
            (content, 0, lang)
        }
    }

    pub fn cmd_mkdir(&mut self, args: &[String]) -> (String, i32) {
        let mut parents = false;
        let mut dirs = Vec::new();

        let mut parser = lexopt::Parser::from_args(args.iter().cloned());
        loop {
            match parser.next() {
                Ok(Some(lexopt::Arg::Short('p'))) => parents = true,
                Ok(Some(lexopt::Arg::Value(val))) => dirs.push(val.to_string_lossy().to_string()),
                Ok(Some(_)) => {}
                Ok(None) | Err(_) => break,
            }
        }

        for d in &dirs {
            let path = self.resolve(d);
            if parents {
                self.mkdir_p(&path);
            } else {
                let parent = path
                    .rsplit_once('/')
                    .map(|(p, _)| if p.is_empty() { "/" } else { p })
                    .unwrap_or("/");
                match self.fs.get(parent) {
                    Some(e) if e.is_dir() => {}
                    _ => {
                        return (
                            format!(
                                "mkdir: cannot create directory '{}': No such file or directory",
                                d
                            ),
                            1,
                        )
                    }
                }
                self.fs.insert(path, FsEntry::dir());
            }
        }
        (String::new(), 0)
    }

    pub fn cmd_touch(&mut self, args: &[String]) -> (String, i32) {
        for arg in args {
            if arg.starts_with('-') {
                continue;
            }
            let path = self.resolve(arg);
            self.fs.entry(path).or_insert(FsEntry::file(String::new()));
        }
        (String::new(), 0)
    }

    pub fn cmd_rm(&mut self, args: &[String]) -> (String, i32) {
        let mut recursive = false;
        let mut force = false;
        let mut targets = Vec::new();

        let mut parser = lexopt::Parser::from_args(args.iter().cloned());
        loop {
            match parser.next() {
                Ok(Some(lexopt::Arg::Short('r' | 'R'))) => recursive = true,
                Ok(Some(lexopt::Arg::Short('f'))) => force = true,
                Ok(Some(lexopt::Arg::Value(val))) => {
                    targets.push(val.to_string_lossy().to_string())
                }
                Ok(Some(_)) => {}
                Ok(None) | Err(_) => break,
            }
        }

        for t in &targets {
            let path = self.resolve(t);
            match self.fs.get(&path) {
                Some(e) if e.is_dir() => {
                    if !recursive {
                        return (format!("rm: cannot remove '{}': Is a directory", t), 1);
                    }
                    let prefix = format!("{}/", path);
                    let to_rm: Vec<String> = self
                        .fs
                        .keys()
                        .filter(|k| *k == &path || k.starts_with(&prefix))
                        .cloned()
                        .collect();
                    for k in to_rm {
                        self.fs.remove(&k);
                    }
                }
                Some(e) if e.is_file() => {
                    self.fs.remove(&path);
                }
                _ => {
                    if !force {
                        return (
                            format!("rm: cannot remove '{}': No such file or directory", t),
                            1,
                        );
                    }
                }
            }
        }
        (String::new(), 0)
    }

    pub fn cmd_cp(&mut self, args: &[String]) -> (String, i32) {
        let files: Vec<&String> = args.iter().filter(|a| !a.starts_with('-')).collect();
        if files.len() < 2 {
            return ("cp: missing operand".into(), 1);
        }

        let src_path = self.resolve(files[0]);
        let dst_path = self.resolve(files[1]);

        let content = match self.fs.get(&src_path) {
            Some(e) if e.is_file() && !e.is_readable() => {
                return (format!("cp: '{}': Permission denied", files[0]), 1)
            }
            Some(e) if e.is_file() => e.content().unwrap().to_string(),
            Some(e) if e.is_dir() => return ("cp: omitting directory".into(), 1),
            _ => {
                return (
                    format!("cp: cannot stat '{}': No such file or directory", files[0]),
                    1,
                )
            }
        };

        let target = match self.fs.get(&dst_path) {
            Some(e) if e.is_dir() => {
                let name = src_path.rsplit('/').next().unwrap_or(files[0]);
                format!("{}/{}", dst_path, name)
            }
            _ => dst_path,
        };
        self.fs.insert(target, FsEntry::file(content));
        (String::new(), 0)
    }

    pub fn cmd_mv(&mut self, args: &[String]) -> (String, i32) {
        let (out, code) = self.cmd_cp(args);
        if code != 0 {
            return (out.replace("cp:", "mv:"), code);
        }
        let files: Vec<&String> = args.iter().filter(|a| !a.starts_with('-')).collect();
        let src = self.resolve(files[0]);
        self.fs.remove(&src);
        (String::new(), 0)
    }

    pub fn cmd_find(&self, args: &[String]) -> (String, i32) {
        let mut search_path = ".".to_string();
        let mut name_pattern = None;
        let mut type_filter = None;

        let mut i = 0;
        while i < args.len() {
            match args[i].as_str() {
                "-name" if i + 1 < args.len() => {
                    name_pattern = Some(args[i + 1].clone());
                    i += 2;
                }
                "-type" if i + 1 < args.len() => {
                    type_filter = Some(args[i + 1].clone());
                    i += 2;
                }
                s if !s.starts_with('-') && i == 0 => {
                    search_path = s.to_string();
                    i += 1;
                }
                _ => {
                    i += 1;
                }
            }
        }

        let root = self.resolve(&search_path);
        match self.fs.get(&root) {
            Some(e) if e.is_dir() => {}
            _ => {
                return (
                    format!("find: '{}': No such file or directory", search_path),
                    1,
                )
            }
        }

        let glob = name_pattern
            .as_ref()
            .and_then(|p| Glob::new(p).ok().map(|g| g.compile_matcher()));
        let prefix = if root == "/" {
            "/".to_string()
        } else {
            format!("{}/", root)
        };

        let mut results = Vec::new();
        for (p, entry) in &self.fs {
            if p != &root && !p.starts_with(&prefix) {
                continue;
            }

            if let Some(ref tf) = type_filter {
                match tf.as_str() {
                    "f" if !entry.is_file() => continue,
                    "d" if !entry.is_dir() => continue,
                    _ => {}
                }
            }

            if let Some(ref g) = glob {
                let name = p.rsplit('/').next().unwrap_or(p);
                if !g.is_match(name) {
                    continue;
                }
            }

            let display = if p == &root {
                ".".to_string()
            } else {
                let rel = &p[root.len()..];
                let rel = rel.strip_prefix('/').unwrap_or(rel);
                format!("./{}", rel)
            };
            results.push(display);
        }

        results.sort();
        (results.join("\n"), 0)
    }

    pub fn cmd_tee(&mut self, args: &[String], stdin: Option<&str>) -> (String, i32) {
        let mut append = false;
        let mut files = Vec::new();

        for arg in args {
            match arg.as_str() {
                "-a" => append = true,
                _ => files.push(arg.as_str()),
            }
        }

        let input = stdin.unwrap_or("").to_string();

        for f in &files {
            let path = self.resolve(f);
            if let Some(e) = self.fs.get(&path) {
                if !e.is_writable() {
                    return (format!("tee: {}: Permission denied", f), 1);
                }
            }
            if append {
                let existing = match self.fs.get(&path) {
                    Some(e) if e.is_file() => e.content().unwrap().to_string(),
                    _ => String::new(),
                };
                let content = if existing.is_empty() {
                    input.clone()
                } else {
                    format!("{}\n{}", existing, input)
                };
                self.fs.insert(path, FsEntry::file(content));
            } else {
                self.fs.insert(path, FsEntry::file(input.clone()));
            }
        }

        (input, 0)
    }

    pub fn cmd_chmod(&mut self, args: &[String]) -> (String, i32) {
        if args.len() < 2 {
            return ("chmod: missing operand".into(), 1);
        }
        let mode_str = &args[0];
        let mode: u16 = match u16::from_str_radix(mode_str, 8) {
            Ok(m) => m,
            Err(_) => return (format!("chmod: invalid mode: '{}'", mode_str), 1),
        };

        for arg in &args[1..] {
            let path = self.resolve(arg);
            match self.fs.get(&path).cloned() {
                Some(FsEntry::File { content, .. }) => {
                    self.fs.insert(path, FsEntry::file_with_mode(content, mode));
                }
                Some(FsEntry::Dir { .. }) => {
                    self.fs.insert(path, FsEntry::Dir { mode });
                }
                None => {
                    return (
                        format!("chmod: cannot access '{}': No such file or directory", arg),
                        1,
                    )
                }
            }
        }
        (String::new(), 0)
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
}

fn detect_lang(filename: &str) -> Option<String> {
    let ext = filename.rsplit('.').next()?;
    let lang = match ext {
        "typ" => "typ",
        "rs" => "rust",
        "py" => "python",
        "js" => "javascript",
        "ts" => "typescript",
        "html" => "html",
        "css" => "css",
        "json" => "json",
        "toml" => "toml",
        "yaml" | "yml" => "yaml",
        "md" => "markdown",
        "sh" | "bash" | "zsh" => "bash",
        "c" | "h" => "c",
        "cpp" | "cc" | "hpp" => "cpp",
        "java" => "java",
        "go" => "go",
        "rb" => "ruby",
        "xml" => "xml",
        "sql" => "sql",
        "r" => "r",
        _ => return None,
    };
    Some(lang.to_string())
}
