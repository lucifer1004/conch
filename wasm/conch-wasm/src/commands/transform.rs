use crate::shell::Shell;
use regex_lite::Regex;

impl Shell {
    /// diff [-u] [-q]: compare two files line by line, producing normal or unified diff output.
    pub fn cmd_diff(&mut self, args: &[String]) -> (String, i32) {
        let mut unified = false;
        let mut brief = false;
        let mut files = Vec::new();
        for arg in args {
            match arg.as_str() {
                "-u" => unified = true,
                "-q" => brief = true,
                s if s.starts_with('-') => {}
                _ => files.push(arg),
            }
        }
        if files.len() < 2 {
            return ("diff: missing operand".into(), 2);
        }

        let path_a = self.resolve(files[0]);
        let path_b = self.resolve(files[1]);

        let content_a = match self.fs.read_to_string(&path_a) {
            Ok(s) => s.to_string(),
            Err(e) => return (format!("diff: {}: {}", files[0], e), 2),
        };
        let content_b = match self.fs.read_to_string(&path_b) {
            Ok(s) => s.to_string(),
            Err(e) => return (format!("diff: {}: {}", files[1], e), 2),
        };

        if brief {
            if content_a == content_b {
                return (String::new(), 0);
            } else {
                return (format!("Files {} and {} differ", files[0], files[1]), 1);
            }
        }

        let lines_a: Vec<&str> = content_a.lines().collect();
        let lines_b: Vec<&str> = content_b.lines().collect();

        // Compute LCS table
        let lcs = compute_lcs(&lines_a, &lines_b);

        // Generate edit script from LCS
        let edits = build_edit_script(&lines_a, &lines_b, &lcs);

        let has_changes = edits.iter().any(|(op, _, _)| *op != EditOp::Equal);
        if !has_changes {
            return (String::new(), 0);
        }

        let out = if unified {
            format_unified_diff(&lines_a, &lines_b, &edits, files[0], files[1])
        } else {
            format_normal_diff(&lines_a, &lines_b, &edits)
        };

        (out, 1)
    }

    /// sed [-i] [-n] [-E] [-e EXPR] [file]: stream editor — apply substitution, delete, print, append, insert, and change commands to each line.
    pub fn cmd_sed(&mut self, args: &[String], stdin: Option<&str>) -> (String, i32) {
        let mut in_place = false;
        let mut suppress_print = false;
        let mut use_regex = false;
        let mut expressions: Vec<String> = Vec::new();
        let mut file: Option<String> = None;

        let mut i = 0;
        while i < args.len() {
            match args[i].as_str() {
                "-i" => {
                    in_place = true;
                    i += 1;
                }
                "-n" => {
                    suppress_print = true;
                    i += 1;
                }
                "-E" | "-r" => {
                    use_regex = true;
                    i += 1;
                }
                "-e" if i + 1 < args.len() => {
                    expressions.push(args[i + 1].clone());
                    i += 2;
                }
                s if !s.starts_with('-') && expressions.is_empty() => {
                    expressions.push(args[i].clone());
                    i += 1;
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

        if expressions.is_empty() {
            return ("sed: missing expression".into(), 1);
        }

        // Parse all expressions up front; semicolons split into multiple commands
        let parsed: Vec<SedInstruction> = {
            let mut v = Vec::new();
            for expr in &expressions {
                // Split on unescaped semicolons
                for sub_expr in split_sed_semicolons(expr) {
                    let sub_expr = sub_expr.trim();
                    if sub_expr.is_empty() {
                        continue;
                    }
                    match parse_sed_expr(sub_expr) {
                        Some(r) => v.push(r),
                        None => return (format!("sed: invalid expression: {}", sub_expr), 1),
                    }
                }
            }
            v
        };

        let input = match self.resolve_input(file.as_deref(), stdin) {
            Ok(s) => s,
            Err(e) => return (format!("sed: {}", e), 1),
        };

        let all_lines: Vec<&str> = input.lines().collect();
        let total_lines = all_lines.len();
        let mut output_lines: Vec<String> = Vec::new();

        for (line_num, line) in all_lines.iter().enumerate() {
            let line_1based = line_num + 1;
            let mut current = line.to_string();
            let mut deleted = false;
            let mut explicitly_printed = false;

            let mut append_after: Vec<String> = Vec::new();

            for instr in &parsed {
                if !addr_matches(&instr.addr, line_1based, &current, total_lines) {
                    continue;
                }
                match &instr.cmd {
                    SedCommand::Delete => {
                        deleted = true;
                        break;
                    }
                    SedCommand::Print => {
                        explicitly_printed = true;
                    }
                    SedCommand::Subst {
                        pattern,
                        replacement,
                        global,
                        print,
                        use_re,
                    } => {
                        let actually_use_re = use_regex || *use_re;
                        let before = current.clone();
                        current = apply_sed_subst(
                            &current,
                            pattern,
                            replacement,
                            *global,
                            actually_use_re,
                        );
                        if *print && current != before {
                            explicitly_printed = true;
                        }
                    }
                    SedCommand::Append(text) => {
                        append_after.push(text.clone());
                    }
                    SedCommand::Insert(text) => {
                        output_lines.push(text.clone());
                    }
                    SedCommand::Change(text) => {
                        current = text.clone();
                    }
                }
            }

            if deleted {
                continue;
            }

            if suppress_print {
                if explicitly_printed {
                    output_lines.push(current);
                }
            } else {
                output_lines.push(current);
            }

            for appended in append_after {
                output_lines.push(appended);
            }
        }

        let output = output_lines.join("\n");

        if in_place {
            if let Some(ref f) = file {
                let path = self.resolve(f);
                if let Err(e) = self.fs.write(&path, output.as_bytes()) {
                    return (format!("sed: {}: {}", f, e), 1);
                }
                return (String::new(), 0);
            }
        }

        (output, 0)
    }

    /// xxd [file]: produce a hex dump of a file or stdin with offset, hex pairs, and ASCII columns.
    pub fn cmd_xxd(&mut self, args: &[String], stdin: Option<&str>) -> (String, i32) {
        let file_arg = args.iter().find(|a| !a.starts_with('-'));

        let bytes: Vec<u8> = if let Some(f) = file_arg {
            let path = self.resolve(f);
            match self.fs.read(&path) {
                Ok(b) => b.to_vec(),
                Err(e) => return (format!("xxd: {}: {}", f, e), 1),
            }
        } else if let Some(input) = stdin {
            input.as_bytes().to_vec()
        } else {
            return ("xxd: missing file operand".into(), 1);
        };

        let mut out = Vec::new();
        let mut offset = 0usize;

        for chunk in bytes.chunks(16) {
            // Hex part: groups of 2 bytes separated by space
            let hex_pairs: Vec<String> = chunk
                .chunks(2)
                .map(|pair| {
                    pair.iter()
                        .map(|b| format!("{:02x}", b))
                        .collect::<String>()
                })
                .collect();
            let hex_str = hex_pairs.join(" ");

            // ASCII part
            let ascii: String = chunk
                .iter()
                .map(|&b| {
                    if (0x20..0x7f).contains(&b) {
                        b as char
                    } else {
                        '.'
                    }
                })
                .collect();

            out.push(format!("{:08x}: {:<48}  {}", offset, hex_str, ascii));
            offset += chunk.len();
        }

        (out.join("\n"), 0)
    }

    /// base64 [-d] [file]: encode or decode data using Base64 from a file or stdin.
    pub fn cmd_base64(&mut self, args: &[String], stdin: Option<&str>) -> (String, i32) {
        let mut decode = false;
        let mut file: Option<String> = None;

        for arg in args {
            match arg.as_str() {
                "-d" | "--decode" => decode = true,
                s if !s.starts_with('-') => file = Some(arg.clone()),
                _ => {}
            }
        }

        if decode {
            let input = match self.resolve_input(file.as_deref(), stdin) {
                Ok(s) => s,
                Err(e) => return (format!("base64: {}", e), 1),
            };
            let cleaned: String = input.chars().filter(|c| !c.is_whitespace()).collect();
            match base64_decode(&cleaned) {
                Ok(bytes) => {
                    let s = String::from_utf8(bytes).unwrap_or_else(|_| String::new());
                    (s, 0)
                }
                Err(e) => (format!("base64: invalid input: {}", e), 1),
            }
        } else {
            let input = match self.resolve_input(file.as_deref(), stdin) {
                Ok(s) => s,
                Err(e) => return (format!("base64: {}", e), 1),
            };
            (base64_encode(input.as_bytes()), 0)
        }
    }
}

// -- sed data structures --

/// Sed command types: substitution, delete, print, append, insert, change.
enum SedCommand {
    Subst {
        pattern: String,
        replacement: String,
        global: bool,
        print: bool,
        use_re: bool,
    },
    Delete,
    Print,
    Append(String),
    Insert(String),
    Change(String),
}

/// A parsed sed instruction with optional address and command.
struct SedInstruction {
    addr: Option<SedAddr>,
    cmd: SedCommand,
}

/// Address selector for sed commands: line number, pattern, or range.
enum SedAddr {
    Line(usize),
    LastLine,
    Range(usize, usize),
    RangeToPattern(usize, String),
    Pattern(String),
}

/// Check if an address matches the current line.
fn addr_matches(
    addr: &Option<SedAddr>,
    line_num: usize,
    line_content: &str,
    total_lines: usize,
) -> bool {
    match addr {
        None => true,
        Some(SedAddr::Line(n)) => line_num == *n,
        Some(SedAddr::LastLine) => line_num == total_lines,
        Some(SedAddr::Range(start, end)) => line_num >= *start && line_num <= *end,
        Some(SedAddr::RangeToPattern(start, pat)) => {
            if line_num < *start {
                return false;
            }
            if line_num == *start {
                return true;
            }
            // After start: match until pattern is found (inclusive)
            if let Ok(re) = Regex::new(pat) {
                re.is_match(line_content)
            } else {
                line_content.contains(pat.as_str())
            }
        }
        Some(SedAddr::Pattern(pat)) => {
            if let Ok(re) = Regex::new(pat) {
                re.is_match(line_content)
            } else {
                line_content.contains(pat.as_str())
            }
        }
    }
}

/// Apply a sed substitution to a line.
/// Convert sed-style backreferences (\1, \2, etc.) to regex-lite style ($1, $2, etc.)
fn convert_sed_backrefs(replacement: &str) -> String {
    let mut result = String::with_capacity(replacement.len());
    let mut chars = replacement.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.peek() {
                Some(&d) if d.is_ascii_digit() => {
                    result.push('$');
                    result.push(d);
                    chars.next();
                }
                _ => {
                    result.push('\\');
                }
            }
        } else {
            result.push(c);
        }
    }
    result
}

fn apply_sed_subst(
    line: &str,
    pattern: &str,
    replacement: &str,
    global: bool,
    use_regex: bool,
) -> String {
    if use_regex {
        if let Ok(re) = Regex::new(pattern) {
            let repl = convert_sed_backrefs(replacement);
            if global {
                re.replace_all(line, repl.as_str()).to_string()
            } else {
                re.replace(line, repl.as_str()).to_string()
            }
        } else {
            apply_sed_subst_literal(line, pattern, replacement, global)
        }
    } else {
        apply_sed_subst_literal(line, pattern, replacement, global)
    }
}

fn apply_sed_subst_literal(line: &str, pattern: &str, replacement: &str, global: bool) -> String {
    if global {
        line.replace(pattern, replacement)
    } else if let Some(pos) = line.find(pattern) {
        format!(
            "{}{}{}",
            &line[..pos],
            replacement,
            &line[pos + pattern.len()..]
        )
    } else {
        line.to_string()
    }
}

/// Find the index of the next occurrence of `delim` in `s` that is not preceded by `\`.
/// Returns the byte index into `s`.
fn find_unescaped(s: &str, delim: char) -> Option<usize> {
    let mut chars = s.char_indices().peekable();
    while let Some((idx, c)) = chars.next() {
        if c == '\\' {
            chars.next();
        } else if c == delim {
            return Some(idx);
        }
    }
    None
}

/// Unescape delimiter sequences like `\/` -> `/` in sed patterns/replacements.
fn unescape_delim(s: &str, delim: char) -> String {
    let mut result = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.peek().copied() {
                Some(next) if next == delim => {
                    result.push(delim);
                    chars.next();
                }
                _ => {
                    result.push('\\');
                }
            }
        } else {
            result.push(c);
        }
    }
    result
}

/// Split a sed expression on unescaped semicolons, respecting s/.../.../
/// and /pattern/ delimiters so we don't split inside them.
fn split_sed_semicolons(expr: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut chars = expr.chars().peekable();

    while let Some(c) = chars.next() {
        match c {
            '\\' => {
                // Escaped character: consume next char verbatim
                current.push(c);
                if let Some(next) = chars.next() {
                    current.push(next);
                }
            }
            ';' => {
                parts.push(current.clone());
                current.clear();
            }
            's' if current.trim().is_empty() || current.trim().ends_with(';') => {
                // Start of substitution command: consume s<delim>...<delim>...<delim>[flags]
                current.push(c);
                if let Some(delim) = chars.next() {
                    current.push(delim);
                    // Consume pattern (up to unescaped delim)
                    loop {
                        match chars.next() {
                            None => break,
                            Some('\\') => {
                                current.push('\\');
                                if let Some(n) = chars.next() {
                                    current.push(n);
                                }
                            }
                            Some(ch) if ch == delim => {
                                current.push(ch);
                                break;
                            }
                            Some(ch) => current.push(ch),
                        }
                    }
                    // Consume replacement
                    loop {
                        match chars.next() {
                            None => break,
                            Some('\\') => {
                                current.push('\\');
                                if let Some(n) = chars.next() {
                                    current.push(n);
                                }
                            }
                            Some(ch) if ch == delim => {
                                current.push(ch);
                                break;
                            }
                            Some(ch) => current.push(ch),
                        }
                    }
                    // Consume flags (letters, not semicolon)
                    while !matches!(chars.peek(), Some(&';') | None) {
                        if let Some(ch) = chars.next() {
                            current.push(ch);
                        }
                    }
                }
            }
            '/' => {
                // Pattern address: consume /pattern/
                current.push(c);
                loop {
                    match chars.next() {
                        None => break,
                        Some('\\') => {
                            current.push('\\');
                            if let Some(n) = chars.next() {
                                current.push(n);
                            }
                        }
                        Some('/') => {
                            current.push('/');
                            break;
                        }
                        Some(ch) => current.push(ch),
                    }
                }
            }
            _ => current.push(c),
        }
    }
    if !current.is_empty() {
        parts.push(current);
    }
    parts
}

/// Parse a sed expression into a `SedInstruction`.
/// Supports:
///   s<delim>pat<delim>repl<delim>[gp]  -- substitution (alternate delimiters)
///   /pattern/d                          -- delete matching lines
///   /pattern/p                          -- print matching lines
///   Nd                                  -- delete line N
///   N,Md                                -- delete lines N through M
fn parse_sed_expr(expr: &str) -> Option<SedInstruction> {
    if expr.is_empty() {
        return None;
    }

    let (addr, rest) = parse_sed_address(expr)?;

    if rest.is_empty() {
        return None;
    }

    let first = rest.as_bytes()[0];

    // Substitution command: s<delim>pattern<delim>replacement<delim>[flags]
    if first == b's' && rest.len() >= 2 {
        let delim = rest.as_bytes()[1] as char;
        let after_s = &rest[2..];
        let pat_end = find_unescaped(after_s, delim)?;
        let pattern = unescape_delim(&after_s[..pat_end], delim);
        let after_pat = &after_s[pat_end + 1..];
        let repl_end = find_unescaped(after_pat, delim)?;
        let replacement = unescape_delim(&after_pat[..repl_end], delim);
        let flags = &after_pat[repl_end + 1..];
        let global = flags.contains('g');
        let print = flags.contains('p');
        return Some(SedInstruction {
            addr,
            cmd: SedCommand::Subst {
                pattern,
                replacement,
                global,
                print,
                use_re: false,
            },
        });
    }

    // Delete command
    if first == b'd' {
        return Some(SedInstruction {
            addr,
            cmd: SedCommand::Delete,
        });
    }

    // Print command
    if first == b'p' {
        return Some(SedInstruction {
            addr,
            cmd: SedCommand::Print,
        });
    }

    // Append command: a\TEXT
    if first == b'a' && rest.len() >= 2 && rest.as_bytes()[1] == b'\\' {
        let text = rest[2..].to_string();
        return Some(SedInstruction {
            addr,
            cmd: SedCommand::Append(text),
        });
    }

    // Insert command: i\TEXT
    if first == b'i' && rest.len() >= 2 && rest.as_bytes()[1] == b'\\' {
        let text = rest[2..].to_string();
        return Some(SedInstruction {
            addr,
            cmd: SedCommand::Insert(text),
        });
    }

    // Change command: c\TEXT
    if first == b'c' && rest.len() >= 2 && rest.as_bytes()[1] == b'\\' {
        let text = rest[2..].to_string();
        return Some(SedInstruction {
            addr,
            cmd: SedCommand::Change(text),
        });
    }

    None
}

/// Parse an optional address prefix from a sed expression.
fn parse_sed_address(expr: &str) -> Option<(Option<SedAddr>, &str)> {
    if expr.is_empty() {
        return Some((None, expr));
    }

    let bytes = expr.as_bytes();

    // $ address: last line
    if bytes[0] == b'$' && (bytes.len() == 1 || !bytes[1].is_ascii_digit()) {
        return Some((Some(SedAddr::LastLine), &expr[1..]));
    }

    // Pattern address: /pattern/<cmd>
    if bytes[0] == b'/' {
        let rest = &expr[1..];
        let end = find_unescaped(rest, '/')?;
        let pattern = rest[..end].to_string();
        let after = &rest[end + 1..];
        return Some((Some(SedAddr::Pattern(pattern)), after));
    }

    // Numeric address: N or N,M or N,/pattern/
    if bytes[0].is_ascii_digit() {
        let mut i = 0;
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            i += 1;
        }
        let n: usize = expr[..i].parse().ok()?;

        if i < bytes.len() && bytes[i] == b',' {
            let after_comma = &expr[i + 1..];
            let ab = after_comma.as_bytes();

            // N,/pattern/ range
            if !ab.is_empty() && ab[0] == b'/' {
                let rest = &after_comma[1..];
                let end = find_unescaped(rest, '/')?;
                let pattern = rest[..end].to_string();
                let after = &rest[end + 1..];
                return Some((Some(SedAddr::RangeToPattern(n, pattern)), after));
            }

            // N,M numeric range
            let mut j = 0;
            while j < ab.len() && ab[j].is_ascii_digit() {
                j += 1;
            }
            if j == 0 {
                return None;
            }
            let m: usize = after_comma[..j].parse().ok()?;
            return Some((Some(SedAddr::Range(n, m)), &after_comma[j..]));
        }

        return Some((Some(SedAddr::Line(n)), &expr[i..]));
    }

    // No address prefix
    Some((None, expr))
}

const BASE64_CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn base64_encode(input: &[u8]) -> String {
    let mut out = String::new();
    let mut i = 0;
    while i < input.len() {
        let b0 = input[i] as u32;
        let b1 = if i + 1 < input.len() {
            input[i + 1] as u32
        } else {
            0
        };
        let b2 = if i + 2 < input.len() {
            input[i + 2] as u32
        } else {
            0
        };

        let triple = (b0 << 16) | (b1 << 8) | b2;

        out.push(BASE64_CHARS[((triple >> 18) & 0x3F) as usize] as char);
        out.push(BASE64_CHARS[((triple >> 12) & 0x3F) as usize] as char);
        if i + 1 < input.len() {
            out.push(BASE64_CHARS[((triple >> 6) & 0x3F) as usize] as char);
        } else {
            out.push('=');
        }
        if i + 2 < input.len() {
            out.push(BASE64_CHARS[(triple & 0x3F) as usize] as char);
        } else {
            out.push('=');
        }

        i += 3;
    }
    out
}

fn base64_char_value(c: char) -> Option<u8> {
    match c {
        'A'..='Z' => Some(c as u8 - b'A'),
        'a'..='z' => Some(c as u8 - b'a' + 26),
        '0'..='9' => Some(c as u8 - b'0' + 52),
        '+' => Some(62),
        '/' => Some(63),
        _ => None,
    }
}

fn base64_decode(input: &str) -> Result<Vec<u8>, &'static str> {
    if !input.len().is_multiple_of(4) {
        return Err("invalid length");
    }
    let mut out = Vec::new();
    let chars: Vec<char> = input.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c0 = base64_char_value(chars[i]).ok_or("invalid char")?;
        let c1 = base64_char_value(chars[i + 1]).ok_or("invalid char")?;
        out.push((c0 << 2) | (c1 >> 4));

        if chars[i + 2] != '=' {
            let c2 = base64_char_value(chars[i + 2]).ok_or("invalid char")?;
            out.push(((c1 & 0x0F) << 4) | (c2 >> 2));
            if chars[i + 3] != '=' {
                let c3 = base64_char_value(chars[i + 3]).ok_or("invalid char")?;
                out.push(((c2 & 0x03) << 6) | c3);
            }
        }
        i += 4;
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// LCS-based diff helpers
// ---------------------------------------------------------------------------

/// Diff edit operation: equal, delete, or insert.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EditOp {
    Equal,
    Delete, // line in A only
    Insert, // line in B only
}

/// Compute the LCS length table (O(n*m) DP).
fn compute_lcs(a: &[&str], b: &[&str]) -> Vec<Vec<usize>> {
    let n = a.len();
    let m = b.len();
    let mut dp = vec![vec![0usize; m + 1]; n + 1];
    for i in 1..=n {
        for j in 1..=m {
            if a[i - 1] == b[j - 1] {
                dp[i][j] = dp[i - 1][j - 1] + 1;
            } else {
                dp[i][j] = dp[i - 1][j].max(dp[i][j - 1]);
            }
        }
    }
    dp
}

/// Backtrack through the LCS table to produce a sequence of edit operations.
fn build_edit_script<'a>(
    a: &[&'a str],
    b: &[&'a str],
    dp: &[Vec<usize>],
) -> Vec<(EditOp, usize, usize)> {
    let mut edits = Vec::new();
    let mut i = a.len();
    let mut j = b.len();

    while i > 0 || j > 0 {
        if i > 0 && j > 0 && a[i - 1] == b[j - 1] {
            edits.push((EditOp::Equal, i - 1, j - 1));
            i -= 1;
            j -= 1;
        } else if j > 0 && (i == 0 || dp[i][j - 1] >= dp[i - 1][j]) {
            edits.push((EditOp::Insert, i, j - 1));
            j -= 1;
        } else {
            edits.push((EditOp::Delete, i - 1, j));
            i -= 1;
        }
    }

    edits.reverse();
    edits
}

/// Format diff output in normal diff format (NcN, NdN, NaN).
fn format_normal_diff(a: &[&str], b: &[&str], edits: &[(EditOp, usize, usize)]) -> String {
    // Group consecutive edits into hunks
    let mut out = Vec::new();
    let mut idx = 0;

    while idx < edits.len() {
        if edits[idx].0 == EditOp::Equal {
            idx += 1;
            continue;
        }

        // Collect a contiguous run of non-Equal edits
        let start = idx;
        while idx < edits.len() && edits[idx].0 != EditOp::Equal {
            idx += 1;
        }
        let hunk = &edits[start..idx];

        let deletes: Vec<usize> = hunk
            .iter()
            .filter(|e| e.0 == EditOp::Delete)
            .map(|e| e.1)
            .collect();
        let inserts: Vec<usize> = hunk
            .iter()
            .filter(|e| e.0 == EditOp::Insert)
            .map(|e| e.2)
            .collect();

        if !deletes.is_empty() && !inserts.is_empty() {
            // Change
            let a_range = format_range_normal(&deletes);
            let b_range = format_range_normal(&inserts);
            out.push(format!("{}c{}", a_range, b_range));
            for &di in &deletes {
                out.push(format!("< {}", a[di]));
            }
            out.push("---".to_string());
            for &ii in &inserts {
                out.push(format!("> {}", b[ii]));
            }
        } else if !deletes.is_empty() {
            // Delete
            let a_range = format_range_normal(&deletes);
            let after = if !inserts.is_empty() {
                inserts[0]
            } else {
                // The line in b after which the deletion would appear
                if start > 0 {
                    match edits[start - 1].0 {
                        EditOp::Equal => edits[start - 1].2 + 1,
                        EditOp::Insert => edits[start - 1].2 + 1,
                        _ => 0,
                    }
                } else {
                    0
                }
            };
            out.push(format!("{}d{}", a_range, after));
            for &di in &deletes {
                out.push(format!("< {}", a[di]));
            }
        } else if !inserts.is_empty() {
            // Add
            let b_range = format_range_normal(&inserts);
            let after = if start > 0 {
                match edits[start - 1].0 {
                    EditOp::Equal => edits[start - 1].1 + 1,
                    EditOp::Delete => edits[start - 1].1 + 1,
                    _ => 0,
                }
            } else {
                0
            };
            out.push(format!("{}a{}", after, b_range));
            for &ii in &inserts {
                out.push(format!("> {}", b[ii]));
            }
        }
    }

    out.join("\n")
}

fn format_range_normal(indices: &[usize]) -> String {
    if indices.len() == 1 {
        format!("{}", indices[0] + 1)
    } else {
        format!("{},{}", indices[0] + 1, indices[indices.len() - 1] + 1)
    }
}

/// Format diff output in unified format with --- / +++ / @@ markers.
fn format_unified_diff(
    a: &[&str],
    b: &[&str],
    edits: &[(EditOp, usize, usize)],
    name_a: &str,
    name_b: &str,
) -> String {
    let mut out = Vec::new();
    out.push(format!("--- {}", name_a));
    out.push(format!("+++ {}", name_b));

    // Group edits into hunks with 3 lines of context
    let context = 3usize;
    let mut hunks: Vec<(usize, usize)> = Vec::new(); // (start, end) indices into edits

    let mut idx = 0;
    while idx < edits.len() {
        if edits[idx].0 == EditOp::Equal {
            idx += 1;
            continue;
        }
        // Find start of change region, including context before
        let change_start = idx;
        while idx < edits.len() && edits[idx].0 != EditOp::Equal {
            idx += 1;
        }
        let change_end = idx;

        // Extend with context
        let ctx_start = change_start.saturating_sub(context);
        let ctx_end = (change_end + context).min(edits.len());

        if let Some(last) = hunks.last_mut() {
            if ctx_start <= last.1 {
                last.1 = ctx_end;
            } else {
                hunks.push((ctx_start, ctx_end));
            }
        } else {
            hunks.push((ctx_start, ctx_end));
        }
    }

    for (hunk_start, hunk_end) in hunks {
        let slice = &edits[hunk_start..hunk_end];
        // Compute line ranges
        let mut a_start = usize::MAX;
        let mut a_count = 0usize;
        let mut b_start = usize::MAX;
        let mut b_count = 0usize;

        for &(op, ai, bi) in slice {
            match op {
                EditOp::Equal => {
                    if a_start == usize::MAX {
                        a_start = ai;
                        b_start = bi;
                    }
                    a_count += 1;
                    b_count += 1;
                }
                EditOp::Delete => {
                    if a_start == usize::MAX {
                        a_start = ai;
                        b_start = bi;
                    }
                    a_count += 1;
                }
                EditOp::Insert => {
                    if a_start == usize::MAX {
                        a_start = ai;
                        b_start = bi;
                    }
                    b_count += 1;
                }
            }
        }

        out.push(format!(
            "@@ -{},{} +{},{} @@",
            a_start + 1,
            a_count,
            b_start + 1,
            b_count
        ));

        for &(op, ai, bi) in slice {
            match op {
                EditOp::Equal => out.push(format!(" {}", a[ai])),
                EditOp::Delete => out.push(format!("-{}", a[ai])),
                EditOp::Insert => out.push(format!("+{}", b[bi])),
            }
        }
    }

    out.join("\n")
}
