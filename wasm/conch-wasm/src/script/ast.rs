use super::span::Span;
use super::word::CommandList;
use crate::Str;

/// A parsed shell script — a sequence of statements.
#[derive(Debug, Clone, PartialEq)]
pub struct Script {
    pub stmts: Vec<Stmt>,
}

/// A shell statement.
#[derive(Debug, Clone, PartialEq)]
pub enum Stmt {
    /// A structured command list produced by the word-level parser.
    Structured { cmd: CommandList, span: Span },

    /// `if cond; then body; [elif cond; then body;]* [else body;] fi`
    If {
        clauses: Vec<IfClause>,
        else_body: Option<Vec<Stmt>>,
        span: Span,
    },

    /// `for var [in words...]; do body; done`
    For {
        var: Str,
        words: Vec<Str>,
        body: Vec<Stmt>,
        span: Span,
    },

    /// `while cond; do body; done`
    While {
        condition: Vec<Stmt>,
        body: Vec<Stmt>,
        span: Span,
    },

    /// `until cond; do body; done`
    Until {
        condition: Vec<Stmt>,
        body: Vec<Stmt>,
        span: Span,
    },

    /// `name() { body; }` or `function name { body; }`
    FunctionDef {
        name: Str,
        body: Vec<Stmt>,
        span: Span,
    },

    /// `break [n]`
    Break(Option<u32>, Span),

    /// `continue [n]`
    Continue(Option<u32>, Span),

    /// `return [n]`
    Return(Option<Str>, Span),

    /// `case word in pattern) body;; ... esac`
    Case {
        word: Str,
        arms: Vec<CaseArm>,
        span: Span,
    },

    /// `(cmd1; cmd2)` — subshell: runs commands in a child environment.
    /// Changes to env/cwd are not visible to the parent.
    Subshell { body: Vec<Stmt>, span: Span },

    /// `{ cmd1; cmd2; }` — brace group: runs commands in current shell.
    /// Unlike subshell, changes persist. Used for grouping with redirects.
    BraceGroup { body: Vec<Stmt>, span: Span },

    /// `for (( init; cond; step )); do body; done` — C-style arithmetic for loop.
    ForArith {
        init: String,
        cond: String,
        step: String,
        body: Vec<Stmt>,
        span: Span,
    },
}

impl Stmt {
    /// Get the source span for this statement.
    pub fn span(&self) -> Span {
        match self {
            Stmt::Structured { span, .. }
            | Stmt::If { span, .. }
            | Stmt::For { span, .. }
            | Stmt::While { span, .. }
            | Stmt::Until { span, .. }
            | Stmt::FunctionDef { span, .. }
            | Stmt::Case { span, .. }
            | Stmt::Subshell { span, .. }
            | Stmt::BraceGroup { span, .. }
            | Stmt::ForArith { span, .. } => *span,
            Stmt::Break(_, span) | Stmt::Continue(_, span) | Stmt::Return(_, span) => *span,
        }
    }
}

/// One arm of a `case` statement.
#[derive(Debug, Clone, PartialEq)]
pub struct CaseArm {
    /// Patterns separated by `|` (e.g. `"foo" | "bar"`).
    pub patterns: Vec<Str>,
    /// Statements to execute if a pattern matches.
    pub body: Vec<Stmt>,
}

/// One `if`/`elif` clause: a condition and the body to execute when the
/// condition's last command exits with status 0.
#[derive(Debug, Clone, PartialEq)]
pub struct IfClause {
    pub condition: Vec<Stmt>,
    pub body: Vec<Stmt>,
}
