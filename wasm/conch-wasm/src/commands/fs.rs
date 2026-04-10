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

        let multi = paths.len() > 1;
        let mut sections = Vec::new();

        for path_arg in &paths {
            let target = self.resolve(path_arg);

            match self.fs.get(&target) {
                Some(e) if e.is_file() => {
                    sections.push(target.rsplit('/').next().unwrap_or(&target).to_string());
                    continue;
                }
                None => {
                    return (
                        format!(
                            "ls: cannot access '{}': No such file or directory",
                            path_arg
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
                        let ms = FsEntry::format_mode(*mode);
                        let entry_path = format!("{}/{}", target, name);
                        let is_symlink = self.fs.is_symlink(&entry_path);
                        let ty = if is_symlink {
                            "l"
                        } else if *is_dir {
                            "d"
                        } else {
                            "-"
                        };
                        let display = if *is_dir {
                            format!("{}{}/{}", ansi::BOLD_BLUE, name, ansi::RESET)
                        } else {
                            name.clone()
                        };
                        let (owner, group, nlink, size) =
                            if let Ok(meta) = self.fs.metadata(&entry_path) {
                                let owner = self.users.uid_to_name(meta.uid());
                                let group_name = self.users.gid_to_name(meta.gid());
                                (owner, group_name, meta.nlink(), meta.len())
                            } else {
                                (self.user.clone(), self.user.clone(), 1, 0)
                            };
                        format!(
                            "{}{} {:>2} {} {} {:>5} {}",
                            ty, ms, nlink, owner, group, size, display
                        )
                    } else if *is_dir {
                        format!("{}{}/{}", ansi::BOLD_BLUE, name, ansi::RESET)
                    } else {
                        name.clone()
                    }
                })
                .collect();

            let sep = if long { "\n" } else { "  " };
            let mut section = String::new();
            if multi {
                section.push_str(path_arg);
                section.push(':');
                section.push('\n');
            }
            section.push_str(&formatted.join(sep));
            sections.push(section);
        }

        (sections.join("\n\n"), 0)
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
            parts.join("")
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
                if let Err(e) = self.mkdir_p(&path) {
                    return (format!("mkdir: cannot create directory '{}': {}", d, e), 1);
                }
            } else {
                match self.fs.create_dir(&path) {
                    Ok(()) => {}
                    Err(_) => {
                        return (
                            format!(
                                "mkdir: cannot create directory '{}': No such file or directory",
                                d
                            ),
                            1,
                        )
                    }
                };
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
            if self.fs.touch(&path).is_err() {
                return (
                    format!("touch: cannot touch '{}': No such file or directory", arg),
                    1,
                );
            }
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
                if let Err(e) = self.fs.remove_dir_all(&path) {
                    if !force || *e.kind() != bare_vfs::VfsErrorKind::NotFound {
                        return (format!("rm: cannot remove '{}': {}", t, e), 1);
                    }
                }
            } else if self.fs.is_file(&path) {
                if self.fs.remove(&path).is_none() && !force {
                    return (
                        format!("rm: cannot remove '{}': No such file or directory", t),
                        1,
                    );
                }
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

        // When more than 2 args, last arg must be a directory
        if files.len() > 2 {
            let dst_path = self.resolve(files[files.len() - 1]);
            if !self.fs.is_dir(&dst_path) {
                return (
                    format!("cp: target '{}' is not a directory", files[files.len() - 1]),
                    1,
                );
            }
            for src in &files[..files.len() - 1] {
                let src_path = self.resolve(src);
                let content = match self.fs.read_to_string(&src_path) {
                    Ok(s) => s.to_string(),
                    Err(ref e) if *e.kind() == bare_vfs::VfsErrorKind::IsADirectory => {
                        return ("cp: omitting directory".into(), 1)
                    }
                    Err(ref e) if *e.kind() == bare_vfs::VfsErrorKind::NotFound => {
                        return (
                            format!("cp: cannot stat '{}': No such file or directory", src),
                            1,
                        )
                    }
                    Err(_) => return (format!("cp: '{}': Permission denied", src), 1),
                };
                let name = src_path.rsplit('/').next().unwrap_or(src);
                let target = format!("{}/{}", dst_path, name);
                if let Err(e) = self.fs.write(&target, content.as_bytes()) {
                    return (format!("cp: cannot create '{}': {}", src, e), 1);
                }
            }
            return (String::new(), 0);
        }

        let src_path = self.resolve(files[0]);
        let dst_path = self.resolve(files[1]);

        let content = match self.fs.read_to_string(&src_path) {
            Ok(s) => s.to_string(),
            Err(ref e) if *e.kind() == bare_vfs::VfsErrorKind::IsADirectory => {
                return ("cp: omitting directory".into(), 1)
            }
            Err(ref e) if *e.kind() == bare_vfs::VfsErrorKind::NotFound => {
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
        if let Err(e) = self.fs.write(&target, content.as_bytes()) {
            return (format!("cp: cannot create '{}': {}", files[1], e), 1);
        }
        (String::new(), 0)
    }

    pub fn cmd_mv(&mut self, args: &[String]) -> (String, i32) {
        let files: Vec<&String> = args.iter().filter(|a| !a.starts_with('-')).collect();
        if files.len() < 2 {
            return ("mv: missing operand".into(), 1);
        }
        let src_path = self.resolve(files[0]);
        let mut dst_path = self.resolve(files[1]);

        // If destination is a directory, move into it
        if self.fs.is_dir(&dst_path) {
            let name = src_path.rsplit('/').next().unwrap_or(files[0]);
            dst_path = format!("{}/{}", dst_path, name);
        }

        match self.fs.rename(&src_path, &dst_path) {
            Ok(()) => (String::new(), 0),
            Err(e) => (format!("mv: {}", e), 1),
        }
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

        let mut results = Vec::new();
        for (p, entry) in self.fs.walk_prefix(&root) {
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
                if !self.fs.exists(&path) {
                    if let Err(e) = self.fs.write(&path, input.as_bytes()) {
                        return (format!("tee: {}: {}", f, e), 1);
                    }
                } else if let Err(e) = self.fs.append(&path, input.as_bytes()) {
                    return (format!("tee: {}: {}", f, e), 1);
                }
            } else {
                if let Err(e) = self.fs.write(&path, input.as_bytes()) {
                    return (format!("tee: {}: {}", f, e), 1);
                }
            }
        }

        (input, 0)
    }

    pub fn cmd_chmod(&mut self, args: &[String]) -> (String, i32) {
        if args.len() < 2 {
            return ("chmod: missing operand".into(), 1);
        }
        let mode_str = &args[0];

        // Try octal first, then symbolic
        let is_symbolic = mode_str
            .chars()
            .next()
            .map(|c| !c.is_ascii_digit())
            .unwrap_or(false);

        for arg in &args[1..] {
            let path = self.resolve(arg);

            let mode = if is_symbolic {
                // Parse symbolic mode, get current mode first
                let current = match self.fs.metadata(&path) {
                    Ok(m) => m.mode(),
                    Err(e) if *e.kind() == bare_vfs::VfsErrorKind::NotFound => {
                        return (
                            format!("chmod: cannot access '{}': No such file or directory", arg),
                            1,
                        );
                    }
                    Err(_) => {
                        return (
                            format!(
                                "chmod: changing permissions of '{}': Operation not permitted",
                                arg
                            ),
                            1,
                        );
                    }
                };
                match apply_symbolic_mode(mode_str, current) {
                    Some(m) => m,
                    None => return (format!("chmod: invalid mode: '{}'", mode_str), 1),
                }
            } else {
                match u16::from_str_radix(mode_str, 8) {
                    Ok(m) => m,
                    Err(_) => return (format!("chmod: invalid mode: '{}'", mode_str), 1),
                }
            };

            match self.fs.set_mode(&path, mode) {
                Err(e) if *e.kind() == bare_vfs::VfsErrorKind::NotFound => {
                    return (
                        format!("chmod: cannot access '{}': No such file or directory", arg),
                        1,
                    );
                }
                Err(_) => {
                    return (
                        format!(
                            "chmod: changing permissions of '{}': Operation not permitted",
                            arg
                        ),
                        1,
                    );
                }
                Ok(()) => {}
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
            if !self.fs.is_empty_dir(&path) {
                return (
                    format!("rmdir: failed to remove '{}': Directory not empty", arg),
                    1,
                );
            }
            if self.fs.remove(&path).is_none() {
                return (
                    format!(
                        "rmdir: failed to remove '{}': No such file or directory",
                        arg
                    ),
                    1,
                );
            }
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
        if self.fs.create_dir_all("/tmp").is_err() {
            return ("mktemp: failed to create /tmp".into(), 1);
        }

        // Generate a unique name using a simple counter approach
        let name = self.generate_tmpname();
        let path = format!("/tmp/{}", name);

        if make_dir {
            if let Err(e) = self.fs.create_dir(&path) {
                return (format!("mktemp: failed to create directory: {}", e), 1);
            }
        } else {
            if let Err(e) = self.fs.write(&path, b"" as &[u8]) {
                return (format!("mktemp: failed to create file: {}", e), 1);
            }
        }

        (path, 0)
    }

    fn generate_tmpname(&mut self) -> String {
        // Use a monotonic counter to guarantee uniqueness even after deletions
        let count = self.tmp_counter;
        self.tmp_counter += 1;
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

        let target = positional[0];
        let link_path = self.resolve(positional[1]);

        if !symbolic {
            let target_path = self.resolve(target);
            if let Err(e) = self.fs.hard_link(&target_path, &link_path) {
                return (format!("ln: {}: {}", positional[1], e), 1);
            }
            return (String::new(), 0);
        }
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
        let mut canonicalize = false;
        let mut target_arg = None;
        for arg in args {
            match arg.as_str() {
                "-f" => canonicalize = true,
                s if !s.starts_with('-') && target_arg.is_none() => {
                    target_arg = Some(arg.as_str());
                }
                _ => {}
            }
        }
        let target_str = match target_arg {
            Some(t) => t,
            None => return ("readlink: missing operand".into(), 1),
        };
        let path = self.resolve(target_str);
        if canonicalize {
            match self.fs.canonical_path(&path) {
                Ok(canonical) => (canonical, 0),
                Err(_) => (
                    format!("readlink: {}: No such file or directory", target_str),
                    1,
                ),
            }
        } else {
            match self.fs.read_link(&path) {
                Ok(target) => (target, 0),
                Err(_) => (format!("readlink: {}: Invalid argument", target_str), 1),
            }
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
        let (uid, gid) = match self.parse_owner_spec_userdb(spec) {
            Ok(v) => v,
            Err(msg) => return (format!("chown: {}", msg), 1),
        };

        let targets: Vec<String> = positional[1..].iter().map(|s| self.resolve(s)).collect();
        let orig_args: Vec<String> = positional[1..].to_vec();

        for (path, orig) in targets.iter().zip(orig_args.iter()) {
            if recursive && self.fs.is_dir(path) {
                let all_paths: Vec<String> = self.fs.walk_prefix(path).map(|(p, _)| p).collect();
                for p in all_paths {
                    match self.fs.chown(&p, uid, gid) {
                        Err(e) if *e.kind() == bare_vfs::VfsErrorKind::NotFound => {
                            return (
                                format!(
                                    "chown: cannot access '{}': No such file or directory",
                                    orig
                                ),
                                1,
                            );
                        }
                        Err(_) => {
                            return (
                                format!(
                                    "chown: changing ownership of '{}': Operation not permitted",
                                    orig
                                ),
                                1,
                            );
                        }
                        Ok(()) => {}
                    }
                }
            } else {
                match self.fs.chown(path, uid, gid) {
                    Err(e) if *e.kind() == bare_vfs::VfsErrorKind::NotFound => {
                        return (
                            format!("chown: cannot access '{}': No such file or directory", orig),
                            1,
                        );
                    }
                    Err(_) => {
                        return (
                            format!(
                                "chown: changing ownership of '{}': Operation not permitted",
                                orig
                            ),
                            1,
                        );
                    }
                    Ok(()) => {}
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
        let gid = match self.users.resolve_gid(group_str) {
            Some(gid) => gid,
            None => return (format!("chgrp: invalid group: '{}'", group_str), 1),
        };

        let targets: Vec<String> = positional[1..].iter().map(|s| self.resolve(s)).collect();
        let orig_args: Vec<String> = positional[1..].to_vec();

        for (path, orig) in targets.iter().zip(orig_args.iter()) {
            if recursive && self.fs.is_dir(path) {
                let all_paths: Vec<String> = self.fs.walk_prefix(path).map(|(p, _)| p).collect();
                for p in all_paths {
                    let uid = self.fs.get(&p).map(|e| e.uid()).unwrap_or(0);
                    match self.fs.chown(&p, uid, gid) {
                        Err(e) if *e.kind() == bare_vfs::VfsErrorKind::NotFound => {
                            return (
                                format!(
                                    "chgrp: cannot access '{}': No such file or directory",
                                    orig
                                ),
                                1,
                            );
                        }
                        Err(_) => {
                            return (
                                format!(
                                    "chgrp: changing group of '{}': Operation not permitted",
                                    orig
                                ),
                                1,
                            );
                        }
                        Ok(()) => {}
                    }
                }
            } else {
                let uid = self.fs.get(path).map(|e| e.uid()).unwrap_or(0);
                match self.fs.chown(path, uid, gid) {
                    Err(e) if *e.kind() == bare_vfs::VfsErrorKind::NotFound => {
                        return (
                            format!("chgrp: cannot access '{}': No such file or directory", orig),
                            1,
                        );
                    }
                    Err(_) => {
                        return (
                            format!(
                                "chgrp: changing group of '{}': Operation not permitted",
                                orig
                            ),
                            1,
                        );
                    }
                    Ok(()) => {}
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
    fn parse_owner_spec_userdb(&self, spec: &str) -> Result<(u32, u32), String> {
        if let Some((user_part, group_part)) = spec.split_once(':') {
            let uid = if user_part.is_empty() {
                self.fs.current_uid()
            } else {
                self.users
                    .resolve_uid(user_part)
                    .ok_or_else(|| format!("invalid user: '{}'", user_part))?
            };
            let gid = if group_part.is_empty() {
                self.fs.current_gid()
            } else {
                self.users
                    .resolve_gid(group_part)
                    .ok_or_else(|| format!("invalid group: '{}'", group_part))?
            };
            Ok((uid, gid))
        } else {
            let uid = self
                .users
                .resolve_uid(spec)
                .ok_or_else(|| format!("invalid user: '{}'", spec))?;
            Ok((uid, self.fs.current_gid()))
        }
    }
}

/// Apply a symbolic chmod mode string (e.g., "+x", "u+rw", "a-w", "go+rx") to a current mode.
/// Returns None if the mode string cannot be parsed.
fn apply_symbolic_mode(spec: &str, current: u16) -> Option<u16> {
    let mut mode = current;
    // Support comma-separated clauses like "u+x,g+r"
    for clause in spec.split(',') {
        let clause = clause.trim();
        if clause.is_empty() {
            continue;
        }
        // Parse who: u, g, o, a (or empty = a)
        let mut chars = clause.chars().peekable();
        let mut who_u = false;
        let mut who_g = false;
        let mut who_o = false;
        loop {
            match chars.peek() {
                Some('u') => {
                    who_u = true;
                    chars.next();
                }
                Some('g') => {
                    who_g = true;
                    chars.next();
                }
                Some('o') => {
                    who_o = true;
                    chars.next();
                }
                Some('a') => {
                    who_u = true;
                    who_g = true;
                    who_o = true;
                    chars.next();
                }
                _ => break,
            }
        }
        // If no who specified, default to all
        if !who_u && !who_g && !who_o {
            who_u = true;
            who_g = true;
            who_o = true;
        }
        // Parse operator: +, -, =
        let op = chars.next()?;
        if op != '+' && op != '-' && op != '=' {
            return None;
        }
        // Parse permissions: r, w, x
        let mut perm_r = false;
        let mut perm_w = false;
        let mut perm_x = false;
        for c in chars {
            match c {
                'r' => perm_r = true,
                'w' => perm_w = true,
                'x' => perm_x = true,
                _ => return None,
            }
        }
        // Build masks
        let mut set_mask: u16 = 0;
        if who_u {
            if perm_r {
                set_mask |= 0o400;
            }
            if perm_w {
                set_mask |= 0o200;
            }
            if perm_x {
                set_mask |= 0o100;
            }
        }
        if who_g {
            if perm_r {
                set_mask |= 0o040;
            }
            if perm_w {
                set_mask |= 0o020;
            }
            if perm_x {
                set_mask |= 0o010;
            }
        }
        if who_o {
            if perm_r {
                set_mask |= 0o004;
            }
            if perm_w {
                set_mask |= 0o002;
            }
            if perm_x {
                set_mask |= 0o001;
            }
        }
        match op {
            '+' => mode |= set_mask,
            '-' => {
                mode &= !set_mask;
            }
            '=' => {
                // Clear all bits for the selected who, then set
                let mut who_mask: u16 = 0;
                if who_u {
                    who_mask |= 0o700;
                }
                if who_g {
                    who_mask |= 0o070;
                }
                if who_o {
                    who_mask |= 0o007;
                }
                mode = (mode & !who_mask) | set_mask;
            }
            _ => return None,
        }
    }
    Some(mode)
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
