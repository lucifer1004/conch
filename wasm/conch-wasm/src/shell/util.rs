// ---------------------------------------------------------------------------
// Standalone utility free-functions used across shell modules.
// ---------------------------------------------------------------------------

/// Convert a "decimal-encoded octal" mode (e.g., 755 as u16) to actual octal (0o755).
/// Users write `mode: 755` in Typst, which JSON encodes as decimal 755.
/// We reinterpret the digits: 7*64 + 5*8 + 5 = 493 = 0o755.
pub fn parse_mode_digits(decimal: u16) -> u16 {
    let d2 = decimal / 100;
    let d1 = (decimal / 10) % 10;
    let d0 = decimal % 10;
    d2 * 64 + d1 * 8 + d0
}

// ---------------------------------------------------------------------------
// Brace expansion: {a,b,c} and {N..M[..S]} / {a..z}
// ---------------------------------------------------------------------------

/// Expand brace expressions in a single word, returning one or more words.
/// Handles comma form `pre{a,b,c}post`, range form `{1..5}`, `{a..z}`,
/// `{1..10..2}`, and nested braces. `${...}` is NOT treated as brace expansion.
pub fn expand_braces(word: &str) -> Vec<String> {
    // Find the first `{` that is NOT preceded by `$` (to distinguish from ${var})
    let bytes = word.as_bytes();
    let mut start = None;
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'{' && !(i > 0 && bytes[i - 1] == b'$') {
            start = Some(i);
            break;
        }
        i += 1;
    }
    let open = match start {
        Some(o) => o,
        None => return vec![word.to_string()],
    };

    // Find the matching `}` respecting nesting
    let mut depth = 0u32;
    let mut close = None;
    for (j, &byte) in bytes.iter().enumerate().skip(open) {
        match byte {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    close = Some(j);
                    break;
                }
            }
            _ => {}
        }
    }
    let close = match close {
        Some(c) => c,
        None => return vec![word.to_string()],
    };

    let prefix = &word[..open];
    let inner = &word[open + 1..close];
    let suffix = &word[close + 1..];

    // Try range form first: N..M or N..M..S or a..z
    if let Some(range_items) = try_brace_range(inner) {
        let mut result = Vec::new();
        for item in &range_items {
            let combined = format!("{}{}{}", prefix, item, suffix);
            // Recursively expand suffix braces
            result.extend(expand_braces(&combined));
        }
        return result;
    }

    // Comma form: split on top-level commas (respecting nested braces)
    let parts = split_brace_commas(inner);
    if parts.len() <= 1 {
        // No commas found at top level — not a valid brace expansion
        return vec![word.to_string()];
    }

    let mut result = Vec::new();
    for part in &parts {
        let combined = format!("{}{}{}", prefix, part, suffix);
        // Recursively expand for nested braces
        result.extend(expand_braces(&combined));
    }
    result
}

/// Split the inner content of a brace expression on top-level commas,
/// respecting nested `{...}` pairs.
fn split_brace_commas(s: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut depth = 0u32;
    for c in s.chars() {
        match c {
            '{' => {
                depth += 1;
                current.push(c);
            }
            '}' => {
                depth = depth.saturating_sub(1);
                current.push(c);
            }
            ',' if depth == 0 => {
                parts.push(current.clone());
                current.clear();
            }
            _ => current.push(c),
        }
    }
    parts.push(current);
    parts
}

/// Try to parse a brace range expression: `N..M`, `N..M..S`, `a..z`, `a..z..S`.
fn try_brace_range(inner: &str) -> Option<Vec<String>> {
    let segments: Vec<&str> = inner.split("..").collect();
    if segments.len() < 2 || segments.len() > 3 {
        return None;
    }

    // Character range: single char on both sides
    let a = segments[0].trim();
    let b = segments[1].trim();

    if a.len() == 1
        && b.len() == 1
        && a.chars().next()?.is_ascii_alphabetic()
        && b.chars().next()?.is_ascii_alphabetic()
    {
        let ca = a.chars().next()? as i32;
        let cb = b.chars().next()? as i32;
        let step = if segments.len() == 3 {
            segments[2].trim().parse::<i32>().ok()?.max(1)
        } else {
            1
        };
        let mut items = Vec::new();
        if ca <= cb {
            let mut v = ca;
            while v <= cb {
                items.push((v as u8 as char).to_string());
                v += step;
            }
        } else {
            let mut v = ca;
            while v >= cb {
                items.push((v as u8 as char).to_string());
                v -= step;
            }
        }
        return Some(items);
    }

    // Numeric range
    let na = a.parse::<i64>().ok()?;
    let nb = b.parse::<i64>().ok()?;
    let step = if segments.len() == 3 {
        let s = segments[2].trim().parse::<i64>().ok()?;
        if s == 0 {
            return None;
        }
        s.abs()
    } else {
        1
    };

    let mut items = Vec::new();
    if na <= nb {
        let mut v = na;
        while v <= nb {
            items.push(v.to_string());
            v += step;
        }
    } else {
        let mut v = na;
        while v >= nb {
            items.push(v.to_string());
            v -= step;
        }
    }
    Some(items)
}

/// Process `$'...'` ANSI-C quoting escape sequences. The input should be
/// the content between `$'` and the closing `'`.
pub fn process_ansi_c_escapes(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\\' && i + 1 < bytes.len() {
            i += 1;
            match bytes[i] {
                b'n' => {
                    result.push('\n');
                    i += 1;
                }
                b't' => {
                    result.push('\t');
                    i += 1;
                }
                b'r' => {
                    result.push('\r');
                    i += 1;
                }
                b'a' => {
                    result.push('\x07');
                    i += 1;
                }
                b'b' => {
                    result.push('\x08');
                    i += 1;
                }
                b'e' | b'E' => {
                    result.push('\x1B');
                    i += 1;
                }
                b'f' => {
                    result.push('\x0C');
                    i += 1;
                }
                b'v' => {
                    result.push('\x0B');
                    i += 1;
                }
                b'\\' => {
                    result.push('\\');
                    i += 1;
                }
                b'\'' => {
                    result.push('\'');
                    i += 1;
                }
                b'"' => {
                    result.push('"');
                    i += 1;
                }
                b'0' => {
                    // Octal: \0, \0N, \0NN, \0NNN
                    i += 1;
                    let mut val = 0u8;
                    let mut count = 0;
                    while count < 3 && i < bytes.len() && bytes[i] >= b'0' && bytes[i] <= b'7' {
                        val = val * 8 + (bytes[i] - b'0');
                        i += 1;
                        count += 1;
                    }
                    result.push(val as char);
                }
                b'x' => {
                    // Hex: \xHH
                    i += 1;
                    let mut val = 0u8;
                    let mut count = 0;
                    while count < 2 && i < bytes.len() {
                        let d = match bytes[i] {
                            b'0'..=b'9' => bytes[i] - b'0',
                            b'a'..=b'f' => bytes[i] - b'a' + 10,
                            b'A'..=b'F' => bytes[i] - b'A' + 10,
                            _ => break,
                        };
                        val = val * 16 + d;
                        i += 1;
                        count += 1;
                    }
                    result.push(val as char);
                }
                _ => {
                    // Unknown escape — keep as-is
                    result.push('\\');
                    result.push(bytes[i] as char);
                    i += 1;
                }
            }
        } else {
            result.push(bytes[i] as char);
            i += 1;
        }
    }
    result
}

/// Process a token that might be `$'...'` ANSI-C quoted.
/// If the token starts with `$'` and ends with `'`, strip the delimiters
/// and process escape sequences. Otherwise return unchanged.
pub fn process_dollar_single_quote(s: &str) -> String {
    if let Some(inner) = s.strip_prefix("$'").and_then(|r| r.strip_suffix('\'')) {
        process_ansi_c_escapes(inner)
    } else {
        s.to_string()
    }
}

/// Split a string into tokens respecting single and double quotes.
/// Replaces `shlex::split` — returns `None` if quotes are unterminated.
pub fn split_shell_words(s: &str) -> Option<Vec<String>> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        match c {
            b' ' | b'\t' => {
                if !current.is_empty() {
                    tokens.push(std::mem::take(&mut current));
                }
                i += 1;
            }
            b'\'' => {
                i += 1;
                while i < bytes.len() && bytes[i] != b'\'' {
                    current.push(bytes[i] as char);
                    i += 1;
                }
                if i >= bytes.len() {
                    return None; // unterminated
                }
                i += 1; // skip closing '
            }
            b'"' => {
                i += 1;
                while i < bytes.len() && bytes[i] != b'"' {
                    if bytes[i] == b'\\' && i + 1 < bytes.len() {
                        let next = bytes[i + 1];
                        if matches!(next, b'"' | b'\\' | b'$' | b'`') {
                            current.push(next as char);
                            i += 2;
                        } else {
                            current.push('\\');
                            current.push(next as char);
                            i += 2;
                        }
                    } else {
                        current.push(bytes[i] as char);
                        i += 1;
                    }
                }
                if i >= bytes.len() {
                    return None; // unterminated
                }
                i += 1; // skip closing "
            }
            b'\\' if i + 1 < bytes.len() => {
                current.push(bytes[i + 1] as char);
                i += 2;
            }
            _ => {
                current.push(c as char);
                i += 1;
            }
        }
    }
    if !current.is_empty() {
        tokens.push(current);
    }
    Some(tokens)
}

/// Check if a string has unterminated single or double quotes.
pub fn has_unterminated_quote(s: &str) -> bool {
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        // Backslash escape outside quotes
        if c == b'\\' && i + 1 < bytes.len() {
            i += 2;
            continue;
        }
        // $'...' — ANSI-C quoting (supports \' inside)
        if c == b'$' && i + 1 < bytes.len() && bytes[i + 1] == b'\'' {
            i += 2; // skip $'
            while i < bytes.len() {
                if bytes[i] == b'\\' && i + 1 < bytes.len() {
                    i += 2; // skip escape sequence (including \')
                    continue;
                }
                if bytes[i] == b'\'' {
                    break; // found closing '
                }
                i += 1;
            }
            if i >= bytes.len() {
                return true; // unterminated $'...'
            }
            i += 1; // skip closing '
            continue;
        }
        // Single-quoted string
        if c == b'\'' {
            i += 1;
            while i < bytes.len() && bytes[i] != b'\'' {
                i += 1;
            }
            if i >= bytes.len() {
                return true; // unterminated '...'
            }
            i += 1; // skip closing '
            continue;
        }
        // Double-quoted string
        if c == b'"' {
            i += 1;
            while i < bytes.len() {
                if bytes[i] == b'\\' && i + 1 < bytes.len() {
                    i += 2;
                    continue;
                }
                if bytes[i] == b'"' {
                    break;
                }
                i += 1;
            }
            if i >= bytes.len() {
                return true; // unterminated "..."
            }
            i += 1; // skip closing "
            continue;
        }
        i += 1;
    }
    false
}
