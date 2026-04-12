use super::ast::{CaseArm, IfClause, Script, Stmt};
use super::span::Span;
use super::tokenize::{Token, TokenKind};
use crate::Str;

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Parse a token stream into a [`Script`] AST.
pub fn parse(tokens: &[Token]) -> Result<Script, ParseError> {
    let mut pos = 0;
    let stmts = parse_compound_list(tokens, &mut pos, &[])?;
    skip_seps(tokens, &mut pos);
    if pos < tokens.len() {
        return Err(ParseError::new(
            pos,
            format!("unexpected token: {:?}", tokens[pos]),
        ));
    }
    Ok(Script { stmts })
}

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct ParseError {
    pub position: usize,
    pub message: String,
}

impl ParseError {
    fn new(position: usize, message: impl Into<String>) -> Self {
        Self {
            position,
            message: message.into(),
        }
    }
}

impl core::fmt::Display for ParseError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            f,
            "parse error at token {}: {}",
            self.position, self.message
        )
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Peek at the current token.
fn peek(tokens: &[Token], pos: usize) -> Option<&Token> {
    tokens.get(pos)
}

/// Peek at the word text of the current token (if it is a Word).
fn peek_word(tokens: &[Token], pos: usize) -> Option<&str> {
    tokens.get(pos).and_then(|t| t.as_word())
}

/// Return true if the current token is `Word(kw)`.
fn at_keyword(tokens: &[Token], pos: usize, kw: &str) -> bool {
    peek_word(tokens, pos) == Some(kw)
}

/// Consume a specific keyword. Fails if the current token doesn't match.
fn expect_keyword(tokens: &[Token], pos: &mut usize, kw: &str) -> Result<(), ParseError> {
    if at_keyword(tokens, *pos, kw) {
        *pos += 1;
        Ok(())
    } else {
        Err(ParseError::new(
            *pos,
            format!(
                "expected `{}`, got {:?}",
                kw,
                tokens.get(*pos).map(|t| t.to_source())
            ),
        ))
    }
}

/// Consume the next Word token and return its text.
fn expect_word(tokens: &[Token], pos: &mut usize) -> Result<Str, ParseError> {
    match tokens.get(*pos) {
        Some(t) => match &t.kind {
            TokenKind::Word(w) => {
                let w = w.clone();
                *pos += 1;
                Ok(w)
            }
            _ => Err(ParseError::new(
                *pos,
                format!("expected a word, got {:?}", t.to_source()),
            )),
        },
        None => Err(ParseError::new(*pos, "expected a word, got end of input")),
    }
}

/// Skip separator tokens (`;`, `Newline`).
fn skip_seps(tokens: &[Token], pos: &mut usize) {
    while let Some(t) = tokens.get(*pos) {
        if matches!(t.kind, TokenKind::Semi | TokenKind::Newline) {
            *pos += 1;
        } else {
            break;
        }
    }
}

/// Is this word a reserved keyword at all? (Used to detect block boundaries
/// so that simple commands don't accidentally swallow keywords.)
fn is_reserved(word: &str) -> bool {
    matches!(
        word,
        "if" | "then"
            | "elif"
            | "else"
            | "fi"
            | "for"
            | "in"
            | "do"
            | "done"
            | "while"
            | "until"
            | "case"
            | "esac"
            | "function"
            | "{"
            | "}"
            | "return"
            | "break"
            | "continue"
    )
}

/// Get the span of a token at a position, or Span::default() if out of bounds.
fn span_at(tokens: &[Token], pos: usize) -> Span {
    tokens.get(pos).map(|t| t.span).unwrap_or_default()
}

// ---------------------------------------------------------------------------
// Compound list — sequence of statements separated by `;` or `\n`
// ---------------------------------------------------------------------------

/// Parse a list of statements until we hit a terminating keyword (checked
/// only at command-position, i.e. after a separator).
fn parse_compound_list(
    tokens: &[Token],
    pos: &mut usize,
    terminators: &[&str],
) -> Result<Vec<Stmt>, ParseError> {
    let mut stmts = Vec::new();
    loop {
        skip_seps(tokens, pos);
        if *pos >= tokens.len() {
            break;
        }
        // Check for terminator keyword at command position
        if let Some(w) = peek_word(tokens, *pos) {
            if terminators.contains(&w) {
                break;
            }
        }
        stmts.push(parse_statement(tokens, pos, terminators)?);
    }
    Ok(stmts)
}

// ---------------------------------------------------------------------------
// Statement dispatch
// ---------------------------------------------------------------------------

fn parse_statement(
    tokens: &[Token],
    pos: &mut usize,
    terminators: &[&str],
) -> Result<Stmt, ParseError> {
    // Check first word for compound-command keywords
    if let Some(w) = peek_word(tokens, *pos) {
        match w {
            "if" => return parse_if(tokens, pos),
            "for" => return parse_for(tokens, pos),
            "while" => return parse_while(tokens, pos),
            "until" => return parse_until(tokens, pos),
            "case" => return parse_case(tokens, pos),
            "function" => return parse_function_keyword(tokens, pos),
            "break" => return parse_break(tokens, pos),
            "continue" => return parse_continue(tokens, pos),
            "return" => return parse_return(tokens, pos),
            _ => {}
        }
    }
    // Check for `name() {` function syntax
    if is_paren_function(tokens, *pos) {
        return parse_function_paren(tokens, pos);
    }
    // Check for brace group `{ list; }`
    if at_keyword(tokens, *pos, "{") {
        return parse_brace_group(tokens, pos);
    }
    // Check for subshell `( ... )`
    // Must be LParen but NOT followed by another LParen (that would be arithmetic (( )))
    if let Some(t) = peek(tokens, *pos) {
        if t.kind == TokenKind::LParen
            && !matches!(
                tokens.get(*pos + 1).map(|t| &t.kind),
                Some(TokenKind::LParen)
            )
        {
            return parse_subshell(tokens, pos);
        }
    }
    parse_simple(tokens, pos, terminators)
}

/// Detect `WORD ( )` pattern for function definition.
fn is_paren_function(tokens: &[Token], pos: usize) -> bool {
    if let (Some(t0), Some(t1), Some(t2)) =
        (tokens.get(pos), tokens.get(pos + 1), tokens.get(pos + 2))
    {
        if let TokenKind::Word(name) = &t0.kind {
            if !is_reserved(name) && t1.kind == TokenKind::LParen && t2.kind == TokenKind::RParen {
                return true;
            }
        }
    }
    false
}

// ---------------------------------------------------------------------------
// if / elif / else / fi
// ---------------------------------------------------------------------------

fn parse_if(tokens: &[Token], pos: &mut usize) -> Result<Stmt, ParseError> {
    let start = span_at(tokens, *pos);
    expect_keyword(tokens, pos, "if")?;
    let condition = parse_compound_list(tokens, pos, &["then"])?;
    expect_keyword(tokens, pos, "then")?;
    let body = parse_compound_list(tokens, pos, &["elif", "else", "fi"])?;

    let mut clauses = vec![IfClause { condition, body }];

    while at_keyword(tokens, *pos, "elif") {
        *pos += 1;
        let cond = parse_compound_list(tokens, pos, &["then"])?;
        expect_keyword(tokens, pos, "then")?;
        let body = parse_compound_list(tokens, pos, &["elif", "else", "fi"])?;
        clauses.push(IfClause {
            condition: cond,
            body,
        });
    }

    let else_body = if at_keyword(tokens, *pos, "else") {
        *pos += 1;
        Some(parse_compound_list(tokens, pos, &["fi"])?)
    } else {
        None
    };

    expect_keyword(tokens, pos, "fi")?;
    let end = span_at(tokens, pos.saturating_sub(1));

    Ok(Stmt::If {
        clauses,
        else_body,
        span: Span::merge(start, end),
    })
}

// ---------------------------------------------------------------------------
// for / in / do / done
// ---------------------------------------------------------------------------

fn parse_for(tokens: &[Token], pos: &mut usize) -> Result<Stmt, ParseError> {
    let start = span_at(tokens, *pos);
    expect_keyword(tokens, pos, "for")?;

    // Check for C-style: for (( init; cond; step ))
    if matches!(
        (tokens.get(*pos), tokens.get(*pos + 1)),
        (Some(t1), Some(t2)) if t1.kind == TokenKind::LParen && t2.kind == TokenKind::LParen
    ) {
        return parse_for_arith(tokens, pos, start);
    }

    let var = expect_word(tokens, pos)?;

    // Optional `in word...` clause
    let words = if at_keyword(tokens, *pos, "in") {
        *pos += 1;
        let mut words = Vec::new();
        loop {
            match peek(tokens, *pos) {
                Some(t) if matches!(t.kind, TokenKind::Semi | TokenKind::Newline) => break,
                Some(t) if t.is_word("do") => break,
                Some(t) => match &t.kind {
                    TokenKind::Word(w) => {
                        words.push(w.clone());
                        *pos += 1;
                    }
                    _ => break,
                },
                None => break,
            }
        }
        words
    } else {
        // No `in` → defaults to `"$@"`
        vec![Str::from("\"$@\"")]
    };

    skip_seps(tokens, pos);
    expect_keyword(tokens, pos, "do")?;
    let body = parse_compound_list(tokens, pos, &["done"])?;
    expect_keyword(tokens, pos, "done")?;
    let end = span_at(tokens, pos.saturating_sub(1));

    Ok(Stmt::For {
        var,
        words,
        body,
        span: Span::merge(start, end),
    })
}

/// Parse `(( init; cond; step )) do body done` — called after consuming `for`.
fn parse_for_arith(tokens: &[Token], pos: &mut usize, start: Span) -> Result<Stmt, ParseError> {
    // Consume the two `(` tokens
    *pos += 2; // skip `(` `(`

    // Collect all tokens until we hit `)` `)` (two consecutive RParen)
    let mut arith_text = String::new();
    while *pos < tokens.len() {
        // Check for )) — two consecutive RParen
        if matches!(
            (tokens.get(*pos), tokens.get(*pos + 1)),
            (Some(t1), Some(t2)) if t1.kind == TokenKind::RParen && t2.kind == TokenKind::RParen
        ) {
            *pos += 2; // consume ))
            break;
        }
        // Also accept a single token that might be `))`-style Word if tokenizer merges them
        if let Some(t) = tokens.get(*pos) {
            arith_text.push_str(t.to_source());
            arith_text.push(' ');
            *pos += 1;
        } else {
            return Err(ParseError::new(*pos, "unexpected end inside for (( ))"));
        }
    }

    // Split arith_text on `;` into init, cond, step
    let parts: Vec<&str> = arith_text.splitn(3, ';').collect();
    let init = parts.first().unwrap_or(&"").trim().to_string();
    let cond = parts.get(1).unwrap_or(&"").trim().to_string();
    let step = parts.get(2).unwrap_or(&"").trim().to_string();

    skip_seps(tokens, pos);
    expect_keyword(tokens, pos, "do")?;
    let body = parse_compound_list(tokens, pos, &["done"])?;
    expect_keyword(tokens, pos, "done")?;
    let end = span_at(tokens, pos.saturating_sub(1));

    Ok(Stmt::ForArith {
        init,
        cond,
        step,
        body,
        span: Span::merge(start, end),
    })
}

// ---------------------------------------------------------------------------
// while / until
// ---------------------------------------------------------------------------

fn parse_while(tokens: &[Token], pos: &mut usize) -> Result<Stmt, ParseError> {
    let start = span_at(tokens, *pos);
    expect_keyword(tokens, pos, "while")?;
    let condition = parse_compound_list(tokens, pos, &["do"])?;
    expect_keyword(tokens, pos, "do")?;
    let body = parse_compound_list(tokens, pos, &["done"])?;
    expect_keyword(tokens, pos, "done")?;
    let end = span_at(tokens, pos.saturating_sub(1));
    Ok(Stmt::While {
        condition,
        body,
        span: Span::merge(start, end),
    })
}

fn parse_until(tokens: &[Token], pos: &mut usize) -> Result<Stmt, ParseError> {
    let start = span_at(tokens, *pos);
    expect_keyword(tokens, pos, "until")?;
    let condition = parse_compound_list(tokens, pos, &["do"])?;
    expect_keyword(tokens, pos, "do")?;
    let body = parse_compound_list(tokens, pos, &["done"])?;
    expect_keyword(tokens, pos, "done")?;
    let end = span_at(tokens, pos.saturating_sub(1));
    Ok(Stmt::Until {
        condition,
        body,
        span: Span::merge(start, end),
    })
}

// ---------------------------------------------------------------------------
// case / esac  (stretch — basic support)
// ---------------------------------------------------------------------------

fn parse_case(tokens: &[Token], pos: &mut usize) -> Result<Stmt, ParseError> {
    let start = span_at(tokens, *pos);
    expect_keyword(tokens, pos, "case")?;
    let word = expect_word(tokens, pos)?;
    expect_keyword(tokens, pos, "in")?;
    skip_seps(tokens, pos);

    let mut arms = Vec::new();
    while !at_keyword(tokens, *pos, "esac") && *pos < tokens.len() {
        // Parse patterns: pat1 | pat2 | pat3 )
        let mut patterns = Vec::new();
        // Optional leading ( before first pattern
        if let Some(t) = peek(tokens, *pos) {
            if t.kind == TokenKind::LParen {
                *pos += 1;
            }
        }
        loop {
            let pat = expect_word(tokens, pos)?;
            patterns.push(pat);
            // Check for | (alternatives) — it's TokenKind::Pipe
            if let Some(t) = peek(tokens, *pos) {
                if t.kind == TokenKind::Pipe {
                    *pos += 1;
                } else {
                    break;
                }
            } else {
                break;
            }
        }
        // Expect )
        match peek(tokens, *pos) {
            Some(t) if t.kind == TokenKind::RParen => *pos += 1,
            _ => {
                return Err(ParseError::new(*pos, "expected `)` after case pattern"));
            }
        }

        // Parse body until ;; or esac
        let body = parse_case_body(tokens, pos)?;

        // Consume ;; if present
        if let Some(t) = peek(tokens, *pos) {
            if t.kind == TokenKind::DoubleSemi {
                *pos += 1;
            }
        }
        skip_seps(tokens, pos);

        arms.push(CaseArm { patterns, body });
    }

    expect_keyword(tokens, pos, "esac")?;
    let end = span_at(tokens, pos.saturating_sub(1));
    Ok(Stmt::Case {
        word,
        arms,
        span: Span::merge(start, end),
    })
}

/// Parse case arm body: statements until `;;` or `esac`.
fn parse_case_body(tokens: &[Token], pos: &mut usize) -> Result<Vec<Stmt>, ParseError> {
    let mut stmts = Vec::new();
    loop {
        skip_seps(tokens, pos);
        if *pos >= tokens.len() {
            break;
        }
        // Stop at ;; or esac
        if let Some(t) = peek(tokens, *pos) {
            if t.kind == TokenKind::DoubleSemi {
                break;
            }
        }
        if at_keyword(tokens, *pos, "esac") {
            break;
        }
        stmts.push(parse_statement(tokens, pos, &["esac"])?);
    }
    Ok(stmts)
}

// ---------------------------------------------------------------------------
// Function definitions
// ---------------------------------------------------------------------------

/// `function name [( )] { body; }`
fn parse_function_keyword(tokens: &[Token], pos: &mut usize) -> Result<Stmt, ParseError> {
    let start = span_at(tokens, *pos);
    expect_keyword(tokens, pos, "function")?;
    let name = expect_word(tokens, pos)?;
    // Optional `()`
    if let (Some(t0), Some(t1)) = (tokens.get(*pos), tokens.get(*pos + 1)) {
        if t0.kind == TokenKind::LParen && t1.kind == TokenKind::RParen {
            *pos += 2;
        }
    }
    skip_seps(tokens, pos);
    expect_keyword(tokens, pos, "{")?;
    let body = parse_compound_list(tokens, pos, &["}"])?;
    expect_keyword(tokens, pos, "}")?;
    let end = span_at(tokens, pos.saturating_sub(1));
    Ok(Stmt::FunctionDef {
        name,
        body,
        span: Span::merge(start, end),
    })
}

/// `name() { body; }`
fn parse_function_paren(tokens: &[Token], pos: &mut usize) -> Result<Stmt, ParseError> {
    let start = span_at(tokens, *pos);
    let name = expect_word(tokens, pos)?;
    // Consume `( )`
    match tokens.get(*pos) {
        Some(t) if t.kind == TokenKind::LParen => *pos += 1,
        _ => {
            return Err(ParseError::new(*pos, "expected `(`"));
        }
    }
    match tokens.get(*pos) {
        Some(t) if t.kind == TokenKind::RParen => *pos += 1,
        _ => {
            return Err(ParseError::new(*pos, "expected `)`"));
        }
    }
    skip_seps(tokens, pos);
    expect_keyword(tokens, pos, "{")?;
    let body = parse_compound_list(tokens, pos, &["}"])?;
    expect_keyword(tokens, pos, "}")?;
    let end = span_at(tokens, pos.saturating_sub(1));
    Ok(Stmt::FunctionDef {
        name,
        body,
        span: Span::merge(start, end),
    })
}

// ---------------------------------------------------------------------------
// subshell: ( cmd1; cmd2 )
// ---------------------------------------------------------------------------

fn parse_subshell(tokens: &[Token], pos: &mut usize) -> Result<Stmt, ParseError> {
    let start = span_at(tokens, *pos);
    // Consume the opening `(`
    match tokens.get(*pos) {
        Some(t) if t.kind == TokenKind::LParen => *pos += 1,
        _ => return Err(ParseError::new(*pos, "expected `(`")),
    }

    // Parse the body — we need a custom terminator approach since `)` is
    // Token::RParen, not a keyword word. We parse statements until we see RParen.
    let mut stmts = Vec::new();
    loop {
        skip_seps(tokens, pos);
        if *pos >= tokens.len() {
            return Err(ParseError::new(*pos, "expected `)` to close subshell"));
        }
        // Check for closing RParen
        if let Some(t) = peek(tokens, *pos) {
            if t.kind == TokenKind::RParen {
                break;
            }
        }
        stmts.push(parse_statement(tokens, pos, &[])?);
    }

    // Consume the closing `)`
    match tokens.get(*pos) {
        Some(t) if t.kind == TokenKind::RParen => *pos += 1,
        _ => return Err(ParseError::new(*pos, "expected `)` to close subshell")),
    }

    let end = span_at(tokens, pos.saturating_sub(1));
    Ok(Stmt::Subshell {
        body: stmts,
        span: Span::merge(start, end),
    })
}

// ---------------------------------------------------------------------------
// brace group: { cmd1; cmd2; }
// ---------------------------------------------------------------------------

fn parse_brace_group(tokens: &[Token], pos: &mut usize) -> Result<Stmt, ParseError> {
    let start = span_at(tokens, *pos);
    expect_keyword(tokens, pos, "{")?;
    let body = parse_compound_list(tokens, pos, &["}"])?;
    expect_keyword(tokens, pos, "}")?;
    let end = span_at(tokens, pos.saturating_sub(1));
    Ok(Stmt::BraceGroup {
        body,
        span: Span::merge(start, end),
    })
}

// ---------------------------------------------------------------------------
// break / continue / return
// ---------------------------------------------------------------------------

fn parse_break(tokens: &[Token], pos: &mut usize) -> Result<Stmt, ParseError> {
    let start = span_at(tokens, *pos);
    *pos += 1; // consume "break"
    let n = try_parse_u32(tokens, pos);
    let end = span_at(tokens, pos.saturating_sub(1));
    Ok(Stmt::Break(n, Span::merge(start, end)))
}

fn parse_continue(tokens: &[Token], pos: &mut usize) -> Result<Stmt, ParseError> {
    let start = span_at(tokens, *pos);
    *pos += 1; // consume "continue"
    let n = try_parse_u32(tokens, pos);
    let end = span_at(tokens, pos.saturating_sub(1));
    Ok(Stmt::Continue(n, Span::merge(start, end)))
}

fn parse_return(tokens: &[Token], pos: &mut usize) -> Result<Stmt, ParseError> {
    let start = span_at(tokens, *pos);
    *pos += 1; // consume "return"
               // Return argument can be a variable like $?, not just a literal
    let arg = match peek(tokens, *pos) {
        Some(t) if matches!(t.kind, TokenKind::Semi | TokenKind::Newline) => None,
        None => None,
        Some(t) => match &t.kind {
            TokenKind::Word(w) if is_reserved(w) => None,
            TokenKind::Word(w) => {
                let w = w.clone();
                *pos += 1;
                Some(w)
            }
            _ => None,
        },
    };
    let end = span_at(tokens, pos.saturating_sub(1));
    Ok(Stmt::Return(arg, Span::merge(start, end)))
}

fn try_parse_u32(tokens: &[Token], pos: &mut usize) -> Option<u32> {
    if let Some(t) = tokens.get(*pos) {
        if let TokenKind::Word(w) = &t.kind {
            if let Ok(n) = w.parse::<u32>() {
                *pos += 1;
                return Some(n);
            }
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Simple command — everything until a separator
// ---------------------------------------------------------------------------

/// Collect tokens into a simple command string. Stops at `;`, `\n`, or
/// a terminator keyword in command position (which only matters at the
/// start — once we've consumed at least one token we keep going until
/// a real separator).
fn parse_simple(
    tokens: &[Token],
    pos: &mut usize,
    _terminators: &[&str],
) -> Result<Stmt, ParseError> {
    let start_pos = *pos;
    let mut parts: Vec<String> = Vec::new();
    // Track parenthesis depth so that `(( expr ))` and `arr=(a b)` are
    // consumed as a single simple command, while an unmatched `)` (closing
    // a subshell or case pattern) stops the parse.
    let mut paren_depth: u32 = 0;
    loop {
        match peek(tokens, *pos) {
            None => break,
            Some(t) if matches!(t.kind, TokenKind::Newline | TokenKind::DoubleSemi) => break,
            // Semi (;) breaks the command only when followed by a reserved
            // keyword (then/do/done/fi/else/elif/esac/}) or newline/EOF.
            // Otherwise it's a chain operator within the same statement,
            // matching bash: `a; b; c` on one line = one prompt.
            Some(t) if t.kind == TokenKind::Semi => {
                let next = tokens.get(*pos + 1);
                let next_is_keyword_or_end = match next {
                    None => true,
                    Some(nt) if nt.kind == TokenKind::Newline => true,
                    Some(nt) => {
                        if let TokenKind::Word(w) = &nt.kind {
                            is_reserved(w)
                        } else {
                            false
                        }
                    }
                };
                if next_is_keyword_or_end {
                    break;
                }
                // Consume ; as part of this command
                parts.push(t.to_source().to_string());
                *pos += 1;
            }
            // RParen: only stop if it doesn't match an LParen we already
            // consumed (paren_depth == 0 means it belongs to an outer
            // construct like a subshell).
            Some(t) if t.kind == TokenKind::RParen && paren_depth == 0 => break,
            // Don't swallow tokens that belong to enclosing block structures.
            // A reserved word at the start of a command (parts is empty) that
            // isn't handled by parse_statement means we've hit a block-ender
            // like `fi`, `done`, `}`, etc.  Let the caller handle it.
            Some(t) => {
                if let TokenKind::Word(w) = &t.kind {
                    if parts.is_empty() && is_reserved(w) {
                        break;
                    }
                }
                if t.kind == TokenKind::LParen {
                    paren_depth += 1;
                } else if t.kind == TokenKind::RParen {
                    paren_depth = paren_depth.saturating_sub(1);
                }
                parts.push(t.to_source().to_string());
                *pos += 1;

                // After `&&`, `||`, or `|`, skip newlines (line continuation
                // semantics). This handles the case where normalization didn't
                // remove them (e.g. `&&\n` inside a simple command collected
                // across multiple separator-delimited pieces).
                if matches!(parts.last().map(|s| s.as_str()), Some("&&" | "||" | "|")) {
                    while let Some(t) = peek(tokens, *pos) {
                        if t.kind == TokenKind::Newline {
                            *pos += 1;
                        } else {
                            break;
                        }
                    }
                }
            }
        }
    }
    if parts.is_empty() {
        return Err(ParseError::new(*pos, "expected a command"));
    }

    let end_pos = *pos;
    let start = span_at(tokens, start_pos);
    let end = span_at(tokens, end_pos.saturating_sub(1));
    let span = Span::merge(start, end);

    // Smart join: avoid inserting spaces between adjacent parentheses
    // so that `(( expr ))` round-trips correctly instead of becoming `( ( expr ) )`.
    // Also avoid space between `VAR=` and `(` so that `arr=(a b c)` stays connected.
    let mut joined = String::new();
    for (i, part) in parts.iter().enumerate() {
        if i > 0 {
            let prev = &parts[i - 1];
            let need_space = !((prev == "(" && part == "(")
                || (prev == ")" && part == ")")
                || (prev == "(" && part == ")")
                || (prev.ends_with('=') && part == "(")
                || (prev.ends_with("+=") && part == "(")
                // |& — pipe with stderr: keep together so split_on_pipe sees |&
                || (prev == "|" && part == "&"));
            if need_space {
                joined.push(' ');
            }
        }
        joined.push_str(part);
    }
    let command_list = crate::script::word_parser::parse_command_line(&joined);
    Ok(Stmt::Structured {
        cmd: command_list,
        span,
    })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::script::tokenize::tokenize;

    fn parse_str(input: &str) -> Result<Script, String> {
        let tokens = tokenize(input).map_err(|e| format!("tokenize failed: {}", e))?;
        parse(&tokens).map_err(|e| format!("parse failed: {}", e))
    }

    fn stmts(input: &str) -> Result<Vec<Stmt>, String> {
        Ok(parse_str(input)?.stmts)
    }

    /// Check if a Stmt represents a simple command with the given source text.
    fn check_simple(stmt: &Stmt, expected: &str) -> Result<(), String> {
        match stmt {
            Stmt::Structured { cmd: cl, .. } => {
                let source = cl.to_source();
                if source != expected {
                    return Err(format!(
                        "Structured command source mismatch: got {:?}, expected {:?}",
                        source, expected
                    ));
                }
                Ok(())
            }
            other => Err(format!("expected structured command, got {:?}", other)),
        }
    }

    /// Check that a Vec<Stmt> matches a list of expected simple command strings.
    fn check_simple_list(stmts: &[Stmt], expected: &[&str]) -> Result<(), String> {
        if stmts.len() != expected.len() {
            return Err(format!(
                "statement count mismatch: got {}, expected {}",
                stmts.len(),
                expected.len()
            ));
        }
        for (stmt, exp) in stmts.iter().zip(expected.iter()) {
            check_simple(stmt, exp)?;
        }
        Ok(())
    }

    // -- Simple commands --

    #[test]
    fn simple_command() -> Result<(), String> {
        let s = stmts("echo hello")?;
        assert_eq!(s.len(), 1);
        check_simple(&s[0], "echo hello")?;
        Ok(())
    }

    #[test]
    fn two_commands_semicolon() -> Result<(), String> {
        // `echo a; echo b` on one line = one statement (bash semantics:
        // ; is a chain operator within a line, not a statement separator)
        let s = stmts("echo a; echo b")?;
        assert_eq!(s.len(), 1, "semicolon on one line = 1 statement");
        check_simple(&s[0], "echo a; echo b")?;
        Ok(())
    }

    #[test]
    fn two_commands_newline() -> Result<(), String> {
        let s = stmts("echo a\necho b")?;
        check_simple_list(&s, &["echo a", "echo b"])?;
        Ok(())
    }

    #[test]
    fn pipeline_in_simple() -> Result<(), String> {
        let s = stmts("ls | grep foo")?;
        assert_eq!(s.len(), 1);
        check_simple(&s[0], "ls | grep foo")?;
        Ok(())
    }

    #[test]
    fn chained_in_simple() -> Result<(), String> {
        let s = stmts("cmd1 && cmd2 || cmd3")?;
        assert_eq!(s.len(), 1);
        check_simple(&s[0], "cmd1 && cmd2 || cmd3")?;
        Ok(())
    }

    // -- if --

    #[test]
    fn if_then_fi() -> Result<(), String> {
        let s = stmts("if true; then echo yes; fi")?;
        assert_eq!(s.len(), 1);
        match &s[0] {
            Stmt::If {
                clauses, else_body, ..
            } => {
                assert_eq!(clauses.len(), 1);
                check_simple_list(&clauses[0].condition, &["true"])?;
                check_simple_list(&clauses[0].body, &["echo yes"])?;
                assert!(else_body.is_none());
            }
            other => return Err(format!("expected If, got {:?}", other)),
        }
        Ok(())
    }

    #[test]
    fn if_else() -> Result<(), String> {
        let s = stmts("if false; then echo no; else echo yes; fi")?;
        match &s[0] {
            Stmt::If {
                clauses, else_body, ..
            } => {
                check_simple_list(&clauses[0].condition, &["false"])?;
                check_simple_list(&clauses[0].body, &["echo no"])?;
                let eb = else_body.as_ref().ok_or("expected else body")?;
                check_simple_list(eb, &["echo yes"])?;
            }
            other => return Err(format!("expected If, got {:?}", other)),
        }
        Ok(())
    }

    #[test]
    fn if_elif_else() -> Result<(), String> {
        let s = stmts("if c1; then b1; elif c2; then b2; else b3; fi")?;
        match &s[0] {
            Stmt::If {
                clauses, else_body, ..
            } => {
                assert_eq!(clauses.len(), 2);
                check_simple_list(&clauses[0].condition, &["c1"])?;
                check_simple_list(&clauses[0].body, &["b1"])?;
                check_simple_list(&clauses[1].condition, &["c2"])?;
                check_simple_list(&clauses[1].body, &["b2"])?;
                let eb = else_body.as_ref().ok_or("expected else body")?;
                check_simple_list(eb, &["b3"])?;
            }
            other => return Err(format!("expected If, got {:?}", other)),
        }
        Ok(())
    }

    #[test]
    fn if_multiline() -> Result<(), String> {
        let input = "if true\nthen\n  echo yes\nfi";
        let s = stmts(input)?;
        match &s[0] {
            Stmt::If { clauses, .. } => {
                check_simple_list(&clauses[0].condition, &["true"])?;
                check_simple_list(&clauses[0].body, &["echo yes"])?;
            }
            other => return Err(format!("expected If, got {:?}", other)),
        }
        Ok(())
    }

    #[test]
    fn keyword_as_argument() -> Result<(), String> {
        // "then" after "echo" is an argument, not a keyword
        let s = stmts("if echo then; then echo fi; fi")?;
        match &s[0] {
            Stmt::If { clauses, .. } => {
                check_simple_list(&clauses[0].condition, &["echo then"])?;
                check_simple_list(&clauses[0].body, &["echo fi"])?;
            }
            other => return Err(format!("expected If, got {:?}", other)),
        }
        Ok(())
    }

    #[test]
    fn nested_if() -> Result<(), String> {
        let input = "if true; then if false; then echo inner; fi; echo outer; fi";
        let s = stmts(input)?;
        match &s[0] {
            Stmt::If { clauses, .. } => {
                assert_eq!(clauses[0].body.len(), 2);
                assert!(matches!(&clauses[0].body[0], Stmt::If { .. }));
                check_simple(&clauses[0].body[1], "echo outer")?;
            }
            other => return Err(format!("expected If, got {:?}", other)),
        }
        Ok(())
    }

    // -- for --

    #[test]
    fn for_loop() -> Result<(), String> {
        let s = stmts("for x in a b c; do echo $x; done")?;
        match &s[0] {
            Stmt::For {
                var, words, body, ..
            } => {
                assert_eq!(var, "x");
                assert_eq!(words, &["a", "b", "c"]);
                assert_eq!(body.len(), 1);
                check_simple(&body[0], "echo $x")?;
            }
            other => return Err(format!("expected For, got {:?}", other)),
        }
        Ok(())
    }

    #[test]
    fn for_multiline() -> Result<(), String> {
        let input = "for f in *.txt\ndo\n  cat $f\ndone";
        let s = stmts(input)?;
        match &s[0] {
            Stmt::For {
                var, words, body, ..
            } => {
                assert_eq!(var, "f");
                assert_eq!(words, &["*.txt"]);
                assert_eq!(body.len(), 1);
                check_simple(&body[0], "cat $f")?;
            }
            other => return Err(format!("expected For, got {:?}", other)),
        }
        Ok(())
    }

    // -- while --

    #[test]
    fn while_loop() -> Result<(), String> {
        let s = stmts("while true; do echo loop; done")?;
        match &s[0] {
            Stmt::While {
                condition, body, ..
            } => {
                check_simple_list(condition, &["true"])?;
                check_simple_list(body, &["echo loop"])?;
            }
            other => return Err(format!("expected While, got {:?}", other)),
        }
        Ok(())
    }

    // -- until --

    #[test]
    fn until_loop() -> Result<(), String> {
        let s = stmts("until false; do echo loop; done")?;
        match &s[0] {
            Stmt::Until {
                condition, body, ..
            } => {
                check_simple_list(condition, &["false"])?;
                check_simple_list(body, &["echo loop"])?;
            }
            other => return Err(format!("expected Until, got {:?}", other)),
        }
        Ok(())
    }

    // -- function --

    #[test]
    fn function_paren_syntax() -> Result<(), String> {
        let s = stmts("greet() { echo hello; }")?;
        match &s[0] {
            Stmt::FunctionDef { name, body, .. } => {
                assert_eq!(name, "greet");
                assert_eq!(body.len(), 1);
                check_simple(&body[0], "echo hello")?;
            }
            other => return Err(format!("expected FunctionDef, got {:?}", other)),
        }
        Ok(())
    }

    #[test]
    fn function_keyword_syntax() -> Result<(), String> {
        let s = stmts("function greet { echo hello; }")?;
        match &s[0] {
            Stmt::FunctionDef { name, body, .. } => {
                assert_eq!(name, "greet");
                assert_eq!(body.len(), 1);
                check_simple(&body[0], "echo hello")?;
            }
            other => return Err(format!("expected FunctionDef, got {:?}", other)),
        }
        Ok(())
    }

    #[test]
    fn function_keyword_with_parens() -> Result<(), String> {
        let s = stmts("function greet() { echo hello; }")?;
        match &s[0] {
            Stmt::FunctionDef { name, body, .. } => {
                assert_eq!(name, "greet");
                assert_eq!(body.len(), 1);
                check_simple(&body[0], "echo hello")?;
            }
            other => return Err(format!("expected FunctionDef, got {:?}", other)),
        }
        Ok(())
    }

    // -- break / continue / return --

    #[test]
    fn break_simple() -> Result<(), String> {
        let s = stmts("break")?;
        assert_eq!(s.len(), 1);
        assert!(
            matches!(&s[0], Stmt::Break(None, _)),
            "expected Break(None), got {:?}",
            s[0]
        );
        Ok(())
    }

    #[test]
    fn break_with_level() -> Result<(), String> {
        let s = stmts("break 2")?;
        assert_eq!(s.len(), 1);
        assert!(
            matches!(&s[0], Stmt::Break(Some(2), _)),
            "expected Break(Some(2)), got {:?}",
            s[0]
        );
        Ok(())
    }

    #[test]
    fn continue_simple() -> Result<(), String> {
        let s = stmts("continue")?;
        assert_eq!(s.len(), 1);
        assert!(
            matches!(&s[0], Stmt::Continue(None, _)),
            "expected Continue(None), got {:?}",
            s[0]
        );
        Ok(())
    }

    #[test]
    fn return_simple() -> Result<(), String> {
        let s = stmts("return")?;
        assert_eq!(s.len(), 1);
        assert!(
            matches!(&s[0], Stmt::Return(None, _)),
            "expected Return(None), got {:?}",
            s[0]
        );
        Ok(())
    }

    #[test]
    fn return_with_code() -> Result<(), String> {
        let s = stmts("return 0")?;
        assert_eq!(s.len(), 1);
        assert!(
            matches!(&s[0], Stmt::Return(Some(v), _) if v == "0"),
            "expected Return(Some(\"0\")), got {:?}",
            s[0]
        );
        Ok(())
    }

    #[test]
    fn return_with_var() -> Result<(), String> {
        let s = stmts("return $?")?;
        assert_eq!(s.len(), 1);
        assert!(
            matches!(&s[0], Stmt::Return(Some(v), _) if v == "$?"),
            "expected Return(Some(\"$?\")), got {:?}",
            s[0]
        );
        Ok(())
    }

    // -- edge cases --

    #[test]
    fn empty_script() -> Result<(), String> {
        assert_eq!(stmts("")?, Vec::<Stmt>::new());
        Ok(())
    }

    #[test]
    fn only_separators() -> Result<(), String> {
        assert_eq!(stmts(";\n;\n")?, Vec::<Stmt>::new());
        Ok(())
    }

    #[test]
    fn function_then_call() -> Result<(), String> {
        let s = stmts("greet() { echo hi; }\ngreet")?;
        assert_eq!(s.len(), 2);
        assert!(matches!(&s[0], Stmt::FunctionDef { name, .. } if name == "greet"));
        check_simple(&s[1], "greet")?;
        Ok(())
    }

    #[test]
    fn for_inside_if() -> Result<(), String> {
        let input = "if true; then for x in a b; do echo $x; done; fi";
        let s = stmts(input)?;
        match &s[0] {
            Stmt::If { clauses, .. } => {
                assert_eq!(clauses[0].body.len(), 1);
                assert!(matches!(&clauses[0].body[0], Stmt::For { .. }));
            }
            other => return Err(format!("expected If, got {:?}", other)),
        }
        Ok(())
    }

    #[test]
    fn if_with_pipeline_condition() -> Result<(), String> {
        let s = stmts("if echo test | grep -q t; then echo found; fi")?;
        match &s[0] {
            Stmt::If { clauses, .. } => {
                check_simple_list(&clauses[0].condition, &["echo test | grep -q t"])?;
            }
            other => return Err(format!("expected If, got {:?}", other)),
        }
        Ok(())
    }

    // -- Span tracking tests --

    #[test]
    fn span_simple_command() -> Result<(), String> {
        let s = stmts("echo hello")?;
        assert_eq!(s[0].span().start_line, 0);
        assert_eq!(s[0].span().end_line, 0);
        Ok(())
    }

    #[test]
    fn span_multiline_if_block() -> Result<(), String> {
        let s = stmts("if true\nthen\n  echo yes\nfi")?;
        // The if statement should span from line 0 to line 3
        assert_eq!(s[0].span().start_line, 0);
        assert_eq!(s[0].span().end_line, 3);
        Ok(())
    }

    #[test]
    fn span_multiple_statements() -> Result<(), String> {
        let s = stmts("echo a\necho b\necho c")?;
        assert_eq!(s[0].span().start_line, 0);
        assert_eq!(s[0].span().end_line, 0);
        assert_eq!(s[1].span().start_line, 1);
        assert_eq!(s[1].span().end_line, 1);
        assert_eq!(s[2].span().start_line, 2);
        assert_eq!(s[2].span().end_line, 2);
        Ok(())
    }
}
