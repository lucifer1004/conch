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
                    let entry_path = format!("{}/{}", target, name);
                    let (owner, group) = if let Some(e) = self.fs.get(&entry_path) {
                        let uid = e.uid();
                        let gid = e.gid();
                        let owner = self.users.uid_to_name(uid);
                        let group_name = self.users.gid_to_name(gid);
                        (owner, group_name)
                    } else {
                        (self.user.clone(), self.user.clone())
                    };
                    format!("{}{}  {} {}  {}", ty, ms, owner, group, display)
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
                match self.fs.read_to_string(&path) {
                    Ok(s) => parts.push(s.to_string()),
                    Err(e) => return (format!("cat: {}: {}", arg, e), 1, None),
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
            self.fs.touch(&path);
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
            if self.fs.is_dir(&path) {
                if !recursive {
                    return (format!("rm: cannot remove '{}': Is a directory", t), 1);
                }
                let _ = self.fs.remove_dir_all(&path);
            } else if self.fs.is_file(&path) {
                self.fs.remove(&path);
            } else if !force {
                return (
                    format!("rm: cannot remove '{}': No such file or directory", t),
                    1,
                );
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

        let content = match self.fs.read_to_string(&src_path) {
            Ok(s) => s.to_string(),
            Err(bare_vfs::VfsError::IsADirectory) => return ("cp: omitting directory".into(), 1),
            Err(bare_vfs::VfsError::NotFound) => {
                return (
                    format!("cp: cannot stat '{}': No such file or directory", files[0]),
                    1,
                )
            }
            Err(_) => return (format!("cp: '{}': Permission denied", files[0]), 1),
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
        for (p, entry) in self.fs.iter() {
            if p != root && !p.starts_with(&prefix) {
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
                let name = p.rsplit('/').next().unwrap_or(&p);
                if !g.is_match(name) {
                    continue;
                }
            }

            let display = if p == root {
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
                let existing = self
                    .fs
                    .read_to_string(&path)
                    .map(|s| s.to_string())
                    .unwrap_or_default();
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
            if let Err(_) = self.fs.set_mode(&path, mode) {
                return (
                    format!("chmod: cannot access '{}': No such file or directory", arg),
                    1,
                );
            }
        }
        (String::new(), 0)
    }

    pub fn cmd_rmdir(&mut self, args: &[String]) -> (String, i32) {
        for arg in args {
            if arg.starts_with('-') {
                continue;
            }
            let path = self.resolve(arg);
            match self.fs.get(&path) {
                None => {
                    return (
                        format!(
                            "rmdir: failed to remove '{}': No such file or directory",
                            arg
                        ),
                        1,
                    )
                }
                Some(e) if e.is_file() => {
                    return (
                        format!("rmdir: failed to remove '{}': Not a directory", arg),
                        1,
                    )
                }
                _ => {}
            }
            // Check if directory is empty
            let entries = self.fs.read_dir(&path).unwrap_or_default();
            if !entries.is_empty() {
                return (
                    format!("rmdir: failed to remove '{}': Directory not empty", arg),
                    1,
                );
            }
            self.fs.remove(&path);
        }
        (String::new(), 0)
    }

    pub fn cmd_mktemp(&mut self, args: &[String]) -> (String, i32) {
        let mut make_dir = false;
        for arg in args {
            if arg == "-d" {
                make_dir = true;
            }
        }

        // Ensure /tmp exists
        self.fs.create_dir_all("/tmp");

        // Generate a unique name using a simple counter approach
        let name = self.generate_tmpname();
        let path = format!("/tmp/{}", name);

        if make_dir {
            self.fs.insert(path.clone(), FsEntry::dir());
        } else {
            self.fs.insert(path.clone(), FsEntry::file(String::new()));
        }

        (path, 0)
    }

    fn generate_tmpname(&self) -> String {
        // Count existing /tmp entries to create unique suffix
        let count = self
            .fs
            .read_dir("/tmp")
            .map(|entries| entries.len())
            .unwrap_or(0);
        format!("tmp.{:010}", count)
    }

    pub fn cmd_ln(&mut self, args: &[String]) -> (String, i32) {
        let mut symbolic = false;
        let mut positional = Vec::new();

        for arg in args {
            match arg.as_str() {
                "-s" => symbolic = true,
                s if !s.starts_with('-') => positional.push(s),
                _ => {}
            }
        }

        if positional.len() < 2 {
            return ("ln: missing operand".into(), 1);
        }
        if !symbolic {
            return ("ln: hard links not supported".into(), 1);
        }

        let target = positional[0];
        let link_path = self.resolve(positional[1]);
        if self.fs.exists(&link_path) {
            return (
                format!(
                    "ln: failed to create symbolic link '{}': File exists",
                    positional[1]
                ),
                1,
            );
        }
        if let Err(e) = self.fs.symlink(target, &link_path) {
            return (format!("ln: {}: {}", positional[1], e), 1);
        }
        (String::new(), 0)
    }

    pub fn cmd_readlink(&self, args: &[String]) -> (String, i32) {
        if args.is_empty() {
            return ("readlink: missing operand".into(), 1);
        }
        let path = self.resolve(&args[0]);
        match self.fs.read_link(&path) {
            Ok(target) => (target, 0),
            Err(_) => (format!("readlink: {}: Invalid argument", args[0]), 1),
        }
    }

    pub fn cmd_chown(&mut self, args: &[String]) -> (String, i32) {
        let mut recursive = false;
        let mut positional = Vec::new();

        let mut parser = lexopt::Parser::from_args(args.iter().cloned());
        loop {
            match parser.next() {
                Ok(Some(lexopt::Arg::Short('R'))) => recursive = true,
                Ok(Some(lexopt::Arg::Value(val))) => {
                    positional.push(val.to_string_lossy().to_string())
                }
                Ok(Some(_)) => {}
                Ok(None) | Err(_) => break,
            }
        }

        if positional.len() < 2 {
            return ("chown: missing operand".into(), 1);
        }

        let spec = &positional[0];
        let (uid, gid) = self.parse_owner_spec_userdb(spec);

        let targets: Vec<String> = positional[1..].iter().map(|s| self.resolve(s)).collect();
        let orig_args: Vec<String> = positional[1..].iter().cloned().collect();

        for (path, orig) in targets.iter().zip(orig_args.iter()) {
            if recursive && self.fs.is_dir(path) {
                let prefix = format!("{}/", path);
                let all_paths: Vec<String> = self
                    .fs
                    .iter()
                    .into_iter()
                    .filter(|(p, _)| p == path || p.starts_with(&prefix))
                    .map(|(p, _)| p.to_string())
                    .collect();
                for p in all_paths {
                    if let Err(_) = self.fs.chown(&p, uid, gid) {
                        return (
                            format!("chown: cannot access '{}': No such file or directory", orig),
                            1,
                        );
                    }
                }
            } else {
                if let Err(_) = self.fs.chown(path, uid, gid) {
                    return (
                        format!("chown: cannot access '{}': No such file or directory", orig),
                        1,
                    );
                }
            }
        }
        (String::new(), 0)
    }

    pub fn cmd_chgrp(&mut self, args: &[String]) -> (String, i32) {
        let mut recursive = false;
        let mut positional = Vec::new();

        let mut parser = lexopt::Parser::from_args(args.iter().cloned());
        loop {
            match parser.next() {
                Ok(Some(lexopt::Arg::Short('R'))) => recursive = true,
                Ok(Some(lexopt::Arg::Value(val))) => {
                    positional.push(val.to_string_lossy().to_string())
                }
                Ok(Some(_)) => {}
                Ok(None) | Err(_) => break,
            }
        }

        if positional.len() < 2 {
            return ("chgrp: missing operand".into(), 1);
        }

        let group_str = &positional[0];
        let gid = self
            .users
            .resolve_gid(group_str)
            .unwrap_or(self.fs.current_gid());

        let targets: Vec<String> = positional[1..].iter().map(|s| self.resolve(s)).collect();
        let orig_args: Vec<String> = positional[1..].iter().cloned().collect();

        for (path, orig) in targets.iter().zip(orig_args.iter()) {
            if recursive && self.fs.is_dir(path) {
                let prefix = format!("{}/", path);
                let all_paths: Vec<String> = self
                    .fs
                    .iter()
                    .into_iter()
                    .filter(|(p, _)| p == path || p.starts_with(&prefix))
                    .map(|(p, _)| p.to_string())
                    .collect();
                for p in all_paths {
                    let uid = self.fs.get(&p).map(|e| e.uid()).unwrap_or(0);
                    if let Err(_) = self.fs.chown(&p, uid, gid) {
                        return (
                            format!("chgrp: cannot access '{}': No such file or directory", orig),
                            1,
                        );
                    }
                }
            } else {
                let uid = self.fs.get(path).map(|e| e.uid()).unwrap_or(0);
                if let Err(_) = self.fs.chown(path, uid, gid) {
                    return (
                        format!("chgrp: cannot access '{}': No such file or directory", orig),
                        1,
                    );
                }
            }
        }
        (String::new(), 0)
    }

    pub fn cmd_id(&self, _args: &[String]) -> (String, i32) {
        let uid = self.fs.current_uid();
        let gid = self.fs.current_gid();
        let uname = self.users.uid_to_name(uid);
        let gname = self.users.gid_to_name(gid);
        let groups_str = {
            let mut gs = self.users.user_groups(&uname);
            gs.sort_by_key(|g| g.gid);
            gs.iter()
                .map(|g| format!("{}({})", g.gid, g.name))
                .collect::<Vec<_>>()
                .join(",")
        };
        let out = format!(
            "uid={}({}) gid={}({}) groups={}",
            uid, uname, gid, gname, groups_str
        );
        (out, 0)
    }

    pub fn cmd_groups(&self, _args: &[String]) -> (String, i32) {
        let uname = self.users.uid_to_name(self.fs.current_uid());
        let mut gs = self.users.user_groups(&uname);
        gs.sort_by_key(|g| g.gid);
        let out = gs
            .iter()
            .map(|g| g.name.as_str())
            .collect::<Vec<_>>()
            .join(" ");
        (out, 0)
    }

    /// Parse `user:group`, `user:`, `:group`, or `user` into (uid, gid) using UserDb.
    fn parse_owner_spec_userdb(&self, spec: &str) -> (u32, u32) {
        let default_uid = self.fs.current_uid();
        let default_gid = self.fs.current_gid();
        if let Some((user_part, group_part)) = spec.split_once(':') {
            let uid = if user_part.is_empty() {
                default_uid
            } else {
                self.users.resolve_uid(user_part).unwrap_or(default_uid)
            };
            let gid = if group_part.is_empty() {
                default_gid
            } else {
                self.users.resolve_gid(group_part).unwrap_or(default_gid)
            };
            (uid, gid)
        } else {
            let uid = self.users.resolve_uid(spec).unwrap_or(default_uid);
            (uid, default_gid)
        }
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
