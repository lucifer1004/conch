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

fn plain(r: (String, i32)) -> CmdResult {
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
                plain(shell.cmd_echo(args))
            } else {
                plain(shell.cmd_printf(args))
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

        // Text transformation
        "sed" => plain(shell.cmd_sed(args, stdin)),
        "diff" => plain(shell.cmd_diff(args)),
        "xxd" => plain(shell.cmd_xxd(args)),
        "base64" => plain(shell.cmd_base64(args)),

        // File inspection
        "stat" => plain(shell.cmd_stat(args)),
        "test" | "[" => plain(shell.cmd_test(args)),
        "du" => plain(shell.cmd_du(args)),
        "tree" => plain(shell.cmd_tree(args)),

        // Navigation & environment
        "cd" => plain(shell.cmd_cd(args)),
        "pwd" => plain((shell.cwd.clone(), 0)),
        "basename" => plain(shell.cmd_basename(args)),
        "dirname" => plain(shell.cmd_dirname(args)),
        "realpath" => plain(shell.cmd_realpath(args)),
        "whoami" => plain((shell.user.clone(), 0)),
        "hostname" => plain((shell.hostname.clone(), 0)),
        "date" => plain(shell.cmd_date()),
        "which" => plain(shell.cmd_which(args)),
        "type" => plain(shell.cmd_type(args)),
        "env" | "printenv" => plain(shell.cmd_env()),
        "export" => plain(shell.cmd_export(args)),

        // Script execution
        "bash" | "sh" => plain(shell.cmd_bash(args)),

        // User management
        "useradd" | "adduser" => plain(shell.cmd_useradd(args)),
        "groupadd" | "addgroup" => plain(shell.cmd_groupadd(args)),
        "userdel" | "deluser" => plain(shell.cmd_userdel(args)),
        "usermod" => plain(shell.cmd_usermod(args)),
        "su" => plain(shell.cmd_su(args)),
        "sudo" => plain(shell.cmd_sudo(args)),
        "passwd" => plain(shell.cmd_passwd(args)),

        // Builtins
        "clear" => plain(("__CLEAR__".to_string(), 0)),
        "true" => plain((String::new(), 0)),
        "false" => plain((String::new(), 1)),

        _ if cmd.starts_with("./") || cmd.starts_with('/') => plain(shell.cmd_exec(cmd, args)),
        _ => plain((format!("conch: command not found: {}", cmd), 127)),
    }
}

/// List of all known command names (used by `which` and `type`)
pub const BUILTINS: &[&str] = &[
    // Filesystem
    "ls", "cat", "mkdir", "rmdir", "touch", "mktemp", "rm", "cp", "mv", "ln", "readlink", "find",
    "tee", "chmod", "chown", "chgrp", "id", "groups", // Text processing
    "echo", "printf", "head", "tail", "wc", "grep", "sort", "uniq", "cut", "tr", "rev", "seq",
    "tac", "nl", "paste", // Text transformation
    "sed", "diff", "xxd", "base64", // File inspection
    "stat", "test", "[", "du", "tree", // Navigation & environment
    "cd", "pwd", "basename", "dirname", "realpath", "whoami", "hostname", "date", "which", "type",
    "env", "printenv", "export", // Script execution
    "bash", "sh", // User management
    "useradd", "adduser", "groupadd", "addgroup", "userdel", "deluser", "usermod", "su", "sudo",
    "passwd", // Shell builtins
    "clear", "true", "false",
];
