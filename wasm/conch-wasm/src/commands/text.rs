use crate::ansi;
use crate::shell::Shell;

impl Shell {
    pub fn cmd_printf(&self, args: &[String]) -> (String, i32) {
        if args.is_empty() {
            return ("printf: missing format string".into(), 1);
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
                match chars.next() {
                    Some('s') => {
                        let val = positional_args.next().map(|s| s.as_str()).unwrap_or("");
                        output.push_str(val);
                    }
                    Some('d') => {
                        let val = positional_args.next().map(|s| s.as_str()).unwrap_or("0");
                        let n: i64 = val.parse().unwrap_or(0);
                        output.push_str(&n.to_string());
                    }
                    Some('%') => output.push('%'),
                    Some(other) => {
                        output.push('%');
                        output.push(other);
                    }
                    None => output.push('%'),
                }
            } else {
                output.push(c);
            }
        }

        (output, 0)
    }

    pub fn cmd_echo(&self, args: &[String]) -> (String, i32) {
        let mut interpret_escapes = false;
        let mut skip = 0;
        for arg in args {
            match arg.as_str() {
                "-n" => skip += 1,
                "-e" => {
                    interpret_escapes = true;
                    skip += 1;
                }
                _ => break,
            }
        }
        let output = args[skip..].join(" ");
        if interpret_escapes {
            (unescape(&output), 0)
        } else {
            (output, 0)
        }
    }

    pub fn cmd_head(&self, args: &[String], stdin: Option<&str>) -> (String, i32) {
        let (n, file) = Self::parse_n_file(args, 10);
        match file {
            Some(f) => self.read_lines(&f, |lines| lines.iter().take(n).cloned().collect(), "head"),
            None => {
                let lines: Vec<&str> = stdin.unwrap_or("").lines().take(n).collect();
                (lines.join("\n"), 0)
            }
        }
    }

    pub fn cmd_tail(&self, args: &[String], stdin: Option<&str>) -> (String, i32) {
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

    pub fn cmd_wc(&self, args: &[String], stdin: Option<&str>) -> (String, i32) {
        let file_args: Vec<&String> = args.iter().filter(|a| !a.starts_with('-')).collect();
        if file_args.is_empty() {
            let input = stdin.unwrap_or("");
            return (
                format!(
                    "  {}  {}  {}",
                    input.lines().count(),
                    input.split_whitespace().count(),
                    input.len()
                ),
                0,
            );
        }
        let mut out = Vec::new();
        for arg in &file_args {
            let path = self.resolve(arg);
            match self.fs.read_to_string(&path) {
                Ok(c) => {
                    out.push(format!(
                        "  {}  {}  {} {}",
                        c.lines().count(),
                        c.split_whitespace().count(),
                        c.len(),
                        arg
                    ));
                }
                Err(e) => return (format!("wc: {}: {}", arg, e), 1),
            }
        }
        (out.join("\n"), 0)
    }

    pub fn cmd_grep(&self, args: &[String], stdin: Option<&str>) -> (String, i32) {
        let mut case_insensitive = false;
        let mut line_numbers = false;
        let mut invert = false;
        let mut count_only = false;
        let mut positional = Vec::new();

        let mut parser = lexopt::Parser::from_args(args.iter().cloned());
        loop {
            match parser.next() {
                Ok(Some(lexopt::Arg::Short('i'))) => case_insensitive = true,
                Ok(Some(lexopt::Arg::Short('n'))) => line_numbers = true,
                Ok(Some(lexopt::Arg::Short('v'))) => invert = true,
                Ok(Some(lexopt::Arg::Short('c'))) => count_only = true,
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

        // No file args → read from stdin
        if files.is_empty() {
            let input = stdin.unwrap_or("");
            return self.grep_content(
                input,
                None,
                pattern,
                case_insensitive,
                &match_line,
                line_numbers,
                count_only,
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
            let (out, code) = self.grep_content(
                content,
                prefix,
                pattern,
                case_insensitive,
                &match_line,
                line_numbers,
                count_only,
            );
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
    fn grep_content(
        &self,
        content: &str,
        prefix: Option<&str>,
        pattern: &str,
        case_insensitive: bool,
        match_fn: &dyn Fn(&str) -> bool,
        line_numbers: bool,
        count_only: bool,
    ) -> (String, i32) {
        let mut out = Vec::new();
        let mut count = 0;

        for (i, line) in content.lines().enumerate() {
            if match_fn(line) {
                count += 1;
                if !count_only {
                    let mut entry = String::new();
                    if let Some(p) = prefix {
                        entry.push_str(&format!("{}{}{}", ansi::MAGENTA, p, ansi::RESET));
                        entry.push(':');
                    }
                    if line_numbers {
                        entry.push_str(&format!("{}{}{}", ansi::GREEN, i + 1, ansi::RESET));
                        entry.push(':');
                    }
                    entry.push_str(&ansi::highlight_matches(line, pattern, case_insensitive));
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

    pub fn cmd_sort(&self, args: &[String], stdin: Option<&str>) -> (String, i32) {
        let mut reverse = false;
        let mut numeric = false;
        let mut file = None;

        let mut parser = lexopt::Parser::from_args(args.iter().cloned());
        loop {
            match parser.next() {
                Ok(Some(lexopt::Arg::Short('r'))) => reverse = true,
                Ok(Some(lexopt::Arg::Short('n'))) => numeric = true,
                Ok(Some(lexopt::Arg::Value(val))) => file = Some(val.to_string_lossy().to_string()),
                Ok(Some(_)) => {}
                Ok(None) | Err(_) => break,
            }
        }

        let input = match self.resolve_input(file.as_deref(), stdin) {
            Ok(s) => s,
            Err(e) => return (format!("sort: {}", e), 1),
        };

        let mut lines: Vec<&str> = input.lines().collect();
        if numeric {
            lines.sort_by(|a, b| {
                let na: f64 = a.trim().parse().unwrap_or(0.0);
                let nb: f64 = b.trim().parse().unwrap_or(0.0);
                na.partial_cmp(&nb).unwrap_or(std::cmp::Ordering::Equal)
            });
        } else {
            lines.sort();
        }
        if reverse {
            lines.reverse();
        }
        (lines.join("\n"), 0)
    }

    pub fn cmd_uniq(&self, args: &[String], stdin: Option<&str>) -> (String, i32) {
        let mut count = false;
        let mut file = None;

        let mut parser = lexopt::Parser::from_args(args.iter().cloned());
        loop {
            match parser.next() {
                Ok(Some(lexopt::Arg::Short('c'))) => count = true,
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
                    out.push(if count {
                        format!("{:>7} {}", cnt, p)
                    } else {
                        p.to_string()
                    });
                }
                prev = Some(line);
                cnt = 1;
            }
        }
        if let Some(p) = prev {
            out.push(if count {
                format!("{:>7} {}", cnt, p)
            } else {
                p.to_string()
            });
        }

        (out.join("\n"), 0)
    }

    pub fn cmd_cut(&self, args: &[String], stdin: Option<&str>) -> (String, i32) {
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

    pub fn cmd_tr(&self, _args: &[String], stdin: Option<&str>) -> (String, i32) {
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

    pub fn cmd_rev(&self, args: &[String], stdin: Option<&str>) -> (String, i32) {
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

    pub fn cmd_seq(&self, args: &[String]) -> (String, i32) {
        let nums: Vec<i64> = args.iter().filter_map(|a| a.parse().ok()).collect();
        let (start, end) = match nums.len() {
            1 => (1, nums[0]),
            2 => (nums[0], nums[1]),
            _ => return ("seq: missing operand".into(), 1),
        };

        let mut results = Vec::new();
        if start <= end {
            let mut i = start;
            while i <= end {
                results.push(i.to_string());
                i += 1;
            }
        } else {
            let mut i = start;
            while i >= end {
                results.push(i.to_string());
                i -= 1;
            }
        }

        (results.join("\n"), 0)
    }

    pub fn cmd_tac(&self, args: &[String], stdin: Option<&str>) -> (String, i32) {
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

    pub fn cmd_nl(&self, args: &[String], stdin: Option<&str>) -> (String, i32) {
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

    pub fn cmd_paste(&self, args: &[String], stdin: Option<&str>) -> (String, i32) {
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

    // -- helpers --

    /// Resolve input from a file path or stdin
    pub(crate) fn resolve_input(
        &self,
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
        let mut parser = lexopt::Parser::from_args(args.iter().cloned());
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
        &self,
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
