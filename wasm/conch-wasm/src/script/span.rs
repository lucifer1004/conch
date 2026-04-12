/// Source location span — tracks where a token or AST node came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Span {
    pub start_line: u32, // 0-based
    pub end_line: u32,   // 0-based, inclusive
    pub start_byte: u32, // byte offset in original input
    pub end_byte: u32,   // byte offset, exclusive
}

impl Span {
    pub fn new(start_line: u32, end_line: u32, start_byte: u32, end_byte: u32) -> Self {
        Self {
            start_line,
            end_line,
            start_byte,
            end_byte,
        }
    }

    /// Merge two spans into one covering both.
    pub fn merge(a: Span, b: Span) -> Span {
        Span {
            start_line: a.start_line.min(b.start_line),
            end_line: a.end_line.max(b.end_line),
            start_byte: a.start_byte.min(b.start_byte),
            end_byte: a.end_byte.max(b.end_byte),
        }
    }
}
