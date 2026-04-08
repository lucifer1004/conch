# Contributing & maintainer guide

## Building from source

Requires Rust with the `wasm32-unknown-unknown` target and [just](https://github.com/casey/just):

```bash
rustup target add wasm32-unknown-unknown
just build
```

Unit tests for the shell, parser, and helpers live in the crate sources (`#[cfg(test)]`); run:

```bash
just test
# or: cd wasm && cargo test
```

The build writes `conch.wasm` to the package root. **Ship that file with the package** (it is required at runtime); rebuild and commit it whenever the WASM code changes. Typst loads it from `src/wasm.typ` via `plugin("../conch.wasm")` (path is relative to that file).

## Formatting

`just fmt` runs Just self-format (`just --fmt`), `cargo fmt` in `wasm/`, [typstyle](https://github.com/Enter-tainer/typstyle) on `*.typ`, and [Prettier](https://prettier.io) on `*.md` at the repo root. Options for Markdown live in [`.prettierrc.json`](.prettierrc.json) (e.g. `proseWrap: preserve`). Install `typstyle` and `prettier` on your `PATH` so the `just` recipe `require()` calls succeed.

## Package layout

- **`lib.typ`** — package entrypoint (`typst.toml` → `entrypoint`). Re-exports the public API only.
- **`src/`** — implementation modules (`theme`, `wasm`, `frame`, `ansi`, `session`, `render`, `terminal`). Internal imports are relative within `src/`.
- **`wasm/`** — Rust source for `conch.wasm` (not the published Typst bundle; see `typst.toml` `exclude`).

## Maintainer commands

- `just thumbnail` — regenerate `thumbnail.png` (Typst Universe requires it for template packages).
- `just gif` — compile a Typst animation to GIF (needs `ffmpeg`, just ≥ 1.46 for CLI flags). Defaults to `demo/demo.typ` → `demo/demo.gif`. Options are `--src`, `-o`/`--out`, `-f`/`--fps`, `--frames-dir`, and `--hold-*` (forwards to `typst --input conch_hold_*`; implementation in `src/terminal.typ`; see `just --usage gif`).
  - **Frames directory:** by default PNGs go in a temp dir and are removed when the recipe exits. If you pass **`--frames-dir`**, that path is wiped and recreated before rendering, then **left on disk** afterward (so you can inspect frames). Only temp dirs are deleted automatically.
- `just demos` — compile `demo/*.typ` to PDF (each demo imports `../lib.typ`).
- `typst-package-check check .` — optional lint for `typst.toml` and fenced code in `README.md`. If it warns on snippets that **compile fine with `typst compile`**, treat that as a checker limitation; do not contort documentation to silence it.

## Publishing (Typst Universe)

1. Bump `version` in `typst.toml` (and `@preview/conch:…` strings in `README.md`, `template/main.typ`, and anywhere else — e.g. `rg '@preview/conch'` from the repo root).
2. Run `just build` and `just thumbnail`. Optionally run `typst-package-check` as above; fix real issues, keep examples readable.
3. Add a `repository = "https://github.com/…/conch"` (or `homepage`) entry under `[package]` if the project is public — Universe links it from the package page.
4. Copy the package directory into a fork of [typst/packages](https://github.com/typst/packages) at `packages/preview/conch/<version>/` (path segments must match `name` and `version` in the manifest) and open a PR.

The `exclude` list in `typst.toml` keeps heavy assets (e.g. `demo/`) out of the compiler download bundle; README illustrations remain available on the package’s Universe page.
