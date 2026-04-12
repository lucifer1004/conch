// ---------------------------------------------------------------------------
// Expansion methods on Shell.
//
// `expand()` and `expand_full()` delegate to the word-level engine
// (expand_word.rs) by parsing the input string into a Word AST first.
// The remaining helpers (eval_brace_expr, resolve_nameref, eval_arith_expr,
// etc.) are shared infrastructure used by both the old and new paths.
// ---------------------------------------------------------------------------

use crate::shell::Shell;

impl Shell {
    /// Expand tilde, shell variables, and `$(...)` command substitutions.
    /// Parses the string into a Word AST and delegates to expand_word_nosplit.
    pub fn expand(&mut self, s: &str) -> String {
        let word = crate::script::word_parser::parse_word(s);
        self.expand_word_nosplit(&word)
    }

    /// Evaluate a single `${...}` expression (the content between the braces).
    pub(super) fn eval_brace_expr(&mut self, expr: &str) -> String {
        // ${!var} — indirect expansion or ${!arr[@]} array keys
        if let Some(rest) = expr.strip_prefix('!') {
            // ${!arr[@]} or ${!arr[*]} — return indices/keys
            if let Some(bracket_start) = rest.find('[') {
                let arr_name = &rest[..bracket_start];
                let idx_part = &rest[bracket_start + 1..];
                if let Some(idx_inner) = idx_part.strip_suffix(']') {
                    if idx_inner == "@" || idx_inner == "*" {
                        if let Some(assoc) = self.vars.assoc_arrays.get(arr_name) {
                            return assoc.keys().cloned().collect::<Vec<_>>().join(" ");
                        }
                        if let Some(arr) = self.vars.arrays.get(arr_name) {
                            return (0..arr.len())
                                .map(|i| i.to_string())
                                .collect::<Vec<_>>()
                                .join(" ");
                        }
                        return String::new();
                    }
                }
            }
            // ${!var} — indirect expansion
            let var_name = self.resolve_nameref(rest);
            let intermediate = self.vars.env.get(&var_name).cloned().unwrap_or_default();
            if intermediate.is_empty() {
                return String::new();
            }
            let target = self.resolve_nameref(&intermediate);
            return self.vars.env.get(&target).cloned().unwrap_or_default();
        }

        // ${#var} — string length or ${#arr[@]} array length
        if let Some(var) = expr.strip_prefix('#') {
            // ${#arr[@]} or ${#arr[*]}
            if let Some(bracket_start) = var.find('[') {
                let arr_name = &var[..bracket_start];
                let idx_part = &var[bracket_start + 1..];
                if let Some(idx_inner) = idx_part.strip_suffix(']') {
                    if idx_inner == "@" || idx_inner == "*" {
                        if let Some(assoc) = self.vars.assoc_arrays.get(arr_name) {
                            return assoc.len().to_string();
                        }
                        if let Some(arr) = self.vars.arrays.get(arr_name) {
                            return arr.len().to_string();
                        }
                        return "0".to_string();
                    }
                }
            }
            let resolved = self.resolve_nameref(var);
            let val = self.vars.get(&resolved).unwrap_or_default().to_string();
            return val.len().to_string();
        }

        // Try to find the variable name (alphanumeric + underscore prefix)
        let name_end = expr
            .find(|c: char| !c.is_ascii_alphanumeric() && c != '_')
            .unwrap_or(expr.len());
        if name_end == 0 {
            return format!("${{{}}}", expr); // can't parse, return as-is
        }
        let name = &expr[..name_end];
        let op = &expr[name_end..];

        // ${arr[idx]} — array element access
        if let Some(bracket_content) = op.strip_prefix('[') {
            if let Some(close) = bracket_content.find(']') {
                let idx_str = &bracket_content[..close];
                let after_bracket = &bracket_content[close + 1..];

                // Associative array lookup
                if let Some(assoc) = self.vars.assoc_arrays.get(name) {
                    let val = assoc.get(idx_str).cloned().unwrap_or_default();
                    if after_bracket.is_empty() {
                        return val;
                    }
                    return self.apply_string_ops(&val, after_bracket);
                }

                // Indexed array lookup
                if let Some(arr) = self.vars.arrays.get(name) {
                    if idx_str == "@" || idx_str == "*" {
                        let joined = arr.join(" ");
                        if after_bracket.is_empty() {
                            return joined;
                        }
                        return self.apply_string_ops(&joined, after_bracket);
                    }
                    if let Ok(idx) = idx_str.parse::<usize>() {
                        let val = arr.get(idx).cloned().unwrap_or_default();
                        if after_bracket.is_empty() {
                            return val;
                        }
                        return self.apply_string_ops(&val, after_bracket);
                    }
                }
                return String::new();
            }
        }

        // Resolve namerefs for normal variable access
        let resolved_name = self.resolve_nameref(name);
        let val = self
            .vars
            .get(&resolved_name)
            .unwrap_or_default()
            .to_string();

        if op.is_empty() {
            // Simple ${var}
            return val;
        }

        // ${var:-default} — default if empty/unset
        if let Some(default) = op.strip_prefix(":-") {
            return if val.is_empty() {
                let word = crate::script::word_parser::parse_word(default);
                self.expand_word_nosplit(&word)
            } else {
                val
            };
        }
        // ${var-default} — default if unset (not if empty)
        if let Some(default) = op.strip_prefix('-') {
            return if !self.vars.env.contains_key(&resolved_name) {
                let word = crate::script::word_parser::parse_word(default);
                self.expand_word_nosplit(&word)
            } else {
                val
            };
        }
        // ${var:=default} — assign default if empty/unset
        if let Some(default) = op.strip_prefix(":=") {
            if val.is_empty() {
                let word = crate::script::word_parser::parse_word(default);
                let expanded = self.expand_word_nosplit(&word);
                self.vars
                    .env
                    .insert(resolved_name.clone(), expanded.clone());
                return expanded;
            } else {
                return val;
            }
        }
        // ${var=default} — assign default if unset (not empty)
        if let Some(default) = op.strip_prefix('=') {
            if !self.vars.env.contains_key(resolved_name.as_str()) {
                let word = crate::script::word_parser::parse_word(default);
                let expanded = self.expand_word_nosplit(&word);
                self.vars
                    .env
                    .insert(resolved_name.clone(), expanded.clone());
                return expanded;
            } else {
                return val;
            }
        }
        // ${var:?error} — error if empty/unset
        if let Some(msg) = op.strip_prefix(":?") {
            if val.is_empty() {
                let err_msg = if msg.is_empty() {
                    "parameter null or not set"
                } else {
                    msg
                };
                return format!("conch: {}: {}", name, err_msg);
            } else {
                return val;
            }
        }
        // ${var?error} — error if unset
        if let Some(msg) = op.strip_prefix('?') {
            if !self.vars.env.contains_key(resolved_name.as_str()) {
                let err_msg = if msg.is_empty() {
                    "parameter not set"
                } else {
                    msg
                };
                return format!("conch: {}: {}", name, err_msg);
            } else {
                return val;
            }
        }
        // ${var:+alt} — alt if set and non-empty
        if let Some(alt) = op.strip_prefix(":+") {
            return if !val.is_empty() {
                let word = crate::script::word_parser::parse_word(alt);
                self.expand_word_nosplit(&word)
            } else {
                String::new()
            };
        }
        // ${var+alt} — alt if set
        if let Some(alt) = op.strip_prefix('+') {
            return if self.vars.env.contains_key(&resolved_name) {
                let word = crate::script::word_parser::parse_word(alt);
                self.expand_word_nosplit(&word)
            } else {
                String::new()
            };
        }

        // ${var##pattern} — remove longest prefix
        if let Some(pat) = op.strip_prefix("##") {
            return Self::strip_longest_prefix(&val, pat);
        }
        // ${var#pattern} — remove shortest prefix
        if let Some(pat) = op.strip_prefix('#') {
            return Self::strip_shortest_prefix(&val, pat);
        }
        // ${var%%pattern} — remove longest suffix
        if let Some(pat) = op.strip_prefix("%%") {
            return Self::strip_longest_suffix(&val, pat);
        }
        // ${var%pattern} — remove shortest suffix
        if let Some(pat) = op.strip_prefix('%') {
            return Self::strip_shortest_suffix(&val, pat);
        }

        // ${var//old/new} — replace all
        if let Some(rest) = op.strip_prefix("//") {
            if let Some((old, new)) = rest.split_once('/') {
                return val.replace(old, new);
            }
            // ${var//pattern} — remove all occurrences
            return val.replace(rest, "");
        }
        // ${var/old/new} — replace first
        if let Some(rest) = op.strip_prefix('/') {
            if let Some((old, new)) = rest.split_once('/') {
                return val.replacen(old, new, 1);
            }
            return val.replacen(rest, "", 1);
        }

        // ${var:offset:length} or ${var:offset}
        if let Some(rest) = op.strip_prefix(':') {
            let (off_str, len_str) = rest
                .split_once(':')
                .map(|(a, b)| (a, Some(b)))
                .unwrap_or((rest, None));
            let offset = off_str.parse::<isize>().unwrap_or(0);
            let start = if offset < 0 {
                (val.len() as isize + offset).max(0) as usize
            } else {
                (offset as usize).min(val.len())
            };
            let end = if let Some(ls) = len_str {
                let len = ls.parse::<usize>().unwrap_or(val.len());
                (start + len).min(val.len())
            } else {
                val.len()
            };
            return val.get(start..end).unwrap_or("").to_string();
        }

        // ${var^^} — uppercase all
        if op == "^^" {
            return val.to_uppercase();
        }
        // ${var^} — capitalize first character
        if op == "^" {
            let mut chars = val.chars();
            return match chars.next() {
                Some(c) => c.to_uppercase().to_string() + chars.as_str(),
                None => String::new(),
            };
        }
        // ${var,,} — lowercase all
        if op == ",," {
            return val.to_lowercase();
        }
        // ${var,} — lowercase first character
        if op == "," {
            let mut chars = val.chars();
            return match chars.next() {
                Some(c) => c.to_lowercase().to_string() + chars.as_str(),
                None => String::new(),
            };
        }

        // Unknown form — return as-is
        format!("${{{}}}", expr)
    }

    pub(super) fn strip_shortest_prefix(val: &str, pattern: &str) -> String {
        // Simple glob: try removing from the front, smallest match first
        for i in 0..=val.len() {
            if Self::glob_match_str(pattern, &val[..i]) {
                return val[i..].to_string();
            }
        }
        val.to_string()
    }

    pub(super) fn strip_longest_prefix(val: &str, pattern: &str) -> String {
        for i in (0..=val.len()).rev() {
            if Self::glob_match_str(pattern, &val[..i]) {
                return val[i..].to_string();
            }
        }
        val.to_string()
    }

    pub(super) fn strip_shortest_suffix(val: &str, pattern: &str) -> String {
        for i in (0..=val.len()).rev() {
            if Self::glob_match_str(pattern, &val[i..]) {
                return val[..i].to_string();
            }
        }
        val.to_string()
    }

    pub(super) fn strip_longest_suffix(val: &str, pattern: &str) -> String {
        for i in 0..=val.len() {
            if Self::glob_match_str(pattern, &val[i..]) {
                return val[..i].to_string();
            }
        }
        val.to_string()
    }

    /// Expand tilde, variables, AND command substitutions `$(...)` / `` `...` ``.
    /// Requires `&mut self` because command substitution executes commands.
    pub fn expand_full(&mut self, s: &str) -> String {
        // expand() now handles everything via the word-parser path.
        self.expand(s)
    }

    /// Check for unset variable references when `set -u` is active.
    /// Scans the string for `$NAME` or `${NAME}` patterns where NAME is not
    /// in `self.vars.env`. Returns `Some(varname)` for the first unbound variable
    /// found, or `None` if all are set.
    pub(crate) fn check_nounset(&self, s: &str) -> Option<String> {
        if !self.exec.opts.nounset {
            return None;
        }
        let bytes = s.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i] == b'\'' {
                // Skip single-quoted content
                i += 1;
                while i < bytes.len() && bytes[i] != b'\'' {
                    i += 1;
                }
                i += 1;
                continue;
            }
            if bytes[i] == b'$' && i + 1 < bytes.len() {
                let next = bytes[i + 1];
                // Skip special params: $?, $#, $@, $*, $!, $$, $-
                if b"?#@*!$-".contains(&next) {
                    i += 2;
                    continue;
                }
                // Skip $( (command substitution) and $(( (arithmetic)
                if next == b'(' {
                    i += 2;
                    continue;
                }
                // ${...} form
                if next == b'{' {
                    i += 2;
                    // Skip ${#var} (length), ${var:-...} etc. — just extract the var name
                    let skip_hash = if i < bytes.len() && bytes[i] == b'#' {
                        i += 1;
                        true
                    } else {
                        false
                    };
                    let _ = skip_hash;
                    let name_start = i;
                    while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_')
                    {
                        i += 1;
                    }
                    let name = std::str::from_utf8(&bytes[name_start..i]).unwrap_or("");
                    if !name.is_empty() && !name.chars().next().is_some_and(|c| c.is_ascii_digit())
                    {
                        // Check for operators that provide defaults: :-, -, :+, +
                        let has_default = i < bytes.len()
                            && (bytes[i] == b':' || bytes[i] == b'-' || bytes[i] == b'+');
                        if !has_default && !self.vars.env.contains_key(name) {
                            return Some(name.to_string());
                        }
                    }
                    // Skip to closing }
                    while i < bytes.len() && bytes[i] != b'}' {
                        i += 1;
                    }
                    i += 1;
                    continue;
                }
                // $NAME form
                if next.is_ascii_alphabetic() || next == b'_' {
                    i += 1;
                    let name_start = i;
                    while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_')
                    {
                        i += 1;
                    }
                    let name = std::str::from_utf8(&bytes[name_start..i]).unwrap_or("");
                    if !name.is_empty() && !self.vars.env.contains_key(name) {
                        return Some(name.to_string());
                    }
                    continue;
                }
                // Digit positional param $0..$9
                if next.is_ascii_digit() {
                    let key = (next as char).to_string();
                    if !self.vars.env.contains_key(key.as_str()) && next != b'0' {
                        return Some(key);
                    }
                    i += 2;
                    continue;
                }
            }
            i += 1;
        }
        None
    }

    /// Evaluate an arithmetic expression, resolving variables from env.
    pub fn eval_arith_expr(&mut self, expr: &str) -> i64 {
        // First expand variables and command substitutions in the expression
        let expanded = self.expand_full(expr);
        let env_snapshot: Vec<(String, String)> = self
            .vars
            .env
            .iter()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect();
        let env_map: std::collections::BTreeMap<String, String> =
            env_snapshot.into_iter().collect();
        let get_var =
            |name: &str| -> i64 { env_map.get(name).and_then(|v| v.parse().ok()).unwrap_or(0) };
        let mut assignments: Vec<(String, i64)> = Vec::new();
        let result = crate::script::arith::eval_arith(&expanded, &get_var, &mut |name, val| {
            assignments.push((name.to_string(), val));
        });
        // Apply any assignments back to env
        for (name, val) in assignments {
            self.vars
                .env
                .insert(crate::Str::from(name.as_str()), val.to_string());
        }
        result.unwrap_or(0)
    }

    /// `(( expr ))` — arithmetic command. Exit 0 if result is non-zero, 1 if zero.
    pub fn cmd_arith(&mut self, expr: &str) -> (String, i32) {
        let val = self.eval_arith_expr(expr);
        (String::new(), if val != 0 { 0 } else { 1 })
    }

    /// Detect `(( expr ))` arithmetic command, handling both `((expr))`
    /// and `( ( expr ) )` (from tokenizer reconstruction with spaces).
    /// Guards against false positives like nested subshells `( (cmd) )`.
    pub(super) fn extract_arith_command(s: &str) -> Option<&str> {
        // Direct form: ((expr))
        if let Some(rest) = s.strip_prefix("((") {
            let inner = rest.strip_suffix("))")?;
            // Guard: if the inner content starts with '(' or ends with ')',
            // it's likely a nested subshell, not arithmetic.
            if inner.starts_with('(') || inner.ends_with(')') {
                return None;
            }
            return Some(inner);
        }
        // Spaced form from parser: ( ( expr ) )
        let rest = s.strip_prefix('(')?.trim_start();
        let rest = rest.strip_prefix('(')?.trim_start();
        let rest = rest.strip_suffix(')')?.trim_end();
        let inner = rest.strip_suffix(')')?.trim_end();
        // Guard against nested subshells
        if inner.starts_with('(') || inner.ends_with(')') {
            return None;
        }
        Some(inner)
    }

    /// Apply string operations on a value (used after array bracket access).
    pub(super) fn apply_string_ops(&mut self, val: &str, op: &str) -> String {
        if op == "^^" {
            return val.to_uppercase();
        }
        if op == "^" {
            let mut chars = val.chars();
            return match chars.next() {
                Some(c) => c.to_uppercase().to_string() + chars.as_str(),
                None => String::new(),
            };
        }
        if op == ",," {
            return val.to_lowercase();
        }
        if op == "," {
            let mut chars = val.chars();
            return match chars.next() {
                Some(c) => c.to_lowercase().to_string() + chars.as_str(),
                None => String::new(),
            };
        }
        if let Some(default) = op.strip_prefix(":-") {
            return if val.is_empty() {
                let word = crate::script::word_parser::parse_word(default);
                self.expand_word_nosplit(&word)
            } else {
                val.to_string()
            };
        }
        if let Some(rest) = op.strip_prefix("//") {
            if let Some((old, new)) = rest.split_once('/') {
                return val.replace(old, new);
            }
            return val.replace(rest, "");
        }
        if let Some(rest) = op.strip_prefix('/') {
            if let Some((old, new)) = rest.split_once('/') {
                return val.replacen(old, new, 1);
            }
            return val.replacen(rest, "", 1);
        }
        val.to_string()
    }
}
