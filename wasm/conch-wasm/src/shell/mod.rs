use std::borrow::Cow;
use std::collections::BTreeMap;

use bare_vfs::MemFs;
use globset::Glob;

use crate::types::*;
use crate::userdb::UserDb;

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
    pub(crate) users: UserDb,
}

impl Shell {
    pub fn new(config: &Config) -> Self {
        let mut fs = MemFs::new();
        let mut users_db = UserDb::new();

        // Resolve system config from either new `system` field or legacy flat fields
        let hostname: String;
        let home: String;

        if let Some(ref sys) = config.system {
            hostname = sys.hostname.clone();
            // Find main user's home from user specs, or default
            let main_spec = sys.users.iter().find(|u| u.name == config.user);
            home = main_spec
                .and_then(|s| s.home.clone())
                .unwrap_or_else(|| format!("/home/{}", config.user));
        } else {
            // Legacy mode
            hostname = config
                .hostname
                .clone()
                .unwrap_or_else(|| "conch".to_string());
            home = config
                .home
                .clone()
                .unwrap_or_else(|| format!("/home/{}", config.user));
        }

        // 1. Create root
        users_db.add_root();

        // 2. Create main user
        let main_uid;
        let main_gid;
        if let Some(ref sys) = config.system {
            let main_spec = sys.users.iter().find(|u| u.name == config.user);
            main_uid = main_spec.and_then(|s| s.uid).unwrap_or(1000);
            main_gid = main_uid;
        } else {
            main_uid = 1000;
            main_gid = 1000;
        }
        users_db.add_user_with_ids(&config.user, main_uid, main_gid, &home);

        // 3. Create additional users and groups from system spec
        if let Some(ref sys) = config.system {
            for spec in &sys.users {
                if spec.name == config.user {
                    continue;
                }
                let uid = spec.uid.unwrap_or_else(|| users_db.next_available_uid());
                let user_home = spec
                    .home
                    .clone()
                    .unwrap_or_else(|| format!("/home/{}", spec.name));
                users_db.add_user_with_ids(&spec.name, uid, uid, &user_home);
                let _ = fs.create_dir_all(&user_home);
                fs.chown(&user_home, uid, uid).unwrap_or(());
            }

            // 4. Create groups from group specs
            for spec in &sys.groups {
                if let Some(g) = spec.gid {
                    users_db.add_group_with_id(&spec.name, g);
                } else {
                    users_db.add_group(&spec.name);
                }
                for member in &spec.members {
                    users_db.add_user_to_group(member, &spec.name).ok();
                }
            }

            // 5. Add users to groups specified in user specs
            for spec in &sys.users {
                for group_name in &spec.groups {
                    if users_db.get_group_by_name(group_name).is_none() {
                        users_db.add_group(group_name);
                    }
                    users_db.add_user_to_group(&spec.name, group_name).ok();
                }
            }
        }

        // Create standard system directories as root before switching to user.
        // /tmp is world-writable (0o1777 on real systems, 0o777 here)
        let _ = fs.create_dir_all("/tmp");
        let _ = fs.set_mode("/tmp", 0o777);
        // Create home hierarchy as root so intermediate dirs (e.g. /home) are
        // owned by root with 0o755. Switch to the main user only after setup.
        let _ = fs.create_dir_all(&home);
        // Chown the home directory to the main user
        fs.chown(&home, main_uid, main_gid).unwrap_or(());

        // Switch to the main user so that user-provided files are owned by them
        fs.set_current_user(main_uid, main_gid);

        // Resolve files to populate
        let empty_files: BTreeMap<String, FileSpec> = BTreeMap::new();
        let files = if let Some(ref sys) = config.system {
            &sys.files
        } else {
            config.files.as_ref().unwrap_or(&empty_files)
        };

        // Populate user-provided files as the main user so they have correct ownership
        for (file_path, spec) in files {
            let full = if file_path.starts_with('/') {
                file_path.clone()
            } else {
                format!("{}/{}", home, file_path)
            };

            // Ensure parent directories exist
            if let Some(parent) = MemFs::parent(&full) {
                let _ = fs.create_dir_all(parent);
            }

            match spec {
                FileSpec::Content(content) => {
                    let _ = fs.write(&full, content.as_bytes());
                }
                FileSpec::WithMode { content, mode } => {
                    // User provides mode as "octal-looking" decimal (e.g., 755).
                    // Convert: 755 decimal → parse digits as octal → 0o755.
                    let octal = parse_mode_digits(*mode);
                    let _ = fs.write_with_mode(&full, content.as_bytes(), octal);
                }
            };
        }

        let mut env = BTreeMap::new();
        env.insert("HOME".to_string(), home.clone());
        env.insert("USER".to_string(), config.user.clone());
        env.insert("HOSTNAME".to_string(), hostname.clone());
        env.insert("PWD".to_string(), home.clone());
        env.insert("SHELL".to_string(), "/bin/conch".to_string());
        if let Some(ref date) = config.date {
            env.insert("DATE".to_string(), date.clone());
        }

        Shell {
            fs,
            cwd: home.clone(),
            user: config.user.clone(),
            hostname,
            home,
            env,
            last_exit_code: 0,
            users: users_db,
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
        let _ = self.fs.create_dir_all(abs_path);
    }

    /// Build an output entry
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
        let pre_user = self.user.clone();
        let pre_hostname = self.hostname.clone();
        let (output, code, lang) = self.run_line(line);
        OutputEntry {
            user: pre_user,
            hostname: pre_hostname,
            path: display,
            command: line.to_string(),
            output,
            exit_code: code,
            lang,
        }
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
            // Check write permission: existing file needs write perm; new file needs parent dir write perm
            if self.fs.exists(&target) {
                if let Some(e) = self.fs.get(&target) {
                    if !e.is_writable() {
                        return (
                            format!("conch: {}: Permission denied", redir.target),
                            1,
                            None,
                        );
                    }
                }
            } else {
                let parent = target
                    .rsplit_once('/')
                    .map(|(p, _)| if p.is_empty() { "/" } else { p })
                    .unwrap_or("/");
                if self.fs.access(parent, bare_vfs::AccessMode::W_OK).is_err() {
                    return (
                        format!("conch: {}: Permission denied", redir.target),
                        1,
                        None,
                    );
                }
            }
            match redir.typ {
                crate::parser::RedirectType::Overwrite => {
                    let _ = self.fs.write(&target, output.as_bytes());
                }
                crate::parser::RedirectType::Append => {
                    if !self.fs.exists(&target) {
                        let _ = self.fs.write(&target, output.as_bytes());
                    } else {
                        let needs_newline = self.fs.read(&target).is_ok_and(|b| !b.is_empty());
                        if needs_newline {
                            let _ = self.fs.append(&target, b"\n");
                        }
                        let _ = self.fs.append(&target, output.as_bytes());
                    }
                }
            }
            return (String::new(), last_code, None); // redirect suppresses display
        }

        (output, last_code, last_lang)
    }
}

#[cfg(test)]
mod tests;
