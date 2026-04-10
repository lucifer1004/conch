//! ANSI escape code helpers for colored terminal output.

pub const RESET: &str = "\x1b[0m";
pub const BOLD: &str = "\x1b[1m";

pub const RED: &str = "\x1b[31m";
pub const GREEN: &str = "\x1b[32m";
pub const YELLOW: &str = "\x1b[33m";
pub const BLUE: &str = "\x1b[34m";
pub const MAGENTA: &str = "\x1b[35m";
pub const CYAN: &str = "\x1b[36m";

pub const BOLD_RED: &str = "\x1b[1;31m";
pub const BOLD_GREEN: &str = "\x1b[1;32m";
pub const BOLD_BLUE: &str = "\x1b[1;34m";

/// Highlight all occurrences of `pattern` in `line` with bold red.
pub fn highlight_matches(line: &str, pattern: &str, case_insensitive: bool) -> String {
    if case_insensitive {
        let lower_line = line.to_lowercase();
        let lower_pattern = pattern.to_lowercase();
        let mut result = String::new();
        let mut last_end = 0;
        let mut search_from = 0;

        while let Some(rel_pos) = lower_line[search_from..].find(&lower_pattern) {
            let abs_pos = search_from + rel_pos;
            result.push_str(&line[last_end..abs_pos]);
            result.push_str(BOLD_RED);
            result.push_str(&line[abs_pos..abs_pos + pattern.len()]);
            result.push_str(RESET);
            last_end = abs_pos + pattern.len();
            search_from = last_end;
        }
        result.push_str(&line[last_end..]);
        result
    } else {
        line.replace(pattern, &format!("{}{}{}", BOLD_RED, pattern, RESET))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn highlight_case_sensitive_replaces_all() {
        let s = highlight_matches("foo bar foo", "foo", false);
        assert_eq!(s.matches(BOLD_RED).count(), 2);
        assert!(s.contains("bar"));
    }

    #[test]
    fn highlight_case_insensitive_finds_mixed() {
        let s = highlight_matches("Hello HELLO", "hello", true);
        assert_eq!(s.matches(BOLD_RED).count(), 2);
    }
}
