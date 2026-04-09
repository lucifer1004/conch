use std::borrow::Cow;
use std::collections::BTreeMap;

use bare_vfs::MemFs;
use globset::Glob;

use crate::types::*;

/// Convert a "decimal-encoded octal" mode (e.g., 755 as u16) to actual octal (0o755).
/// Users write `mode: 755` in Typst, which JSON encodes as decimal 755.
/// We reinterpret the digits: 7*64 + 5*8 + 5 = 493 = 0o755.
fn parse_mode_digits(decimal: u16) -> u16 {
    let d2 = decimal / 100;
    let d1 = (decimal / 10) % 10;
    let d0 = decimal % 10;
    d2 * 64 + d1 * 8 + d0
}

/// Virtual shell state
pub struct Shell {
    pub(crate) fs: MemFs,
    pub(crate) cwd: String,
    pub(crate) user: String,
    pub(crate) hostname: String,
    pub(crate) home: String,
    pub(crate) env: BTreeMap<String, String>,
    pub(crate) last_exit_code: i32,
}

impl Shell {
    pub fn new(config: &Config) -> Self {
        let mut fs = MemFs::new();
        let home = &config.home;

        // Create home hierarchy (root `/` is created by MemFs::new)
        fs.create_dir_all(home);

        // Populate user-provided files
        for (file_path, spec) in &config.files {
            let full = if file_path.starts_with('/') {
                file_path.clone()
            } else {
                format!("{}/{}", home, file_path)
            };

            // Ensure parent directories exist
            if let Some(parent) = MemFs::parent(&full) {
                fs.create_dir_all(parent);
            }

            let entry = match spec {
                FileSpec::Content(content) => FsEntry::file(content.clone()),
                FileSpec::WithMode { content, mode } => {
                    // User provides mode as "octal-looking" decimal (e.g., 755).
                    // Convert: 755 decimal → parse digits as octal → 0o755.
                    let octal = parse_mode_digits(*mode);
                    FsEntry::file_with_mode(content.clone(), octal)
                }
            };
            fs.insert(full, entry);
        }

        let mut env = BTreeMap::new();
        env.insert("HOME".to_string(), home.clone());
        env.insert("USER".to_string(), config.user.clone());
        env.insert("HOSTNAME".to_string(), config.hostname.clone());
        env.insert("PWD".to_string(), home.clone());
        env.insert("SHELL".to_string(), "/bin/conch".to_string());
        if let Some(ref date) = config.date {
            env.insert("DATE".to_string(), date.clone());
        }

        Shell {
            fs,
            cwd: home.clone(),
            user: config.user.clone(),
            hostname: config.hostname.clone(),
            home: home.clone(),
            env,
            last_exit_code: 0,
        }
    }

    /// Display path: replace home prefix with ~
    pub fn display_path(&self) -> String {
        if self.cwd == self.home {
            "~".to_string()
        } else if let Some(rest) = self.cwd.strip_prefix(&self.home) {
            format!("~{}", rest)
        } else {
            self.cwd.clone()
        }
    }

    /// Expand tilde and shell variables
    pub fn expand(&self, s: &str) -> String {
        let after_tilde =
            shellexpand::tilde_with_context(s, || Some(self.home.as_str())).to_string();
        shellexpand::env_with_context_no_errors(&after_tilde, |var| {
            self.env.get(var).map(|v| Cow::Owned(v.clone()))
        })
        .to_string()
    }

    /// Resolve a possibly-relative path to a normalized absolute path
    pub fn resolve(&self, path: &str) -> String {
        let expanded = self.expand(path);
        let abs = if expanded.starts_with('/') {
            expanded
        } else {
            format!("{}/{}", self.cwd, expanded)
        };
        MemFs::normalize(&abs)
    }

    /// List direct children of a directory, sorted by name.
    /// Returns (name, is_dir, mode).
    pub fn list_dir(&self, dir: &str) -> Vec<(String, bool, u16)> {
        self.fs
            .read_dir(dir)
            .unwrap_or_default()
            .into_iter()
            .map(|e| (e.name, e.is_dir, e.mode))
            .collect()
    }

    /// Expand glob patterns in arguments
    pub fn expand_globs(&self, args: &[String]) -> Vec<String> {
        let mut result = Vec::new();
        for arg in args {
            if arg.contains('*') || arg.contains('?') {
                if let Some(expanded) = self.glob_expand(arg) {
                    result.extend(expanded);
                    continue;
                }
            }
            result.push(arg.clone());
        }
        result
    }

    fn glob_expand(&self, pattern: &str) -> Option<Vec<String>> {
        let (dir, file_pattern) = if let Some((d, f)) = pattern.rsplit_once('/') {
            (
                self.resolve(if d.is_empty() { "/" } else { d }),
                f.to_string(),
            )
        } else {
            (self.cwd.clone(), pattern.to_string())
        };

        let glob = Glob::new(&file_pattern).ok()?.compile_matcher();
        let children = self.list_dir(&dir);

        let mut entries: Vec<String> = children
            .into_iter()
            .filter(|(name, _, _)| glob.is_match(name.as_str()))
            .map(|(name, _, _)| {
                if pattern.contains('/') {
                    format!("{}/{}", dir, name)
                } else {
                    name
                }
            })
            .collect();

        if entries.is_empty() {
            None
        } else {
            entries.sort();
            Some(entries)
        }
    }

    /// Create directory and all parents
    pub fn mkdir_p(&mut self, abs_path: &str) {
        self.fs.create_dir_all(abs_path);
    }

    /// Build an output entry
    pub fn entry(
        &self,
        path: String,
        command: &str,
        output: String,
        exit_code: i32,
    ) -> OutputEntry {
        OutputEntry {
            user: self.user.clone(),
            hostname: self.hostname.clone(),
            path,
            command: command.to_string(),
            output,
            exit_code,
            lang: None,
        }
    }

    /// Run a command line and return (output, exit_code, lang_hint).
    /// This is the core execution engine used by both interactive and script modes.
    pub fn run_line(&mut self, line: &str) -> (String, i32, Option<String>) {
        let chain = crate::parser::parse(line);

        let mut all_output = String::new();
        let mut final_code: i32 = 0;
        let mut final_lang: Option<String> = None;

        for (i, (pipeline, _op)) in chain.pipelines.iter().enumerate() {
            if i > 0 {
                if let Some(prev_op) = &chain.pipelines[i - 1].1 {
                    match prev_op {
                        crate::parser::ChainOp::And if final_code != 0 => continue,
                        crate::parser::ChainOp::Or if final_code == 0 => continue,
                        _ => {}
                    }
                }
            }

            let (output, code, lang) = self.execute_pipeline(pipeline);
            if !output.is_empty() {
                if !all_output.is_empty() {
                    all_output.push('\n');
                }
                all_output.push_str(&output);
            }
            final_code = code;
            if chain.pipelines.len() == 1 {
                final_lang = lang;
            }
        }

        self.last_exit_code = final_code;
        self.env.insert("?".to_string(), final_code.to_string());
        (all_output, final_code, final_lang)
    }

    /// Execute a full command line (handles pipes, redirects, chaining).
    /// Returns an OutputEntry for terminal display.
    pub fn execute(&mut self, line: &str) -> OutputEntry {
        let display = self.display_path();
        let (output, code, lang) = self.run_line(line);
        let mut entry = self.entry(display, line, output, code);
        entry.lang = lang;
        entry
    }

    /// Execute a script file line by line, returning combined output.
    pub fn run_script(&mut self, script: &str) -> (String, i32) {
        let mut output = Vec::new();
        let mut last_code = 0;

        for line in script.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }
            let (out, code, _) = self.run_line(trimmed);
            if !out.is_empty() {
                output.push(out);
            }
            last_code = code;
        }

        (output.join("\n"), last_code)
    }

    /// Execute a single pipeline (commands joined by `|`), with optional redirect.
    /// Returns (output, exit_code, optional_lang_hint).
    fn execute_pipeline(
        &mut self,
        pipeline: &crate::parser::Pipeline,
    ) -> (String, i32, Option<String>) {
        let mut stdin: Option<String> = None;
        let mut last_code: i32 = 0;
        let mut last_lang: Option<String> = None;
        let segment_count = pipeline.segments.len();

        for segment in &pipeline.segments {
            let trimmed = segment.trim();
            if trimmed.is_empty() {
                continue;
            }

            let tokens = match shlex::split(trimmed) {
                Some(t) => t,
                None => return ("conch: syntax error: unterminated quote".into(), 2, None),
            };
            if tokens.is_empty() {
                continue;
            }

            let cmd = &tokens[0];
            let raw_args = &tokens[1..];
            let expanded: Vec<String> = raw_args.iter().map(|a| self.expand(a)).collect();
            let args = self.expand_globs(&expanded);

            let (output, code, lang) =
                crate::commands::dispatch(self, cmd, &args, stdin.as_deref());
            stdin = Some(output);
            last_code = code;
            // Only keep lang hint for single-command pipelines (not piped)
            last_lang = if segment_count == 1 { lang } else { None };
        }

        let output = stdin.unwrap_or_default();

        // Handle redirect
        if let Some(ref redir) = pipeline.redirect {
            let target = self.resolve(&self.expand(&redir.target));
            // Check write permission on existing file
            if let Some(e) = self.fs.get(&target) {
                if !e.is_writable() {
                    return (
                        format!("conch: {}: Permission denied", redir.target),
                        1,
                        None,
                    );
                }
            }
            match redir.typ {
                crate::parser::RedirectType::Overwrite => {
                    self.fs.insert(target, FsEntry::file(output));
                }
                crate::parser::RedirectType::Append => {
                    let existing = self
                        .fs
                        .read_to_string(&target)
                        .map(|s| s.to_string())
                        .unwrap_or_default();
                    let appended = if existing.is_empty() {
                        output
                    } else {
                        format!("{}\n{}", existing, output)
                    };
                    self.fs.insert(target, FsEntry::file(appended));
                }
            }
            return (String::new(), last_code, None); // redirect suppresses display
        }

        (output, last_code, last_lang)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Config;

    fn shell_with_files(files: serde_json::Value) -> Shell {
        let v = serde_json::json!({
            "user": "u",
            "hostname": "h",
            "home": "/home/u",
            "files": files,
            "commands": [],
        });
        let c: Config = serde_json::from_value(v).unwrap();
        Shell::new(&c)
    }

    fn shell() -> Shell {
        shell_with_files(serde_json::json!({}))
    }

    #[test]
    fn run_line_echo() {
        let mut s = shell();
        let (out, code, _) = s.run_line("echo hi");
        assert_eq!(code, 0);
        assert_eq!(out.trim_end(), "hi");
    }

    #[test]
    fn run_line_unknown_command_exit_127() {
        let mut s = shell();
        let (out, code, _) = s.run_line("nosuchcmd");
        assert_eq!(code, 127);
        assert!(out.contains("not found"));
    }

    #[test]
    fn and_skips_second_when_first_fails() {
        let mut s = shell();
        let (out, code, _) = s.run_line("false && echo no");
        assert_ne!(code, 0);
        assert!(!out.contains("no"));
    }

    #[test]
    fn or_runs_second_when_first_fails() {
        let mut s = shell();
        let (out, code, _) = s.run_line("false || echo yes");
        assert_eq!(code, 0);
        assert!(out.contains("yes"));
    }

    #[test]
    fn pipe_feeds_stdin() {
        let mut s = shell();
        let (out, code, _) = s.run_line("echo ab | wc");
        assert_eq!(code, 0);
        // stdin `wc` prints: lines, words, bytes — last field is byte count (`echo` has no trailing newline)
        let last = out.split_whitespace().last().expect("wc output");
        assert_eq!(last, "2");
    }

    #[test]
    fn expand_replaces_home() {
        let s = shell();
        assert_eq!(s.expand("$HOME"), "/home/u");
    }

    #[test]
    fn display_path_tilde_at_home() {
        let s = shell();
        assert_eq!(s.display_path(), "~");
    }

    #[test]
    fn execute_entry_records_command() {
        let mut s = shell();
        let e = s.execute("pwd");
        assert_eq!(e.exit_code, 0);
        assert_eq!(e.command, "pwd");
        assert!(e.output.contains("/home/u") || e.output == "/home/u");
    }

    // --- Virtual FS & permissions (seeded `files` in Config) ---

    #[test]
    fn cat_reads_seeded_file() {
        let mut s = shell_with_files(serde_json::json!({
            "note.txt": "hello"
        }));
        let (out, code, _) = s.run_line("cat note.txt");
        assert_eq!(code, 0);
        assert_eq!(out, "hello");
    }

    #[test]
    fn cat_unreadable_mode_zero() {
        let mut s = shell_with_files(serde_json::json!({
            "sec.txt": { "content": "secret", "mode": 0 }
        }));
        let (out, code, _) = s.run_line("cat sec.txt");
        assert_eq!(code, 1);
        assert!(out.contains("Permission denied"), "got {:?}", out);
    }

    #[test]
    fn chmod_restores_read_then_cat_succeeds() {
        let mut s = shell_with_files(serde_json::json!({
            "sec.txt": { "content": "ok", "mode": 0 }
        }));
        let (_, c1, _) = s.run_line("cat sec.txt");
        assert_ne!(c1, 0);
        let (_, c2, _) = s.run_line("chmod 644 sec.txt");
        assert_eq!(c2, 0);
        let (out, c3, _) = s.run_line("cat sec.txt");
        assert_eq!(c3, 0);
        assert_eq!(out, "ok");
    }

    #[test]
    fn redirect_overwrite_rejects_read_only_file() {
        let mut s = shell_with_files(serde_json::json!({
            "ro.txt": { "content": "orig", "mode": 444 }
        }));
        let (out, code, _) = s.run_line("echo hijack > ro.txt");
        assert_eq!(code, 1);
        assert!(out.contains("Permission denied"), "got {:?}", out);
        let (content, _, _) = s.run_line("cat ro.txt");
        assert_eq!(content, "orig");
    }

    #[test]
    fn tee_rejects_read_only_target() {
        let mut s = shell_with_files(serde_json::json!({
            "ro.txt": { "content": "orig", "mode": 444 }
        }));
        let (out, code, _) = s.run_line("echo x | tee ro.txt");
        assert_eq!(code, 1);
        assert!(out.contains("Permission denied"), "got {:?}", out);
    }

    #[test]
    fn mkdir_touch_ls_lists_new_file() {
        let mut s = shell();
        let (_, c1, _) = s.run_line("mkdir d");
        assert_eq!(c1, 0);
        let (_, c2, _) = s.run_line("touch d/x.txt");
        assert_eq!(c2, 0);
        let (listing, c3, _) = s.run_line("ls d");
        assert_eq!(c3, 0);
        assert!(
            listing.contains("x.txt"),
            "expected x.txt in listing, got {:?}",
            listing
        );
    }

    #[test]
    fn cp_copies_file() {
        let mut s = shell_with_files(serde_json::json!({
            "a.txt": "alpha"
        }));
        let (_, c1, _) = s.run_line("cp a.txt b.txt");
        assert_eq!(c1, 0);
        let (out, c2, _) = s.run_line("cat b.txt");
        assert_eq!(c2, 0);
        assert_eq!(out, "alpha");
    }

    #[test]
    fn mv_removes_source() {
        let mut s = shell_with_files(serde_json::json!({
            "a.txt": "moved"
        }));
        let (_, c1, _) = s.run_line("mv a.txt z.txt");
        assert_eq!(c1, 0);
        let (_, c2, _) = s.run_line("cat a.txt");
        assert_ne!(c2, 0);
        let (out, c3, _) = s.run_line("cat z.txt");
        assert_eq!(c3, 0);
        assert_eq!(out, "moved");
    }

    #[test]
    fn rm_removes_file() {
        let mut s = shell_with_files(serde_json::json!({
            "gone.txt": "bye"
        }));
        let (_, c1, _) = s.run_line("rm gone.txt");
        assert_eq!(c1, 0);
        let (_, c2, _) = s.run_line("cat gone.txt");
        assert_ne!(c2, 0);
    }

    #[test]
    fn find_name_filter() {
        let mut s = shell_with_files(serde_json::json!({
            "sub/a.rs": "",
            "sub/b.txt": ""
        }));
        let (out, code, _) = s.run_line("find sub -name '*.rs'");
        assert_eq!(code, 0);
        assert!(out.contains("a.rs"), "got {:?}", out);
        assert!(!out.contains("b.txt"));
    }

    #[test]
    fn cp_source_unreadable() {
        let mut s = shell_with_files(serde_json::json!({
            "locked.txt": { "content": "x", "mode": 0 }
        }));
        let (out, code, _) = s.run_line("cp locked.txt copy.txt");
        assert_eq!(code, 1);
        assert!(out.contains("Permission denied"), "got {:?}", out);
    }

    #[test]
    fn cp_missing_source() {
        let mut s = shell();
        let (out, code, _) = s.run_line("cp nowhere.txt out.txt");
        assert_eq!(code, 1);
        assert!(out.contains("cannot stat"), "got {:?}", out);
    }

    #[test]
    fn cat_missing_file() {
        let mut s = shell();
        let (out, code, _) = s.run_line("cat missing.txt");
        assert_eq!(code, 1);
        assert!(out.contains("No such file"), "got {:?}", out);
    }

    #[test]
    fn ls_missing_path() {
        let mut s = shell();
        let (out, code, _) = s.run_line("ls ghost_dir");
        assert_eq!(code, 2);
        assert!(
            out.contains("cannot access") || out.contains("No such file"),
            "got {:?}",
            out
        );
    }

    #[test]
    fn chmod_missing_target() {
        let mut s = shell();
        let (out, code, _) = s.run_line("chmod 644 nope.txt");
        assert_eq!(code, 1);
        assert!(out.contains("cannot access"), "got {:?}", out);
    }

    #[test]
    fn redirect_append_rejects_read_only() {
        let mut s = shell_with_files(serde_json::json!({
            "ro.txt": { "content": "line1", "mode": 444 }
        }));
        let (out, code, _) = s.run_line("echo line2 >> ro.txt");
        assert_eq!(code, 1);
        assert!(out.contains("Permission denied"), "got {:?}", out);
    }

    #[test]
    fn mkdir_nested_without_p_fails() {
        let mut s = shell();
        let (out, code, _) = s.run_line("mkdir a/b/c");
        assert_eq!(code, 1);
        assert!(
            out.contains("cannot create") || out.contains("No such file"),
            "got {:?}",
            out
        );
    }

    #[test]
    fn rm_directory_requires_recursive() {
        let mut s = shell();
        let (_, c1, _) = s.run_line("mkdir mydir");
        assert_eq!(c1, 0);
        let (out, code, _) = s.run_line("rm mydir");
        assert_eq!(code, 1);
        assert!(out.contains("Is a directory"), "got {:?}", out);
    }

    #[test]
    fn find_missing_root() {
        let mut s = shell();
        let (out, code, _) = s.run_line("find nowhere -name '*'");
        assert_eq!(code, 1);
        assert!(out.contains("No such file"), "got {:?}", out);
    }

    #[test]
    fn touch_creates_empty_file_then_cat() {
        let mut s = shell();
        let (_, c1, _) = s.run_line("touch empty.txt");
        assert_eq!(c1, 0);
        let (out, c2, _) = s.run_line("cat empty.txt");
        assert_eq!(c2, 0);
        assert_eq!(out, "");
    }

    // --- env, nav, text, scripts, parser errors ---

    #[test]
    fn cd_subdir_then_pwd() {
        let mut s = shell();
        assert_eq!(s.run_line("mkdir deep").1, 0);
        assert_eq!(s.run_line("cd deep").1, 0);
        let (out, code, _) = s.run_line("pwd");
        assert_eq!(code, 0);
        assert!(out.contains("/home/u/deep"), "got {:?}", out);
    }

    #[test]
    fn export_semicolon_echo_expands_var() {
        let mut s = shell();
        let (out, code, _) = s.run_line("export ZZ=hello; echo $ZZ");
        assert_eq!(code, 0);
        assert_eq!(out.trim(), "hello");
    }

    #[test]
    fn which_and_type_builtin() {
        let mut s = shell();
        let (w, c1, _) = s.run_line("which echo");
        assert_eq!(c1, 0);
        assert!(w.contains("/bin/echo"), "got {:?}", w);
        let (t, c2, _) = s.run_line("type cd");
        assert_eq!(c2, 0);
        assert!(t.contains("builtin"), "got {:?}", t);
    }

    #[test]
    fn date_uses_config_env_date() {
        let c: Config = serde_json::from_value(serde_json::json!({
            "user": "u",
            "hostname": "h",
            "home": "/home/u",
            "files": {},
            "commands": [],
            "date": "Wed Apr  8 12:00:00 UTC 2026",
        }))
        .unwrap();
        let mut s = Shell::new(&c);
        let (out, code, _) = s.run_line("date");
        assert_eq!(code, 0);
        assert_eq!(out, "Wed Apr  8 12:00:00 UTC 2026");
    }

    #[test]
    fn grep_file_filter() {
        let mut s = shell_with_files(serde_json::json!({
            "lines.txt": "alpha\nbeta\nalpha2\n"
        }));
        let (out, code, _) = s.run_line("grep alpha lines.txt");
        assert_eq!(code, 0);
        assert!(out.contains("alpha"));
        assert!(!out.contains("beta"));
    }

    #[test]
    fn mkdir_p_nested() {
        let mut s = shell();
        assert_eq!(s.run_line("mkdir -p x/y/z").1, 0);
        let (listing, code, _) = s.run_line("ls x/y");
        assert_eq!(code, 0);
        assert!(listing.contains("z"), "got {:?}", listing);
    }

    #[test]
    fn unterminated_quote_is_syntax_error() {
        let mut s = shell();
        let (out, code, _) = s.run_line("echo 'broken");
        assert_eq!(code, 2);
        assert!(out.contains("unterminated"), "got {:?}", out);
    }

    #[test]
    fn bash_runs_script_from_file() {
        let mut s = shell_with_files(serde_json::json!({
            "run.sh": { "content": "echo frombash\n", "mode": 755 }
        }));
        let (out, code, _) = s.run_line("bash run.sh");
        assert_eq!(code, 0);
        assert!(out.contains("frombash"), "got {:?}", out);
    }

    #[test]
    fn tree_shows_nested_files() {
        let mut s = shell();
        assert_eq!(s.run_line("mkdir -p t/a").1, 0);
        assert_eq!(s.run_line("touch t/a/f.txt").1, 0);
        let (out, code, _) = s.run_line("tree t");
        assert_eq!(code, 0);
        assert!(
            out.contains("f.txt") || out.contains(".txt"),
            "got {:?}",
            out
        );
    }

    // --- `./` exec, `bash` errors, text tools ---

    #[test]
    fn exec_dot_slash_runs_with_execute_bit() {
        let mut s = shell_with_files(serde_json::json!({
            "hello.sh": { "content": "echo from_exec\n", "mode": 755 }
        }));
        let (out, code, _) = s.run_line("./hello.sh");
        assert_eq!(code, 0);
        assert!(out.contains("from_exec"), "got {:?}", out);
    }

    #[test]
    fn exec_rejects_non_executable_file() {
        let mut s = shell_with_files(serde_json::json!({
            "no_x.sh": { "content": "echo x\n", "mode": 644 }
        }));
        let (out, code, _) = s.run_line("./no_x.sh");
        assert_eq!(code, 126);
        assert!(out.contains("Permission denied"), "got {:?}", out);
    }

    #[test]
    fn exec_rejects_unreadable_script() {
        let mut s = shell_with_files(serde_json::json!({
            "locked.sh": { "content": "x", "mode": 0 }
        }));
        let (out, code, _) = s.run_line("./locked.sh");
        assert_eq!(code, 126);
        assert!(out.contains("Permission denied"), "got {:?}", out);
    }

    #[test]
    fn exec_missing_script() {
        let mut s = shell();
        let (out, code, _) = s.run_line("./missing.sh");
        assert_eq!(code, 127);
        assert!(out.contains("No such file"), "got {:?}", out);
    }

    #[test]
    fn bash_missing_script() {
        let mut s = shell();
        let (out, code, _) = s.run_line("bash nowhere.sh");
        assert_eq!(code, 1);
        assert!(out.contains("No such file"), "got {:?}", out);
    }

    #[test]
    fn bash_rejects_directory() {
        let mut s = shell();
        assert_eq!(s.run_line("mkdir adir").1, 0);
        let (out, code, _) = s.run_line("bash adir");
        assert_eq!(code, 1);
        assert!(out.contains("Is a directory"), "got {:?}", out);
    }

    #[test]
    fn bash_rejects_unreadable_script() {
        let mut s = shell_with_files(serde_json::json!({
            "secret.sh": { "content": "echo no\n", "mode": 0 }
        }));
        let (out, code, _) = s.run_line("bash secret.sh");
        assert_eq!(code, 1);
        assert!(out.contains("Permission denied"), "got {:?}", out);
    }

    #[test]
    fn head_tail_take_lines_from_file() {
        let mut s = shell_with_files(serde_json::json!({
            "rows.txt": "a\nb\nc\n"
        }));
        let (h, c1, _) = s.run_line("head -n 2 rows.txt");
        assert_eq!(c1, 0);
        assert_eq!(h, "a\nb");
        let (t, c2, _) = s.run_line("tail -n 1 rows.txt");
        assert_eq!(c2, 0);
        assert_eq!(t, "c");
    }

    #[test]
    fn grep_line_numbers_and_count() {
        let mut s = shell_with_files(serde_json::json!({
            "g.txt": "one\ntwo\none\n"
        }));
        let (out, c1, _) = s.run_line("grep -n one g.txt");
        assert_eq!(c1, 0);
        assert!(out.contains("1") && out.contains("3"), "got {:?}", out);
        let (cnt, c2, _) = s.run_line("grep -c one g.txt");
        assert_eq!(c2, 0);
        assert!(cnt.contains('2'), "got {:?}", cnt);
    }

    #[test]
    fn chmod_invalid_mode() {
        let mut s = shell_with_files(serde_json::json!({ "f.txt": "x" }));
        let (out, code, _) = s.run_line("chmod zzz f.txt");
        assert_eq!(code, 1);
        assert!(out.contains("invalid mode"), "got {:?}", out);
    }

    #[test]
    fn grep_no_match_exits_one() {
        let mut s = shell_with_files(serde_json::json!({
            "only.txt": "foo\nbar\n"
        }));
        let (out, code, _) = s.run_line("grep nomatch only.txt");
        assert_eq!(code, 1);
        assert!(out.is_empty());
    }

    #[test]
    fn sort_sorts_and_numeric() {
        let mut s = shell_with_files(serde_json::json!({
            "letters.txt": "c\na\nb\n",
            "nums.txt": "10\n2\n"
        }));
        let (out, c1, _) = s.run_line("sort letters.txt");
        assert_eq!(c1, 0);
        assert_eq!(out, "a\nb\nc");
        let (nout, c2, _) = s.run_line("sort -n nums.txt");
        assert_eq!(c2, 0);
        assert_eq!(nout, "2\n10");
    }

    #[test]
    fn uniq_dedupes_and_counts() {
        let mut s = shell_with_files(serde_json::json!({
            "u.txt": "a\na\nb\n"
        }));
        let (out, c1, _) = s.run_line("uniq u.txt");
        assert_eq!(c1, 0);
        assert_eq!(out, "a\nb");
        let (cout, c2, _) = s.run_line("uniq -c u.txt");
        assert_eq!(c2, 0);
        assert!(cout.contains("2") && cout.contains("a"), "got {:?}", cout);
    }

    #[test]
    fn cut_fields() {
        let mut s = shell_with_files(serde_json::json!({
            "csv.txt": "x,y\np,q\n"
        }));
        let (out, code, _) = s.run_line("cut -d, -f2 csv.txt");
        assert_eq!(code, 0);
        assert_eq!(out, "y\nq");
    }

    #[test]
    fn tr_substitutes_stdin() {
        let mut s = shell();
        let (out, code, _) = s.run_line("echo hello | tr h H");
        assert_eq!(code, 0);
        assert_eq!(out, "Hello");
    }

    #[test]
    fn rev_reverses_file_lines() {
        let mut s = shell_with_files(serde_json::json!({ "r.txt": "abc\nxy\n" }));
        let (out, code, _) = s.run_line("rev r.txt");
        assert_eq!(code, 0);
        assert_eq!(out, "cba\nyx");
    }

    #[test]
    fn seq_inclusive_range() {
        let mut s = shell();
        let (out, code, _) = s.run_line("seq 2 4");
        assert_eq!(code, 0);
        assert_eq!(out, "2\n3\n4");
    }

    #[test]
    fn cp_into_existing_directory() {
        let mut s = shell_with_files(serde_json::json!({
            "src.txt": "payload"
        }));
        assert_eq!(s.run_line("mkdir bin").1, 0);
        let (_, c1, _) = s.run_line("cp src.txt bin");
        assert_eq!(c1, 0);
        let (out, c2, _) = s.run_line("cat bin/src.txt");
        assert_eq!(c2, 0);
        assert_eq!(out, "payload");
    }

    #[test]
    fn rm_force_missing_succeeds() {
        let mut s = shell();
        let (_, code, _) = s.run_line("rm -f does_not_exist.txt");
        assert_eq!(code, 0);
    }

    #[test]
    fn rm_rf_removes_directory_tree() {
        let mut s = shell();
        assert_eq!(s.run_line("mkdir -p tree/sub").1, 0);
        assert_eq!(s.run_line("touch tree/sub/f.txt").1, 0);
        assert_eq!(s.run_line("rm -rf tree").1, 0);
        let (_, code, _) = s.run_line("ls tree");
        assert_ne!(code, 0);
    }

    #[test]
    fn cd_without_args_returns_home() {
        let mut s = shell();
        assert_eq!(s.run_line("mkdir deep").1, 0);
        assert_eq!(s.run_line("cd deep").1, 0);
        assert_eq!(s.run_line("cd").1, 0);
        let (out, code, _) = s.run_line("pwd");
        assert_eq!(code, 0);
        assert!(
            out.contains("/home/u") && !out.contains("deep"),
            "got {:?}",
            out
        );
    }

    #[test]
    fn sort_reverse_order() {
        let mut s = shell_with_files(serde_json::json!({
            "rev.txt": "a\nb\nc\n"
        }));
        let (out, code, _) = s.run_line("sort -r rev.txt");
        assert_eq!(code, 0);
        assert_eq!(out, "c\nb\na");
    }

    #[test]
    fn tr_delete_chars_from_stdin() {
        let mut s = shell();
        let (out, code, _) = s.run_line("echo hello | tr -d l");
        assert_eq!(code, 0);
        assert_eq!(out, "heo");
    }

    #[test]
    fn cp_source_directory_omits() {
        let mut s = shell();
        assert_eq!(s.run_line("mkdir srcdir").1, 0);
        let (out, code, _) = s.run_line("cp srcdir out.txt");
        assert_eq!(code, 1);
        assert!(out.contains("omitting directory"), "got {:?}", out);
    }

    #[test]
    fn env_and_printenv_list_variables() {
        let mut s = shell();
        let (eout, c1, _) = s.run_line("env");
        assert_eq!(c1, 0);
        assert!(eout.contains("HOME=/home/u"), "got {:?}", eout);
        assert!(eout.contains("USER=u"));
        let (pout, c2, _) = s.run_line("printenv");
        assert_eq!(c2, 0);
        assert_eq!(eout, pout);
    }

    #[test]
    fn grep_multiple_files_shows_filename_prefix() {
        let mut s = shell_with_files(serde_json::json!({
            "ga.txt": "hit\n",
            "gb.txt": "hit\n"
        }));
        let (out, code, _) = s.run_line("grep hit ga.txt gb.txt");
        assert_eq!(code, 0);
        assert!(out.contains("ga.txt"), "got {:?}", out);
        assert!(out.contains("gb.txt"), "got {:?}", out);
        assert!(out.lines().count() >= 2);
    }

    #[test]
    fn sort_numeric_then_reverse() {
        let mut s = shell_with_files(serde_json::json!({
            "nums.txt": "2\n10\n1\n"
        }));
        let (out, code, _) = s.run_line("sort -n -r nums.txt");
        assert_eq!(code, 0);
        assert_eq!(out, "10\n2\n1");
    }

    #[test]
    fn which_only_finds_builtins() {
        let mut s = shell();
        let (out, code, _) = s.run_line("which /bin/ls");
        assert_eq!(code, 1);
        assert!(
            out.contains("no /bin/ls") || out.contains("no "),
            "got {:?}",
            out
        );
    }

    #[test]
    fn export_adds_variable_to_env() {
        let mut s = shell();
        let (out, code, _) = s.run_line("export MYVAR=testval; env");
        assert_eq!(code, 0);
        assert!(out.contains("MYVAR=testval"), "got {:?}", out);
    }

    #[test]
    fn cd_fails_when_target_is_file() {
        let mut s = shell_with_files(serde_json::json!({ "notdir.txt": "x" }));
        let (out, code, _) = s.run_line("cd notdir.txt");
        assert_eq!(code, 1);
        assert!(out.contains("not a directory"), "got {:?}", out);
    }

    #[test]
    fn true_and_runs_following_command() {
        let mut s = shell();
        let (out, code, _) = s.run_line("true && echo chained");
        assert_eq!(code, 0);
        assert_eq!(out.trim(), "chained");
    }

    #[test]
    fn grep_ignore_case() {
        let mut s = shell_with_files(serde_json::json!({
            "mix.txt": "Hello\n"
        }));
        let (out, code, _) = s.run_line("grep -i hello mix.txt");
        assert_eq!(code, 0);
        assert!(out.contains("Hello"), "got {:?}", out);
    }

    // --- Gap coverage: redirects, tee, wc, basename/dirname, echo flags, glob, tilde, cat -n, grep -v ---

    #[test]
    fn redirect_overwrite_writes_and_reads_back() {
        let mut s = shell();
        let (_, c1, _) = s.run_line("echo payload > out.txt");
        assert_eq!(c1, 0);
        let (out, c2, _) = s.run_line("cat out.txt");
        assert_eq!(c2, 0);
        assert_eq!(out, "payload");
    }

    #[test]
    fn redirect_append_accumulates() {
        let mut s = shell();
        s.run_line("echo first > log.txt");
        s.run_line("echo second >> log.txt");
        let (out, code, _) = s.run_line("cat log.txt");
        assert_eq!(code, 0);
        assert!(out.contains("first"), "got {:?}", out);
        assert!(out.contains("second"), "got {:?}", out);
    }

    #[test]
    fn tee_writes_file_and_passes_through() {
        let mut s = shell();
        let (out, code, _) = s.run_line("echo hello | tee copy.txt");
        assert_eq!(code, 0);
        assert_eq!(out, "hello");
        let (content, c2, _) = s.run_line("cat copy.txt");
        assert_eq!(c2, 0);
        assert_eq!(content, "hello");
    }

    #[test]
    fn tee_append_mode() {
        let mut s = shell_with_files(serde_json::json!({
            "log.txt": "line1"
        }));
        let (_, code, _) = s.run_line("echo line2 | tee -a log.txt");
        assert_eq!(code, 0);
        let (out, _, _) = s.run_line("cat log.txt");
        assert!(out.contains("line1"), "got {:?}", out);
        assert!(out.contains("line2"), "got {:?}", out);
    }

    #[test]
    fn wc_counts_file() {
        let mut s = shell_with_files(serde_json::json!({
            "w.txt": "one two\nthree\n"
        }));
        let (out, code, _) = s.run_line("wc w.txt");
        assert_eq!(code, 0);
        // Should contain line count, word count, byte count, filename
        assert!(out.contains("w.txt"), "got {:?}", out);
        assert!(out.contains("3"), "expected 3 words, got {:?}", out);
    }

    #[test]
    fn basename_extracts_filename() {
        let mut s = shell();
        let (out, code, _) = s.run_line("basename /home/u/file.txt");
        assert_eq!(code, 0);
        assert_eq!(out, "file.txt");
    }

    #[test]
    fn basename_strips_suffix() {
        let mut s = shell();
        let (out, code, _) = s.run_line("basename /home/u/file.txt .txt");
        assert_eq!(code, 0);
        assert_eq!(out, "file");
    }

    #[test]
    fn dirname_extracts_directory() {
        let mut s = shell();
        let (out, code, _) = s.run_line("dirname /home/u/file.txt");
        assert_eq!(code, 0);
        assert_eq!(out, "/home/u");
    }

    #[test]
    fn dirname_bare_filename_returns_dot() {
        let mut s = shell();
        let (out, code, _) = s.run_line("dirname file.txt");
        assert_eq!(code, 0);
        assert_eq!(out, ".");
    }

    #[test]
    fn echo_e_interprets_escapes() {
        let mut s = shell();
        let (out, code, _) = s.run_line("echo -e 'a\\nb'");
        assert_eq!(code, 0);
        assert!(out.contains('\n'), "expected newline, got {:?}", out);
        assert!(out.starts_with('a'), "got {:?}", out);
    }

    #[test]
    fn echo_n_omits_nothing_extra() {
        let mut s = shell();
        // -n in real bash suppresses trailing newline; in our impl it's a flag
        let (out, code, _) = s.run_line("echo -n hello");
        assert_eq!(code, 0);
        assert_eq!(out, "hello");
    }

    #[test]
    fn glob_expansion_matches_files() {
        let mut s = shell_with_files(serde_json::json!({
            "a.txt": "aa",
            "b.txt": "bb",
            "c.rs": "cc"
        }));
        let (out, code, _) = s.run_line("echo *.txt");
        assert_eq!(code, 0);
        assert!(out.contains("a.txt"), "got {:?}", out);
        assert!(out.contains("b.txt"), "got {:?}", out);
        assert!(!out.contains("c.rs"), "got {:?}", out);
    }

    #[test]
    fn tilde_expansion_in_cat() {
        let mut s = shell_with_files(serde_json::json!({
            "note.txt": "from home"
        }));
        let (out, code, _) = s.run_line("cat ~/note.txt");
        assert_eq!(code, 0);
        assert_eq!(out, "from home");
    }

    #[test]
    fn cat_n_shows_line_numbers() {
        let mut s = shell_with_files(serde_json::json!({
            "lines.txt": "alpha\nbeta"
        }));
        let (out, code, _) = s.run_line("cat -n lines.txt");
        assert_eq!(code, 0);
        assert!(out.contains("1"), "got {:?}", out);
        assert!(out.contains("2"), "got {:?}", out);
        assert!(out.contains("alpha"), "got {:?}", out);
        assert!(out.contains("beta"), "got {:?}", out);
    }

    #[test]
    fn grep_v_inverts_match() {
        let mut s = shell_with_files(serde_json::json!({
            "data.txt": "keep\ndrop\nkeep2\n"
        }));
        let (out, code, _) = s.run_line("grep -v drop data.txt");
        assert_eq!(code, 0);
        assert!(out.contains("keep"), "got {:?}", out);
        assert!(!out.contains("drop"), "got {:?}", out);
    }
}
