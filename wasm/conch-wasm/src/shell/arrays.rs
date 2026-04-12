// ---------------------------------------------------------------------------
// Array detection helpers defined as impl Shell methods.
// ---------------------------------------------------------------------------

use crate::shell::Shell;
use crate::Str;

impl Shell {
    /// Detect `name=(...)` or `name+=(...)` array assignment in raw segment.
    pub(super) fn detect_array_assignment(segment: &str) -> Option<(String, bool, String)> {
        let s = segment.trim();
        let eq_pos = s.find('=')?;
        let before_eq = &s[..eq_pos];
        let after_eq = &s[eq_pos + 1..];
        let (name, is_append) = if let Some(stripped) = before_eq.strip_suffix('+') {
            (stripped, true)
        } else {
            (before_eq, false)
        };
        if name.is_empty()
            || !name.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_')
            || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        {
            return None;
        }
        let trimmed_val = after_eq.trim();
        if !trimmed_val.starts_with('(') || !trimmed_val.ends_with(')') {
            return None;
        }
        let body = &trimmed_val[1..trimmed_val.len() - 1];
        Some((name.to_string(), is_append, body.to_string()))
    }

    /// Parse array elements from the body between parens, respecting quotes.
    pub(super) fn parse_array_elements(body: &str, shell: &mut Shell) -> Vec<String> {
        let trimmed = body.trim();
        if trimmed.is_empty() {
            return Vec::new();
        }
        match super::split_shell_words(trimmed) {
            Some(tokens) => tokens.iter().map(|t| shell.expand_full(t)).collect(),
            None => trimmed
                .split_whitespace()
                .map(|w| shell.expand_full(w))
                .collect(),
        }
    }

    /// Detect `name[key]=value` array element assignment.
    pub(super) fn detect_assoc_elem_assignment(segment: &str) -> Option<(String, String, String)> {
        let s = segment.trim();
        let bracket_open = s.find('[')?;
        let name = &s[..bracket_open];
        if name.is_empty()
            || !name.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_')
            || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        {
            return None;
        }
        let rest = &s[bracket_open + 1..];
        let bracket_close = rest.find(']')?;
        let key = &rest[..bracket_close];
        let after_bracket = &rest[bracket_close + 1..];
        let val = after_bracket.strip_prefix('=')?;
        Some((name.to_string(), key.to_string(), val.to_string()))
    }

    /// Resolve namerefs — delegates to VarStore.
    pub(crate) fn resolve_nameref(&self, name: &str) -> Str {
        self.vars.resolve_nameref(name)
    }
}
