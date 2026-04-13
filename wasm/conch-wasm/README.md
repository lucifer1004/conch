# conch-wasm

The Rust WASM backend for [conch](../../README.md) — a shell simulator that renders interactive terminal sessions in Typst.

conch-wasm implements a virtual Unix shell with 90+ builtin commands, backed by [bare-vfs](../bare-vfs/) for the in-memory filesystem. It compiles to `wasm32-unknown-unknown` and is loaded as a Typst WASM plugin.

## Architecture

```
Typst document
  └── conch WASM plugin (this crate)
        ├── Shell       — command dispatch, pipes, redirects, variable expansion
        ├── Commands    — 90+ Unix command implementations
        ├── Parser      — pipeline parsing (;, &&, ||, |, >, >>)
        ├── UserDb      — in-memory user/group database
        └── bare-vfs    — inode-based virtual filesystem
```

## Builtin Commands

### Filesystem

`ls` (`-l`, `-a`, `-R`, `-1`, `-h`, `-t`), `cat` (`-`, stdin), `mkdir`, `touch`, `rm`, `cp` (`-r`, `-n`, `-p`), `mv` (multi-source), `ln` (`-s`, `-f`, hard links), `find` (`-name`, `-type`, `-maxdepth`, `-exec`, `-iname`, `-delete`, `-path`), `tee`, `chmod` (octal + symbolic, `-R`), `chown`, `chgrp`, `rmdir`, `mktemp`, `readlink` (`-f`)

### Text Processing

`echo` (`-n`, `-e`), `printf` (width, precision, float), `grep` (`-i`, `-n`, `-c`, `-v`, `-E`, `-q`, `-w`, `-l`, `-o`, `-A`/`-B`/`-C`), `head`, `tail` (`+N`), `sort` (`-r`, `-n`, `-u`, `-k`, `-t`), `uniq` (`-c`, `-d`, `-u`), `wc` (`-l`, `-w`, `-c`, multi-file), `cut` (`-d`, `-f`, `-c`), `tr` (`-d`, `-s`, ranges, POSIX classes), `rev`, `seq` (`-s`, `-w`, floats), `tac`, `nl`, `paste` (`-d`, `-s`), `column`, `xargs`

### Inspection

`stat` (`-c FORMAT`), `test`/`[` (with `!` negation, `-a`/`-o`, `-L`/`-h`, `-nt`/`-ot`), `[[ ]]` (extended test, `-L`/`-h`), `du` (`-h`, `-s`, `-d`/`--max-depth`, `-c`), `tree` (`-L`, `-a`, summary line)

### Transform

`sed` (`-i`, `-n`, `-E`, `s///`, `s///g`, `s///p`, `d`, `a\`/`i\`/`c\`, ranges, alternate delimiters, backreferences), `diff` (LCS algorithm, `-u`, `-q`), `base64` (`-d`, stdin), `xxd` (stdin)

### Navigation & Environment

`cd` (`cd -`), `pwd`, `pushd`, `popd`, `dirs`, `env`, `printenv` (single-variable), `export` (no-arg listing), `unset`, `which`, `type` (`-t`, alias detection), `hostname`, `whoami`, `date` (`+FORMAT`), `basename`, `dirname`, `realpath`, `sleep` (suffix support), `time`, `timeout`

### Scripting

`bash`, `sh`, `source`/`.`, `exec`, `./script.sh`

### User Management

`useradd`, `userdel` (`-r`), `usermod` (`-aG`), `groupadd`, `su` (`-`, `-c COMMAND`), `sudo` (`-u USER`), `passwd`, `id`, `groups`

### Builtins

`set` (no-arg listing, `-e`/`-u`/`-x`/`-f`/`-C`, `-o pipefail`), `shopt` (shell options), `declare`/`local` (`-p`, `-i`, `-x`, `-r`, `-f`, `-F`, `-a`, `-A`, `-n`), `read` (`-r`, `-p`, `-a`, `-d`, `-n`), `trap` (EXIT, ERR, INT, TERM, HUP, DEBUG, RETURN, `-p`), `alias`, `unalias`, `readonly`, `unset` (`-f`), `export`, `command` (`-v`, `-V`), `type` (`-t`), `shift`, `getopts`, `mapfile`/`readarray`, `wait` (`-n`), `jobs`, `kill` (`-l`), `bash`, `sh`, `source`/`.`

## Shell Features

- **Pipes**: `cmd1 | cmd2 | cmd3`
- **Chaining**: `cmd1 && cmd2`, `cmd1 || cmd2`, `cmd1; cmd2`
- **Background execution**: `cmd &`, `jobs`, `wait`, `kill`
- **Redirects**: `>` (overwrite), `>>` (append)
- **Heredocs**: `<<EOF`, `<<-EOF`, `<<<`
- **Command substitution**: `$(cmd)`, `` `cmd` ``
- **Subshells**: `(cmd)`
- **Process substitution**: `<(cmd)`, `>(cmd)`
- **Variable expansion**: `$VAR`, `$HOME`, `$USER`, `$SHELL`
- **Tilde expansion**: `~/file` expands to home directory
- **Glob expansion**: `*.txt`, `src/*.rs`
- **Extended globs**: `?(pat)`, `*(pat)`, `+(pat)`, `@(pat)`, `!(pat)`
- **Brace expansion**: `{a,b,c}`, `{1..10}`
- **Arithmetic expansion**: `$((expr))`, `(( ))` command
- **C-style for loops**: `for((i=0;i<10;i++))`
- **Case statements**: `case ... esac`
- **Functions**: user-defined functions with `local`, `return`, positional params
- **Arrays**: indexed arrays, associative arrays
- **[[]] extended test**: regex matching `=~`, `BASH_REMATCH`
- **Traps**: EXIT, ERR, INT, TERM, HUP, DEBUG, RETURN; `-p` display
- **Namerefs**: `declare -n`
- **Permission model**: Unix uid/gid with owner/group/other rwx enforcement
- **Syntax highlighting**: language detection for `cat` output
- **Command history**: `history` builtin; Up/Down arrow navigation in per-char animations

## Plugin System

conch supports two types of plugins:

**Typst Function Plugins** — Define custom commands as Typst functions without compilation. Plugins receive `(args, stdin, files)` and return `(stdout, exit-code)`.

**WASM Plugins** — Compile commands to WebAssembly (via wasmi) and load them alongside built-in commands. WASM plugins follow the `wasm-minimal-protocol`, exchanging JSON input/output:

- Input: `{"args": [...], "stdin": "...", "files": {"name": "content"}}`
- Output: `{"stdout": "...", "exit-code": 0}`

WASM plugins work seamlessly in pipelines, redirects, and command chains. See `wasm/demo-plugin/` for a complete example.

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

- `su`/`sudo` have no authentication — any user can escalate
- No real process forking — background jobs are simulated
- No signal delivery — traps store handlers but INT/TERM don't fire
- No `fg`/`bg` builtins
- `sed` hold space, multi-line patterns, and labels not supported
- `sleep` advances VFS time but cannot block real time
