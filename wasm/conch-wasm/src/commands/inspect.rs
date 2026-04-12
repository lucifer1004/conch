use crate::ansi;
use crate::shell::{format_timestamp, Shell};
use crate::types::FsEntry;
use regex_lite::Regex;

struct TreeOpts {
    max_depth: Option<usize>,
    show_all: bool,
    dir_count: usize,
    file_count: usize,
}

impl Shell {
    /// stat [-c FORMAT]: display file or file-system status; `-c` uses a custom printf-style format.
    pub fn cmd_stat(&mut self, args: &[String]) -> (String, i32) {
        // Parse -c FORMAT flag
        let mut format_str: Option<String> = None;
        let mut file_args: Vec<&String> = Vec::new();
        let mut i = 0;
        while i < args.len() {
            if args[i] == "-c" && i + 1 < args.len() {
                format_str = Some(args[i + 1].clone());
                i += 2;
            } else if args[i].starts_with("-c") && args[i].len() > 2 {
                // -cFORMAT (no space)
                format_str = Some(args[i][2..].to_string());
                i += 1;
            } else if !args[i].starts_with('-') || args[i] == "-" {
                file_args.push(&args[i]);
                i += 1;
            } else {
                i += 1;
            }
        }

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

            if let Some(ref fmt) = format_str {
                // Custom format: %s size, %a octal perms, %n filename,
                // %F file type, %U owner, %G group, %i inode, %h hard links
                let owner = self
                    .ident
                    .users
                    .get_user_by_uid(meta.uid())
                    .map(|u| u.name.clone())
                    .unwrap_or_else(|| meta.uid().to_string());
                let group = self
                    .ident
                    .users
                    .get_group_by_gid(meta.gid())
                    .map(|g| g.name.clone())
                    .unwrap_or_else(|| meta.gid().to_string());
                let mode_octal = format!("{:o}", mode & 0o7777);

                let mut result = String::new();
                let chars: Vec<char> = fmt.chars().collect();
                let mut ci = 0;
                while ci < chars.len() {
                    if chars[ci] == '%' && ci + 1 < chars.len() {
                        match chars[ci + 1] {
                            's' => result.push_str(&size.to_string()),
                            'a' => result.push_str(&mode_octal),
                            'n' => result.push_str(name),
                            'F' => result.push_str(type_str),
                            'U' => result.push_str(&owner),
                            'G' => result.push_str(&group),
                            'i' => result.push_str(&meta.ino().to_string()),
                            'h' => result.push_str(&meta.nlink().to_string()),
                            // Timestamp specifiers
                            'Y' => result.push_str(
                                &(meta.mtime() / crate::shell::pipeline::TICKS_PER_SECOND)
                                    .to_string(),
                            ),
                            'y' => result.push_str(&format_timestamp(meta.mtime())),
                            'X' => result.push_str(
                                &(meta.atime() / crate::shell::pipeline::TICKS_PER_SECOND)
                                    .to_string(),
                            ),
                            'x' => result.push_str(&format_timestamp(meta.atime())),
                            'Z' => result.push_str(
                                &(meta.ctime() / crate::shell::pipeline::TICKS_PER_SECOND)
                                    .to_string(),
                            ),
                            'z' => result.push_str(&format_timestamp(meta.ctime())),
                            'W' => result.push_str(
                                &(meta.ctime() / crate::shell::pipeline::TICKS_PER_SECOND)
                                    .to_string(),
                            ),
                            '%' => result.push('%'),
                            other => {
                                result.push('%');
                                result.push(other);
                            }
                        }
                        ci += 2;
                    } else {
                        result.push(chars[ci]);
                        ci += 1;
                    }
                }
                out.push(result);
            } else {
                let mode_str = FsEntry::format_mode(mode);
                let mode_octal = format!("{:04o}", mode);

                out.push(format!(
                    "  File: {}\n  Size: {:<12}Type: {}\n  Inode: {:<8}Links: {}\n  Mode: ({}/{}{})\n  Uid: {:<8} Gid: {}\nAccess: {}\nModify: {}\nChange: {}",
                    name,
                    size,
                    type_str,
                    meta.ino(),
                    meta.nlink(),
                    mode_octal,
                    if meta.is_dir() { "d" } else { "-" },
                    mode_str,
                    meta.uid(),
                    meta.gid(),
                    format_timestamp(meta.atime()),
                    format_timestamp(meta.mtime()),
                    format_timestamp(meta.ctime()),
                ));
            }
        }

        (out.join("\n"), 0)
    }

    /// test [-e] [-f] [-d] [-r] [-w] [-x] [-s] [-L] [-h] [-z] [-n] [= != -eq -ne -lt -le -gt -ge -nt -ot]: evaluate a conditional expression.
    pub fn cmd_test(&mut self, args: &[String]) -> (String, i32) {
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

    fn evaluate_test(&mut self, args: &[&String]) -> bool {
        // Handle compound -a (AND) and -o (OR) operators by splitting
        // at the lowest-precedence operator. -o binds looser than -a.
        // Scan for -o first (lowest precedence).
        for i in 0..args.len() {
            if args[i].as_str() == "-o" {
                let lhs = self.evaluate_test(&args[..i]);
                let rhs = self.evaluate_test(&args[i + 1..]);
                return lhs || rhs;
            }
        }
        // Then scan for -a.
        for i in 0..args.len() {
            if args[i].as_str() == "-a" {
                let lhs = self.evaluate_test(&args[..i]);
                let rhs = self.evaluate_test(&args[i + 1..]);
                return lhs && rhs;
            }
        }

        // Handle `!` negation operator
        if let Some(first) = args.first() {
            if first.as_str() == "!" {
                return !self.evaluate_test(&args[1..]);
            }
        }
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
            [flag, path] if flag.as_str() == "-L" || flag.as_str() == "-h" => {
                let p = self.resolve(path.as_str());
                self.fs.is_symlink(&p)
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
            // -nt (newer than) / -ot (older than) file comparison
            [a, op, b] if op.as_str() == "-nt" => {
                let pa = self.resolve(a.as_str());
                let pb = self.resolve(b.as_str());
                let ma = self.fs.metadata(&pa).ok().map(|m| m.mtime());
                let mb = self.fs.metadata(&pb).ok().map(|m| m.mtime());
                matches!((ma, mb), (Some(a), Some(b)) if a > b)
            }
            [a, op, b] if op.as_str() == "-ot" => {
                let pa = self.resolve(a.as_str());
                let pb = self.resolve(b.as_str());
                let ma = self.fs.metadata(&pa).ok().map(|m| m.mtime());
                let mb = self.fs.metadata(&pb).ok().map(|m| m.mtime());
                matches!((ma, mb), (Some(a), Some(b)) if a < b)
            }
            _ => false,
        }
    }

    /// du [-s] [-h] [-c] [-d N] [--max-depth=N]: estimate file-space usage; `-s` summary, `-h` human-readable, `-c` grand total, `-d` max depth.
    pub fn cmd_du(&mut self, args: &[String]) -> (String, i32) {
        let mut summary = false;
        let mut human = false;
        let mut grand_total = false;
        let mut max_depth: Option<usize> = None;
        let mut paths = Vec::new();

        let mut i = 0;
        while i < args.len() {
            match args[i].as_str() {
                "-s" => {
                    summary = true;
                    i += 1;
                }
                "-h" => {
                    human = true;
                    i += 1;
                }
                "-c" => {
                    grand_total = true;
                    i += 1;
                }
                "-d" if i + 1 < args.len() => {
                    max_depth = args[i + 1].parse().ok();
                    i += 2;
                }
                s if s.starts_with("--max-depth=") => {
                    max_depth = s.strip_prefix("--max-depth=").and_then(|v| v.parse().ok());
                    i += 1;
                }
                "--max-depth" if i + 1 < args.len() => {
                    max_depth = args[i + 1].parse().ok();
                    i += 2;
                }
                s if s.starts_with('-') && s.len() > 1 => {
                    // Handle combined flags like -sh, -shc
                    for c in s[1..].chars() {
                        match c {
                            's' => summary = true,
                            'h' => human = true,
                            'c' => grand_total = true,
                            _ => {}
                        }
                    }
                    i += 1;
                }
                _ => {
                    paths.push(args[i].clone());
                    i += 1;
                }
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
        let mut all_total: usize = 0;

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
                    all_total += size;
                    out.push(format!("{}\t{}", format_size(size), target_arg));
                    continue;
                }
                _ => {}
            }

            // It's a directory -- collect all entries under it
            let root_depth = root.matches('/').count();
            let mut total: usize = 0;
            let mut entries_out: Vec<(String, usize)> = Vec::new();

            for (path, entry) in self.fs.walk_prefix(&root) {
                let size = entry.len();
                total += size;
                if !summary {
                    // Calculate depth relative to root
                    let entry_depth = if path == root {
                        0
                    } else {
                        path.matches('/').count() - root_depth
                    };

                    // Apply max_depth filter
                    if let Some(md) = max_depth {
                        if entry_depth > md {
                            continue;
                        }
                    }

                    // Only show directories in du output (like real du)
                    if entry.is_dir() || max_depth.is_none() {
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
            }

            all_total += total;

            if summary {
                out.push(format!("{}\t{}", format_size(total), target_arg));
            } else {
                entries_out.sort_by(|a, b| a.0.cmp(&b.0));
                for (display, size) in entries_out {
                    out.push(format!("{}\t{}", format_size(size), display));
                }
                // Only add the root total line if it wasn't already included
                let root_display = target_arg.clone();
                if !out
                    .iter()
                    .any(|l| l.ends_with(&format!("\t{}", root_display)))
                {
                    out.push(format!("{}\t{}", format_size(total), target_arg));
                }
            }
        }

        if grand_total {
            out.push(format!("{}\ttotal", format_size(all_total)));
        }

        (out.join("\n"), 0)
    }

    /// tree [-L N] [-a]: display directory tree; `-L` limits depth, `-a` shows hidden files.
    pub fn cmd_tree(&mut self, args: &[String]) -> (String, i32) {
        let mut max_depth: Option<usize> = None;
        let mut show_all = false;
        let mut path_arg: Option<String> = None;

        let mut i = 0;
        while i < args.len() {
            match args[i].as_str() {
                "-L" if i + 1 < args.len() => {
                    max_depth = args[i + 1].parse().ok();
                    i += 2;
                }
                "-a" => {
                    show_all = true;
                    i += 1;
                }
                s if !s.starts_with('-') && path_arg.is_none() => {
                    path_arg = Some(s.to_string());
                    i += 1;
                }
                _ => {
                    i += 1;
                }
            }
        }

        let display_arg = path_arg.as_deref().unwrap_or(".");
        let root_path: String = if let Some(ref p) = path_arg {
            self.resolve(p)
        } else {
            self.cwd.to_string()
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
        let mut opts = TreeOpts {
            max_depth,
            show_all,
            dir_count: 0,
            file_count: 0,
        };
        self.tree_recurse(&root_path, "", &mut lines, 1, &mut opts);

        // Append summary line
        let dir_word = if opts.dir_count == 1 {
            "directory"
        } else {
            "directories"
        };
        let file_word = if opts.file_count == 1 {
            "file"
        } else {
            "files"
        };
        lines.push(String::new()); // blank line before summary
        lines.push(format!(
            "{} {}, {} {}",
            opts.dir_count, dir_word, opts.file_count, file_word
        ));

        (lines.join("\n"), 0)
    }

    fn tree_recurse(
        &self,
        dir: &str,
        prefix: &str,
        lines: &mut Vec<String>,
        depth: usize,
        opts: &mut TreeOpts,
    ) {
        let children = self.list_dir(dir);
        let filtered: Vec<&(String, bool, u16)> = children
            .iter()
            .filter(|(name, _, _)| opts.show_all || !name.starts_with('.'))
            .collect();
        for (i, (name, is_dir, _mode)) in filtered.iter().enumerate() {
            let is_last = i == filtered.len() - 1;
            let connector = if is_last {
                "\u{2514}\u{2500}\u{2500} "
            } else {
                "\u{251c}\u{2500}\u{2500} "
            };
            let display = if *is_dir && self.color {
                format!("{}{}/{}", ansi::BOLD_BLUE, name, ansi::RESET)
            } else if *is_dir {
                format!("{}/", name)
            } else {
                name.clone()
            };
            lines.push(format!("{}{}{}", prefix, connector, display));

            if *is_dir {
                opts.dir_count += 1;
                // Check max_depth before recursing
                if let Some(md) = opts.max_depth {
                    if depth >= md {
                        continue;
                    }
                }
                let child_prefix = if is_last {
                    format!("{}    ", prefix)
                } else {
                    format!("{}\u{2502}   ", prefix)
                };
                self.tree_recurse(
                    &format!("{}/{}", dir, name),
                    &child_prefix,
                    lines,
                    depth + 1,
                    opts,
                );
            } else {
                opts.file_count += 1;
            }
        }
    }

    // -----------------------------------------------------------------------
    // [[ ]] extended test
    // -----------------------------------------------------------------------

    /// [[ ]]: extended conditional expression with `&&`, `||`, `!`, `=~` regex, glob `==`, and all `test` operators.
    pub fn cmd_double_bracket(&mut self, args: &[String]) -> (String, i32) {
        // Strip trailing ]]
        let args: Vec<&str> = if args.last().map(|s| s.as_str()) == Some("]]") {
            args[..args.len() - 1].iter().map(|s| s.as_str()).collect()
        } else {
            args.iter().map(|s| s.as_str()).collect()
        };

        let mut pos = 0;
        let result = self.dbl_or_expr(&args, &mut pos);
        (String::new(), if result { 0 } else { 1 })
    }

    /// or_expr → and_expr ("||" and_expr)*
    fn dbl_or_expr(&mut self, args: &[&str], pos: &mut usize) -> bool {
        let mut result = self.dbl_and_expr(args, pos);
        while *pos < args.len() && args[*pos] == "||" {
            *pos += 1;
            let rhs = self.dbl_and_expr(args, pos);
            result = result || rhs;
        }
        result
    }

    /// and_expr → not_expr ("&&" not_expr)*
    fn dbl_and_expr(&mut self, args: &[&str], pos: &mut usize) -> bool {
        let mut result = self.dbl_not_expr(args, pos);
        while *pos < args.len() && args[*pos] == "&&" {
            *pos += 1;
            let rhs = self.dbl_not_expr(args, pos);
            result = result && rhs;
        }
        result
    }

    /// not_expr → "!" not_expr | primary
    fn dbl_not_expr(&mut self, args: &[&str], pos: &mut usize) -> bool {
        if *pos < args.len() && args[*pos] == "!" {
            *pos += 1;
            return !self.dbl_not_expr(args, pos);
        }
        self.dbl_primary(args, pos)
    }

    /// primary → "(" or_expr ")" | unary_test | binary_test | single_word
    fn dbl_primary(&mut self, args: &[&str], pos: &mut usize) -> bool {
        if *pos >= args.len() {
            return false;
        }

        // Parenthesized sub-expression
        if args[*pos] == "(" {
            *pos += 1;
            let result = self.dbl_or_expr(args, pos);
            if *pos < args.len() && args[*pos] == ")" {
                *pos += 1;
            }
            return result;
        }

        let token = args[*pos];

        // Unary tests: -f, -d, -e, -r, -w, -x, -s, -z, -n, -v, -L, -h
        if matches!(
            token,
            "-f" | "-d" | "-e" | "-r" | "-w" | "-x" | "-s" | "-z" | "-n" | "-v" | "-L" | "-h"
        ) && *pos + 1 < args.len()
        {
            let flag = token;
            *pos += 1;
            let operand = args[*pos];
            *pos += 1;
            return self.dbl_unary_test(flag, operand);
        }

        // Check for binary test: word OP word
        if *pos + 2 <= args.len() {
            // Peek ahead for a binary operator
            if *pos + 1 < args.len() {
                let maybe_op = args[*pos + 1];
                if matches!(
                    maybe_op,
                    "==" | "!="
                        | "=~"
                        | "="
                        | "<"
                        | ">"
                        | "-eq"
                        | "-ne"
                        | "-lt"
                        | "-gt"
                        | "-le"
                        | "-ge"
                        | "-nt"
                        | "-ot"
                ) {
                    let lhs = args[*pos];
                    *pos += 2; // skip lhs and op
                    if *pos < args.len() {
                        let rhs = args[*pos];
                        *pos += 1;
                        return self.dbl_binary_test(lhs, maybe_op, rhs);
                    }
                    return false;
                }
            }
        }

        // Single word: non-empty string is true
        *pos += 1;
        !token.is_empty()
    }

    fn dbl_unary_test(&mut self, flag: &str, operand: &str) -> bool {
        match flag {
            "-f" => {
                let p = self.resolve(operand);
                self.fs.is_file(&p)
            }
            "-d" => {
                let p = self.resolve(operand);
                self.fs.is_dir(&p)
            }
            "-e" => {
                let p = self.resolve(operand);
                self.fs.exists(&p)
            }
            "-r" => {
                let p = self.resolve(operand);
                self.fs
                    .metadata(&p)
                    .map(|m| m.is_readable())
                    .unwrap_or(false)
            }
            "-w" => {
                let p = self.resolve(operand);
                self.fs
                    .metadata(&p)
                    .map(|m| m.is_writable())
                    .unwrap_or(false)
            }
            "-x" => {
                let p = self.resolve(operand);
                self.fs
                    .metadata(&p)
                    .map(|m| m.is_executable())
                    .unwrap_or(false)
            }
            "-s" => {
                let p = self.resolve(operand);
                self.fs.metadata(&p).map(|m| !m.is_empty()).unwrap_or(false)
            }
            "-L" | "-h" => {
                let p = self.resolve(operand);
                self.fs.is_symlink(&p)
            }
            "-z" => operand.is_empty(),
            "-n" => !operand.is_empty(),
            "-v" => self.vars.env.contains_key(operand),
            _ => false,
        }
    }

    fn dbl_binary_test(&mut self, lhs: &str, op: &str, rhs: &str) -> bool {
        match op {
            "==" | "=" => {
                // Glob matching for unquoted RHS
                Self::glob_match_str(rhs, lhs)
            }
            "!=" => !Self::glob_match_str(rhs, lhs),
            "=~" => {
                // Regex matching via regex-lite
                match Regex::new(rhs) {
                    Ok(re) => {
                        if let Some(caps) = re.captures(lhs) {
                            // Set BASH_REMATCH env vars
                            if let Some(m) = caps.get(0) {
                                self.vars
                                    .env
                                    .insert("BASH_REMATCH_0".into(), m.as_str().to_string());
                            }
                            // Capture groups
                            for i in 1..caps.len() {
                                let key = format!("BASH_REMATCH_{}", i);
                                let val = caps
                                    .get(i)
                                    .map(|m| m.as_str().to_string())
                                    .unwrap_or_default();
                                self.vars.env.insert(key.into(), val);
                            }
                            true
                        } else {
                            // Clear BASH_REMATCH on no match
                            self.vars.env.remove("BASH_REMATCH_0");
                            false
                        }
                    }
                    Err(_) => false,
                }
            }
            "<" => lhs < rhs,
            ">" => lhs > rhs,
            "-eq" => {
                let a: i64 = lhs.parse().unwrap_or(0);
                let b: i64 = rhs.parse().unwrap_or(0);
                a == b
            }
            "-ne" => {
                let a: i64 = lhs.parse().unwrap_or(0);
                let b: i64 = rhs.parse().unwrap_or(0);
                a != b
            }
            "-lt" => {
                let a: i64 = lhs.parse().unwrap_or(0);
                let b: i64 = rhs.parse().unwrap_or(0);
                a < b
            }
            "-gt" => {
                let a: i64 = lhs.parse().unwrap_or(0);
                let b: i64 = rhs.parse().unwrap_or(0);
                a > b
            }
            "-le" => {
                let a: i64 = lhs.parse().unwrap_or(0);
                let b: i64 = rhs.parse().unwrap_or(0);
                a <= b
            }
            "-ge" => {
                let a: i64 = lhs.parse().unwrap_or(0);
                let b: i64 = rhs.parse().unwrap_or(0);
                a >= b
            }
            "-nt" => {
                let pa = self.resolve(lhs);
                let pb = self.resolve(rhs);
                let ma = self.fs.metadata(&pa).ok().map(|m| m.mtime());
                let mb = self.fs.metadata(&pb).ok().map(|m| m.mtime());
                matches!((ma, mb), (Some(a), Some(b)) if a > b)
            }
            "-ot" => {
                let pa = self.resolve(lhs);
                let pb = self.resolve(rhs);
                let ma = self.fs.metadata(&pa).ok().map(|m| m.mtime());
                let mb = self.fs.metadata(&pb).ok().map(|m| m.mtime());
                matches!((ma, mb), (Some(a), Some(b)) if a < b)
            }
            _ => false,
        }
    }
}
