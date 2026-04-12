/// Structured shell word types — preserves quoting context for correct expansion.
///
/// A `Word` is a sequence of `WordPart`s that together form a single shell token.
/// For example, `hello"$USER"world` is three parts: `Literal("hello")`,
/// `DoubleQuoted([Variable("USER")])`, `Literal("world")`.
use crate::Str;

/// Chain operator between pipelines in a command list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChainOp {
    Semi,       // ;
    And,        // &&
    Or,         // ||
    Background, // &
}

/// A structured shell word — preserves quoting context for correct expansion.
pub type Word = Vec<WordPart>;

#[derive(Debug, Clone, PartialEq)]
pub enum WordPart {
    /// Unquoted text.
    Literal(Str),
    /// `'...'` content (no expansion).
    SingleQuoted(Str),
    /// `"..."` content (expand vars/cmdsubst, no split/glob).
    DoubleQuoted(Vec<WordPart>),
    /// `$VAR`, `$1`, `$@`, `$?`, etc.
    Variable(Str),
    /// `${...}` raw inner expression.
    BraceExpr(Str),
    /// `$(...)` inner command text.
    CommandSubst(Str),
    /// `` `...` `` inner command text.
    BacktickSubst(Str),
    /// `$((...))` inner expression.
    ArithSubst(Str),
    /// `$'...'` raw inner (before escape processing).
    DollarSingleQuoted(Str),
    /// `~` (None) or `~user` (Some("user")).
    Tilde(Option<Str>),
    /// `*`, `?`, `[...]` pattern chars.
    GlobPattern(Str),
    /// `{a,b,c}` or `{1..5}` (NOT `${...}`).
    BraceExpansion(Str),
    /// `<(cmd)` or `>(cmd)` — process substitution.
    ProcessSubst { dir: char, cmd: Str },
}

// ---------------------------------------------------------------------------
// to_source — reconstruct original shell text
// ---------------------------------------------------------------------------

/// Extension trait for `Word` (Vec<WordPart>) to reconstruct source text.
pub trait WordToSource {
    fn to_source(&self) -> String;
}

impl WordToSource for Word {
    fn to_source(&self) -> String {
        let mut out = String::new();
        for part in self {
            part.write_source(&mut out);
        }
        out
    }
}

impl WordPart {
    fn write_source(&self, out: &mut String) {
        match self {
            WordPart::Literal(s) => out.push_str(s),
            WordPart::SingleQuoted(s) => {
                out.push('\'');
                out.push_str(s);
                out.push('\'');
            }
            WordPart::DoubleQuoted(parts) => {
                out.push('"');
                for p in parts {
                    p.write_source(out);
                }
                out.push('"');
            }
            WordPart::Variable(name) => {
                out.push('$');
                out.push_str(name);
            }
            WordPart::BraceExpr(expr) => {
                out.push_str("${");
                out.push_str(expr);
                out.push('}');
            }
            WordPart::CommandSubst(cmd) => {
                out.push_str("$(");
                out.push_str(cmd);
                out.push(')');
            }
            WordPart::BacktickSubst(cmd) => {
                out.push('`');
                out.push_str(cmd);
                out.push('`');
            }
            WordPart::ArithSubst(expr) => {
                out.push_str("$((");
                out.push_str(expr);
                out.push_str("))");
            }
            WordPart::DollarSingleQuoted(s) => {
                out.push_str("$'");
                out.push_str(s);
                out.push('\'');
            }
            WordPart::Tilde(None) => out.push('~'),
            WordPart::Tilde(Some(user)) => {
                out.push('~');
                out.push_str(user);
            }
            WordPart::GlobPattern(s) => out.push_str(s),
            WordPart::BraceExpansion(s) => {
                out.push('{');
                out.push_str(s);
                out.push('}');
            }
            WordPart::ProcessSubst { dir, cmd } => {
                out.push(*dir);
                out.push('(');
                out.push_str(cmd);
                out.push(')');
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Structured command types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub enum AssignOp {
    /// `=`
    Assign,
    /// `+=`
    PlusAssign,
}

#[derive(Debug, Clone, PartialEq)]
pub enum AssignValue {
    Scalar(Word),
    Array(Vec<Word>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Assignment {
    pub name: Str,
    pub op: AssignOp,
    pub value: AssignValue,
}

#[derive(Debug, Clone, PartialEq)]
pub enum RedirectOp {
    /// `>`
    Write,
    /// `>>`
    Append,
    /// `<`
    Read,
    /// `<<<`
    HereString,
}

#[derive(Debug, Clone, PartialEq)]
pub enum RedirectTarget {
    File(Word),
    FdDup(u32),
}

#[derive(Debug, Clone, PartialEq)]
pub struct WordRedirect {
    pub fd: Option<u32>,
    pub op: RedirectOp,
    pub target: RedirectTarget,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SimpleCommand {
    pub assignments: Vec<Assignment>,
    pub words: Vec<Word>,
    pub redirects: Vec<WordRedirect>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct StructuredPipeline {
    pub commands: Vec<SimpleCommand>,
    /// True when the pipeline is prefixed with `!` (negate exit code).
    pub bang: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CommandList {
    pub items: Vec<(StructuredPipeline, Option<ChainOp>)>,
}

// ---------------------------------------------------------------------------
// CommandList to_source — reconstruct full command line
// ---------------------------------------------------------------------------

impl CommandList {
    pub fn to_source(&self) -> String {
        let mut out = String::new();
        for (i, (pipeline, chain_op)) in self.items.iter().enumerate() {
            if i > 0 && !out.ends_with(' ') {
                out.push(' ');
            }
            pipeline.write_source(&mut out);
            if let Some(op) = chain_op {
                match op {
                    ChainOp::Semi => out.push_str("; "),
                    ChainOp::And => out.push_str(" && "),
                    ChainOp::Or => out.push_str(" || "),
                    ChainOp::Background => out.push_str(" & "),
                }
            }
        }
        out
    }
}

impl StructuredPipeline {
    pub(crate) fn to_source(&self) -> String {
        let mut out = String::new();
        self.write_source(&mut out);
        out
    }

    pub(crate) fn write_source(&self, out: &mut String) {
        for (i, cmd) in self.commands.iter().enumerate() {
            if i > 0 {
                out.push_str(" | ");
            }
            cmd.write_source(out);
        }
    }
}

impl SimpleCommand {
    pub(crate) fn write_source(&self, out: &mut String) {
        let mut first = true;
        for assign in &self.assignments {
            if !first {
                out.push(' ');
            }
            first = false;
            assign.write_source(out);
        }
        for word in &self.words {
            if !first {
                out.push(' ');
            }
            first = false;
            out.push_str(&word.to_source());
        }
        for redir in &self.redirects {
            if !first {
                out.push(' ');
            }
            first = false;
            redir.write_source(out);
        }
    }
}

impl Assignment {
    fn write_source(&self, out: &mut String) {
        out.push_str(&self.name);
        match self.op {
            AssignOp::Assign => out.push('='),
            AssignOp::PlusAssign => out.push_str("+="),
        }
        match &self.value {
            AssignValue::Scalar(word) => out.push_str(&word.to_source()),
            AssignValue::Array(words) => {
                out.push('(');
                for (i, w) in words.iter().enumerate() {
                    if i > 0 {
                        out.push(' ');
                    }
                    out.push_str(&w.to_source());
                }
                out.push(')');
            }
        }
    }
}

impl WordRedirect {
    fn write_source(&self, out: &mut String) {
        if let Some(fd) = self.fd {
            out.push_str(&fd.to_string());
        }
        match self.op {
            RedirectOp::Write => out.push('>'),
            RedirectOp::Append => out.push_str(">>"),
            RedirectOp::Read => out.push('<'),
            RedirectOp::HereString => out.push_str("<<<"),
        }
        match &self.target {
            RedirectTarget::File(word) => {
                out.push(' ');
                out.push_str(&word.to_source());
            }
            RedirectTarget::FdDup(fd) => {
                out.push('&');
                out.push_str(&fd.to_string());
            }
        }
    }
}
