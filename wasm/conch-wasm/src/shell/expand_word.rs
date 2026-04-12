/// Word-level expansion engine.
///
/// Walks `Word`/`WordPart` AST nodes directly, replacing the old
/// string-heuristic expansion chain (`expand_command_subst` → tokenize
/// → `expand_braces` → `expand` → `expand_globs`).
use crate::script::word::{Word, WordPart, WordToSource};
use crate::shell::{expand_braces, process_ansi_c_escapes};

impl super::Shell {
    /// Expand a Word into one or more result strings.
    /// Performs in order: brace expansion, tilde, variable, command subst,
    /// arithmetic, word splitting (IFS, unquoted only), glob expansion.
    pub fn expand_word(&mut self, word: &Word) -> Vec<String> {
        // 1. Brace expansion: if word contains BraceExpansion parts,
        //    use expand_braces on the source text to get multiple words,
        //    then recursively expand each.
        let has_brace = word
            .iter()
            .any(|p| matches!(p, WordPart::BraceExpansion(_)));
        if has_brace {
            let source = word.to_source();
            let braced = expand_braces(&source);
            if braced.len() > 1 {
                return braced
                    .iter()
                    .flat_map(|w| {
                        let parsed = crate::script::word_parser::parse_word(w);
                        self.expand_word_inner(&parsed)
                    })
                    .collect();
            }
        }
        self.expand_word_inner(word)
    }

    /// Expand a Word into exactly one string (no word splitting, no glob).
    /// Used for assignment RHS, redirect targets, case words, etc.
    pub fn expand_word_nosplit(&mut self, word: &Word) -> String {
        let mut result = String::new();
        for part in word {
            let (text, _quoted) = self.expand_part(part);
            result.push_str(&text);
        }
        result
    }

    /// Check if a word part can introduce field splitting (i.e., is an
    /// unquoted parameter/command/arithmetic expansion).
    fn part_is_splittable(part: &WordPart) -> bool {
        matches!(
            part,
            WordPart::Variable(_)
                | WordPart::BraceExpr(_)
                | WordPart::CommandSubst(_)
                | WordPart::BacktickSubst(_)
                | WordPart::ArithSubst(_)
        )
    }

    /// Check recursively if a word contains any splittable expansion
    /// (including inside DoubleQuoted parts, which suppress splitting
    /// but the word still shouldn't be split at the top level if
    /// DoubleQuoted is the only expansion-bearing part).
    fn word_has_unquoted_expansion(word: &Word) -> bool {
        for part in word {
            if Self::part_is_splittable(part) {
                return true;
            }
            // DoubleQuoted with expansion parts don't count as unquoted
            // — they suppress splitting by construction
        }
        false
    }

    /// Internal: expand a word with splitting and globbing.
    fn expand_word_inner(&mut self, word: &Word) -> Vec<String> {
        // Expand all parts, tracking quoting
        let mut expanded = String::new();
        let mut all_quoted = true;
        for part in word {
            let (text, quoted) = self.expand_part(part);
            expanded.push_str(&text);
            if !quoted {
                all_quoted = false;
            }
        }

        // If all parts were quoted, return as single string (no split/glob)
        if all_quoted || expanded.is_empty() {
            return vec![expanded];
        }

        // Determine if the word has unquoted expansion that triggers IFS splitting
        let has_expansion = Self::word_has_unquoted_expansion(word);

        // Word splitting (IFS) only on results of parameter/command/arith expansion
        let split: Vec<String> = if has_expansion {
            let ifs = self
                .vars
                .env
                .get("IFS")
                .cloned()
                .unwrap_or_else(|| " \t\n".to_string());
            let ifs_chars: Vec<char> = ifs.chars().collect();
            let parts: Vec<String> = expanded
                .split(|c: char| ifs_chars.contains(&c))
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string())
                .collect();
            if parts.is_empty() {
                return vec![];
            }
            parts
        } else {
            vec![expanded]
        };

        // Glob expansion on each resulting word (applies regardless of splitting)
        let mut result = Vec::new();
        for s in split {
            let globbed = self.expand_globs(std::slice::from_ref(&s));
            if globbed.len() == 1 && globbed[0] == s {
                result.push(s);
            } else {
                result.extend(globbed);
            }
        }
        result
    }

    /// Expand a single WordPart, returning (expanded_text, is_quoted).
    fn expand_part(&mut self, part: &WordPart) -> (String, bool) {
        match part {
            WordPart::Literal(s) => (s.to_string(), false),

            WordPart::SingleQuoted(s) => (s.to_string(), true),

            WordPart::DoubleQuoted(parts) => {
                let mut result = String::new();
                for p in parts {
                    let (text, _) = self.expand_part(p);
                    result.push_str(&text);
                }
                (result, true)
            }

            WordPart::Variable(name) => {
                // nounset check
                if self.exec.opts.nounset
                    && !self.vars.env.contains_key(name.as_str())
                    && !matches!(name.as_str(), "@" | "#" | "*" | "?" | "!" | "-" | "$" | "0")
                    && !name.chars().next().is_some_and(|c| c.is_ascii_digit())
                {
                    // For nounset, we still return empty but the caller should
                    // have already checked. The check_nounset in the old path
                    // is on the raw string. For the new path, we just return
                    // empty and let the caller handle it.
                }
                // Process-related special variables
                match name.as_str() {
                    "$" => return (self.procs.shell_pid().to_string(), false),
                    "!" => {
                        return (
                            self.procs
                                .last_bg_pid()
                                .map(|p| p.to_string())
                                .unwrap_or_default(),
                            false,
                        );
                    }
                    "BASHPID" => return (self.procs.current_pid().to_string(), false),
                    "PPID" => return (self.procs.parent_pid().to_string(), false),
                    _ => {}
                }
                // Special shell variables
                match name.as_str() {
                    "SECONDS" => {
                        let elapsed = (self.fs.time().saturating_sub(self.start_time))
                            / crate::shell::pipeline::TICKS_PER_SECOND;
                        return (elapsed.to_string(), false);
                    }
                    "UID" => return (self.fs.current_uid().to_string(), false),
                    "EUID" => return (self.fs.current_uid().to_string(), false),
                    "GROUPS" => {
                        let gids = self.fs.supplementary_gids();
                        let s = gids
                            .iter()
                            .map(|g| g.to_string())
                            .collect::<Vec<_>>()
                            .join(" ");
                        return (s, false);
                    }
                    "HOSTTYPE" => return ("wasm32".to_string(), false),
                    "OSTYPE" => return ("linux-wasm".to_string(), false),
                    _ => {}
                }
                // $RANDOM — pseudo-random number 0–32767
                if name.as_str() == "RANDOM" {
                    self.tmp_counter = self
                        .tmp_counter
                        .wrapping_mul(6364136223846793005)
                        .wrapping_add(1442695040888963407);
                    let rand_val = ((self.tmp_counter >> 33) % 32768).to_string();
                    return (rand_val, false);
                }
                let resolved = self.resolve_nameref(name);
                let val = self.vars.get(&resolved).unwrap_or_default().to_string();
                (val, false)
            }

            WordPart::BraceExpr(expr) => {
                let result = self.eval_brace_expr(expr);
                (result, false)
            }

            WordPart::CommandSubst(cmd) | WordPart::BacktickSubst(cmd) => {
                // Command substitution runs in a full subshell
                let snap = self.snapshot_subshell();
                self.procs.enter_subshell();
                let (out, code, _) = self.run_line(cmd);
                self.restore_subshell(snap);
                // Preserve exit code from substitution in parent
                self.exec.last_exit_code = code;
                self.vars.env.insert("?".into(), code.to_string());
                let trimmed = out.trim_end_matches('\n').to_string();
                (trimmed, false)
            }

            WordPart::ArithSubst(expr) => {
                let val = self.eval_arith_expr(expr);
                (val.to_string(), false)
            }

            WordPart::DollarSingleQuoted(s) => {
                let processed = process_ansi_c_escapes(s);
                (processed, true)
            }

            WordPart::Tilde(None) => (self.ident.home.to_string(), false),
            WordPart::Tilde(Some(user)) => {
                // Look up user's home directory
                if let Some(u) = self.ident.users.get_user_by_name(user) {
                    (u.home.clone(), false)
                } else {
                    (format!("~{}", user), false)
                }
            }

            WordPart::GlobPattern(s) => (s.to_string(), false),

            WordPart::ProcessSubst { dir, cmd } => {
                if *dir == '<' {
                    // <(cmd): run cmd in subshell, write output to temp file, return path
                    let snap = self.snapshot_subshell();
                    let (out, _, _) = self.run_line(cmd);
                    self.restore_subshell(snap);
                    let tmp_path = format!("/tmp/.proc_subst_{}", self.tmp_counter);
                    self.tmp_counter += 1;
                    let prev_uid = self.fs.current_uid();
                    let prev_gid = self.fs.current_gid();
                    self.fs.set_current_user(0, 0);
                    let _ = self.fs.write(&tmp_path, out.as_bytes());
                    self.fs.set_current_user(prev_uid, prev_gid);
                    (tmp_path, false)
                } else {
                    // >(cmd): output process substitution — create temp file,
                    // return its path. After command writes to it, run cmd with
                    // the file content as stdin.
                    let tmp_path = format!("/tmp/.proc_subst_{}", self.tmp_counter);
                    self.tmp_counter += 1;
                    let prev_uid = self.fs.current_uid();
                    let prev_gid = self.fs.current_gid();
                    self.fs.set_current_user(0, 0);
                    let _ = self.fs.write(&tmp_path, b"");
                    let _ = self.fs.set_mode(&tmp_path, 0o666);
                    self.fs.set_current_user(prev_uid, prev_gid);
                    // Register deferred command to run after pipeline
                    self.deferred_process_substs
                        .push((tmp_path.clone(), cmd.to_string()));
                    (tmp_path, false)
                }
            }

            WordPart::BraceExpansion(s) => {
                // Should have been handled by expand_word's brace expansion step.
                // If we get here, just return the raw text.
                (format!("{{{}}}", s), false)
            }
        }
    }
}
