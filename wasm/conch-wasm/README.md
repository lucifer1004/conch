# conch-wasm

The Rust WASM backend for [conch](../../README.md) — a shell simulator that renders interactive terminal sessions in Typst.

conch-wasm implements a virtual Unix shell with 70+ builtin commands, backed by [bare-vfs](../bare-vfs/) for the in-memory filesystem. It compiles to `wasm32-unknown-unknown` and is loaded as a Typst WASM plugin.

## Architecture

```
Typst document
  └── conch WASM plugin (this crate)
        ├── Shell       — command dispatch, pipes, redirects, variable expansion
        ├── Commands    — 70+ Unix command implementations
        ├── Parser      — pipeline parsing (;, &&, ||, |, >, >>)
        ├── UserDb      — in-memory user/group database
        └── bare-vfs    — inode-based virtual filesystem
```

## Builtin Commands

### Filesystem

`ls`, `cat`, `mkdir`, `touch`, `rm`, `cp` (`-r`), `mv`, `ln` (`-s`, hard links), `find`, `tee`, `chmod` (octal + symbolic), `chown`, `chgrp`, `rmdir`, `mktemp`, `readlink` (`-f`)

### Text Processing

`echo` (`-n`, `-e`), `printf`, `grep` (`-i`, `-n`, `-c`, `-v`), `head`, `tail`, `sort` (`-r`, `-n`), `uniq` (`-c`), `wc` (`-l`, `-w`, `-c`), `cut` (`-d`, `-f`), `tr` (`-d`), `rev`, `seq`, `tac`, `nl`, `paste` (`-d`)

### Inspection

`stat`, `test`/`[` (with `!` negation), `du` (`-h`, `-s`), `tree`

### Transform

`sed` (`-i`, `s///`, `s///g`), `diff`, `base64` (`-d`), `xxd`

### Navigation & Environment

`cd`, `pwd`, `env`, `printenv`, `export`, `unset`, `which`, `type`, `hostname`, `whoami`, `date`, `basename`, `dirname`, `realpath`, `sleep`

### Scripting

`bash`, `sh`, `source`/`.`, `exec`, `./script.sh`

### User Management

`useradd`, `userdel` (`-r`), `usermod` (`-aG`), `groupadd`, `su` (`-`), `sudo`, `passwd`, `id`, `groups`

## Shell Features

- **Pipes**: `cmd1 | cmd2 | cmd3`
- **Chaining**: `cmd1 && cmd2`, `cmd1 || cmd2`, `cmd1; cmd2`
- **Redirects**: `>` (overwrite), `>>` (append)
- **Variable expansion**: `$VAR`, `$HOME`, `$USER`, `$SHELL`
- **Tilde expansion**: `~/file` expands to home directory
- **Glob expansion**: `*.txt`, `src/*.rs`
- **Permission model**: Unix uid/gid with owner/group/other rwx enforcement
- **Syntax highlighting**: language detection for `cat` output

## Permission Model

The shell enforces Unix-style permissions:

- File read/write/execute checks on all operations
- Directory execute (search) permission for path traversal
- Directory read permission for `ls`/`readdir`
- Parent directory write permission for creating/deleting entries
- `chmod` requires file owner or root
- `chown` (uid change) requires root; owner can change gid to own groups
- `su`/`sudo` switch user context (no auth in WASM simulation)

## Building

```sh
cargo build --release --target wasm32-unknown-unknown
```

Or via the workspace justfile:

```sh
just build
```

## Testing

```sh
cargo test              # without bare-vfs std features
cargo test --all-features  # with serde, std
```

## Known Limitations

- `grep` and `sed` use literal string matching, not regex
- `diff` uses a naive line-by-line algorithm, not LCS
- `tr` does not support character classes (`[:upper:]`) or ranges (`a-z`)
- `su`/`sudo` have no authentication — any user can escalate
- No job control (`bg`, `fg`, `jobs`, `kill`)
- No command history or arrow-key navigation
- No heredocs, subshells, or `$()` command substitution
- `sleep` is a no-op (WASM cannot block)
