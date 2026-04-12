use crate::ansi;
use crate::shell::Shell;

impl Shell {
    /// printf [-v var] format [args...]: format and print data according to a format string.
    pub fn cmd_printf(&mut self, args: &[String]) -> (String, i32) {
        if args.is_empty() {
            return ("printf: missing format string".into(), 2);
        }
        // Detect -v varname flag
        let (assign_var, args) = if args[0] == "-v" {
            if args.len() < 2 {
                return ("printf: -v: missing variable name".into(), 1);
            }
            (Some(args[1].clone()), &args[2..])
        } else {
            (None, args)
        };
        if args.is_empty() {
            return ("printf: missing format string".into(), 2);
        }
        let fmt = &args[0];
        let mut positional_args = args[1..].iter();
        let mut output = String::new();
        let mut chars = fmt.chars().peekable();

        while let Some(c) = chars.next() {
            if c == '\\' {
                match chars.next() {
                    Some('n') => output.push('\n'),
                    Some('t') => output.push('\t'),
                    Some('\\') => output.push('\\'),
                    Some(other) => {
                        output.push('\\');
                        output.push(other);
                    }
                    None => output.push('\\'),
                }
            } else if c == '%' {
                // Collect optional flags, width, and precision
                let mut flags = String::new();
                let mut width_str = String::new();
                let mut prec_str: Option<String> = None;

                // Flags: -, +, space, 0, #
                while let Some(&'-') | Some(&'+') | Some(&' ') | Some(&'0') | Some(&'#') =
                    chars.peek()
                {
                    if let Some(ch) = chars.next() {
                        flags.push(ch);
                    }
                }
                // Width digits
                while matches!(chars.peek(), Some(&c) if c.is_ascii_digit()) {
                    if let Some(ch) = chars.next() {
                        width_str.push(ch);
                    }
                }
                // Precision: .digits
                if chars.peek() == Some(&'.') {
                    chars.next();
                    let mut ps = String::new();
                    while matches!(chars.peek(), Some(&c) if c.is_ascii_digit()) {
                        if let Some(ch) = chars.next() {
                            ps.push(ch);
                        }
                    }
                    prec_str = Some(ps);
                }

                let left_align = flags.contains('-');
                let zero_pad = flags.contains('0') && !left_align;
                let width: usize = width_str.parse().unwrap_or(0);
                let precision: Option<usize> = prec_str.as_deref().and_then(|p| p.parse().ok());

                let apply_width = |s: String| -> String {
                    if width == 0 || s.len() >= width {
                        return s;
                    }
                    let pad = width - s.len();
                    if left_align {
                        format!("{}{}", s, " ".repeat(pad))
                    } else if zero_pad {
                        // For numbers: sign before zeros
                        format!("{}{}", "0".repeat(pad), s)
                    } else {
                        format!("{}{}", " ".repeat(pad), s)
                    }
                };

                match chars.next() {
                    Some('s') => {
                        let val = positional_args.next().map(|s| s.as_str()).unwrap_or("");
                        let s = if let Some(p) = precision {
                            val.chars().take(p).collect::<String>()
                        } else {
                            val.to_string()
                        };
                        output.push_str(&apply_width(s));
                    }
                    Some('d') | Some('i') => {
                        let val = positional_args.next().map(|s| s.as_str()).unwrap_or("0");
                        let n: i64 = val.parse().unwrap_or(0);
                        output.push_str(&apply_width(n.to_string()));
                    }
                    Some('x') => {
                        let val = positional_args.next().map(|s| s.as_str()).unwrap_or("0");
                        let n: i64 = val.parse().unwrap_or(0);
                        output.push_str(&apply_width(format!("{:x}", n)));
                    }
                    Some('X') => {
                        let val = positional_args.next().map(|s| s.as_str()).unwrap_or("0");
                        let n: i64 = val.parse().unwrap_or(0);
                        output.push_str(&apply_width(format!("{:X}", n)));
                    }
                    Some('o') => {
                        let val = positional_args.next().map(|s| s.as_str()).unwrap_or("0");
                        let n: i64 = val.parse().unwrap_or(0);
                        output.push_str(&apply_width(format!("{:o}", n)));
                    }
                    Some('f') => {
                        let val = positional_args.next().map(|s| s.as_str()).unwrap_or("0");
                        let f: f64 = val.parse().unwrap_or(0.0);
                        let prec = precision.unwrap_or(6);
                        output.push_str(&apply_width(format!("{:.prec$}", f, prec = prec)));
                    }
                    Some('e') => {
                        let val = positional_args.next().map(|s| s.as_str()).unwrap_or("0");
                        let f: f64 = val.parse().unwrap_or(0.0);
                        let prec = precision.unwrap_or(6);
                        output.push_str(&apply_width(format_scientific(f, prec, false)));
                    }
                    Some('E') => {
                        let val = positional_args.next().map(|s| s.as_str()).unwrap_or("0");
                        let f: f64 = val.parse().unwrap_or(0.0);
                        let prec = precision.unwrap_or(6);
                        output.push_str(&apply_width(format_scientific(f, prec, true)));
                    }
                    Some('%') => output.push('%'),
                    Some(other) => {
                        output.push('%');
                        output.push_str(&flags);
                        output.push_str(&width_str);
                        if let Some(ref ps) = prec_str {
                            output.push('.');
                            output.push_str(ps);
                        }
                        output.push(other);
                    }
                    None => output.push('%'),
                }
            } else {
                output.push(c);
            }
        }

        if let Some(varname) = assign_var {
            self.vars.env.insert(varname.as_str().into(), output);
            return (String::new(), 0);
        }
        (output, 0)
    }

    /// echo [-n] [-e]: print arguments to stdout, optionally interpreting escape sequences.
    pub fn cmd_echo(&mut self, args: &[String]) -> (String, i32) {
        let mut interpret_escapes = false;
        let mut no_newline = false;
        let mut skip = 0;
        for arg in args {
            match arg.as_str() {
                "-n" => {
                    no_newline = true;
                    skip += 1;
                }
                "-e" => {
                    interpret_escapes = true;
                    skip += 1;
                }
                "-en" | "-ne" => {
                    interpret_escapes = true;
                    no_newline = true;
                    skip += 1;
                }
                _ => break,
            }
        }
        let mut output = args[skip..].join(" ");
        if interpret_escapes {
            output = unescape(&output);
        }
        if !no_newline {
            output.push('\n');
        }
        (output, 0)
    }

    /// head [-n N]: output the first N lines (default 10) of a file or stdin.
    pub fn cmd_head(&mut self, args: &[String], stdin: Option<&str>) -> (String, i32) {
        let (n, file) = Self::parse_n_file(args, 10);
        match file {
            Some(f) => self.read_lines(&f, |lines| lines.iter().take(n).cloned().collect(), "head"),
            None => {
                let lines: Vec<&str> = stdin.unwrap_or("").lines().take(n).collect();
                (lines.join("\n"), 0)
            }
        }
    }

    /// tail [-n N]: output the last N lines (default 10) of a file or stdin.
    pub fn cmd_tail(&mut self, args: &[String], stdin: Option<&str>) -> (String, i32) {
        let (n, file) = Self::parse_n_file(args, 10);
        match file {
            Some(f) => self.read_lines(
                &f,
                |lines| {
                    let s = lines.len().saturating_sub(n);
                    lines[s..].to_vec()
                },
                "tail",
            ),
            None => {
                let all: Vec<&str> = stdin.unwrap_or("").lines().collect();
                let s = all.len().saturating_sub(n);
                (all[s..].join("\n"), 0)
            }
        }
    }

    /// wc [-l] [-w] [-c] [-m]: count lines, words, bytes, or characters in a file or stdin.
    pub fn cmd_wc(&mut self, args: &[String], stdin: Option<&str>) -> (String, i32) {
        let mut line_only = false;
        let mut word_only = false;
        let mut char_only = false;
        let mut file_args: Vec<&String> = Vec::new();

        for arg in args {
            if arg.starts_with('-') && arg.len() > 1 {
                for c in arg[1..].chars() {
                    match c {
                        'l' => line_only = true,
                        'w' => word_only = true,
                        'c' | 'm' => char_only = true,
                        _ => {}
                    }
                }
            } else {
                file_args.push(arg);
            }
        }

        let show_all = !line_only && !word_only && !char_only;

        let format_counts = |content: &str, name: Option<&str>| -> String {
            let lines = content.matches('\n').count();
            let words = content.split_whitespace().count();
            let bytes = content.len();
            let mut parts = Vec::new();
            if show_all || line_only {
                parts.push(format!("  {}", lines));
            }
            if show_all || word_only {
                parts.push(format!("  {}", words));
            }
            if show_all || char_only {
                parts.push(format!("  {}", bytes));
            }
            if let Some(n) = name {
                parts.push(format!(" {}", n));
            }
            parts.join("")
        };

        if file_args.is_empty() {
            let input = stdin.unwrap_or("");
            return (format_counts(input, None), 0);
        }
        let mut out = Vec::new();
        let mut total_lines = 0usize;
        let mut total_words = 0usize;
        let mut total_bytes = 0usize;
        for arg in &file_args {
            let path = self.resolve(arg);
            match self.fs.read_to_string(&path) {
                Ok(c) => {
                    total_lines += c.matches('\n').count();
                    total_words += c.split_whitespace().count();
                    total_bytes += c.len();
                    out.push(format_counts(c, Some(arg)));
                }
                Err(e) => return (format!("wc: {}: {}", arg, e), 1),
            }
        }
        if file_args.len() > 1 {
            // Append total line
            let mut parts = Vec::new();
            if show_all || line_only {
                parts.push(format!("  {}", total_lines));
            }
            if show_all || word_only {
                parts.push(format!("  {}", total_words));
            }
            if show_all || char_only {
                parts.push(format!("  {}", total_bytes));
            }
            parts.push(" total".to_string());
            out.push(parts.join(""));
        }
        (out.join("\n"), 0)
    }

    /// grep [-E] [-i] [-v] [-n] [-c] [-l] [-o] [-q] [-w] [-A N] [-B N] [-C N]: search for patterns in files or stdin.
    pub fn cmd_grep(&mut self, args: &[String], stdin: Option<&str>) -> (String, i32) {
        let mut case_insensitive = false;
        let mut line_numbers = false;
        let mut invert = false;
        let mut count_only = false;
        let mut after_ctx: Option<usize> = None;
        let mut before_ctx: Option<usize> = None;
        let mut positional = Vec::new();

        let mut parser = lexopt::Parser::from_args(args.iter().cloned());
        loop {
            match parser.next() {
                Ok(Some(lexopt::Arg::Short('i'))) => case_insensitive = true,
                Ok(Some(lexopt::Arg::Short('n'))) => line_numbers = true,
                Ok(Some(lexopt::Arg::Short('v'))) => invert = true,
                Ok(Some(lexopt::Arg::Short('c'))) => count_only = true,
                Ok(Some(lexopt::Arg::Short('A'))) => {
                    if let Ok(val) = parser.value() {
                        after_ctx = val.to_string_lossy().parse().ok();
                    }
                }
                Ok(Some(lexopt::Arg::Short('B'))) => {
                    if let Ok(val) = parser.value() {
                        before_ctx = val.to_string_lossy().parse().ok();
                    }
                }
                Ok(Some(lexopt::Arg::Short('C'))) => {
                    if let Ok(val) = parser.value() {
                        let n: Option<usize> = val.to_string_lossy().parse().ok();
                        before_ctx = n;
                        after_ctx = n;
                    }
                }
                Ok(Some(lexopt::Arg::Long("color") | lexopt::Arg::Long("colour"))) => {
                    // consume and ignore the value; color is controlled by shell.color
                    let _ = parser.optional_value();
                }
                Ok(Some(lexopt::Arg::Value(val))) => {
                    positional.push(val.to_string_lossy().to_string())
                }
                Ok(Some(_)) => {}
                Ok(None) | Err(_) => break,
            }
        }

        if positional.is_empty() {
            return ("grep: missing pattern".into(), 2);
        }
        let pattern = &positional[0];
        let files = &positional[1..];

        let match_line = |line: &str| -> bool {
            let m = if case_insensitive {
                line.to_lowercase().contains(&pattern.to_lowercase())
            } else {
                line.contains(pattern.as_str())
            };
            if invert {
                !m
            } else {
                m
            }
        };

        let has_context = after_ctx.is_some() || before_ctx.is_some();

        // No file args → read from stdin
        if files.is_empty() {
            let input = stdin.unwrap_or("");
            if has_context && !count_only {
                return self.grep_context(
                    input,
                    None,
                    pattern,
                    case_insensitive,
                    &match_line,
                    line_numbers,
                    before_ctx.unwrap_or(0),
                    after_ctx.unwrap_or(0),
                    self.color,
                );
            }
            return self.grep_content(
                input,
                None,
                pattern,
                case_insensitive,
                &match_line,
                line_numbers,
                count_only,
                self.color,
            );
        }

        let multi = files.len() > 1;
        let mut all_out = Vec::new();
        let mut any_match = false;

        for file in files {
            let path = self.resolve(file);
            let content = match self.fs.read_to_string(&path) {
                Ok(c) => c,
                Err(e) => return (format!("grep: {}: {}", file, e), 2),
            };
            let prefix = if multi { Some(file.as_str()) } else { None };
            let (out, code) = if has_context && !count_only {
                self.grep_context(
                    content,
                    prefix,
                    pattern,
                    case_insensitive,
                    &match_line,
                    line_numbers,
                    before_ctx.unwrap_or(0),
                    after_ctx.unwrap_or(0),
                    self.color,
                )
            } else {
                self.grep_content(
                    content,
                    prefix,
                    pattern,
                    case_insensitive,
                    &match_line,
                    line_numbers,
                    count_only,
                    self.color,
                )
            };
            if code == 0 {
                any_match = true;
            }
            if !out.is_empty() {
                all_out.push(out);
            }
        }

        if any_match {
            (all_out.join("\n"), 0)
        } else {
            (String::new(), 1)
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn grep_context(
        &self,
        content: &str,
        prefix: Option<&str>,
        pattern: &str,
        case_insensitive: bool,
        match_fn: &dyn Fn(&str) -> bool,
        line_numbers: bool,
        before: usize,
        after: usize,
        color: bool,
    ) -> (String, i32) {
        let lines: Vec<&str> = content.lines().collect();
        let total = lines.len();

        // Find all matching line indices
        let match_indices: Vec<usize> = (0..total).filter(|&i| match_fn(lines[i])).collect();

        if match_indices.is_empty() {
            return (String::new(), 1);
        }

        // Build set of line indices to include, grouped into ranges
        // Each range is (start, end) inclusive
        let mut ranges: Vec<(usize, usize)> = Vec::new();
        for &mi in &match_indices {
            let start = mi.saturating_sub(before);
            let end = (mi + after).min(total - 1);
            if let Some(last) = ranges.last_mut() {
                if start <= last.1 + 1 {
                    // Merge overlapping/adjacent ranges
                    last.1 = last.1.max(end);
                    continue;
                }
            }
            ranges.push((start, end));
        }

        let match_set: std::collections::HashSet<usize> = match_indices.into_iter().collect();
        let mut out = Vec::new();

        for (ri, &(start, end)) in ranges.iter().enumerate() {
            if ri > 0 {
                out.push("--".to_string());
            }
            for (i, line) in lines[start..=end]
                .iter()
                .enumerate()
                .map(|(j, l)| (start + j, l))
            {
                let is_match = match_set.contains(&i);
                let sep = if is_match { ':' } else { '-' };
                let mut entry = String::new();
                if let Some(p) = prefix {
                    if color {
                        entry.push_str(&format!("{}{}{}", ansi::MAGENTA, p, ansi::RESET));
                    } else {
                        entry.push_str(p);
                    }
                    entry.push(sep);
                }
                if line_numbers {
                    if color {
                        entry.push_str(&format!("{}{}{}", ansi::GREEN, i + 1, ansi::RESET));
                    } else {
                        entry.push_str(&(i + 1).to_string());
                    }
                    entry.push(sep);
                }
                if color && is_match {
                    entry.push_str(&ansi::highlight_matches(line, pattern, case_insensitive));
                } else {
                    entry.push_str(line);
                }

                out.push(entry);
            }
        }

        (out.join("\n"), 0)
    }

    #[allow(clippy::too_many_arguments)]
    fn grep_content(
        &self,
        content: &str,
        prefix: Option<&str>,
        pattern: &str,
        case_insensitive: bool,
        match_fn: &dyn Fn(&str) -> bool,
        line_numbers: bool,
        count_only: bool,
        color: bool,
    ) -> (String, i32) {
        let mut out = Vec::new();
        let mut count = 0;

        for (i, line) in content.lines().enumerate() {
            if match_fn(line) {
                count += 1;
                if !count_only {
                    let mut entry = String::new();
                    if let Some(p) = prefix {
                        if color {
                            entry.push_str(&format!("{}{}{}", ansi::MAGENTA, p, ansi::RESET));
                        } else {
                            entry.push_str(p);
                        }
                        entry.push(':');
                    }
                    if line_numbers {
                        if color {
                            entry.push_str(&format!("{}{}{}", ansi::GREEN, i + 1, ansi::RESET));
                        } else {
                            entry.push_str(&(i + 1).to_string());
                        }
                        entry.push(':');
                    }
                    if color {
                        entry.push_str(&ansi::highlight_matches(line, pattern, case_insensitive));
                    } else {
                        entry.push_str(line);
                    }
                    out.push(entry);
                }
            }
        }

        if count_only {
            let result = match prefix {
                Some(p) => format!("{}:{}", p, count),
                None => count.to_string(),
            };
            (result, if count > 0 { 0 } else { 1 })
        } else if out.is_empty() {
            (String::new(), 1)
        } else {
            (out.join("\n"), 0)
        }
    }

    /// sort [-r] [-n] [-u] [-k N] [-t SEP]: sort lines of text from a file or stdin.
    pub fn cmd_sort(&mut self, args: &[String], stdin: Option<&str>) -> (String, i32) {
        let mut reverse = false;
        let mut numeric = false;
        let mut delimiter: Option<char> = None;
        let mut key_field: Option<usize> = None;
        let mut file = None;

        let mut parser = lexopt::Parser::from_args(args.iter().cloned());
        loop {
            match parser.next() {
                Ok(Some(lexopt::Arg::Short('r'))) => reverse = true,
                Ok(Some(lexopt::Arg::Short('n'))) => numeric = true,
                Ok(Some(lexopt::Arg::Short('t'))) => {
                    if let Ok(val) = parser.value() {
                        let s = val.to_string_lossy();
                        delimiter = s.chars().next();
                    }
                }
                Ok(Some(lexopt::Arg::Short('k'))) => {
                    if let Ok(val) = parser.value() {
                        let s = val.to_string_lossy();
                        let field_str = s.split(',').next().unwrap_or(&s);
                        if let Ok(n) = field_str.parse::<usize>() {
                            if n > 0 {
                                key_field = Some(n - 1);
                            }
                        }
                    }
                }
                Ok(Some(lexopt::Arg::Value(val))) => file = Some(val.to_string_lossy().to_string()),
                Ok(Some(_)) => {}
                Ok(None) | Err(_) => break,
            }
        }

        let input = match self.resolve_input(file.as_deref(), stdin) {
            Ok(s) => s,
            Err(e) => return (format!("sort: {}", e), 1),
        };

        let extract_key = |line: &str| -> String {
            if let (Some(delim), Some(field)) = (delimiter, key_field) {
                line.split(delim).nth(field).unwrap_or("").to_string()
            } else if let Some(field) = key_field {
                line.split_whitespace().nth(field).unwrap_or("").to_string()
            } else {
                line.to_string()
            }
        };

        let mut lines: Vec<&str> = input.lines().collect();
        if numeric {
            lines.sort_by(|a, b| {
                let ka = extract_key(a);
                let kb = extract_key(b);
                let na: f64 = ka.trim().parse().unwrap_or(0.0);
                let nb: f64 = kb.trim().parse().unwrap_or(0.0);
                na.partial_cmp(&nb).unwrap_or(std::cmp::Ordering::Equal)
            });
        } else {
            lines.sort_by(|a, b| {
                let ka = extract_key(a);
                let kb = extract_key(b);
                ka.cmp(&kb)
            });
        }
        if reverse {
            lines.reverse();
        }
        (lines.join("\n"), 0)
    }

    /// uniq [-c] [-d] [-u] [-i]: filter or count adjacent duplicate lines from a file or stdin.
    pub fn cmd_uniq(&mut self, args: &[String], stdin: Option<&str>) -> (String, i32) {
        let mut count = false;
        let mut only_dupes = false;
        let mut only_unique = false;
        let mut file = None;

        let mut parser = lexopt::Parser::from_args(args.iter().cloned());
        loop {
            match parser.next() {
                Ok(Some(lexopt::Arg::Short('c'))) => count = true,
                Ok(Some(lexopt::Arg::Short('d'))) => only_dupes = true,
                Ok(Some(lexopt::Arg::Short('u'))) => only_unique = true,
                Ok(Some(lexopt::Arg::Value(val))) => file = Some(val.to_string_lossy().to_string()),
                Ok(Some(_)) => {}
                Ok(None) | Err(_) => break,
            }
        }

        let input = match self.resolve_input(file.as_deref(), stdin) {
            Ok(s) => s,
            Err(e) => return (format!("uniq: {}", e), 1),
        };

        let mut out = Vec::new();
        let mut prev: Option<&str> = None;
        let mut cnt: usize = 0;

        for line in input.lines() {
            if prev == Some(line) {
                cnt += 1;
            } else {
                if let Some(p) = prev {
                    let include = if only_dupes {
                        cnt > 1
                    } else if only_unique {
                        cnt == 1
                    } else {
                        true
                    };
                    if include {
                        out.push(if count {
                            format!("{:>7} {}", cnt, p)
                        } else {
                            p.to_string()
                        });
                    }
                }
                prev = Some(line);
                cnt = 1;
            }
        }
        if let Some(p) = prev {
            let include = if only_dupes {
                cnt > 1
            } else if only_unique {
                cnt == 1
            } else {
                true
            };
            if include {
                out.push(if count {
                    format!("{:>7} {}", cnt, p)
                } else {
                    p.to_string()
                });
            }
        }

        (out.join("\n"), 0)
    }

    /// cut [-d DELIM] [-f FIELDS] [-c CHARS]: extract selected fields or characters from each line.
    pub fn cmd_cut(&mut self, args: &[String], stdin: Option<&str>) -> (String, i32) {
        let mut delimiter = '\t';
        let mut fields = Vec::new();
        let mut file = None;

        let mut parser = lexopt::Parser::from_args(args.iter().cloned());
        loop {
            match parser.next() {
                Ok(Some(lexopt::Arg::Short('d'))) => {
                    if let Ok(val) = parser.value() {
                        delimiter = val.to_string_lossy().chars().next().unwrap_or('\t');
                    }
                }
                Ok(Some(lexopt::Arg::Short('f'))) => {
                    if let Ok(val) = parser.value() {
                        fields = parse_field_spec(&val.to_string_lossy());
                    }
                }
                Ok(Some(lexopt::Arg::Value(val))) => file = Some(val.to_string_lossy().to_string()),
                Ok(Some(_)) => {}
                Ok(None) | Err(_) => break,
            }
        }

        if fields.is_empty() {
            return ("cut: no fields specified".into(), 1);
        }

        let input = match self.resolve_input(file.as_deref(), stdin) {
            Ok(s) => s,
            Err(e) => return (format!("cut: {}", e), 1),
        };

        let delim_str = delimiter.to_string();
        let out: Vec<String> = input
            .lines()
            .map(|line| {
                let parts: Vec<&str> = line.split(delimiter).collect();
                fields
                    .iter()
                    .filter_map(|&f| parts.get(f.saturating_sub(1)).copied())
                    .collect::<Vec<_>>()
                    .join(&delim_str)
            })
            .collect();

        (out.join("\n"), 0)
    }

    /// tr [-d] [-s] SET1 [SET2]: translate or delete characters from stdin.
    pub fn cmd_tr(&mut self, _args: &[String], stdin: Option<&str>) -> (String, i32) {
        let mut delete = false;
        let mut positional = Vec::new();

        for arg in _args {
            match arg.as_str() {
                "-d" => delete = true,
                _ => positional.push(arg.as_str()),
            }
        }

        let input = stdin.unwrap_or("");

        if delete {
            if positional.is_empty() {
                return ("tr: missing operand".into(), 1);
            }
            let chars_to_delete: Vec<char> = unescape(positional[0]).chars().collect();
            let result: String = input
                .chars()
                .filter(|c| !chars_to_delete.contains(c))
                .collect();
            return (result, 0);
        }

        if positional.len() < 2 {
            return ("tr: missing operand".into(), 1);
        }

        let from: Vec<char> = unescape(positional[0]).chars().collect();
        let to: Vec<char> = unescape(positional[1]).chars().collect();

        let result: String = input
            .chars()
            .map(|c| {
                if let Some(pos) = from.iter().position(|&fc| fc == c) {
                    to.get(pos).or(to.last()).copied().unwrap_or(c)
                } else {
                    c
                }
            })
            .collect();

        (result, 0)
    }

    /// rev: reverse the characters of each line from a file or stdin.
    pub fn cmd_rev(&mut self, args: &[String], stdin: Option<&str>) -> (String, i32) {
        let input = match args.first().filter(|a| !a.starts_with('-')) {
            Some(f) => match self.resolve_input(Some(f), stdin) {
                Ok(s) => s,
                Err(e) => return (format!("rev: {}", e), 1),
            },
            None => stdin.unwrap_or("").to_string(),
        };

        let reversed: Vec<String> = input.lines().map(|l| l.chars().rev().collect()).collect();
        (reversed.join("\n"), 0)
    }

    /// seq [-s SEP] [-w] [FIRST [INCR]] LAST: print a sequence of numbers.
    pub fn cmd_seq(&mut self, args: &[String]) -> (String, i32) {
        let mut separator = "\n".to_string();
        let mut equal_width = false;
        let mut positional = Vec::new();

        let mut parser = lexopt::Parser::from_args(args.iter().cloned());
        loop {
            match parser.next() {
                Ok(Some(lexopt::Arg::Short('s'))) => {
                    if let Ok(val) = parser.value() {
                        separator = val.to_string_lossy().to_string();
                    }
                }
                Ok(Some(lexopt::Arg::Short('w'))) => equal_width = true,
                Ok(Some(lexopt::Arg::Value(val))) => {
                    positional.push(val.to_string_lossy().to_string())
                }
                Ok(Some(_)) => {}
                Ok(None) | Err(_) => break,
            }
        }

        let nums: Vec<f64> = positional.iter().filter_map(|a| a.parse().ok()).collect();
        let (start, step, end) = match nums.len() {
            1 => (1.0, 1.0, nums[0]),
            2 => (nums[0], 1.0, nums[1]),
            3 => (nums[0], nums[1], nums[2]),
            _ => return ("seq: missing operand".into(), 1),
        };

        if step == 0.0 {
            return ("seq: zero increment".into(), 1);
        }

        // Determine if we are in float mode: any positional arg contains '.'
        let is_float = positional.iter().any(|a| a.contains('.'));

        // Determine decimal places for float formatting
        let decimal_places = if is_float {
            positional
                .iter()
                .map(|a| {
                    if let Some(dot) = a.find('.') {
                        a.len() - dot - 1
                    } else {
                        0
                    }
                })
                .max()
                .unwrap_or(1)
        } else {
            0
        };

        let mut results = Vec::new();
        if step > 0.0 {
            let mut i = start;
            while i <= end + f64::EPSILON * 1000.0 {
                if is_float {
                    results.push(format!("{:.prec$}", i, prec = decimal_places));
                } else {
                    results.push(format!("{}", i as i64));
                }
                i += step;
            }
        } else {
            let mut i = start;
            while i >= end - f64::EPSILON * 1000.0 {
                if is_float {
                    results.push(format!("{:.prec$}", i, prec = decimal_places));
                } else {
                    results.push(format!("{}", i as i64));
                }
                i += step;
            }
        }

        if equal_width && !results.is_empty() {
            let max_len = results.iter().map(|s| s.len()).max().unwrap_or(0);
            for r in &mut results {
                if r.len() < max_len {
                    let pad = max_len - r.len();
                    *r = format!("{}{}", "0".repeat(pad), r);
                }
            }
        }

        if separator == "\n" {
            (results.join("\n"), 0)
        } else {
            // Custom separator: join with separator and no trailing separator
            (results.join(&separator), 0)
        }
    }

    /// tac: concatenate and print lines of a file or stdin in reverse order.
    pub fn cmd_tac(&mut self, args: &[String], stdin: Option<&str>) -> (String, i32) {
        let file = args.iter().find(|a| !a.starts_with('-'));
        let input = match file {
            Some(f) => match self.resolve_input(Some(f), stdin) {
                Ok(s) => s,
                Err(e) => return (format!("tac: {}", e), 1),
            },
            None => stdin.unwrap_or("").to_string(),
        };
        let reversed: Vec<&str> = input.lines().rev().collect();
        (reversed.join("\n"), 0)
    }

    /// nl [-b a] [-n FORMAT] [-w N]: number lines of a file or stdin.
    pub fn cmd_nl(&mut self, args: &[String], stdin: Option<&str>) -> (String, i32) {
        // Support -b a (number all lines, the default behaviour we implement)
        let mut file = None;
        let mut i = 0;
        while i < args.len() {
            match args[i].as_str() {
                "-b" => {
                    i += 2; // skip flag and its value
                }
                s if !s.starts_with('-') => {
                    file = Some(args[i].clone());
                    i += 1;
                }
                _ => {
                    i += 1;
                }
            }
        }

        let input = match self.resolve_input(file.as_deref(), stdin) {
            Ok(s) => s,
            Err(e) => return (format!("nl: {}", e), 1),
        };

        let numbered: Vec<String> = input
            .lines()
            .enumerate()
            .map(|(i, line)| format!("{:>6}\t{}", i + 1, line))
            .collect();
        (numbered.join("\n"), 0)
    }

    /// paste [-d DELIM] [-s] files...: merge lines of files side by side.
    pub fn cmd_paste(&mut self, args: &[String], stdin: Option<&str>) -> (String, i32) {
        let mut delimiter = '\t';
        let mut files: Vec<String> = Vec::new();

        let mut i = 0;
        while i < args.len() {
            match args[i].as_str() {
                "-d" if i + 1 < args.len() => {
                    delimiter = args[i + 1].chars().next().unwrap_or('\t');
                    i += 2;
                }
                s if s.starts_with("-d") && s.len() > 2 => {
                    delimiter = s.chars().nth(2).unwrap_or('\t');
                    i += 1;
                }
                _ => {
                    files.push(args[i].clone());
                    i += 1;
                }
            }
        }

        if files.is_empty() {
            // paste from stdin only
            return (stdin.unwrap_or("").to_string(), 0);
        }

        let mut columns: Vec<Vec<String>> = Vec::new();
        for f in &files {
            let path = self.resolve(f);
            let content = match self.fs.read_to_string(&path) {
                Ok(s) => s.to_string(),
                Err(e) => return (format!("paste: {}: {}", f, e), 1),
            };
            columns.push(content.lines().map(|l| l.to_string()).collect());
        }

        let max_lines = columns.iter().map(|c| c.len()).max().unwrap_or(0);
        let delim_str = delimiter.to_string();
        let out: Vec<String> = (0..max_lines)
            .map(|i| {
                columns
                    .iter()
                    .map(|col| col.get(i).map(|s| s.as_str()).unwrap_or(""))
                    .collect::<Vec<_>>()
                    .join(&delim_str)
            })
            .collect();

        (out.join("\n"), 0)
    }

    /// column [-t] [-s SEP]: format input into aligned columns or a table.
    pub fn cmd_column(&mut self, args: &[String], stdin: Option<&str>) -> (String, i32) {
        let mut table_mode = false;
        let mut delimiter: Option<String> = None;
        let mut files: Vec<String> = Vec::new();

        let mut i = 0;
        while i < args.len() {
            match args[i].as_str() {
                "-t" => {
                    table_mode = true;
                    i += 1;
                }
                "-s" if i + 1 < args.len() => {
                    delimiter = Some(args[i + 1].clone());
                    i += 2;
                }
                s if s.starts_with("-s") && s.len() > 2 => {
                    delimiter = Some(s[2..].to_string());
                    i += 1;
                }
                s if !s.starts_with('-') => {
                    files.push(args[i].clone());
                    i += 1;
                }
                _ => {
                    i += 1;
                }
            }
        }

        let input = if files.is_empty() {
            stdin.unwrap_or("").to_string()
        } else {
            let mut combined = String::new();
            for f in &files {
                match self.resolve_input(Some(f), None) {
                    Ok(s) => combined.push_str(&s),
                    Err(e) => return (format!("column: {}", e), 1),
                }
            }
            combined
        };

        if !table_mode {
            // Without -t, just pass through (basic behavior)
            return (input, 0);
        }

        // Table mode: split each line by delimiter (or whitespace), compute max column widths, pad
        let rows: Vec<Vec<String>> = input
            .lines()
            .map(|line| {
                if let Some(ref delim) = delimiter {
                    line.split(delim.as_str()).map(|s| s.to_string()).collect()
                } else {
                    line.split_whitespace().map(|s| s.to_string()).collect()
                }
            })
            .collect();

        if rows.is_empty() {
            return (String::new(), 0);
        }

        let num_cols = rows.iter().map(|r| r.len()).max().unwrap_or(0);
        let mut col_widths = vec![0usize; num_cols];
        for row in &rows {
            for (ci, cell) in row.iter().enumerate() {
                if cell.len() > col_widths[ci] {
                    col_widths[ci] = cell.len();
                }
            }
        }

        let out_lines: Vec<String> = rows
            .iter()
            .map(|row| {
                let mut parts: Vec<String> = Vec::new();
                for (ci, width) in col_widths.iter().enumerate() {
                    let cell = row.get(ci).map(|s| s.as_str()).unwrap_or("");
                    if ci + 1 < num_cols {
                        // Non-last columns: pad to full width + 2-space separator
                        parts.push(format!("{:<width$}  ", cell, width = width));
                    } else {
                        // Last column: pad to full width (no trailing separator)
                        parts.push(format!("{:<width$}", cell, width = width));
                    }
                }
                parts.concat()
            })
            .collect();

        (out_lines.join("\n"), 0)
    }

    /// xargs [-n N] cmd [args...]: build and execute commands from stdin arguments.
    pub fn cmd_xargs(&mut self, args: &[String], stdin: Option<&str>) -> (String, i32) {
        let mut n_per: Option<usize> = None;
        let mut replace_str: Option<String> = None;
        let mut cmd_and_args: Vec<String> = Vec::new();

        let mut i = 0;
        while i < args.len() {
            match args[i].as_str() {
                "-n" if i + 1 < args.len() => {
                    n_per = args[i + 1].parse().ok();
                    i += 2;
                }
                "-I" if i + 1 < args.len() => {
                    replace_str = Some(args[i + 1].clone());
                    i += 2;
                }
                s if s.starts_with("-I") && s.len() > 2 => {
                    replace_str = Some(s[2..].to_string());
                    i += 1;
                }
                _ => {
                    cmd_and_args.push(args[i].clone());
                    i += 1;
                }
            }
        }

        let input = stdin.unwrap_or("");
        let tokens: Vec<String> = input.split_whitespace().map(|s| s.to_string()).collect();

        let base_cmd = if cmd_and_args.is_empty() {
            "echo".to_string()
        } else {
            cmd_and_args[0].clone()
        };
        let base_extra: Vec<String> = if cmd_and_args.len() > 1 {
            cmd_and_args[1..].to_vec()
        } else {
            Vec::new()
        };

        let mut combined_output = String::new();

        if let Some(repl) = replace_str {
            // -I mode: one invocation per token, replacing repl in base_extra
            for token in &tokens {
                let call_args: Vec<String> =
                    base_extra.iter().map(|a| a.replace(&repl, token)).collect();
                let (out, _code, _) = crate::commands::dispatch(self, &base_cmd, &call_args, None);
                combined_output.push_str(&out);
            }
        } else if let Some(n) = n_per {
            // -n mode: invoke command with at most n tokens at a time
            for chunk in tokens.chunks(n) {
                let mut call_args = base_extra.clone();
                call_args.extend(chunk.iter().cloned());
                let (out, _code, _) = crate::commands::dispatch(self, &base_cmd, &call_args, None);
                combined_output.push_str(&out);
            }
        } else {
            // Default: all tokens in one invocation
            let mut call_args = base_extra.clone();
            call_args.extend(tokens.iter().cloned());
            let (out, _code, _) = crate::commands::dispatch(self, &base_cmd, &call_args, None);
            combined_output.push_str(&out);
        }

        // Strip exactly one trailing newline: plain() in dispatch will add it back.
        // This avoids double-newlines while preserving empty-line output from echo.
        // For multi-invocation output, intermediate newlines are preserved as-is.
        if combined_output.ends_with('\n') && combined_output.len() > 1 {
            combined_output.pop();
        }
        (combined_output, 0)
    }

    // -- helpers --

    /// Resolve input from a file path or stdin
    pub(crate) fn resolve_input(
        &mut self,
        file: Option<&str>,
        stdin: Option<&str>,
    ) -> Result<String, String> {
        match file {
            Some(f) => {
                let path = self.resolve(f);
                self.fs
                    .read_to_string(&path)
                    .map(|s| s.to_string())
                    .map_err(|e| format!("{}: {}", f, e))
            }
            None => Ok(stdin.unwrap_or("").to_string()),
        }
    }

    fn parse_n_file(args: &[String], default_n: usize) -> (usize, Option<String>) {
        let mut n = default_n;
        let mut file = None;

        // Pre-scan for -N shorthand (e.g., "-3" means -n 3)
        let mut transformed: Vec<String> = Vec::new();
        for arg in args {
            if arg.starts_with('-') && arg.len() > 1 {
                let rest = &arg[1..];
                if rest.chars().all(|c| c.is_ascii_digit()) {
                    transformed.push("-n".to_string());
                    transformed.push(rest.to_string());
                    continue;
                }
            }
            transformed.push(arg.clone());
        }

        let mut parser = lexopt::Parser::from_args(transformed);
        loop {
            match parser.next() {
                Ok(Some(lexopt::Arg::Short('n'))) => {
                    if let Ok(val) = parser.value() {
                        n = val.to_string_lossy().parse().unwrap_or(default_n);
                    }
                }
                Ok(Some(lexopt::Arg::Value(val))) => file = Some(val.to_string_lossy().to_string()),
                Ok(Some(_)) => {}
                Ok(None) | Err(_) => break,
            }
        }
        (n, file)
    }

    fn read_lines(
        &mut self,
        filename: &str,
        transform: impl for<'a> FnOnce(&'a [&'a str]) -> Vec<&'a str>,
        cmd_name: &str,
    ) -> (String, i32) {
        let path = self.resolve(filename);
        match self.fs.read_to_string(&path) {
            Ok(c) => {
                let lines: Vec<&str> = c.lines().collect();
                (transform(&lines).join("\n"), 0)
            }
            Err(e) => (format!("{}: {}: {}", cmd_name, filename, e), 1),
        }
    }
}

fn unescape(s: &str) -> String {
    let mut result = String::new();
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('n') => result.push('\n'),
                Some('t') => result.push('\t'),
                Some('\\') => result.push('\\'),
                Some('0') => result.push('\0'),
                Some(other) => {
                    result.push('\\');
                    result.push(other);
                }
                None => result.push('\\'),
            }
        } else {
            result.push(c);
        }
    }
    result
}

fn parse_field_spec(spec: &str) -> Vec<usize> {
    let mut fields = Vec::new();
    for part in spec.split(',') {
        if let Some((start, end)) = part.split_once('-') {
            let s: usize = start.parse().unwrap_or(1);
            let e: usize = end.parse().unwrap_or(s);
            for i in s..=e {
                fields.push(i);
            }
        } else if let Ok(n) = part.parse::<usize>() {
            fields.push(n);
        }
    }
    fields
}

/// Format a float in scientific notation (e.g. 3.140000e+00).
fn format_scientific(f: f64, prec: usize, uppercase: bool) -> String {
    if f == 0.0 {
        let exp_char = if uppercase { 'E' } else { 'e' };
        return format!("{:.prec$}{}{:+03}", 0.0f64, exp_char, 0i32, prec = prec);
    }
    let exp = f.abs().log10().floor() as i32;
    let mantissa = f / 10f64.powi(exp);
    let exp_char = if uppercase { 'E' } else { 'e' };
    format!("{:.prec$}{}{:+03}", mantissa, exp_char, exp, prec = prec)
}
