/// Word-level and command-line parser.
///
/// Two public entry points:
/// - `parse_word(raw)` — decompose a single raw word token into `Vec<WordPart>`
/// - `parse_command_line(input)` — parse a full command line into `CommandList`
use crate::script::word::*;

// ===========================================================================
// parse_word — decompose a single raw word token into structured parts
// ===========================================================================

/// Decompose a single raw word token (as produced by the existing tokenizer)
/// into a `Vec<WordPart>`.
pub fn parse_word(raw: &str) -> Word {
    let chars: Vec<char> = raw.chars().collect();
    let mut pos = 0;
    let mut parts = Vec::new();
    parse_word_parts(&chars, &mut pos, false, &mut parts);
    parts
}

/// Parse word parts from `chars[pos..]`.
/// If `in_double_quote` is true, we are inside `"..."` and only `$`, `\`, `` ` `` are special.
fn parse_word_parts(
    chars: &[char],
    pos: &mut usize,
    in_double_quote: bool,
    parts: &mut Vec<WordPart>,
) {
    let mut literal = String::new();

    while *pos < chars.len() {
        let c = chars[*pos];

        if in_double_quote && c == '"' {
            // End of double-quoted section — don't consume the closing quote
            break;
        }

        // ---------------------------------------------------------------
        // Backslash escape
        // ---------------------------------------------------------------
        if c == '\\' && *pos + 1 < chars.len() {
            if in_double_quote {
                // Inside double quotes, only \$, \`, \\, \" are special escapes
                let next = chars[*pos + 1];
                if matches!(next, '$' | '`' | '\\' | '"') {
                    literal.push('\\');
                    literal.push(next);
                    *pos += 2;
                    continue;
                }
                // Other backslash sequences are literal inside double quotes
                literal.push('\\');
                literal.push(next);
                *pos += 2;
                continue;
            }
            // Outside quotes: backslash escapes next char
            literal.push('\\');
            literal.push(chars[*pos + 1]);
            *pos += 2;
            continue;
        }

        // ---------------------------------------------------------------
        // Single quote (not inside double quotes)
        // ---------------------------------------------------------------
        if c == '\'' && !in_double_quote {
            flush_literal(&mut literal, parts);
            *pos += 1; // skip opening '
            let start = *pos;
            while *pos < chars.len() && chars[*pos] != '\'' {
                *pos += 1;
            }
            let content: String = chars[start..*pos].iter().collect();
            parts.push(WordPart::SingleQuoted(content.into()));
            if *pos < chars.len() {
                *pos += 1; // skip closing '
            }
            continue;
        }

        // ---------------------------------------------------------------
        // $'...' — ANSI-C quoting (not inside double quotes)
        // ---------------------------------------------------------------
        if c == '$' && !in_double_quote && *pos + 1 < chars.len() && chars[*pos + 1] == '\'' {
            flush_literal(&mut literal, parts);
            *pos += 2; // skip $'
            let start = *pos;
            while *pos < chars.len() && chars[*pos] != '\'' {
                if chars[*pos] == '\\' && *pos + 1 < chars.len() {
                    *pos += 2; // skip escaped char
                } else {
                    *pos += 1;
                }
            }
            let content: String = chars[start..*pos].iter().collect();
            parts.push(WordPart::DollarSingleQuoted(content.into()));
            if *pos < chars.len() {
                *pos += 1; // skip closing '
            }
            continue;
        }

        // ---------------------------------------------------------------
        // Double quote (not already inside double quotes)
        // ---------------------------------------------------------------
        if c == '"' && !in_double_quote {
            flush_literal(&mut literal, parts);
            *pos += 1; // skip opening "
            let mut inner_parts = Vec::new();
            parse_word_parts(chars, pos, true, &mut inner_parts);
            if *pos < chars.len() && chars[*pos] == '"' {
                *pos += 1; // skip closing "
            }
            parts.push(WordPart::DoubleQuoted(inner_parts));
            continue;
        }

        // ---------------------------------------------------------------
        // <(cmd) or >(cmd) — process substitution
        // ---------------------------------------------------------------
        if (c == '<' || c == '>')
            && !in_double_quote
            && *pos + 1 < chars.len()
            && chars[*pos + 1] == '('
        {
            flush_literal(&mut literal, parts);
            let dir = c;
            *pos += 2; // skip <( or >(
            let start = *pos;
            let mut depth: u32 = 1;
            while *pos < chars.len() && depth > 0 {
                match chars[*pos] {
                    '(' => depth += 1,
                    ')' => depth -= 1,
                    _ => {}
                }
                if depth > 0 {
                    *pos += 1;
                }
            }
            let content: String = chars[start..*pos].iter().collect();
            parts.push(WordPart::ProcessSubst {
                dir,
                cmd: content.into(),
            });
            if *pos < chars.len() && chars[*pos] == ')' {
                *pos += 1; // skip closing )
            }
            continue;
        }

        // ---------------------------------------------------------------
        // $((expr)) — arithmetic substitution (must check before $(...))
        // ---------------------------------------------------------------
        if c == '$' && *pos + 2 < chars.len() && chars[*pos + 1] == '(' && chars[*pos + 2] == '(' {
            flush_literal(&mut literal, parts);
            *pos += 3; // skip $((
            let start = *pos;
            // Find matching ))
            let mut depth = 1;
            while *pos < chars.len() && depth > 0 {
                if *pos + 1 < chars.len() && chars[*pos] == '(' && chars[*pos + 1] == '(' {
                    depth += 1;
                    *pos += 2;
                } else if *pos + 1 < chars.len() && chars[*pos] == ')' && chars[*pos + 1] == ')' {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                    *pos += 2;
                } else {
                    *pos += 1;
                }
            }
            let content: String = chars[start..*pos].iter().collect();
            parts.push(WordPart::ArithSubst(content.into()));
            if *pos + 1 < chars.len() && chars[*pos] == ')' && chars[*pos + 1] == ')' {
                *pos += 2; // skip ))
            }
            continue;
        }

        // ---------------------------------------------------------------
        // $(cmd) — command substitution
        // ---------------------------------------------------------------
        if c == '$' && *pos + 1 < chars.len() && chars[*pos + 1] == '(' {
            flush_literal(&mut literal, parts);
            *pos += 2; // skip $(
            let start = *pos;
            let mut depth: u32 = 1;
            while *pos < chars.len() && depth > 0 {
                match chars[*pos] {
                    '(' => {
                        depth += 1;
                        *pos += 1;
                    }
                    ')' => {
                        depth -= 1;
                        if depth > 0 {
                            *pos += 1;
                        }
                    }
                    '\'' => {
                        *pos += 1;
                        while *pos < chars.len() && chars[*pos] != '\'' {
                            *pos += 1;
                        }
                        if *pos < chars.len() {
                            *pos += 1;
                        }
                    }
                    '"' => {
                        *pos += 1;
                        while *pos < chars.len() && chars[*pos] != '"' {
                            if chars[*pos] == '\\' && *pos + 1 < chars.len() {
                                *pos += 2;
                            } else {
                                *pos += 1;
                            }
                        }
                        if *pos < chars.len() {
                            *pos += 1;
                        }
                    }
                    _ => {
                        *pos += 1;
                    }
                }
            }
            let content: String = chars[start..*pos].iter().collect();
            parts.push(WordPart::CommandSubst(content.into()));
            if *pos < chars.len() && chars[*pos] == ')' {
                *pos += 1; // skip )
            }
            continue;
        }

        // ---------------------------------------------------------------
        // ${expr} — brace expression
        // ---------------------------------------------------------------
        if c == '$' && *pos + 1 < chars.len() && chars[*pos + 1] == '{' {
            flush_literal(&mut literal, parts);
            *pos += 2; // skip ${
            let start = *pos;
            let mut depth: u32 = 1;
            while *pos < chars.len() && depth > 0 {
                match chars[*pos] {
                    '{' => {
                        depth += 1;
                    }
                    '}' => {
                        depth -= 1;
                        if depth == 0 {
                            break;
                        }
                    }
                    _ => {}
                }
                *pos += 1;
            }
            let content: String = chars[start..*pos].iter().collect();
            parts.push(WordPart::BraceExpr(content.into()));
            if *pos < chars.len() && chars[*pos] == '}' {
                *pos += 1; // skip }
            }
            continue;
        }

        // ---------------------------------------------------------------
        // $VAR, $?, $@, etc. — variable
        // ---------------------------------------------------------------
        if c == '$' && *pos + 1 < chars.len() {
            let next = chars[*pos + 1];
            if next.is_ascii_alphabetic() || next == '_' {
                flush_literal(&mut literal, parts);
                *pos += 1; // skip $
                let start = *pos;
                while *pos < chars.len()
                    && (chars[*pos].is_ascii_alphanumeric() || chars[*pos] == '_')
                {
                    *pos += 1;
                }
                let name: String = chars[start..*pos].iter().collect();
                parts.push(WordPart::Variable(name.into()));
                continue;
            }
            if "?$!#@*-0123456789".contains(next) {
                flush_literal(&mut literal, parts);
                *pos += 2; // skip $ + special char
                parts.push(WordPart::Variable(next.to_string().into()));
                continue;
            }
            // Bare $ — treat as literal
            literal.push('$');
            *pos += 1;
            continue;
        }
        // Bare $ at end of input
        if c == '$' {
            literal.push('$');
            *pos += 1;
            continue;
        }

        // ---------------------------------------------------------------
        // `cmd` — backtick substitution
        // ---------------------------------------------------------------
        if c == '`' {
            flush_literal(&mut literal, parts);
            *pos += 1; // skip opening `
            let start = *pos;
            while *pos < chars.len() && chars[*pos] != '`' {
                if chars[*pos] == '\\' && *pos + 1 < chars.len() {
                    *pos += 2;
                } else {
                    *pos += 1;
                }
            }
            let content: String = chars[start..*pos].iter().collect();
            parts.push(WordPart::BacktickSubst(content.into()));
            if *pos < chars.len() {
                *pos += 1; // skip closing `
            }
            continue;
        }

        // The remaining checks only apply outside double quotes
        if !in_double_quote {
            // ---------------------------------------------------------------
            // ~ — tilde expansion (only at start of word, i.e. parts is empty
            //     and literal is empty)
            // ---------------------------------------------------------------
            if c == '~' && parts.is_empty() && literal.is_empty() {
                *pos += 1;
                // Read optional username (alphanumeric, _, -, .)
                let start = *pos;
                while *pos < chars.len() {
                    let ch = chars[*pos];
                    if ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' || ch == '.' {
                        *pos += 1;
                    } else {
                        break;
                    }
                }
                if *pos > start {
                    let user: String = chars[start..*pos].iter().collect();
                    parts.push(WordPart::Tilde(Some(user.into())));
                } else {
                    parts.push(WordPart::Tilde(None));
                }
                continue;
            }

            // ---------------------------------------------------------------
            // Glob patterns: *, ?, [...]
            // ---------------------------------------------------------------
            if c == '*' || c == '?' {
                flush_literal(&mut literal, parts);
                parts.push(WordPart::GlobPattern(c.to_string().into()));
                *pos += 1;
                continue;
            }
            if c == '[' {
                // Look for matching ]
                let start = *pos;
                *pos += 1;
                // Handle [! or [^ at start
                if *pos < chars.len() && (chars[*pos] == '!' || chars[*pos] == '^') {
                    *pos += 1;
                }
                // ] immediately after [ (or [!) is literal
                if *pos < chars.len() && chars[*pos] == ']' {
                    *pos += 1;
                }
                while *pos < chars.len() && chars[*pos] != ']' {
                    *pos += 1;
                }
                if *pos < chars.len() && chars[*pos] == ']' {
                    *pos += 1;
                    flush_literal(&mut literal, parts);
                    let pattern: String = chars[start..*pos].iter().collect();
                    parts.push(WordPart::GlobPattern(pattern.into()));
                } else {
                    // No matching ] — treat [ as literal, reset
                    *pos = start;
                    literal.push(chars[*pos]);
                    *pos += 1;
                }
                continue;
            }

            // ---------------------------------------------------------------
            // {a,b,c} or {1..5} — brace expansion (NOT ${...})
            // ---------------------------------------------------------------
            if c == '{' {
                // Look ahead to see if this is a valid brace expansion
                // (must contain , or ..)
                let start = *pos;
                *pos += 1;
                let inner_start = *pos;
                let mut depth: u32 = 1;
                let mut has_comma = false;
                let mut has_dotdot = false;
                while *pos < chars.len() && depth > 0 {
                    match chars[*pos] {
                        '{' => {
                            depth += 1;
                        }
                        '}' => {
                            depth -= 1;
                            if depth == 0 {
                                break;
                            }
                        }
                        ',' => {
                            if depth == 1 {
                                has_comma = true;
                            }
                        }
                        '.' => {
                            if depth == 1 && *pos + 1 < chars.len() && chars[*pos + 1] == '.' {
                                has_dotdot = true;
                            }
                        }
                        _ => {}
                    }
                    *pos += 1;
                }
                if depth == 0 && (has_comma || has_dotdot) {
                    flush_literal(&mut literal, parts);
                    let inner: String = chars[inner_start..*pos].iter().collect();
                    parts.push(WordPart::BraceExpansion(inner.into()));
                    *pos += 1; // skip closing }
                } else {
                    // Not a valid brace expansion — treat as literal
                    *pos = start;
                    literal.push(chars[*pos]);
                    *pos += 1;
                }
                continue;
            }
        }

        // ---------------------------------------------------------------
        // Default: accumulate as literal
        // ---------------------------------------------------------------
        literal.push(c);
        *pos += 1;
    }

    flush_literal(&mut literal, parts);
}

/// If `literal` is non-empty, push it as a `Literal` part and clear it.
fn flush_literal(literal: &mut String, parts: &mut Vec<WordPart>) {
    if !literal.is_empty() {
        parts.push(WordPart::Literal(std::mem::take(literal).into()));
    }
}

// ===========================================================================
// parse_command_line — parse a full command line into CommandList
// ===========================================================================

/// Parse a complete command line into structured commands.
///
/// This handles pipes, chain operators (`;`, `&&`, `||`), redirects,
/// assignments, and quoting. It does NOT handle block keywords like
/// `if/for/while` — those are handled by the script parser.
pub fn parse_command_line(input: &str) -> CommandList {
    let chain_segments = split_on_chains(input);
    let mut items = Vec::new();

    for (segment, op) in chain_segments {
        let trimmed = segment.trim();
        if trimmed.is_empty() {
            continue;
        }
        let pipeline = parse_pipeline_str(trimmed);
        items.push((pipeline, op));
    }

    CommandList { items }
}

// ---------------------------------------------------------------------------
// Chain splitting (;, &&, ||)
// ---------------------------------------------------------------------------

/// Split input on unquoted `;`, `&&`, `||`.
fn split_on_chains(input: &str) -> Vec<(String, Option<ChainOp>)> {
    let chars: Vec<char> = input.chars().collect();
    let len = chars.len();
    let mut results = Vec::new();
    let mut start = 0;
    let mut pos = 0;
    let mut in_single = false;
    let mut in_double = false;
    let mut escape = false;

    while pos < len {
        let c = chars[pos];

        if escape {
            escape = false;
            pos += 1;
            continue;
        }
        if c == '\\' && !in_single {
            escape = true;
            pos += 1;
            continue;
        }
        if c == '\'' && !in_double {
            in_single = !in_single;
            pos += 1;
            continue;
        }
        if c == '"' && !in_single {
            in_double = !in_double;
            pos += 1;
            continue;
        }

        if in_single || in_double {
            pos += 1;
            continue;
        }

        // Skip over [[ ... ]] — don't interpret inner &&, ||, <, > as operators
        if c == '[' && pos + 1 < len && chars[pos + 1] == '[' {
            // Check that [[ is at a word boundary (start or preceded by whitespace)
            let at_boundary =
                pos == start || (pos > 0 && (chars[pos - 1] == ' ' || chars[pos - 1] == '\t'));
            if at_boundary {
                pos += 2;
                while pos < len {
                    if pos + 1 < len && chars[pos] == ']' && chars[pos + 1] == ']' {
                        pos += 2;
                        break;
                    }
                    pos += 1;
                }
                continue;
            }
        }

        // Skip over $(...), $(()), ${}, `...` to avoid misinterpreting inner operators
        if c == '$' && pos + 1 < len {
            let next = chars[pos + 1];
            if next == '(' {
                pos = skip_dollar_paren(&chars, pos);
                continue;
            }
            if next == '{' {
                pos = skip_dollar_brace(&chars, pos);
                continue;
            }
        }
        if c == '`' {
            pos += 1;
            while pos < len && chars[pos] != '`' {
                if chars[pos] == '\\' && pos + 1 < len {
                    pos += 2;
                } else {
                    pos += 1;
                }
            }
            if pos < len {
                pos += 1;
            }
            continue;
        }

        // ;
        if c == ';' {
            let seg: String = chars[start..pos].iter().collect();
            results.push((seg, Some(ChainOp::Semi)));
            pos += 1;
            start = pos;
            continue;
        }
        // && or & (background)
        if c == '&' {
            if pos + 1 < len && chars[pos + 1] == '&' {
                let seg: String = chars[start..pos].iter().collect();
                results.push((seg, Some(ChainOp::And)));
                pos += 2;
                start = pos;
                continue;
            }
            // Skip & when part of |&, >&, <& (redirect/pipe syntax)
            let prev_non_ws = (start..pos)
                .rev()
                .find(|&j| !chars[j].is_ascii_whitespace())
                .map(|j| chars[j]);
            let is_redirect_amp = matches!(prev_non_ws, Some('>' | '<' | '|'));
            if !is_redirect_amp {
                // lone & — background operator
                let seg: String = chars[start..pos].iter().collect();
                results.push((seg, Some(ChainOp::Background)));
                pos += 1;
                start = pos;
                continue;
            }
        }
        // || (not single |)
        if c == '|' && pos + 1 < len && chars[pos + 1] == '|' {
            let seg: String = chars[start..pos].iter().collect();
            results.push((seg, Some(ChainOp::Or)));
            pos += 2;
            start = pos;
            continue;
        }

        pos += 1;
    }

    // Trailing segment
    let tail: String = chars[start..len].iter().collect();
    let trimmed = tail.trim();
    if !trimmed.is_empty() {
        results.push((tail, None));
    }

    results
}

/// Skip over `$(...)` or `$((...))` including nested parens.
fn skip_dollar_paren(chars: &[char], start: usize) -> usize {
    let mut pos = start + 2; // skip $(
    let mut depth: u32 = 1;
    while pos < chars.len() && depth > 0 {
        match chars[pos] {
            '(' => {
                depth += 1;
                pos += 1;
            }
            ')' => {
                depth -= 1;
                pos += 1;
            }
            '\'' => {
                pos += 1;
                while pos < chars.len() && chars[pos] != '\'' {
                    pos += 1;
                }
                if pos < chars.len() {
                    pos += 1;
                }
            }
            '"' => {
                pos += 1;
                while pos < chars.len() && chars[pos] != '"' {
                    if chars[pos] == '\\' && pos + 1 < chars.len() {
                        pos += 2;
                    } else {
                        pos += 1;
                    }
                }
                if pos < chars.len() {
                    pos += 1;
                }
            }
            '\\' => {
                pos += 2;
            }
            _ => {
                pos += 1;
            }
        }
    }
    pos
}

/// Skip over `${...}` including nested braces.
fn skip_dollar_brace(chars: &[char], start: usize) -> usize {
    let mut pos = start + 2; // skip ${
    let mut depth: u32 = 1;
    while pos < chars.len() && depth > 0 {
        match chars[pos] {
            '{' => {
                depth += 1;
                pos += 1;
            }
            '}' => {
                depth -= 1;
                pos += 1;
            }
            '\\' => {
                pos += 2;
            }
            _ => {
                pos += 1;
            }
        }
    }
    pos
}

// ---------------------------------------------------------------------------
// Pipeline splitting (on |)
// ---------------------------------------------------------------------------

/// Parse a chain segment into a StructuredPipeline by splitting on unquoted `|`.
fn parse_pipeline_str(input: &str) -> StructuredPipeline {
    // Detect `! ` pipeline negation prefix
    let (bang, input) = if input.starts_with("! ") || input == "!" {
        (true, input[1..].trim_start())
    } else {
        (false, input)
    };
    let pipe_segments = split_on_pipe(input);
    let commands: Vec<SimpleCommand> = pipe_segments
        .iter()
        .map(|seg| parse_simple_command(seg.trim()))
        .collect();
    StructuredPipeline { commands, bang }
}

/// Split on unquoted single `|` (not `||`).
fn split_on_pipe(input: &str) -> Vec<String> {
    let chars: Vec<char> = input.chars().collect();
    let len = chars.len();
    let mut results = Vec::new();
    let mut start = 0;
    let mut pos = 0;
    let mut in_single = false;
    let mut in_double = false;
    let mut escape = false;

    while pos < len {
        let c = chars[pos];

        if escape {
            escape = false;
            pos += 1;
            continue;
        }
        if c == '\\' && !in_single {
            escape = true;
            pos += 1;
            continue;
        }
        if c == '\'' && !in_double {
            in_single = !in_single;
            pos += 1;
            continue;
        }
        if c == '"' && !in_single {
            in_double = !in_double;
            pos += 1;
            continue;
        }

        if in_single || in_double {
            pos += 1;
            continue;
        }

        // Skip $(...), ${...}, `...`
        if c == '$' && pos + 1 < len {
            let next = chars[pos + 1];
            if next == '(' {
                pos = skip_dollar_paren(&chars, pos);
                continue;
            }
            if next == '{' {
                pos = skip_dollar_brace(&chars, pos);
                continue;
            }
        }
        if c == '`' {
            pos += 1;
            while pos < len && chars[pos] != '`' {
                if chars[pos] == '\\' && pos + 1 < len {
                    pos += 2;
                } else {
                    pos += 1;
                }
            }
            if pos < len {
                pos += 1;
            }
            continue;
        }

        // Skip over [[ ... ]] — don't interpret inner | as pipe
        if c == '[' && pos + 1 < len && chars[pos + 1] == '[' {
            let at_boundary =
                pos == start || (pos > 0 && (chars[pos - 1] == ' ' || chars[pos - 1] == '\t'));
            if at_boundary {
                pos += 2;
                while pos < len {
                    if pos + 1 < len && chars[pos] == ']' && chars[pos + 1] == ']' {
                        pos += 2;
                        break;
                    }
                    pos += 1;
                }
                continue;
            }
        }

        // || (double pipe) — skip both characters (not a pipe split)
        if c == '|' && pos + 1 < len && chars[pos + 1] == '|' {
            pos += 2;
            continue;
        }

        // |& (pipe stderr+stdout) — treat as | (single stream model), skip the &
        if c == '|' && pos + 1 < len && chars[pos + 1] == '&' {
            let seg: String = chars[start..pos].iter().collect();
            results.push(seg);
            pos += 2; // skip both | and &
            start = pos;
            continue;
        }

        // Single | (pipe split)
        if c == '|' {
            let seg: String = chars[start..pos].iter().collect();
            results.push(seg);
            pos += 1;
            start = pos;
            continue;
        }

        pos += 1;
    }

    let tail: String = chars[start..len].iter().collect();
    results.push(tail);
    results
}

// ---------------------------------------------------------------------------
// Simple command parsing (assignments, words, redirects)
// ---------------------------------------------------------------------------

/// Parse a single pipeline segment into a SimpleCommand.
fn parse_simple_command(input: &str) -> SimpleCommand {
    let raw_tokens = tokenize_words(input);

    let mut assignments = Vec::new();
    let mut words: Vec<Word> = Vec::new();
    let mut redirects: Vec<WordRedirect> = Vec::new();
    let mut i = 0;
    let mut seen_command = false;

    // Check for [[ ... ]] or (( ... )) — collect everything as words,
    // don't interpret operators like > or < as redirects.
    if !raw_tokens.is_empty() && (raw_tokens[0] == "[[" || raw_tokens[0] == "((") {
        for token in &raw_tokens {
            words.push(parse_word(token));
        }
        return SimpleCommand {
            assignments,
            words,
            redirects,
        };
    }

    while i < raw_tokens.len() {
        let token = &raw_tokens[i];

        // ---------------------------------------------------------------
        // Redirect operators: >, >>, <, <<<, N>, N>>, N>&M
        // ---------------------------------------------------------------
        {
            // Check for N> pattern (digit followed by >)
            if token.len() >= 2 && token.chars().next().is_some_and(|c| c.is_ascii_digit()) {
                let Some(first) = token.chars().next() else {
                    i += 1;
                    continue;
                };
                let rest: String = token.chars().skip(1).collect();
                if let Some(target_str) = rest.strip_prefix(">>") {
                    let Some(fd) = first.to_digit(10) else {
                        i += 1;
                        continue;
                    };
                    let target_word = if target_str.is_empty() {
                        i += 1;
                        if i < raw_tokens.len() {
                            parse_word(&raw_tokens[i])
                        } else {
                            vec![]
                        }
                    } else {
                        parse_word(target_str)
                    };
                    redirects.push(WordRedirect {
                        fd: Some(fd),
                        op: RedirectOp::Append,
                        target: RedirectTarget::File(target_word),
                    });
                    i += 1;
                    continue;
                }
                if let Some(dup_str) = rest.strip_prefix(">&") {
                    let Some(fd) = first.to_digit(10) else {
                        i += 1;
                        continue;
                    };
                    if let Ok(dup_fd) = dup_str.parse::<u32>() {
                        redirects.push(WordRedirect {
                            fd: Some(fd),
                            op: RedirectOp::Write,
                            target: RedirectTarget::FdDup(dup_fd),
                        });
                        i += 1;
                        continue;
                    }
                }
                if let Some(target_str) = rest.strip_prefix('>') {
                    let Some(fd) = first.to_digit(10) else {
                        i += 1;
                        continue;
                    };
                    let target_word = if target_str.is_empty() {
                        i += 1;
                        if i < raw_tokens.len() {
                            parse_word(&raw_tokens[i])
                        } else {
                            vec![]
                        }
                    } else {
                        parse_word(target_str)
                    };
                    redirects.push(WordRedirect {
                        fd: Some(fd),
                        op: RedirectOp::Write,
                        target: RedirectTarget::File(target_word),
                    });
                    i += 1;
                    continue;
                }
            }

            // <<<
            if token == "<<<" {
                i += 1;
                let target_word = if i < raw_tokens.len() {
                    parse_word(&raw_tokens[i])
                } else {
                    vec![]
                };
                redirects.push(WordRedirect {
                    fd: None,
                    op: RedirectOp::HereString,
                    target: RedirectTarget::File(target_word),
                });
                i += 1;
                continue;
            }

            // >>
            if token == ">>" {
                i += 1;
                let target_word = if i < raw_tokens.len() {
                    parse_word(&raw_tokens[i])
                } else {
                    vec![]
                };
                redirects.push(WordRedirect {
                    fd: None,
                    op: RedirectOp::Append,
                    target: RedirectTarget::File(target_word),
                });
                i += 1;
                continue;
            }

            // >
            if token == ">" {
                i += 1;
                let target_word = if i < raw_tokens.len() {
                    parse_word(&raw_tokens[i])
                } else {
                    vec![]
                };
                redirects.push(WordRedirect {
                    fd: None,
                    op: RedirectOp::Write,
                    target: RedirectTarget::File(target_word),
                });
                i += 1;
                continue;
            }

            if token == "<" {
                i += 1;
                let target_word = if i < raw_tokens.len() {
                    parse_word(&raw_tokens[i])
                } else {
                    vec![]
                };
                redirects.push(WordRedirect {
                    fd: None,
                    op: RedirectOp::Read,
                    target: RedirectTarget::File(target_word),
                });
                i += 1;
                continue;
            }
        }

        // ---------------------------------------------------------------
        // Assignment: VAR=value or VAR+=value or VAR=(array)
        // (only before any command word)
        // ---------------------------------------------------------------
        if !seen_command {
            if let Some(assign) = try_parse_assignment(token, &raw_tokens, &mut i) {
                assignments.push(assign);
                i += 1;
                continue;
            }
        }

        // ---------------------------------------------------------------
        // Regular word
        // ---------------------------------------------------------------
        seen_command = true;
        words.push(parse_word(token));
        i += 1;
    }

    SimpleCommand {
        assignments,
        words,
        redirects,
    }
}

/// Try to parse a token as an assignment (VAR=value, VAR+=value, VAR=(a b c)).
/// Returns None if it's not an assignment.
fn try_parse_assignment(token: &str, all_tokens: &[String], idx: &mut usize) -> Option<Assignment> {
    // Must start with a valid variable name character
    let chars: Vec<char> = token.chars().collect();
    if chars.is_empty() || !(chars[0].is_ascii_alphabetic() || chars[0] == '_') {
        return None;
    }

    // Find the = or +=
    let mut name_end = 0;
    while name_end < chars.len()
        && (chars[name_end].is_ascii_alphanumeric() || chars[name_end] == '_')
    {
        name_end += 1;
    }

    if name_end >= chars.len() {
        return None;
    }

    let (op, value_start) = if chars[name_end] == '=' {
        (AssignOp::Assign, name_end + 1)
    } else if name_end + 1 < chars.len() && chars[name_end] == '+' && chars[name_end + 1] == '=' {
        (AssignOp::PlusAssign, name_end + 2)
    } else {
        return None;
    };

    let name: crate::Str = chars[..name_end].iter().collect::<String>().into();
    let value_str: String = chars[value_start..].iter().collect();

    // Check for array assignment: VAR=(...)
    if let Some(inner) = value_str.strip_prefix('(') {
        // Collect tokens until we find the closing )
        // The opening ( may be in this token, and items + closing ) in subsequent tokens
        let mut array_content = String::new();
        if let Some(stripped) = inner.strip_suffix(')') {
            // All in one token: VAR=(a b c)
            array_content.push_str(stripped);
        } else {
            array_content.push_str(inner);
            // Consume subsequent tokens until we find one ending with )
            loop {
                *idx += 1;
                if *idx >= all_tokens.len() {
                    break;
                }
                let next = &all_tokens[*idx];
                if next.ends_with(')') {
                    if !array_content.is_empty() {
                        array_content.push(' ');
                    }
                    array_content.push_str(&next[..next.len() - 1]);
                    break;
                }
                if !array_content.is_empty() {
                    array_content.push(' ');
                }
                array_content.push_str(next);
            }
        }

        let array_words: Vec<Word> = if array_content.trim().is_empty() {
            Vec::new()
        } else {
            tokenize_words(array_content.trim())
                .iter()
                .map(|w| parse_word(w))
                .collect()
        };

        return Some(Assignment {
            name,
            op,
            value: AssignValue::Array(array_words),
        });
    }

    // Scalar assignment
    let value_word = if value_str.is_empty() {
        vec![]
    } else {
        parse_word(&value_str)
    };

    Some(Assignment {
        name,
        op,
        value: AssignValue::Scalar(value_word),
    })
}

// ---------------------------------------------------------------------------
// Word-level tokenizer (split on unquoted whitespace)
// ---------------------------------------------------------------------------

/// Split a command string into word tokens, respecting quoting.
/// This handles `>`, `>>`, `<`, `<<<` as separate tokens.
fn tokenize_words(input: &str) -> Vec<String> {
    let chars: Vec<char> = input.chars().collect();
    let len = chars.len();
    let mut tokens = Vec::new();
    let mut pos = 0;

    while pos < len {
        // Skip whitespace
        while pos < len && (chars[pos] == ' ' || chars[pos] == '\t') {
            pos += 1;
        }
        if pos >= len {
            break;
        }

        // Check for operator tokens: <<<, >>, >, <
        if chars[pos] == '<' && pos + 2 < len && chars[pos + 1] == '<' && chars[pos + 2] == '<' {
            tokens.push("<<<".to_string());
            pos += 3;
            continue;
        }
        if chars[pos] == '>' && pos + 1 < len && chars[pos + 1] == '>' {
            tokens.push(">>".to_string());
            pos += 2;
            continue;
        }
        // >( and <( are process substitution — keep as single word, not redirect
        if (chars[pos] == '>' || chars[pos] == '<') && pos + 1 < len && chars[pos + 1] == '(' {
            let start = pos;
            pos += 2; // skip <( or >(
            let mut depth: u32 = 1;
            while pos < len && depth > 0 {
                match chars[pos] {
                    '(' => depth += 1,
                    ')' => depth -= 1,
                    _ => {}
                }
                pos += 1;
            }
            let word: String = chars[start..pos].iter().collect();
            tokens.push(word);
            continue;
        }
        if chars[pos] == '>' {
            tokens.push(">".to_string());
            pos += 1;
            continue;
        }
        if chars[pos] == '<' {
            tokens.push("<".to_string());
            pos += 1;
            continue;
        }

        // Read a word token
        let start = pos;
        let mut in_single = false;
        let mut in_double = false;
        let mut escape = false;

        while pos < len {
            let c = chars[pos];

            if escape {
                escape = false;
                pos += 1;
                continue;
            }
            if c == '\\' && !in_single {
                escape = true;
                pos += 1;
                continue;
            }
            if c == '\'' && !in_double {
                in_single = !in_single;
                pos += 1;
                continue;
            }
            if c == '"' && !in_single {
                in_double = !in_double;
                pos += 1;
                continue;
            }

            if !in_single && !in_double {
                // Stop at whitespace or redirect operators
                if c == ' ' || c == '\t' {
                    break;
                }
                // Stop at standalone redirect operators, but NOT if they are part of
                // a word like "2>" (which is handled as a single token)
                if (c == '>' || c == '<') && pos > start {
                    // Check if previous char is a digit — that's N> pattern, keep going
                    let prev = chars[pos - 1];
                    if c == '>' && prev.is_ascii_digit() && pos == start + 1 {
                        // This is like "2>" — keep it as one token
                        pos += 1;
                        // Also consume >> if applicable
                        if pos < len && chars[pos] == '>' {
                            pos += 1;
                        }
                        // Also consume &N for fd dup
                        if pos < len && chars[pos] == '&' {
                            pos += 1;
                            while pos < len && chars[pos].is_ascii_digit() {
                                pos += 1;
                            }
                        }
                        // Or consume the target if no space
                        continue;
                    }
                    break;
                }
                if (c == '>' || c == '<') && pos == start {
                    // Standalone operator at the beginning — handled above
                    break;
                }
            }

            // Handle $(...), ${...}, `...` to skip their contents
            if !in_single && c == '$' && pos + 1 < len {
                if chars[pos + 1] == '(' {
                    pos = skip_dollar_paren(&chars, pos);
                    continue;
                }
                if chars[pos + 1] == '{' {
                    pos = skip_dollar_brace(&chars, pos);
                    continue;
                }
                if chars[pos + 1] == '\'' {
                    // $'...'
                    pos += 2;
                    while pos < len && chars[pos] != '\'' {
                        if chars[pos] == '\\' && pos + 1 < len {
                            pos += 2;
                        } else {
                            pos += 1;
                        }
                    }
                    if pos < len {
                        pos += 1;
                    }
                    continue;
                }
            }
            if !in_single && c == '`' {
                pos += 1;
                while pos < len && chars[pos] != '`' {
                    if chars[pos] == '\\' && pos + 1 < len {
                        pos += 2;
                    } else {
                        pos += 1;
                    }
                }
                if pos < len {
                    pos += 1;
                }
                continue;
            }

            pos += 1;
        }

        if pos > start {
            let word: String = chars[start..pos].iter().collect();
            tokens.push(word);
        }
    }

    tokens
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::script::word::WordToSource;

    // ---------------------------------------------------------------
    // Word parsing round-trips
    // ---------------------------------------------------------------

    #[test]
    fn roundtrip_literal() {
        assert_eq!(parse_word("hello").to_source(), "hello");
    }

    #[test]
    fn roundtrip_single_quoted() {
        assert_eq!(parse_word("'hello world'").to_source(), "'hello world'");
    }

    #[test]
    fn roundtrip_double_quoted_with_var() {
        assert_eq!(parse_word("\"hello $USER\"").to_source(), "\"hello $USER\"");
    }

    #[test]
    fn roundtrip_brace_expr() {
        assert_eq!(parse_word("${HOME}").to_source(), "${HOME}");
    }

    #[test]
    fn roundtrip_command_subst() {
        assert_eq!(parse_word("$(whoami)").to_source(), "$(whoami)");
    }

    #[test]
    fn roundtrip_arith_subst() {
        assert_eq!(parse_word("$((1+2))").to_source(), "$((1+2))");
    }

    #[test]
    fn roundtrip_mixed() {
        assert_eq!(
            parse_word("hello${var}world").to_source(),
            "hello${var}world"
        );
    }

    #[test]
    fn roundtrip_ansi_c() {
        assert_eq!(parse_word("$'\\n'").to_source(), "$'\\n'");
    }

    #[test]
    fn roundtrip_glob_star() {
        assert_eq!(parse_word("*.txt").to_source(), "*.txt");
    }

    #[test]
    fn roundtrip_brace_expansion() {
        assert_eq!(parse_word("{a,b,c}").to_source(), "{a,b,c}");
    }

    #[test]
    fn roundtrip_tilde() {
        assert_eq!(parse_word("~").to_source(), "~");
    }

    #[test]
    fn roundtrip_tilde_user() {
        assert_eq!(parse_word("~user").to_source(), "~user");
    }

    #[test]
    fn roundtrip_backtick() {
        assert_eq!(parse_word("`whoami`").to_source(), "`whoami`");
    }

    #[test]
    fn roundtrip_variable_simple() {
        assert_eq!(parse_word("$HOME").to_source(), "$HOME");
    }

    #[test]
    fn roundtrip_variable_special() {
        assert_eq!(parse_word("$?").to_source(), "$?");
        assert_eq!(parse_word("$@").to_source(), "$@");
        assert_eq!(parse_word("$#").to_source(), "$#");
        assert_eq!(parse_word("$1").to_source(), "$1");
    }

    #[test]
    fn roundtrip_backslash_escape() {
        assert_eq!(parse_word("hello\\ world").to_source(), "hello\\ world");
    }

    #[test]
    fn roundtrip_glob_question() {
        assert_eq!(parse_word("file?.txt").to_source(), "file?.txt");
    }

    #[test]
    fn roundtrip_glob_bracket() {
        assert_eq!(parse_word("[abc]").to_source(), "[abc]");
    }

    #[test]
    fn roundtrip_brace_range() {
        assert_eq!(parse_word("{1..5}").to_source(), "{1..5}");
    }

    #[test]
    fn roundtrip_brace_expr_with_default() {
        assert_eq!(parse_word("${var:-default}").to_source(), "${var:-default}");
    }

    // ---------------------------------------------------------------
    // Word structure verification
    // ---------------------------------------------------------------

    #[test]
    fn structure_single_quoted() {
        assert_eq!(
            parse_word("'hello'"),
            vec![WordPart::SingleQuoted("hello".into())]
        );
    }

    #[test]
    fn structure_variable() {
        assert_eq!(parse_word("$HOME"), vec![WordPart::Variable("HOME".into())]);
    }

    #[test]
    fn structure_literal_then_variable() {
        assert_eq!(
            parse_word("hello$USER"),
            vec![
                WordPart::Literal("hello".into()),
                WordPart::Variable("USER".into()),
            ]
        );
    }

    #[test]
    fn structure_double_quoted_variable() {
        assert_eq!(
            parse_word("\"$USER\""),
            vec![WordPart::DoubleQuoted(vec![WordPart::Variable(
                "USER".into()
            ),])]
        );
    }

    #[test]
    fn structure_brace_expr_default() {
        assert_eq!(
            parse_word("${var:-default}"),
            vec![WordPart::BraceExpr("var:-default".into())]
        );
    }

    #[test]
    fn structure_tilde_none() {
        assert_eq!(parse_word("~"), vec![WordPart::Tilde(None)]);
    }

    #[test]
    fn structure_tilde_user() {
        assert_eq!(
            parse_word("~bob"),
            vec![WordPart::Tilde(Some("bob".into()))]
        );
    }

    #[test]
    fn structure_arith() {
        assert_eq!(
            parse_word("$((1+2))"),
            vec![WordPart::ArithSubst("1+2".into())]
        );
    }

    #[test]
    fn structure_command_subst() {
        assert_eq!(
            parse_word("$(ls -la)"),
            vec![WordPart::CommandSubst("ls -la".into())]
        );
    }

    #[test]
    fn structure_backtick_subst() {
        assert_eq!(
            parse_word("`date`"),
            vec![WordPart::BacktickSubst("date".into())]
        );
    }

    #[test]
    fn structure_dollar_single_quoted() {
        assert_eq!(
            parse_word("$'\\n'"),
            vec![WordPart::DollarSingleQuoted("\\n".into())]
        );
    }

    #[test]
    fn structure_glob_star() {
        assert_eq!(
            parse_word("*.txt"),
            vec![
                WordPart::GlobPattern("*".into()),
                WordPart::Literal(".txt".into()),
            ]
        );
    }

    #[test]
    fn structure_brace_expansion() {
        assert_eq!(
            parse_word("{a,b,c}"),
            vec![WordPart::BraceExpansion("a,b,c".into())]
        );
    }

    #[test]
    fn structure_double_quoted_mixed() {
        // "hello $USER world"
        assert_eq!(
            parse_word("\"hello $USER world\""),
            vec![WordPart::DoubleQuoted(vec![
                WordPart::Literal("hello ".into()),
                WordPart::Variable("USER".into()),
                WordPart::Literal(" world".into()),
            ])]
        );
    }

    #[test]
    fn structure_nested_command_subst() {
        assert_eq!(
            parse_word("$(echo $(whoami))"),
            vec![WordPart::CommandSubst("echo $(whoami)".into())]
        );
    }

    // ---------------------------------------------------------------
    // Command line parsing
    // ---------------------------------------------------------------

    #[test]
    fn cmdline_simple_echo() {
        let cl = parse_command_line("echo hello");
        assert_eq!(cl.items.len(), 1);
        let (pipeline, op) = &cl.items[0];
        assert!(op.is_none());
        assert_eq!(pipeline.commands.len(), 1);
        let cmd = &pipeline.commands[0];
        assert_eq!(cmd.words.len(), 2);
        assert_eq!(cmd.words[0].to_source(), "echo");
        assert_eq!(cmd.words[1].to_source(), "hello");
    }

    #[test]
    fn cmdline_pipe() {
        let cl = parse_command_line("echo hello | wc -l");
        assert_eq!(cl.items.len(), 1);
        let (pipeline, _) = &cl.items[0];
        assert_eq!(pipeline.commands.len(), 2);
        assert_eq!(pipeline.commands[0].words[0].to_source(), "echo");
        assert_eq!(pipeline.commands[1].words[0].to_source(), "wc");
    }

    #[test]
    fn cmdline_and_chain() {
        let cl = parse_command_line("cmd1 && cmd2");
        assert_eq!(cl.items.len(), 2);
        assert_eq!(cl.items[0].1, Some(ChainOp::And));
        assert_eq!(cl.items[0].0.commands[0].words[0].to_source(), "cmd1");
        assert_eq!(cl.items[1].0.commands[0].words[0].to_source(), "cmd2");
    }

    #[test]
    fn cmdline_redirect_write() {
        let cl = parse_command_line("echo hi > out.txt");
        let cmd = &cl.items[0].0.commands[0];
        assert_eq!(cmd.words.len(), 2);
        assert_eq!(cmd.words[0].to_source(), "echo");
        assert_eq!(cmd.words[1].to_source(), "hi");
        assert_eq!(cmd.redirects.len(), 1);
        assert_eq!(cmd.redirects[0].op, RedirectOp::Write);
        assert!(
            matches!(cmd.redirects[0].target, RedirectTarget::File(_)),
            "expected File target"
        );
        if let RedirectTarget::File(ref w) = cmd.redirects[0].target {
            assert_eq!(w.to_source(), "out.txt");
        }
    }

    #[test]
    fn cmdline_redirect_read() {
        let cl = parse_command_line("cat < in.txt");
        let cmd = &cl.items[0].0.commands[0];
        assert_eq!(cmd.words.len(), 1);
        assert_eq!(cmd.words[0].to_source(), "cat");
        assert_eq!(cmd.redirects.len(), 1);
        assert_eq!(cmd.redirects[0].op, RedirectOp::Read);
    }

    #[test]
    fn cmdline_assignment_with_cmd() {
        let cl = parse_command_line("x=5 echo $x");
        let cmd = &cl.items[0].0.commands[0];
        assert_eq!(cmd.assignments.len(), 1);
        assert_eq!(cmd.assignments[0].name, "x");
        assert_eq!(cmd.words[0].to_source(), "echo");
    }

    #[test]
    fn cmdline_assignments_only() {
        let cl = parse_command_line("x=5 y=6");
        let cmd = &cl.items[0].0.commands[0];
        assert_eq!(cmd.assignments.len(), 2);
        assert_eq!(cmd.assignments[0].name, "x");
        assert_eq!(cmd.assignments[1].name, "y");
        assert!(cmd.words.is_empty());
    }

    #[test]
    fn cmdline_array_assignment() {
        let cl = parse_command_line("arr=(a b c)");
        let cmd = &cl.items[0].0.commands[0];
        assert_eq!(cmd.assignments.len(), 1);
        assert_eq!(cmd.assignments[0].name, "arr");
        assert!(
            matches!(cmd.assignments[0].value, AssignValue::Array(_)),
            "expected Array value"
        );
        if let AssignValue::Array(ref words) = cmd.assignments[0].value {
            assert_eq!(words.len(), 3);
            assert_eq!(words[0].to_source(), "a");
            assert_eq!(words[1].to_source(), "b");
            assert_eq!(words[2].to_source(), "c");
        }
    }

    #[test]
    fn cmdline_here_string() {
        let cl = parse_command_line("cat <<< 'hello'");
        let cmd = &cl.items[0].0.commands[0];
        assert_eq!(cmd.words.len(), 1);
        assert_eq!(cmd.words[0].to_source(), "cat");
        assert_eq!(cmd.redirects.len(), 1);
        assert_eq!(cmd.redirects[0].op, RedirectOp::HereString);
    }

    #[test]
    fn cmdline_semicolon_chain() {
        let cl = parse_command_line("echo a; echo b");
        assert_eq!(cl.items.len(), 2);
        assert_eq!(cl.items[0].1, Some(ChainOp::Semi));
    }

    #[test]
    fn cmdline_or_chain() {
        let cl = parse_command_line("false || echo fallback");
        assert_eq!(cl.items.len(), 2);
        assert_eq!(cl.items[0].1, Some(ChainOp::Or));
    }

    #[test]
    fn cmdline_append_redirect() {
        let cl = parse_command_line("echo x >> log.txt");
        let cmd = &cl.items[0].0.commands[0];
        assert_eq!(cmd.redirects.len(), 1);
        assert_eq!(cmd.redirects[0].op, RedirectOp::Append);
    }

    #[test]
    fn cmdline_fd_redirect() {
        let cl = parse_command_line("cmd 2>&1");
        let cmd = &cl.items[0].0.commands[0];
        assert_eq!(cmd.redirects.len(), 1);
        assert_eq!(cmd.redirects[0].fd, Some(2));
        assert_eq!(cmd.redirects[0].op, RedirectOp::Write);
        assert_eq!(cmd.redirects[0].target, RedirectTarget::FdDup(1));
    }

    #[test]
    fn cmdline_plus_assign() {
        let cl = parse_command_line("PATH+=/usr/bin");
        let cmd = &cl.items[0].0.commands[0];
        assert_eq!(cmd.assignments.len(), 1);
        assert_eq!(cmd.assignments[0].name, "PATH");
        assert_eq!(cmd.assignments[0].op, AssignOp::PlusAssign);
    }

    // ---------------------------------------------------------------
    // CommandList to_source round-trips
    // ---------------------------------------------------------------

    #[test]
    fn cmdlist_roundtrip_simple() {
        let src = "echo hello";
        let cl = parse_command_line(src);
        assert_eq!(cl.to_source(), src);
    }

    #[test]
    fn cmdlist_roundtrip_pipe() {
        let src = "echo hello | wc -l";
        let cl = parse_command_line(src);
        assert_eq!(cl.to_source(), src);
    }

    #[test]
    fn cmdlist_roundtrip_and() {
        // Note: output will have spaces around &&
        let cl = parse_command_line("cmd1 && cmd2");
        assert_eq!(cl.to_source(), "cmd1 && cmd2");
    }

    #[test]
    fn cmdlist_roundtrip_assignment() {
        let src = "x=5";
        let cl = parse_command_line(src);
        assert_eq!(cl.to_source(), src);
    }

    #[test]
    fn cmdlist_roundtrip_redirect() {
        let src = "echo hi > out.txt";
        let cl = parse_command_line(src);
        assert_eq!(cl.to_source(), src);
    }

    // ---------------------------------------------------------------
    // Edge cases
    // ---------------------------------------------------------------

    #[test]
    fn empty_input() {
        let cl = parse_command_line("");
        assert!(cl.items.is_empty());
    }

    #[test]
    fn whitespace_only() {
        let cl = parse_command_line("   ");
        assert!(cl.items.is_empty());
    }

    #[test]
    fn quoted_pipe_not_split() {
        let cl = parse_command_line("echo 'a|b'");
        assert_eq!(cl.items.len(), 1);
        assert_eq!(cl.items[0].0.commands.len(), 1);
        assert_eq!(cl.items[0].0.commands[0].words.len(), 2);
    }

    #[test]
    fn quoted_semicolon_not_split() {
        let cl = parse_command_line("echo \"a;b\"");
        assert_eq!(cl.items.len(), 1);
    }

    #[test]
    fn double_bracket_preserved() {
        let cl = parse_command_line("[[ -n $foo && $bar == baz ]]");
        let cmd = &cl.items[0].0.commands[0];
        // Everything between [[ and ]] should be words
        assert!(cmd.words.len() >= 2);
        assert_eq!(cmd.words[0].to_source(), "[[");
        assert!(cmd.words.last().is_some(), "expected words to be non-empty");
        // safe: we just asserted it's Some
        if let Some(last_word) = cmd.words.last() {
            assert_eq!(last_word.to_source(), "]]");
        }
    }

    #[test]
    fn dollar_in_subst_not_chain_split() {
        // $(...) containing && should not be split
        let cl = parse_command_line("echo $(cmd1 && cmd2)");
        assert_eq!(cl.items.len(), 1);
    }

    #[test]
    fn bare_dollar_preserved() {
        let w = parse_word("$");
        assert_eq!(w.to_source(), "$");
    }

    #[test]
    fn tilde_with_path() {
        let w = parse_word("~/bin");
        assert_eq!(w.len(), 2);
        assert_eq!(w[0], WordPart::Tilde(None));
        assert_eq!(w[1], WordPart::Literal("/bin".into()));
        assert_eq!(w.to_source(), "~/bin");
    }

    #[test]
    fn nested_brace_expr() {
        let w = parse_word("${var:-${default}}");
        assert_eq!(w, vec![WordPart::BraceExpr("var:-${default}".into())]);
    }

    #[test]
    fn mixed_quotes_word() {
        let w = parse_word("hello'world'\"$USER\"");
        assert_eq!(w.len(), 3);
        assert_eq!(w[0], WordPart::Literal("hello".into()));
        assert_eq!(w[1], WordPart::SingleQuoted("world".into()));
        assert_eq!(
            w[2],
            WordPart::DoubleQuoted(vec![WordPart::Variable("USER".into())])
        );
    }
}
