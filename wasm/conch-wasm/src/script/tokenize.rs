use winnow::combinator::alt;
use winnow::error::{ContextError, ErrMode};
use winnow::prelude::*;
use winnow::token::{any, take_till, take_while};

use super::span::Span;
use crate::Str;

/// The kind of a shell token (the payload without location info).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TokenKind {
    /// A word (command name, argument, variable, quoted string, etc.).
    Word(Str),
    /// `;`
    Semi,
    /// `\n` (or `\r\n`)
    Newline,
    /// `;;`
    DoubleSemi,
    /// `&&`
    AndIf,
    /// `||`
    OrIf,
    /// `|`
    Pipe,
    /// `(`
    LParen,
    /// `)`
    RParen,
}

impl TokenKind {
    /// Reconstruct the source text for this token kind.
    pub fn to_source(&self) -> &str {
        match self {
            TokenKind::Word(s) => s,
            TokenKind::Semi => ";",
            TokenKind::Newline => "\n",
            TokenKind::DoubleSemi => ";;",
            TokenKind::AndIf => "&&",
            TokenKind::OrIf => "||",
            TokenKind::Pipe => "|",
            TokenKind::LParen => "(",
            TokenKind::RParen => ")",
        }
    }
}

/// Shell token produced by the tokenizer — a kind plus source location.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
}

impl Token {
    /// Create a token with a default (zero) span — useful in tests.
    pub fn dummy(kind: TokenKind) -> Self {
        Token {
            kind,
            span: Span::default(),
        }
    }

    /// If this token is a Word, return its text.
    pub fn as_word(&self) -> Option<&str> {
        match &self.kind {
            TokenKind::Word(s) => Some(s),
            _ => None,
        }
    }

    /// Check if this token is a Word matching the expected string.
    pub fn is_word(&self, expected: &str) -> bool {
        matches!(&self.kind, TokenKind::Word(s) if s == expected)
    }

    /// Reconstruct the source text for this token.
    pub fn to_source(&self) -> &str {
        self.kind.to_source()
    }
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Tokenize a shell script into a vector of [`Token`]s.
pub fn tokenize(input: &str) -> Result<Vec<Token>, String> {
    // Strip backslash-newline continuations, but NOT inside single quotes
    // where they should be preserved literally.
    let input = strip_backslash_newline(input);
    let raw = tokenize_raw_with_spans(&input)?;
    Ok(normalize(raw))
}

/// Strip `\<newline>` continuations from input, but preserve them inside
/// single quotes where backslash has no special meaning.
fn strip_backslash_newline(input: &str) -> String {
    let mut result = String::with_capacity(input.len());
    let bytes = input.as_bytes();
    let mut i = 0;
    let mut in_single_quote = false;
    while i < bytes.len() {
        if in_single_quote {
            if bytes[i] == b'\'' {
                in_single_quote = false;
            }
            result.push(bytes[i] as char);
            i += 1;
        } else {
            if bytes[i] == b'\'' {
                in_single_quote = true;
                result.push('\'');
                i += 1;
            } else if bytes[i] == b'\\' {
                if i + 1 < bytes.len() && bytes[i + 1] == b'\n' {
                    i += 2; // strip \<newline>
                } else if i + 2 < bytes.len() && bytes[i + 1] == b'\r' && bytes[i + 2] == b'\n' {
                    i += 3; // strip \<CR><newline>
                } else {
                    result.push('\\');
                    i += 1;
                }
            } else {
                result.push(bytes[i] as char);
                i += 1;
            }
        }
    }
    result
}

// ---------------------------------------------------------------------------
// Top-level tokenizer with span tracking
// ---------------------------------------------------------------------------

/// Produce the token stream with spans (before normalization).
fn tokenize_raw_with_spans(full_input: &str) -> Result<Vec<Token>, String> {
    let mut input: &str = full_input;
    let mut tokens = Vec::new();
    let mut current_line: u32 = 0;

    loop {
        // Skip ignorable content (whitespace, comments)
        let before_skip = input;
        skip_ignorable(&mut input);
        // Count newlines in skipped content
        let skipped = &before_skip[..before_skip.len() - input.len()];
        current_line += skipped.chars().filter(|&c| c == '\n').count() as u32;

        if input.is_empty() {
            break;
        }

        let start_byte = (full_input.len() - input.len()) as u32;
        let start_line = current_line;

        let kind = next_token(&mut input).map_err(|e| format!("tokenize error: {e}"))?;

        let end_byte = (full_input.len() - input.len()) as u32;

        // Count newlines in token content to determine end_line.
        // For Newline tokens: the token represents the end of the current line,
        // so its span stays on start_line. After it, we advance current_line.
        let token_text = &full_input[start_byte as usize..end_byte as usize];
        let end_line;
        if matches!(kind, TokenKind::Newline) {
            end_line = start_line;
            current_line = start_line + 1;
        } else {
            let newlines_in_token = token_text.chars().filter(|&c| c == '\n').count() as u32;
            end_line = start_line + newlines_in_token;
            current_line = end_line;
        }

        tokens.push(Token {
            kind,
            span: Span::new(start_line, end_line, start_byte, end_byte),
        });
    }
    Ok(tokens)
}

/// Skip horizontal whitespace, backslash-newline continuations, and comments.
fn skip_ignorable(input: &mut &str) {
    loop {
        let before = input.len();
        // Spaces and tabs
        let _ = take_while::<_, _, ContextError>(0.., |c: char| c == ' ' || c == '\t')
            .parse_next(input);
        // Backslash-newline continuation
        if input.starts_with("\\\n") {
            *input = &input[2..];
            continue;
        }
        if input.starts_with("\\\r\n") {
            *input = &input[3..];
            continue;
        }
        // Comment (# to end of line, not including the newline)
        if input.starts_with('#') {
            let _ = take_till::<_, _, ContextError>(0.., |c: char| c == '\n').parse_next(input);
            continue;
        }
        if input.len() == before {
            break;
        }
    }
}

/// Parse the next token from the input.
fn next_token(input: &mut &str) -> ModalResult<TokenKind> {
    alt((newline_token, operator_token, word_token)).parse_next(input)
}

// ---------------------------------------------------------------------------
// Newline
// ---------------------------------------------------------------------------

fn newline_token(input: &mut &str) -> ModalResult<TokenKind> {
    alt(("\r\n", "\n"))
        .value(TokenKind::Newline)
        .parse_next(input)
}

// ---------------------------------------------------------------------------
// Operators  (longest-match ordering)
// ---------------------------------------------------------------------------

fn operator_token(input: &mut &str) -> ModalResult<TokenKind> {
    alt((multi_char_op, single_char_op)).parse_next(input)
}

fn multi_char_op(input: &mut &str) -> ModalResult<TokenKind> {
    alt((
        ";;".value(TokenKind::DoubleSemi),
        ">>".value(TokenKind::Word(">>".into())),
        "<<".value(TokenKind::Word("<<".into())),
        "&&".value(TokenKind::AndIf),
        "||".value(TokenKind::OrIf),
    ))
    .parse_next(input)
}

fn single_char_op(input: &mut &str) -> ModalResult<TokenKind> {
    // Don't match < or > when followed by ( — that's process substitution
    if input.starts_with("<(") || input.starts_with(">(") {
        return Err(winnow::error::ErrMode::Backtrack(
            winnow::error::ContextError::new(),
        ));
    }
    alt((
        ";".value(TokenKind::Semi),
        "|".value(TokenKind::Pipe),
        "(".value(TokenKind::LParen),
        ")".value(TokenKind::RParen),
        ">".value(TokenKind::Word(">".into())),
        "<".value(TokenKind::Word("<".into())),
        "&".value(TokenKind::Word("&".into())),
    ))
    .parse_next(input)
}

// ---------------------------------------------------------------------------
// Shell word (handles quoting, escapes, dollar-forms)
// ---------------------------------------------------------------------------

fn word_token(input: &mut &str) -> ModalResult<TokenKind> {
    let start = *input;
    // Must match at least one segment
    word_segment(input)?;
    // Greedily consume more segments
    while word_segment(input).is_ok() {}
    let consumed = &start[..start.len() - input.len()];
    Ok(TokenKind::Word(consumed.into()))
}

/// One segment of a shell word.
fn word_segment(input: &mut &str) -> ModalResult<()> {
    alt((
        process_subst,
        dollar_single_quoted,
        single_quoted,
        double_quoted,
        backtick,
        backslash_char,
        dollar_brace,
        dollar_paren,
        alt((dollar_simple, unquoted_chars)),
    ))
    .parse_next(input)
}

/// `<(cmd)` or `>(cmd)` — process substitution.
fn process_subst(input: &mut &str) -> ModalResult<()> {
    if !(input.starts_with("<(") || input.starts_with(">(")) {
        return Err(winnow::error::ErrMode::Backtrack(
            winnow::error::ContextError::new(),
        ));
    }
    // Consume the <( or >(
    let _: char = any(input)?; // < or >
    let _: char = any(input)?; // (
    let mut depth: u32 = 1;
    while depth > 0 && !input.is_empty() {
        if input.starts_with('(') {
            depth += 1;
            let _: char = any(input)?;
        } else if input.starts_with(')') {
            depth -= 1;
            let _: char = any(input)?;
        } else {
            let _: char = any(input)?;
        }
    }
    Ok(())
}

/// `$'...'` — ANSI-C quoting. Keeps the `$'...'` markers intact for later
/// processing by the execution engine.
fn dollar_single_quoted(input: &mut &str) -> ModalResult<()> {
    "$'".parse_next(input)?;
    loop {
        if input.is_empty() {
            return Err(ErrMode::Backtrack(ContextError::new()));
        }
        if input.starts_with('\'') {
            let _: char = any(input)?;
            return Ok(());
        }
        if input.starts_with('\\') {
            let _: char = any(input)?; // consume backslash
            if !input.is_empty() {
                let _: char = any(input)?; // consume escaped char
            }
        } else {
            let _: char = any(input)?;
        }
    }
}

/// `'...'` — everything literal until closing quote.
fn single_quoted(input: &mut &str) -> ModalResult<()> {
    '\''.parse_next(input)?;
    take_till(0.., |c: char| c == '\'').parse_next(input)?;
    '\''.parse_next(input)?;
    Ok(())
}

/// `"..."` — with `\"`, `\\`, `\$`, `` \` `` escape handling.
fn double_quoted(input: &mut &str) -> ModalResult<()> {
    '"'.parse_next(input)?;
    loop {
        if input.is_empty() {
            return Err(ErrMode::Backtrack(ContextError::new()));
        }
        if input.starts_with('"') {
            let _: char = any(input)?;
            return Ok(());
        }
        if input.starts_with('\\') {
            let _: char = any(input)?; // consume backslash
            if !input.is_empty() {
                let _: char = any(input)?; // consume escaped char
            }
        } else {
            let _: char = any(input)?;
        }
    }
}

/// `` `...` `` — legacy command substitution.
fn backtick(input: &mut &str) -> ModalResult<()> {
    '`'.parse_next(input)?;
    take_till(0.., |c: char| c == '`').parse_next(input)?;
    '`'.parse_next(input)?;
    Ok(())
}

/// `\x` — backslash-escaped character.
fn backslash_char(input: &mut &str) -> ModalResult<()> {
    '\\'.parse_next(input)?;
    let _: char = any(input)?;
    Ok(())
}

/// `${...}` — brace-delimited variable expansion.
fn dollar_brace(input: &mut &str) -> ModalResult<()> {
    "${".parse_next(input)?;
    // Handle nested braces (e.g. ${var:-${default}})
    let mut depth: u32 = 1;
    while depth > 0 && !input.is_empty() {
        if input.starts_with('{') {
            depth += 1;
        } else if input.starts_with('}') {
            depth -= 1;
            if depth == 0 {
                let _: char = any(input)?;
                return Ok(());
            }
        }
        let _: char = any(input)?;
    }
    Err(ErrMode::Backtrack(ContextError::new()))
}

/// `$(...)` — command substitution (handles nesting).
fn dollar_paren(input: &mut &str) -> ModalResult<()> {
    "$(".parse_next(input)?;
    let mut depth: u32 = 1;
    while depth > 0 && !input.is_empty() {
        if input.starts_with('(') {
            depth += 1;
            let _: char = any(input)?;
        } else if input.starts_with(')') {
            depth -= 1;
            let _: char = any(input)?;
        } else {
            let _: char = any(input)?;
        }
    }
    Ok(())
}

/// `$VAR`, `$?`, `$$`, `$!`, `$#`, `$@`, `$*`, `$0`–`$9`, or bare `$`.
fn dollar_simple(input: &mut &str) -> ModalResult<()> {
    '$'.parse_next(input)?;
    // Try special single-char parameters, or alphanumeric name, or just bare $
    if !input.is_empty() {
        let Some(next) = input.chars().next() else {
            return Ok(());
        };
        if "?$!#@*-0123456789".contains(next) {
            let _: char = any(input)?;
        } else if next.is_ascii_alphabetic() || next == '_' {
            let _ = take_while::<_, _, ContextError>(1.., |c: char| {
                c.is_ascii_alphanumeric() || c == '_'
            })
            .parse_next(input);
        }
        // else: bare $ — already consumed, that's fine
    }
    Ok(())
}

/// Unquoted word characters — anything that isn't a metacharacter or quoting char.
fn unquoted_chars(input: &mut &str) -> ModalResult<()> {
    take_while(1.., |c: char| !is_meta_or_quote(c))
        .void()
        .parse_next(input)
}

fn is_meta_or_quote(c: char) -> bool {
    matches!(
        c,
        ' ' | '\t'
            | '\n'
            | '\r'
            | ';'
            | '&'
            | '|'
            | '('
            | ')'
            | '<'
            | '>'
            | '\''
            | '"'
            | '\\'
            | '$'
            | '`'
    )
}

// ---------------------------------------------------------------------------
// Normalization pass
// ---------------------------------------------------------------------------

/// Post-process the raw token stream:
/// - Remove `Newline` tokens after `&&`, `||`, `|` (line continuation).
fn normalize(tokens: Vec<Token>) -> Vec<Token> {
    let mut out: Vec<Token> = Vec::with_capacity(tokens.len());
    for tok in tokens {
        if tok.kind == TokenKind::Newline {
            if let Some(prev) = out.last() {
                if matches!(
                    prev.kind,
                    TokenKind::AndIf | TokenKind::OrIf | TokenKind::Pipe
                ) {
                    continue;
                }
            }
        }
        out.push(tok);
    }
    out
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn tok_kinds(input: &str) -> Result<Vec<TokenKind>, String> {
        let tokens = tokenize(input).map_err(|e| format!("tokenize failed: {}", e))?;
        Ok(tokens.into_iter().map(|t| t.kind).collect())
    }

    fn wk(s: &str) -> TokenKind {
        TokenKind::Word(s.into())
    }

    #[test]
    fn simple_command() -> Result<(), String> {
        assert_eq!(
            tok_kinds("echo hello world")?,
            vec![wk("echo"), wk("hello"), wk("world")]
        );
        Ok(())
    }

    #[test]
    fn pipeline() -> Result<(), String> {
        assert_eq!(
            tok_kinds("ls | grep foo")?,
            vec![wk("ls"), TokenKind::Pipe, wk("grep"), wk("foo")]
        );
        Ok(())
    }

    #[test]
    fn chained() -> Result<(), String> {
        assert_eq!(
            tok_kinds("cmd1 && cmd2 || cmd3")?,
            vec![
                wk("cmd1"),
                TokenKind::AndIf,
                wk("cmd2"),
                TokenKind::OrIf,
                wk("cmd3")
            ]
        );
        Ok(())
    }

    #[test]
    fn semicolons_and_newlines() -> Result<(), String> {
        assert_eq!(
            tok_kinds("a; b\nc")?,
            vec![
                wk("a"),
                TokenKind::Semi,
                wk("b"),
                TokenKind::Newline,
                wk("c")
            ]
        );
        Ok(())
    }

    #[test]
    fn single_quoted() -> Result<(), String> {
        assert_eq!(
            tok_kinds("echo 'hello world'")?,
            vec![wk("echo"), wk("'hello world'")]
        );
        Ok(())
    }

    #[test]
    fn double_quoted() -> Result<(), String> {
        assert_eq!(
            tok_kinds(r#"echo "hello $USER""#)?,
            vec![wk("echo"), wk("\"hello $USER\"")]
        );
        Ok(())
    }

    #[test]
    fn double_quoted_escape() -> Result<(), String> {
        assert_eq!(
            tok_kinds(r#"echo "say \"hi\"""#)?,
            vec![wk("echo"), wk(r#""say \"hi\"""#)]
        );
        Ok(())
    }

    #[test]
    fn backslash_escape() -> Result<(), String> {
        assert_eq!(
            tok_kinds(r"echo hello\ world")?,
            vec![wk("echo"), wk(r"hello\ world")]
        );
        Ok(())
    }

    #[test]
    fn dollar_var() -> Result<(), String> {
        assert_eq!(tok_kinds("echo $HOME")?, vec![wk("echo"), wk("$HOME")]);
        Ok(())
    }

    #[test]
    fn dollar_brace_var() -> Result<(), String> {
        assert_eq!(tok_kinds("echo ${HOME}")?, vec![wk("echo"), wk("${HOME}")]);
        Ok(())
    }

    #[test]
    fn dollar_paren_subst() -> Result<(), String> {
        assert_eq!(
            tok_kinds("echo $(whoami)")?,
            vec![wk("echo"), wk("$(whoami)")]
        );
        Ok(())
    }

    #[test]
    fn redirects_as_words() -> Result<(), String> {
        assert_eq!(
            tok_kinds("echo hello > file.txt")?,
            vec![wk("echo"), wk("hello"), wk(">"), wk("file.txt")]
        );
        Ok(())
    }

    #[test]
    fn append_redirect() -> Result<(), String> {
        assert_eq!(
            tok_kinds("echo hello >> file.txt")?,
            vec![wk("echo"), wk("hello"), wk(">>"), wk("file.txt")]
        );
        Ok(())
    }

    #[test]
    fn if_then_fi() -> Result<(), String> {
        assert_eq!(
            tok_kinds("if true; then echo yes; fi")?,
            vec![
                wk("if"),
                wk("true"),
                TokenKind::Semi,
                wk("then"),
                wk("echo"),
                wk("yes"),
                TokenKind::Semi,
                wk("fi"),
            ]
        );
        Ok(())
    }

    #[test]
    fn for_loop() -> Result<(), String> {
        assert_eq!(
            tok_kinds("for x in a b c; do echo $x; done")?,
            vec![
                wk("for"),
                wk("x"),
                wk("in"),
                wk("a"),
                wk("b"),
                wk("c"),
                TokenKind::Semi,
                wk("do"),
                wk("echo"),
                wk("$x"),
                TokenKind::Semi,
                wk("done"),
            ]
        );
        Ok(())
    }

    #[test]
    fn function_def() -> Result<(), String> {
        assert_eq!(
            tok_kinds("greet() { echo hello; }")?,
            vec![
                wk("greet"),
                TokenKind::LParen,
                TokenKind::RParen,
                wk("{"),
                wk("echo"),
                wk("hello"),
                TokenKind::Semi,
                wk("}"),
            ]
        );
        Ok(())
    }

    #[test]
    fn comment_skipped() -> Result<(), String> {
        assert_eq!(
            tok_kinds("echo hello # this is a comment\necho world")?,
            vec![
                wk("echo"),
                wk("hello"),
                TokenKind::Newline,
                wk("echo"),
                wk("world")
            ]
        );
        Ok(())
    }

    #[test]
    fn line_continuation_backslash() -> Result<(), String> {
        assert_eq!(tok_kinds("echo hel\\\nlo")?, vec![wk("echo"), wk("hello")]);
        Ok(())
    }

    #[test]
    fn line_continuation_after_and() -> Result<(), String> {
        // Newline after && is swallowed during normalization
        assert_eq!(
            tok_kinds("cmd1 &&\ncmd2")?,
            vec![wk("cmd1"), TokenKind::AndIf, wk("cmd2")]
        );
        Ok(())
    }

    #[test]
    fn line_continuation_after_pipe() -> Result<(), String> {
        assert_eq!(
            tok_kinds("cmd1 |\ncmd2")?,
            vec![wk("cmd1"), TokenKind::Pipe, wk("cmd2")]
        );
        Ok(())
    }

    #[test]
    fn parens_break_words() -> Result<(), String> {
        // ( and ) are metacharacters — they break words
        assert_eq!(
            tok_kinds("f()")?,
            vec![wk("f"), TokenKind::LParen, TokenKind::RParen]
        );
        Ok(())
    }

    #[test]
    fn braces_are_words() -> Result<(), String> {
        // { and } are NOT metacharacters — they're regular word chars
        assert_eq!(
            tok_kinds("{ echo; }")?,
            vec![wk("{"), wk("echo"), TokenKind::Semi, wk("}")]
        );
        Ok(())
    }

    #[test]
    fn hash_inside_word() -> Result<(), String> {
        // # inside a word is not a comment
        assert_eq!(tok_kinds("echo foo#bar")?, vec![wk("echo"), wk("foo#bar")]);
        Ok(())
    }

    #[test]
    fn empty_input() -> Result<(), String> {
        assert_eq!(tok_kinds("")?, Vec::<TokenKind>::new());
        Ok(())
    }

    #[test]
    fn only_comments() -> Result<(), String> {
        assert_eq!(tok_kinds("# just a comment")?, Vec::<TokenKind>::new());
        Ok(())
    }

    #[test]
    fn double_semi() -> Result<(), String> {
        assert_eq!(
            tok_kinds("pattern);; esac")?,
            vec![
                wk("pattern"),
                TokenKind::RParen,
                TokenKind::DoubleSemi,
                wk("esac")
            ]
        );
        Ok(())
    }

    #[test]
    fn mixed_quotes() -> Result<(), String> {
        assert_eq!(
            tok_kinds(r#"echo "hello"' world'"#)?,
            vec![wk("echo"), wk(r#""hello"' world'"#)]
        );
        Ok(())
    }

    #[test]
    fn backtick_substitution() -> Result<(), String> {
        assert_eq!(
            tok_kinds("echo `whoami`")?,
            vec![wk("echo"), wk("`whoami`")]
        );
        Ok(())
    }

    #[test]
    fn bare_dollar() -> Result<(), String> {
        assert_eq!(tok_kinds("echo $")?, vec![wk("echo"), wk("$")]);
        Ok(())
    }

    // -- Fix #1: backslash-newline inside single quotes preserved --

    #[test]
    fn backslash_newline_preserved_in_single_quotes() -> Result<(), String> {
        // Inside single quotes, \<newline> should be preserved literally
        let input = "echo 'hello\\\nworld'";
        let kinds: Vec<TokenKind> = tokenize(input)
            .map_err(|e| format!("tokenize failed: {}", e))?
            .into_iter()
            .map(|t| t.kind)
            .collect();
        assert_eq!(kinds, vec![wk("echo"), wk("'hello\\\nworld'")]);
        Ok(())
    }

    #[test]
    fn backslash_newline_stripped_outside_quotes() -> Result<(), String> {
        // Outside quotes, \<newline> is a line continuation
        assert_eq!(tok_kinds("echo hel\\\nlo")?, vec![wk("echo"), wk("hello")]);
        Ok(())
    }

    // -- Span tracking tests --

    #[test]
    fn span_single_line_command() -> Result<(), String> {
        let tokens = tokenize("echo hello").map_err(|e| format!("tokenize failed: {}", e))?;
        assert_eq!(tokens.len(), 2);
        // "echo" at bytes 0..4, line 0
        assert_eq!(tokens[0].span, Span::new(0, 0, 0, 4));
        // "hello" at bytes 5..10, line 0
        assert_eq!(tokens[1].span, Span::new(0, 0, 5, 10));
        Ok(())
    }

    #[test]
    fn span_multi_line() -> Result<(), String> {
        let tokens = tokenize("echo a\necho b").map_err(|e| format!("tokenize failed: {}", e))?;
        // echo(0,0,0,4) a(0,0,5,6) \n(0,0,6,7) echo(1,1,7,11) b(1,1,12,13)
        assert_eq!(tokens[0].span.start_line, 0);
        assert_eq!(tokens[2].kind, TokenKind::Newline);
        assert_eq!(tokens[2].span.start_line, 0);
        assert_eq!(tokens[3].span.start_line, 1); // "echo" on line 1
        assert_eq!(tokens[4].span.start_line, 1); // "b" on line 1
        Ok(())
    }

    #[test]
    fn span_multiline_if() -> Result<(), String> {
        let tokens = tokenize("if true\nthen\n  echo yes\nfi")
            .map_err(|e| format!("tokenize failed: {}", e))?;
        // "if" on line 0
        assert_eq!(tokens[0].span.start_line, 0);
        // "true" on line 0
        assert_eq!(tokens[1].span.start_line, 0);
        // newline on line 0
        assert_eq!(tokens[2].kind, TokenKind::Newline);
        // "then" on line 1
        assert_eq!(tokens[3].span.start_line, 1);
        // newline on line 1
        assert_eq!(tokens[4].kind, TokenKind::Newline);
        // "echo" on line 2
        assert_eq!(tokens[5].span.start_line, 2);
        // "yes" on line 2
        assert_eq!(tokens[6].span.start_line, 2);
        // newline on line 2
        assert_eq!(tokens[7].kind, TokenKind::Newline);
        // "fi" on line 3
        assert_eq!(tokens[8].span.start_line, 3);
        Ok(())
    }
}
