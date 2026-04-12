//! Command dispatch and implementations for the virtual shell.
//!
//! Each submodule groups related commands (filesystem, text, navigation, etc.).
//! The top-level [`dispatch`] function routes a command name to its handler.

mod fs;
mod inspect;
mod nav;
mod script;
mod text;
mod transform;
mod user;

use crate::shell::Shell;

/// Result of a command: (output, exit_code, optional_lang_hint)
pub type CmdResult = (String, i32, Option<String>);

/// Line-oriented command output: appends `\n` if non-empty and missing,
/// matching bash where line-producing commands write a trailing newline.
fn plain(r: (String, i32)) -> CmdResult {
    let mut s = r.0;
    if !s.is_empty() && !s.ends_with('\n') {
        s.push('\n');
    }
    (s, r.1, None)
}

/// Raw command output: caller manages newlines (echo, printf, cat).
fn raw(r: (String, i32)) -> CmdResult {
    (r.0, r.1, None)
}

/// Dispatch a command by name to its implementation.
/// `stdin` carries piped input from the previous command in a pipeline.
pub fn dispatch(shell: &mut Shell, cmd: &str, args: &[String], stdin: Option<&str>) -> CmdResult {
    match cmd {
        // Filesystem
        "ls" => plain(shell.cmd_ls(args)),
        "cat" => shell.cmd_cat(args, stdin),
        "mkdir" | "rmdir" => {
            if cmd == "mkdir" {
                plain(shell.cmd_mkdir(args))
            } else {
                plain(shell.cmd_rmdir(args))
            }
        }
        "touch" | "mktemp" => {
            if cmd == "touch" {
                plain(shell.cmd_touch(args))
            } else {
                plain(shell.cmd_mktemp(args))
            }
        }
        "rm" => plain(shell.cmd_rm(args)),
        "cp" | "mv" => {
            if cmd == "cp" {
                plain(shell.cmd_cp(args))
            } else {
                plain(shell.cmd_mv(args))
            }
        }
        "ln" | "readlink" => {
            if cmd == "ln" {
                plain(shell.cmd_ln(args))
            } else {
                plain(shell.cmd_readlink(args))
            }
        }
        "find" => plain(shell.cmd_find(args)),
        "tee" => plain(shell.cmd_tee(args, stdin)),
        "chmod" => plain(shell.cmd_chmod(args)),
        "chown" => plain(shell.cmd_chown(args)),
        "chgrp" => plain(shell.cmd_chgrp(args)),
        "id" => plain(shell.cmd_id(args)),
        "groups" => plain(shell.cmd_groups(args)),

        // Text processing
        "echo" | "printf" => {
            if cmd == "echo" {
                raw(shell.cmd_echo(args))
            } else {
                raw(shell.cmd_printf(args))
            }
        }
        "head" => plain(shell.cmd_head(args, stdin)),
        "tail" => plain(shell.cmd_tail(args, stdin)),
        "wc" => plain(shell.cmd_wc(args, stdin)),
        "grep" => plain(shell.cmd_grep(args, stdin)),
        "sort" => plain(shell.cmd_sort(args, stdin)),
        "uniq" => plain(shell.cmd_uniq(args, stdin)),
        "cut" => plain(shell.cmd_cut(args, stdin)),
        "tr" => plain(shell.cmd_tr(args, stdin)),
        "rev" => plain(shell.cmd_rev(args, stdin)),
        "seq" => plain(shell.cmd_seq(args)),
        "tac" => plain(shell.cmd_tac(args, stdin)),
        "nl" => plain(shell.cmd_nl(args, stdin)),
        "paste" => plain(shell.cmd_paste(args, stdin)),
        "column" => plain(shell.cmd_column(args, stdin)),
        "xargs" => plain(shell.cmd_xargs(args, stdin)),

        // Text transformation
        "sed" => plain(shell.cmd_sed(args, stdin)),
        "diff" => plain(shell.cmd_diff(args)),
        "xxd" => plain(shell.cmd_xxd(args, stdin)),
        "base64" => plain(shell.cmd_base64(args, stdin)),

        // File inspection
        "stat" => plain(shell.cmd_stat(args)),
        "test" | "[" => plain(shell.cmd_test(args)),
        "[[" => plain(shell.cmd_double_bracket(args)),
        "du" => plain(shell.cmd_du(args)),
        "tree" => plain(shell.cmd_tree(args)),

        // Navigation & environment
        "cd" => plain(shell.cmd_cd(args)),
        "pwd" => plain((shell.cwd.to_string(), 0)),
        "basename" => plain(shell.cmd_basename(args)),
        "dirname" => plain(shell.cmd_dirname(args)),
        "realpath" => plain(shell.cmd_realpath(args)),
        "whoami" => plain((shell.ident.user.to_string(), 0)),
        "hostname" => plain((shell.ident.hostname.to_string(), 0)),
        "date" => plain(shell.cmd_date(args)),
        "pushd" => plain(shell.cmd_pushd(args)),
        "popd" => plain(shell.cmd_popd(args)),
        "dirs" => plain(shell.cmd_dirs(args)),
        "which" => plain(shell.cmd_which(args)),
        "type" => plain(shell.cmd_type(args)),
        "env" => plain(shell.cmd_env()),
        "printenv" => plain(shell.cmd_printenv(args)),
        "export" => plain(shell.cmd_export(args)),
        "unset" => plain(shell.cmd_unset(args)),
        "sleep" => plain(shell.cmd_sleep(args)),
        "history" => plain(shell.cmd_history(args)),

        // Script execution
        "bash" | "sh" => plain(shell.cmd_bash(args)),
        "source" | "." => shell.cmd_source(args),

        // User management
        "useradd" | "adduser" => plain(shell.cmd_useradd(args)),
        "groupadd" | "addgroup" => plain(shell.cmd_groupadd(args)),
        "userdel" | "deluser" => plain(shell.cmd_userdel(args)),
        "usermod" => plain(shell.cmd_usermod(args)),
        "su" => plain(shell.cmd_su(args)),
        "sudo" => plain(shell.cmd_sudo(args)),
        "passwd" => plain(shell.cmd_passwd(args)),

        // Process / job control
        "jobs" => plain(shell.cmd_jobs(args)),
        "wait" => plain(shell.cmd_wait(args)),
        "kill" => plain(shell.cmd_kill(args)),
        "ps" => plain(shell.cmd_ps(args)),

        // time/timeout: wrap another command
        "time" => shell.cmd_time(args, stdin),
        "timeout" => plain(shell.cmd_timeout(args, stdin)),

        // umask
        "umask" => plain(shell.cmd_umask(args)),

        // shopt
        "shopt" => plain(shell.cmd_shopt(args)),

        // Builtins
        ":" => plain((String::new(), 0)),
        "let" => {
            let joined = args
                .iter()
                .map(|a| a.as_str())
                .collect::<Vec<_>>()
                .join(" ");
            let result = shell.eval_arith_expr(&joined);
            // bash `let`: exit 0 if result != 0, exit 1 if result == 0
            plain((String::new(), if result != 0 { 0 } else { 1 }))
        }
        "eval" => {
            let joined = args.join(" ");
            shell.run_line(&joined)
        }
        "command" => {
            if args.is_empty() {
                plain((String::new(), 0))
            } else if args[0] == "-V" {
                // command -V: verbose type info (like type but different format)
                if args.len() < 2 {
                    plain((String::new(), 1))
                } else {
                    let mut out = Vec::new();
                    let mut any_failed = false;
                    for name in &args[1..] {
                        if BUILTINS.contains(&name.as_str()) {
                            out.push(format!("{} is a shell builtin", name));
                        } else if shell.defs.has_function(name) {
                            out.push(format!("{} is a function", name));
                        } else if let Some(val) = shell.defs.get_alias(name) {
                            out.push(format!("{} is aliased to `{}'", name, val));
                        } else if let Some(path) = shell.which_path(name) {
                            out.push(format!("{} is {}", name, path));
                        } else {
                            out.push(format!("conch: command: {}: not found", name));
                            any_failed = true;
                        }
                    }
                    plain((out.join("\n"), if any_failed { 1 } else { 0 }))
                }
            } else if args[0] == "-v" {
                // command -v: check builtins, functions, aliases, and PATH
                if args.len() < 2 {
                    plain((String::new(), 1))
                } else {
                    let mut out = Vec::new();
                    let mut any_failed = false;
                    for name in &args[1..] {
                        if BUILTINS.contains(&name.as_str()) || shell.defs.has_function(name) {
                            out.push(name.clone());
                        } else if let Some(val) = shell.defs.get_alias(name) {
                            out.push(format!("alias {}='{}'", name, val));
                        } else if let Some(path) = shell.which_path(name) {
                            out.push(path);
                        } else {
                            any_failed = true;
                        }
                    }
                    plain((out.join("\n"), if any_failed { 1 } else { 0 }))
                }
            } else {
                // command CMD args...: run CMD, skipping function lookup
                let cmd = &args[0];
                let cmd_args = &args[1..];
                // Dispatch directly — skip function lookup by going through dispatch
                // which already checks functions after builtins; we just need to avoid
                // the function branch. Re-dispatch via the match but skip function check.
                dispatch_no_functions(shell, cmd, cmd_args, stdin)
            }
        }
        "exec" => {
            // exec cmd: run cmd and terminate the script (simulate process replacement).
            if args.is_empty() {
                plain((String::new(), 0))
            } else {
                let result = dispatch(shell, &args[0], &args[1..], stdin);
                shell.exec.exec_pending = true; // signal script to stop
                result
            }
        }
        "builtin" => {
            // builtin cmd args: run cmd as a builtin, skipping functions AND PATH
            if args.is_empty() {
                plain((String::new(), 0))
            } else {
                let cmd = &args[0];
                if BUILTINS.contains(&cmd.as_str()) {
                    dispatch(shell, cmd, &args[1..], stdin)
                } else {
                    plain((format!("conch: builtin: {}: not a shell builtin", cmd), 1))
                }
            }
        }
        "set" => plain(shell.cmd_set(args)),
        "alias" => plain(shell.cmd_alias(args)),
        "unalias" => plain(shell.cmd_unalias(args)),
        "readonly" => plain(shell.cmd_readonly(args)),
        "getopts" => plain(shell.cmd_getopts(args)),
        "clear" => plain(("__CLEAR__".to_string(), 0)),
        "true" => plain((String::new(), 0)),
        "false" => plain((String::new(), 1)),
        "shift" => plain(shell.cmd_shift(args)),
        "local" => plain(shell.cmd_local(args, true)),
        "declare" | "typeset" => plain(shell.cmd_local(args, false)),
        "read" => plain(shell.cmd_read(args, stdin)),
        "trap" => plain(shell.cmd_trap(args)),
        "mapfile" | "readarray" => plain(shell.cmd_mapfile(args, stdin)),

        _ if cmd.starts_with("./") || cmd.starts_with('/') => plain(shell.cmd_exec(cmd, args)),
        // Try user-defined function
        _ if shell.defs.has_function(cmd) => plain(shell.call_function(cmd, args)),
        // Try PATH lookup
        _ if shell.is_in_path(cmd) => plain(shell.exec_from_path(cmd, args)),
        _ => plain((format!("conch: command not found: {}", cmd), 127)),
    }
}

/// Dispatch a command by name, skipping function lookup (used by `command` builtin).
pub fn dispatch_no_functions(
    shell: &mut Shell,
    cmd: &str,
    args: &[String],
    stdin: Option<&str>,
) -> CmdResult {
    match cmd {
        // Try the regular dispatch first for all builtins and external commands
        _ if cmd == "ls"
            || cmd == "cat"
            || cmd == "mkdir"
            || cmd == "rmdir"
            || cmd == "touch"
            || cmd == "mktemp"
            || cmd == "rm"
            || cmd == "cp"
            || cmd == "mv"
            || cmd == "ln"
            || cmd == "readlink"
            || cmd == "find"
            || cmd == "tee"
            || cmd == "chmod"
            || cmd == "chown"
            || cmd == "chgrp"
            || cmd == "id"
            || cmd == "groups"
            || cmd == "echo"
            || cmd == "printf"
            || cmd == "head"
            || cmd == "tail"
            || cmd == "wc"
            || cmd == "grep"
            || cmd == "sort"
            || cmd == "uniq"
            || cmd == "cut"
            || cmd == "tr"
            || cmd == "rev"
            || cmd == "seq"
            || cmd == "tac"
            || cmd == "nl"
            || cmd == "paste"
            || cmd == "column"
            || cmd == "xargs"
            || cmd == "sed"
            || cmd == "diff"
            || cmd == "xxd"
            || cmd == "base64"
            || cmd == "stat"
            || cmd == "test"
            || cmd == "["
            || cmd == "[["
            || cmd == "du"
            || cmd == "tree"
            || cmd == "cd"
            || cmd == "pwd"
            || cmd == "basename"
            || cmd == "dirname"
            || cmd == "realpath"
            || cmd == "whoami"
            || cmd == "hostname"
            || cmd == "date"
            || cmd == "which"
            || cmd == "type"
            || cmd == "env"
            || cmd == "printenv"
            || cmd == "export"
            || cmd == "unset"
            || cmd == "sleep"
            || cmd == "history"
            || cmd == "pushd"
            || cmd == "popd"
            || cmd == "dirs"
            || cmd == "bash"
            || cmd == "sh"
            || cmd == "source"
            || cmd == "."
            || cmd == "useradd"
            || cmd == "adduser"
            || cmd == "groupadd"
            || cmd == "addgroup"
            || cmd == "userdel"
            || cmd == "deluser"
            || cmd == "usermod"
            || cmd == "su"
            || cmd == "sudo"
            || cmd == "passwd"
            || cmd == "clear"
            || cmd == "true"
            || cmd == "false"
            || cmd == "shift"
            || cmd == "local"
            || cmd == "declare"
            || cmd == "typeset"
            || cmd == "read"
            || cmd == ":"
            || cmd == "let"
            || cmd == "eval"
            || cmd == "command"
            || cmd == "set"
            || cmd == "alias"
            || cmd == "unalias"
            || cmd == "readonly"
            || cmd == "getopts"
            || cmd == "trap"
            || cmd == "mapfile"
            || cmd == "readarray"
            || cmd == "jobs"
            || cmd == "wait"
            || cmd == "kill"
            || cmd == "ps"
            || cmd == "time"
            || cmd == "timeout"
            || cmd == "umask"
            || cmd == "shopt" =>
        {
            dispatch(shell, cmd, args, stdin)
        }
        // Try PATH lookup (skip functions)
        _ if cmd.starts_with("./") || cmd.starts_with('/') => plain(shell.cmd_exec(cmd, args)),
        _ if shell.is_in_path(cmd) => plain(shell.exec_from_path(cmd, args)),
        _ => plain((format!("conch: command not found: {}", cmd), 127)),
    }
}

/// List of all known command names (used by `which` and `type`)
pub const BUILTINS: &[&str] = &[
    // Filesystem
    "ls",
    "cat",
    "mkdir",
    "rmdir",
    "touch",
    "mktemp",
    "rm",
    "cp",
    "mv",
    "ln",
    "readlink",
    "find",
    "tee",
    "chmod",
    "chown",
    "chgrp",
    "id",
    "groups", // Text processing
    "echo",
    "printf",
    "head",
    "tail",
    "wc",
    "grep",
    "sort",
    "uniq",
    "cut",
    "tr",
    "rev",
    "seq",
    "tac",
    "nl",
    "paste",
    "column",
    "xargs", // Text transformation
    "sed",
    "diff",
    "xxd",
    "base64", // File inspection
    "stat",
    "test",
    "[",
    "[[",
    "du",
    "tree", // Navigation & environment
    "cd",
    "pwd",
    "basename",
    "dirname",
    "realpath",
    "whoami",
    "hostname",
    "date",
    "which",
    "type",
    "env",
    "printenv",
    "export",
    "unset",
    "sleep",
    "history",
    "pushd",
    "popd",
    "dirs", // Script execution
    "bash",
    "sh",
    "source",
    ".", // User management
    "useradd",
    "adduser",
    "groupadd",
    "addgroup",
    "userdel",
    "deluser",
    "usermod",
    "su",
    "sudo",
    "passwd", // Process / job control
    "jobs",
    "wait",
    "kill",
    "ps",
    "time",
    "timeout",
    "umask",
    "shopt", // Shell builtins
    "clear",
    "true",
    "false",
    "shift",
    "local",
    "declare",
    "typeset",
    "read",
    ":",
    "let",
    "eval",
    "command",
    "exec",
    "builtin",
    "set",
    "alias",
    "unalias",
    "readonly",
    "getopts",
    "trap",
    "mapfile",
    "readarray",
];
