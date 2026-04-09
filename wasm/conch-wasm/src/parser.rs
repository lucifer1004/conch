/// Command-line parser: splits on pipes `|`, redirects `>` `>>`,
/// and chain operators `;` `&&` `||`, respecting quotes.

#[derive(Debug)]
pub enum RedirectType {
    Overwrite,
    Append,
}

#[derive(Debug)]
pub struct Redirect {
    pub typ: RedirectType,
    pub target: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChainOp {
    Semi, // ;
    And,  // &&
    Or,   // ||
}

#[derive(Debug)]
pub struct Pipeline {
    pub segments: Vec<String>,
    pub redirect: Option<Redirect>,
}

#[derive(Debug)]
pub struct CommandChain {
    pub pipelines: Vec<(Pipeline, Option<ChainOp>)>,
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

pub fn parse(input: &str) -> CommandChain {
    let parts = split_chains(input);
    let pipelines = parts
        .into_iter()
        .map(|(seg, op)| (parse_pipeline(&seg), op))
        .collect();
    CommandChain { pipelines }
}

// ---------------------------------------------------------------------------
// Internals
// ---------------------------------------------------------------------------

struct Scanner<'a> {
    chars: &'a [char],
    pos: usize,
    in_single: bool,
    in_double: bool,
    escape: bool,
}

impl<'a> Scanner<'a> {
    fn new(chars: &'a [char]) -> Self {
        Self {
            chars,
            pos: 0,
            in_single: false,
            in_double: false,
            escape: false,
        }
    }

    fn in_quotes(&self) -> bool {
        self.in_single || self.in_double
    }

    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }

    fn peek2(&self) -> Option<char> {
        self.chars.get(self.pos + 1).copied()
    }

    /// Advance one char, updating quote/escape state. Returns the char.
    fn advance(&mut self) -> Option<char> {
        let c = self.chars.get(self.pos).copied()?;
        self.pos += 1;

        if self.escape {
            self.escape = false;
            return Some(c);
        }
        if c == '\\' && !self.in_single {
            self.escape = true;
            return Some(c);
        }
        if c == '\'' && !self.in_double {
            self.in_single = !self.in_single;
        }
        if c == '"' && !self.in_single {
            self.in_double = !self.in_double;
        }
        Some(c)
    }

    fn slice(&self, start: usize, end: usize) -> String {
        self.chars[start..end].iter().collect()
    }
}

/// Split on `;`, `&&`, `||` outside quotes
fn split_chains(input: &str) -> Vec<(String, Option<ChainOp>)> {
    let chars: Vec<char> = input.chars().collect();
    let mut sc = Scanner::new(&chars);
    let mut results = Vec::new();
    let mut start = 0;

    while sc.pos < chars.len() {
        if sc.escape || sc.in_quotes() {
            sc.advance();
            continue;
        }

        let c = sc.peek().unwrap();

        // ;
        if c == ';' {
            results.push((
                sc.slice(start, sc.pos).trim().to_string(),
                Some(ChainOp::Semi),
            ));
            sc.pos += 1;
            start = sc.pos;
            continue;
        }
        // &&
        if c == '&' && sc.peek2() == Some('&') {
            results.push((
                sc.slice(start, sc.pos).trim().to_string(),
                Some(ChainOp::And),
            ));
            sc.pos += 2;
            start = sc.pos;
            continue;
        }
        // || (but not a single |)
        if c == '|' && sc.peek2() == Some('|') {
            results.push((
                sc.slice(start, sc.pos).trim().to_string(),
                Some(ChainOp::Or),
            ));
            sc.pos += 2;
            start = sc.pos;
            continue;
        }

        sc.advance();
    }

    // Trailing segment
    let tail = sc.slice(start, chars.len()).trim().to_string();
    if !tail.is_empty() {
        results.push((tail, None));
    }

    results
}

/// Split a single chain segment on `|` (single pipe, not `||`) and extract redirect
fn parse_pipeline(input: &str) -> Pipeline {
    let chars: Vec<char> = input.chars().collect();
    let mut sc = Scanner::new(&chars);
    let mut segments = Vec::new();
    let mut start = 0;

    while sc.pos < chars.len() {
        if sc.escape || sc.in_quotes() {
            sc.advance();
            continue;
        }

        let c = sc.peek().unwrap();

        // Single | (not ||, already handled)
        if c == '|' && sc.peek2() != Some('|') {
            segments.push(sc.slice(start, sc.pos).trim().to_string());
            sc.pos += 1;
            start = sc.pos;
            continue;
        }

        sc.advance();
    }

    // Last segment — may contain redirect
    let last = sc.slice(start, chars.len()).trim().to_string();
    let (cmd, redirect) = extract_redirect(&last);
    segments.push(cmd);

    Pipeline { segments, redirect }
}

/// Extract `>` or `>>` redirect from the end of a command string
fn extract_redirect(input: &str) -> (String, Option<Redirect>) {
    let chars: Vec<char> = input.chars().collect();
    let mut sc = Scanner::new(&chars);
    let mut last_pos = None;
    let mut is_append = false;

    while sc.pos < chars.len() {
        if sc.escape || sc.in_quotes() {
            sc.advance();
            continue;
        }

        let c = sc.peek().unwrap();

        if c == '>' {
            last_pos = Some(sc.pos);
            if sc.peek2() == Some('>') {
                is_append = true;
                sc.pos += 2;
            } else {
                is_append = false;
                sc.pos += 1;
            }
            continue;
        }

        sc.advance();
    }

    match last_pos {
        Some(pos) => {
            let cmd = chars[..pos].iter().collect::<String>().trim().to_string();
            let skip = if is_append { 2 } else { 1 };
            let mut target = chars[pos + skip..]
                .iter()
                .collect::<String>()
                .trim()
                .to_string();

            // Strip surrounding quotes from target
            if (target.starts_with('"') && target.ends_with('"'))
                || (target.starts_with('\'') && target.ends_with('\''))
            {
                target = target[1..target.len() - 1].to_string();
            }

            let typ = if is_append {
                RedirectType::Append
            } else {
                RedirectType::Overwrite
            };
            (cmd, Some(Redirect { typ, target }))
        }
        None => (input.to_string(), None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pipe_splits_segments() {
        let c = parse("echo a | wc");
        assert_eq!(c.pipelines.len(), 1);
        let p = &c.pipelines[0].0;
        assert_eq!(p.segments, vec!["echo a", "wc"]);
        assert!(p.redirect.is_none());
    }

    #[test]
    fn double_pipe_is_or_not_pipe() {
        let c = parse("false || true");
        assert_eq!(c.pipelines.len(), 2);
        assert_eq!(c.pipelines[0].1, Some(ChainOp::Or));
        assert_eq!(c.pipelines[0].0.segments, vec!["false"]);
        assert_eq!(c.pipelines[1].0.segments, vec!["true"]);
    }

    #[test]
    fn and_chain_two_pipelines() {
        let c = parse("true && echo ok");
        assert_eq!(c.pipelines.len(), 2);
        assert_eq!(c.pipelines[0].1, Some(ChainOp::And));
    }

    #[test]
    fn semicolon_chain() {
        let c = parse("echo a; echo b");
        assert_eq!(c.pipelines.len(), 2);
        assert_eq!(c.pipelines[0].1, Some(ChainOp::Semi));
    }

    #[test]
    fn redirect_on_last_segment() {
        let c = parse("echo hi > out.txt");
        let p = &c.pipelines[0].0;
        assert_eq!(p.segments.len(), 1);
        let r = p.redirect.as_ref().unwrap();
        assert_eq!(r.target, "out.txt");
        assert!(matches!(r.typ, RedirectType::Overwrite));
    }

    #[test]
    fn append_redirect() {
        let c = parse("echo x >> log.txt");
        let r = c.pipelines[0].0.redirect.as_ref().unwrap();
        assert_eq!(r.target, "log.txt");
        assert!(matches!(r.typ, RedirectType::Append));
    }
}
