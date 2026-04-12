// ---------------------------------------------------------------------------
// Heredoc preprocessing methods on Shell.
// ---------------------------------------------------------------------------

use crate::shell::Shell;

impl Shell {
    /// Preprocess heredocs in a script: extract heredoc bodies and return
    /// lines paired with optional heredoc stdin content.
    pub(super) fn preprocess_heredocs(&mut self, script: &str) -> Vec<(String, Option<String>)> {
        let lines: Vec<&str> = script.lines().collect();
        let mut result = Vec::new();
        let mut i = 0;

        while i < lines.len() {
            let line = lines[i];
            if let Some((before, after, delim, strip_tabs, quoted)) =
                Self::find_heredoc_marker(line)
            {
                // Collect body lines until delimiter
                i += 1;
                let mut body_lines = Vec::new();
                while i < lines.len() {
                    let body_line = if strip_tabs {
                        lines[i].trim_start_matches('\t')
                    } else {
                        lines[i]
                    };
                    if body_line.trim_end() == delim {
                        i += 1;
                        break;
                    }
                    body_lines.push(body_line.to_string());
                    i += 1;
                }

                let mut body = body_lines.join("\n");
                // Heredoc bodies end with a newline (bash behavior)
                if !body.is_empty() {
                    body.push('\n');
                }

                // Expand variables (and command substitutions) in unquoted heredocs
                if !quoted {
                    body = self.expand_full(&body);
                }

                // Reconstruct command without the <<DELIM part
                let cmd = before.trim();
                let rest = after.trim();
                let reconstructed = if cmd.is_empty() && rest.is_empty() {
                    "cat".to_string()
                } else if cmd.is_empty() {
                    format!("cat {}", rest)
                } else if rest.is_empty() {
                    cmd.to_string()
                } else {
                    format!("{} {}", cmd, rest)
                };

                result.push((reconstructed, Some(body)));
            } else {
                result.push((line.to_string(), None));
                i += 1;
            }
        }

        result
    }

    /// Detect `<<WORD` or `<<-WORD` heredoc marker in a line.
    /// Returns `(cmd_before, cmd_after, delimiter, strip_tabs, is_quoted)`.
    pub(super) fn find_heredoc_marker(line: &str) -> Option<(&str, &str, String, bool, bool)> {
        // Find << not inside quotes (simple heuristic: not preceded by <)
        let bytes = line.as_bytes();
        let mut i = 0;
        let mut in_single = false;
        let mut in_double = false;
        while i < bytes.len() {
            match bytes[i] {
                b'\'' if !in_double => in_single = !in_single,
                b'"' if !in_single => in_double = !in_double,
                b'<' if !in_single && !in_double => {
                    if i + 1 < bytes.len() && bytes[i + 1] == b'<' {
                        // Found <<, but not <<< (herestring)
                        if i + 2 < bytes.len() && bytes[i + 2] == b'<' {
                            i += 1;
                            continue;
                        }
                        let before = &line[..i];
                        let rest = &line[i + 2..];

                        // Check for - (tab stripping)
                        let (strip_tabs, rest) = if let Some(stripped) = rest.strip_prefix('-') {
                            (true, stripped)
                        } else {
                            (false, rest)
                        };
                        let rest = rest.trim_start();

                        // Extract delimiter — possibly quoted
                        let (delim, quoted, after) = Self::extract_heredoc_delim(rest)?;
                        return Some((before, after, delim, strip_tabs, quoted));
                    }
                }
                _ => {}
            }
            i += 1;
        }
        None
    }

    /// Extract the heredoc delimiter from the text after `<<[-]`.
    /// Handles: `EOF`, `'EOF'`, `"EOF"`. Returns (delimiter, is_quoted, remaining).
    pub(super) fn extract_heredoc_delim(s: &str) -> Option<(String, bool, &str)> {
        if s.is_empty() {
            return None;
        }
        let bytes = s.as_bytes();
        match bytes[0] {
            b'\'' => {
                let end = s[1..].find('\'')?;
                let delim = s[1..1 + end].to_string();
                Some((delim, true, &s[2 + end..]))
            }
            b'"' => {
                let end = s[1..].find('"')?;
                let delim = s[1..1 + end].to_string();
                Some((delim, true, &s[2 + end..]))
            }
            _ => {
                // Unquoted: take word characters
                let end = s
                    .find(|c: char| c.is_whitespace() || c == ';' || c == '|' || c == '&')
                    .unwrap_or(s.len());
                if end == 0 {
                    return None;
                }
                let delim = s[..end].to_string();
                Some((delim, false, &s[end..]))
            }
        }
    }
}
