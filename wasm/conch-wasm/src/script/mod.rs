pub mod arith;
pub mod ast;
pub mod interp;
pub mod parse;
pub mod span;
pub mod tokenize;
pub mod word;
pub mod word_parser;

use ast::Script;
use parse::ParseError;

/// Parse a shell script string into an AST.
pub fn parse_script(input: &str) -> Result<Script, String> {
    let tokens = tokenize::tokenize(input)?;
    parse::parse(&tokens).map_err(|e: ParseError| e.to_string())
}
