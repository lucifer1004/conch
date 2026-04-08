mod fs;
mod nav;
mod text;

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
        // Filesystem (cat returns lang hint)
        "ls" => plain(shell.cmd_ls(args)),
        "cat" => shell.cmd_cat(args, stdin),
        "mkdir" => plain(shell.cmd_mkdir(args)),
        "touch" => plain(shell.cmd_touch(args)),
        "rm" => plain(shell.cmd_rm(args)),
        "cp" => plain(shell.cmd_cp(args)),
        "mv" => plain(shell.cmd_mv(args)),
        "find" => plain(shell.cmd_find(args)),
        "tee" => plain(shell.cmd_tee(args, stdin)),
        "chmod" => plain(shell.cmd_chmod(args)),
        "basename" => plain(shell.cmd_basename(args)),
        "dirname" => plain(shell.cmd_dirname(args)),

        // Text processing (all support stdin)
        "echo" => plain(shell.cmd_echo(args)),
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

        // Navigation & environment
        "cd" => plain(shell.cmd_cd(args)),
        "pwd" => plain((shell.cwd.clone(), 0)),
        "tree" => plain(shell.cmd_tree(args)),
        "whoami" => plain((shell.user.clone(), 0)),
        "hostname" => plain((shell.hostname.clone(), 0)),
        "date" => plain(shell.cmd_date()),
        "which" => plain(shell.cmd_which(args)),
        "type" => plain(shell.cmd_type(args)),
        "env" | "printenv" => plain(shell.cmd_env()),
        "export" => plain(shell.cmd_export(args)),

        // Script execution
        "bash" | "sh" => plain(shell.cmd_bash(args)),

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
    "echo", "ls", "cat", "cd", "pwd", "mkdir", "touch", "rm", "cp", "mv", "head", "tail", "wc",
    "grep", "sort", "uniq", "cut", "tr", "rev", "seq", "find", "tee", "chmod", "basename",
    "dirname", "tree", "whoami", "hostname", "bash", "sh", "date", "which", "type", "env",
    "printenv", "export", "clear", "true", "false",
];
