use globset::Glob;

use crate::ansi;
use crate::commands::CmdResult;
use crate::shell::Shell;
use crate::types::FsEntry;

impl Shell {
    /// ls [-a] [-l] [-R] [-1] [-h] [-t]: list directory contents.
    pub fn cmd_ls(&mut self, args: &[String]) -> (String, i32) {
        let mut show_all = false;
        let mut long = false;
        let mut recursive = false;
        let mut one_per_line = false;
        let mut human_readable = false;
        let mut sort_by_time = false;
        let mut paths = Vec::new();

        let mut parser = lexopt::Parser::from_args(args.iter().cloned());
        loop {
            match parser.next() {
                Ok(Some(lexopt::Arg::Short('a'))) => show_all = true,
                Ok(Some(lexopt::Arg::Short('l'))) => long = true,
                Ok(Some(lexopt::Arg::Short('R'))) => recursive = true,
                Ok(Some(lexopt::Arg::Short('1'))) => one_per_line = true,
                Ok(Some(lexopt::Arg::Short('h'))) => human_readable = true,
                Ok(Some(lexopt::Arg::Short('t'))) => sort_by_time = true,
                Ok(Some(lexopt::Arg::Value(val))) => paths.push(val.to_string_lossy().to_string()),
                Ok(Some(_)) => {}
                Ok(None) | Err(_) => break,
            }
        }
        if paths.is_empty() {
            paths.push(".".to_string());
        }

        let multi = paths.len() > 1 || recursive;
        let mut sections = Vec::new();
        let mut file_entries: Vec<String> = Vec::new(); // file args collected here

        // Collect directories to process (for recursive mode)
        let mut dir_queue: Vec<(String, String)> = Vec::new(); // (display_path, resolved_path)

        for path_arg in &paths {
            let target = self.resolve(path_arg);

            match self.fs.get(&target) {
                Some(e) if e.is_file() => {
                    let name = target.rsplit('/').next().unwrap_or(&target).to_string();
                    if long {
                        if let Ok(meta) = self.fs.metadata(&target) {
                            let ms = FsEntry::format_mode(meta.mode());
                            let ty = if self.fs.is_symlink(&target) {
                                "l"
                            } else {
                                "-"
                            };
                            let owner = self.ident.users.uid_to_name(meta.uid());
                            let group = self.ident.users.gid_to_name(meta.gid());
                            let date = format_mtime(meta.mtime());
                            let size_str = if human_readable {
                                format_human_size(meta.len())
                            } else {
                                format!("{}", meta.len())
                            };
                            file_entries.push(format!(
                                "{}{} {:>2} {} {} {:>5} {} {}",
                                ty,
                                ms,
                                meta.nlink(),
                                owner,
                                group,
                                size_str,
                                date,
                                name
                            ));
                        } else {
                            file_entries.push(name);
                        }
                    } else {
                        sections.push(name);
                    }
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

            dir_queue.push((path_arg.clone(), target));
        }

        // File entries go as one section (no blank lines between them)
        if !file_entries.is_empty() {
            let sep = if long || one_per_line { "\n" } else { "  " };
            sections.push(file_entries.join(sep));
        }

        while let Some((path_arg, target)) = dir_queue.pop() {
            let children = self.list_dir(&target);

            // Build list of (name, is_dir, mode, mtime) including . and .. when show_all
            let mut entries: Vec<(String, bool, u16, u64)> = Vec::new();
            if show_all {
                // Add . and .. entries
                let dot_mtime = self.fs.metadata(&target).map(|m| m.mtime()).unwrap_or(0);
                let dot_mode = self.fs.metadata(&target).map(|m| m.mode()).unwrap_or(0o755);
                entries.push((".".to_string(), true, dot_mode, dot_mtime));
                let parent = if target == "/" {
                    "/".to_string()
                } else {
                    target
                        .rsplit_once('/')
                        .map(|(p, _)| if p.is_empty() { "/" } else { p })
                        .unwrap_or("/")
                        .to_string()
                };
                let dotdot_mtime = self.fs.metadata(&parent).map(|m| m.mtime()).unwrap_or(0);
                let dotdot_mode = self.fs.metadata(&parent).map(|m| m.mode()).unwrap_or(0o755);
                entries.push(("..".to_string(), true, dotdot_mode, dotdot_mtime));
            }
            for (name, is_dir, mode) in &children {
                if !show_all && name.starts_with('.') {
                    continue;
                }
                let entry_path = format!("{}/{}", target, name);
                let mtime = self
                    .fs
                    .metadata(&entry_path)
                    .map(|m| m.mtime())
                    .unwrap_or(0);
                entries.push((name.clone(), *is_dir, *mode, mtime));
            }

            // Sort by time if -t, otherwise keep current order (. and .. first, then alphabetical)
            if sort_by_time {
                // Stable sort: entries with same mtime keep their relative order
                entries.sort_by(|a, b| b.3.cmp(&a.3));
            }

            let formatted: Vec<String> = entries
                .iter()
                .map(|(name, is_dir, mode, _mtime)| {
                    if long {
                        let ms = FsEntry::format_mode(*mode);
                        let entry_path = if name == "." {
                            target.clone()
                        } else if name == ".." {
                            if target == "/" {
                                "/".to_string()
                            } else {
                                target
                                    .rsplit_once('/')
                                    .map(|(p, _)| if p.is_empty() { "/" } else { p })
                                    .unwrap_or("/")
                                    .to_string()
                            }
                        } else {
                            format!("{}/{}", target, name)
                        };
                        let is_symlink =
                            name != "." && name != ".." && self.fs.is_symlink(&entry_path);
                        let ty = if is_symlink {
                            "l"
                        } else if *is_dir {
                            "d"
                        } else {
                            "-"
                        };
                        let display = if *is_dir && self.color {
                            format!("{}{}/{}", ansi::BOLD_BLUE, name, ansi::RESET)
                        } else if *is_dir {
                            format!("{}/", name)
                        } else {
                            name.clone()
                        };
                        let (owner, group, nlink, size, date_str) =
                            if let Ok(meta) = self.fs.metadata(&entry_path) {
                                let owner = self.ident.users.uid_to_name(meta.uid());
                                let group_name = self.ident.users.gid_to_name(meta.gid());
                                let date = format_mtime(meta.mtime());
                                (owner, group_name, meta.nlink(), meta.len(), date)
                            } else {
                                (
                                    self.ident.user.to_string(),
                                    self.ident.user.to_string(),
                                    1,
                                    0,
                                    "Jan  1 00:00".to_string(),
                                )
                            };
                        let size_str = if human_readable {
                            format_human_size(size)
                        } else {
                            format!("{}", size)
                        };
                        format!(
                            "{}{} {:>2} {} {} {:>5} {} {}",
                            ty, ms, nlink, owner, group, size_str, date_str, display
                        )
                    } else if *is_dir && self.color {
                        format!("{}{}/{}", ansi::BOLD_BLUE, name, ansi::RESET)
                    } else if *is_dir {
                        format!("{}/", name)
                    } else {
                        name.clone()
                    }
                })
                .collect();

            let sep = if long || one_per_line { "\n" } else { "  " };
            let mut section = String::new();
            if multi {
                section.push_str(&path_arg);
                section.push(':');
                section.push('\n');
            }
            section.push_str(&formatted.join(sep));
            sections.push(section);

            // If recursive, queue subdirectories (in sorted order)
            if recursive {
                let mut subdirs: Vec<(String, String)> = children
                    .iter()
                    .filter(|(name, is_dir, _)| *is_dir && (show_all || !name.starts_with('.')))
                    .map(|(name, _, _)| {
                        let display = format!("{}/{}", path_arg, name);
                        let resolved = format!("{}/{}", target, name);
                        (display, resolved)
                    })
                    .collect();
                subdirs.sort_by(|a, b| b.0.cmp(&a.0)); // reverse so pop gives sorted order
                dir_queue.extend(subdirs);
            }
        }

        (sections.join("\n\n"), 0)
    }

    /// cat [-n] [-]: concatenate and print files; `-` reads stdin.
    pub fn cmd_cat(&mut self, args: &[String], stdin: Option<&str>) -> CmdResult {
        let mut line_numbers = false;
        let mut file_args = Vec::new();

        for arg in args {
            match arg.as_str() {
                "-n" => line_numbers = true,
                "-" => file_args.push(arg),
                s if s.starts_with('-') => {}
                _ => file_args.push(arg),
            }
        }

        let content = if file_args.is_empty() {
            stdin.unwrap_or("").to_string()
        } else {
            let mut parts = Vec::new();
            for arg in &file_args {
                if arg.as_str() == "-" {
                    parts.push(stdin.unwrap_or("").to_string());
                } else {
                    let path = self.resolve(arg);
                    match self.fs.read_to_string(&path) {
                        Ok(s) => parts.push(s.to_string()),
                        Err(e) => return (format!("cat: {}: {}", arg, e), 1, None),
                    }
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

    /// mkdir [-p]: create directories; `-p` creates parents as needed.
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

    /// touch: update file timestamps or create empty files.
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

    /// rm [-r] [-R] [-f]: remove files or directories; `-r`/`-R` recursive, `-f` force.
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

    /// cp [-r] [-R] [-n] [-p]: copy files or directories; `-r`/`-R` recursive, `-n` no-clobber, `-p` preserve metadata.
    pub fn cmd_cp(&mut self, args: &[String]) -> (String, i32) {
        let mut recursive = false;
        let mut no_clobber = false;
        let mut preserve = false;
        let mut files: Vec<&String> = Vec::new();

        for arg in args {
            match arg.as_str() {
                "-r" | "-R" => recursive = true,
                "-n" => no_clobber = true,
                "-p" => preserve = true,
                s if s.starts_with('-') => {
                    // Handle combined flags like -rn, -rp, etc.
                    for c in s[1..].chars() {
                        match c {
                            'r' | 'R' => recursive = true,
                            'n' => no_clobber = true,
                            'p' => preserve = true,
                            _ => {}
                        }
                    }
                }
                _ => files.push(arg),
            }
        }

        if files.len() < 2 {
            return ("cp: missing operand".into(), 1);
        }

        // Helper closure: copy a single file from src_path to target, respecting no_clobber and preserve.
        // Returns Ok(()) on success, Err((msg, code)) on failure.
        let cp_single_file = |fs: &mut bare_vfs::MemFs,
                              src_path: &str,
                              target: &str,
                              src_display: &str,
                              dst_display: &str,
                              no_clobber: bool,
                              preserve: bool|
         -> Result<(), (String, i32)> {
            if no_clobber && fs.exists(target) {
                return Ok(()); // silently skip
            }
            let content = match fs.read_to_string(src_path) {
                Ok(s) => s.to_string(),
                Err(ref e) if *e.kind() == bare_vfs::VfsErrorKind::IsADirectory => {
                    return Err(("cp: omitting directory".into(), 1));
                }
                Err(ref e) if *e.kind() == bare_vfs::VfsErrorKind::NotFound => {
                    return Err((
                        format!(
                            "cp: cannot stat '{}': No such file or directory",
                            src_display
                        ),
                        1,
                    ));
                }
                Err(_) => return Err((format!("cp: '{}': Permission denied", src_display), 1)),
            };
            let src_mode = if preserve {
                fs.metadata(src_path).ok().map(|m| m.mode())
            } else {
                None
            };
            if let Err(e) = fs.write(target, content.as_bytes()) {
                return Err((format!("cp: cannot create '{}': {}", dst_display, e), 1));
            }
            if let Some(mode) = src_mode {
                let _ = fs.set_mode(target, mode);
            }
            Ok(())
        };

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
                if self.fs.is_dir(&src_path) {
                    if !recursive {
                        return ("cp: omitting directory".into(), 1);
                    }
                    let name = src_path.rsplit('/').next().unwrap_or(src.as_str());
                    let target = format!("{}/{}", dst_path, name);
                    if let Err(e) = self.fs.copy_recursive(&src_path, &target) {
                        return (format!("cp: cannot copy '{}': {}", src, e), 1);
                    }
                    continue;
                }
                let name = src_path.rsplit('/').next().unwrap_or(src);
                let target = format!("{}/{}", dst_path, name);
                if let Err(e) = cp_single_file(
                    &mut self.fs,
                    &src_path,
                    &target,
                    src,
                    src,
                    no_clobber,
                    preserve,
                ) {
                    return e;
                }
            }
            return (String::new(), 0);
        }

        let src_path = self.resolve(files[0]);
        let dst_path = self.resolve(files[1]);

        if self.fs.is_dir(&src_path) {
            if !recursive {
                return ("cp: omitting directory".into(), 1);
            }
            let target = match self.fs.get(&dst_path) {
                Some(e) if e.is_dir() => {
                    let name = src_path.rsplit('/').next().unwrap_or(files[0].as_str());
                    format!("{}/{}", dst_path, name)
                }
                _ => dst_path,
            };
            return match self.fs.copy_recursive(&src_path, &target) {
                Ok(()) => (String::new(), 0),
                Err(e) => (format!("cp: cannot copy '{}': {}", files[0], e), 1),
            };
        }

        let target = match self.fs.get(&dst_path) {
            Some(e) if e.is_dir() => {
                let name = src_path.rsplit('/').next().unwrap_or(files[0]);
                format!("{}/{}", dst_path, name)
            }
            _ => dst_path,
        };
        if let Err(e) = cp_single_file(
            &mut self.fs,
            &src_path,
            &target,
            files[0],
            files[1],
            no_clobber,
            preserve,
        ) {
            return e;
        }
        (String::new(), 0)
    }

    /// mv: move or rename files and directories.
    pub fn cmd_mv(&mut self, args: &[String]) -> (String, i32) {
        let files: Vec<&String> = args.iter().filter(|a| !a.starts_with('-')).collect();
        if files.len() < 2 {
            return ("mv: missing operand".into(), 1);
        }

        // With 3+ args the last element is the destination directory
        if files.len() > 2 {
            let dest_dir = self.resolve(files[files.len() - 1]);
            if !self.fs.is_dir(&dest_dir) {
                return (
                    format!("mv: target '{}' is not a directory", files[files.len() - 1]),
                    1,
                );
            }
            for src in &files[..files.len() - 1] {
                let src_path = self.resolve(src);
                let name = src_path.rsplit('/').next().unwrap_or(src);
                let dst_path = format!("{}/{}", dest_dir, name);
                if let Err(e) = self.fs.rename(&src_path, &dst_path) {
                    return (format!("mv: {}", e), 1);
                }
            }
            return (String::new(), 0);
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

    /// find [-name] [-iname] [-path] [-type f|d] [-maxdepth N] [-delete] [-exec ... ;]: search for files in a directory hierarchy.
    pub fn cmd_find(&mut self, args: &[String]) -> (String, i32) {
        let mut search_path = ".".to_string();
        let mut name_pattern = None;
        let mut iname_pattern = None;
        let mut path_pattern = None;
        let mut type_filter = None;
        let mut max_depth: Option<usize> = None;
        let mut exec_template: Option<Vec<String>> = None;
        let mut delete = false;

        let mut i = 0;
        while i < args.len() {
            match args[i].as_str() {
                "-name" if i + 1 < args.len() => {
                    name_pattern = Some(args[i + 1].clone());
                    i += 2;
                }
                "-iname" if i + 1 < args.len() => {
                    iname_pattern = Some(args[i + 1].clone());
                    i += 2;
                }
                "-path" if i + 1 < args.len() => {
                    path_pattern = Some(args[i + 1].clone());
                    i += 2;
                }
                "-type" if i + 1 < args.len() => {
                    type_filter = Some(args[i + 1].clone());
                    i += 2;
                }
                "-maxdepth" if i + 1 < args.len() => {
                    max_depth = args[i + 1].parse().ok();
                    i += 2;
                }
                "-delete" => {
                    delete = true;
                    i += 1;
                }
                "-exec" => {
                    // Collect everything between -exec and \; (or +)
                    let mut tmpl = Vec::new();
                    i += 1;
                    while i < args.len() {
                        if args[i] == ";" || args[i] == "\\;" || args[i] == "+" {
                            i += 1;
                            break;
                        }
                        tmpl.push(args[i].clone());
                        i += 1;
                    }
                    exec_template = Some(tmpl);
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

        // Build case-insensitive glob for -iname
        let iglob = iname_pattern.as_ref().and_then(|p| {
            let lower = p.to_lowercase();
            Glob::new(&lower).ok().map(|g| g.compile_matcher())
        });

        // Build path glob for -path
        let path_glob = path_pattern
            .as_ref()
            .and_then(|p| Glob::new(p).ok().map(|g| g.compile_matcher()));

        // Collect all entries with their depth
        let root_depth = root.matches('/').count();
        let mut results = Vec::new();
        for (p, entry) in self.fs.walk_prefix(&root) {
            // Calculate depth relative to root
            let entry_depth = if p == root {
                0
            } else {
                p.matches('/').count() - root_depth
            };

            // Check maxdepth
            if let Some(md) = max_depth {
                if entry_depth > md {
                    continue;
                }
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

            if let Some(ref g) = iglob {
                let name = p.rsplit('/').next().unwrap_or(&p);
                if !g.is_match(name.to_lowercase()) {
                    continue;
                }
            }

            let display = if p == root {
                search_path.clone()
            } else {
                let rel = &p[root.len()..];
                let rel = rel.strip_prefix('/').unwrap_or(rel);
                if search_path == "." {
                    format!("./{}", rel)
                } else {
                    format!("{}/{}", search_path, rel)
                }
            };

            if let Some(ref g) = path_glob {
                if !g.is_match(&display) {
                    continue;
                }
            }

            results.push((display, p));
        }

        results.sort_by(|a, b| a.0.cmp(&b.0));

        // Handle -delete: remove matching files (in reverse order to handle children before parents)
        if delete {
            for (_, abs_path) in results.iter().rev() {
                if self.fs.is_dir(abs_path) {
                    let _ = self.fs.remove_dir_all(abs_path);
                } else {
                    self.fs.remove(abs_path);
                }
            }
            return (String::new(), 0);
        }

        let display_results: Vec<&str> = results.iter().map(|(d, _)| d.as_str()).collect();

        // Handle -exec: run the command template for each match
        if let Some(ref tmpl) = exec_template {
            let mut exec_output = Vec::new();
            for file_path in &display_results {
                let cmd_line: Vec<String> = tmpl
                    .iter()
                    .map(|part| part.replace("{}", file_path))
                    .collect();
                let formatted = cmd_line.join(" ");
                let (out, _code, _lang) = self.run_line(&formatted);
                if !out.is_empty() {
                    exec_output.push(out);
                }
            }
            return (exec_output.join(""), 0);
        }

        (display_results.join("\n"), 0)
    }

    /// tee [-a]: read stdin and write to stdout and files; `-a` appends instead of overwriting.
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

    /// chmod [-R]: change file mode bits; accepts octal or symbolic mode, `-R` recursive.
    pub fn cmd_chmod(&mut self, args: &[String]) -> (String, i32) {
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
            return ("chmod: missing operand".into(), 1);
        }

        let mode_str = &positional[0];

        // Try octal first, then symbolic
        let is_symbolic = mode_str
            .chars()
            .next()
            .map(|c| !c.is_ascii_digit())
            .unwrap_or(false);

        for arg in &positional[1..] {
            let path = self.resolve(arg);

            let apply_mode = |fs: &mut bare_vfs::MemFs,
                              p: &str,
                              mode_str: &str,
                              is_symbolic: bool,
                              arg: &str|
             -> Result<(), (String, i32)> {
                let mode = if is_symbolic {
                    let current = match fs.metadata(p) {
                        Ok(m) => m.mode(),
                        Err(e) if *e.kind() == bare_vfs::VfsErrorKind::NotFound => {
                            return Err((
                                format!(
                                    "chmod: cannot access '{}': No such file or directory",
                                    arg
                                ),
                                1,
                            ));
                        }
                        Err(_) => {
                            return Err((
                                format!(
                                    "chmod: changing permissions of '{}': Operation not permitted",
                                    arg
                                ),
                                1,
                            ));
                        }
                    };
                    match apply_symbolic_mode(mode_str, current) {
                        Some(m) => m,
                        None => return Err((format!("chmod: invalid mode: '{}'", mode_str), 1)),
                    }
                } else {
                    match u16::from_str_radix(mode_str, 8) {
                        Ok(m) => m,
                        Err(_) => return Err((format!("chmod: invalid mode: '{}'", mode_str), 1)),
                    }
                };

                match fs.set_mode(p, mode) {
                    Err(e) if *e.kind() == bare_vfs::VfsErrorKind::NotFound => Err((
                        format!("chmod: cannot access '{}': No such file or directory", arg),
                        1,
                    )),
                    Err(_) => Err((
                        format!(
                            "chmod: changing permissions of '{}': Operation not permitted",
                            arg
                        ),
                        1,
                    )),
                    Ok(()) => Ok(()),
                }
            };

            if recursive && self.fs.is_dir(&path) {
                let all_paths: Vec<String> = self.fs.walk_prefix(&path).map(|(p, _)| p).collect();
                for p in all_paths {
                    if let Err(e) = apply_mode(&mut self.fs, &p, mode_str, is_symbolic, arg) {
                        return e;
                    }
                }
            } else {
                if let Err(e) = apply_mode(&mut self.fs, &path, mode_str, is_symbolic, arg) {
                    return e;
                }
            }
        }
        (String::new(), 0)
    }

    /// rmdir: remove empty directories.
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

    /// mktemp [-d]: create a temporary file (or directory with `-d`) under /tmp and print its path.
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

    /// ln [-s] [-f]: create hard or symbolic links; `-s` symbolic, `-f` force overwrite.
    pub fn cmd_ln(&mut self, args: &[String]) -> (String, i32) {
        let mut symbolic = false;
        let mut force = false;
        let mut positional = Vec::new();

        for arg in args {
            match arg.as_str() {
                "-s" => symbolic = true,
                "-f" => force = true,
                s if s.starts_with('-') && s.len() > 1 => {
                    // Handle combined flags like -sf
                    for c in s[1..].chars() {
                        match c {
                            's' => symbolic = true,
                            'f' => force = true,
                            _ => {}
                        }
                    }
                }
                _ => positional.push(arg.as_str()),
            }
        }

        if positional.len() < 2 {
            return ("ln: missing operand".into(), 1);
        }

        let target = positional[0];
        let link_path = self.resolve(positional[1]);

        if !symbolic {
            if force && self.fs.exists(&link_path) {
                self.fs.remove(&link_path);
            }
            let target_path = self.resolve(target);
            if let Err(e) = self.fs.hard_link(&target_path, &link_path) {
                return (format!("ln: {}: {}", positional[1], e), 1);
            }
            return (String::new(), 0);
        }
        if self.fs.exists(&link_path) {
            if force {
                self.fs.remove(&link_path);
            } else {
                return (
                    format!(
                        "ln: failed to create symbolic link '{}': File exists",
                        positional[1]
                    ),
                    1,
                );
            }
        }
        if let Err(e) = self.fs.symlink(target, &link_path) {
            return (format!("ln: {}: {}", positional[1], e), 1);
        }
        (String::new(), 0)
    }

    /// readlink [-f]: print the target of a symbolic link; `-f` canonicalizes the path.
    pub fn cmd_readlink(&mut self, args: &[String]) -> (String, i32) {
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

    /// chown [-R]: change file owner and group (accepts user, user:group, or :group); `-R` recursive.
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

    /// chgrp [-R]: change group ownership of files; `-R` recursive.
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
        let gid = match self.ident.users.resolve_gid(group_str) {
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

    /// id: print real UID, GID, and supplementary groups of the current user.
    pub fn cmd_id(&self, _args: &[String]) -> (String, i32) {
        let uid = self.fs.current_uid();
        let gid = self.fs.current_gid();
        let uname = self.ident.users.uid_to_name(uid);
        let gname = self.ident.users.gid_to_name(gid);
        let groups_str = {
            let mut gs = self.ident.users.user_groups(&uname);
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

    /// groups: print the group memberships of the current user.
    pub fn cmd_groups(&self, _args: &[String]) -> (String, i32) {
        let uname = self.ident.users.uid_to_name(self.fs.current_uid());
        let mut gs = self.ident.users.user_groups(&uname);
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
                self.ident
                    .users
                    .resolve_uid(user_part)
                    .ok_or_else(|| format!("invalid user: '{}'", user_part))?
            };
            let gid = if group_part.is_empty() {
                self.fs.current_gid()
            } else {
                self.ident
                    .users
                    .resolve_gid(group_part)
                    .ok_or_else(|| format!("invalid group: '{}'", group_part))?
            };
            Ok((uid, gid))
        } else {
            let uid = self
                .ident
                .users
                .resolve_uid(spec)
                .ok_or_else(|| format!("invalid user: '{}'", spec))?;
            Ok((uid, self.fs.current_gid()))
        }
    }
}

/// Format a byte size as a human-readable string (e.g. "1.5K", "2.3M", "1.0G").
fn format_human_size(bytes: usize) -> String {
    if bytes >= 1024 * 1024 * 1024 {
        format!("{:.1}G", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
    } else if bytes >= 1024 * 1024 {
        format!("{:.1}M", bytes as f64 / (1024.0 * 1024.0))
    } else if bytes >= 1024 {
        format!("{:.1}K", bytes as f64 / 1024.0)
    } else {
        format!("{}", bytes)
    }
}

/// Format an mtime tick counter as "MMM DD HH:MM" for ls -l output.
fn format_mtime(tick: u64) -> String {
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let (_, mo, d, h, mi, _) = crate::shell::ticks_to_datetime(tick);
    let month_str = MONTHS[(mo as usize).saturating_sub(1).min(11)];
    format!("{} {:>2} {:02}:{:02}", month_str, d, h, mi)
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
