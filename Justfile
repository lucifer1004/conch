# ---------------------------------------------------------------------------- #
#                                 DEPENDENCIES                                 #
# ---------------------------------------------------------------------------- #
# Cargo: https://rustup.rs
# Typst: https://typst.app
# typstyle: https://github.com/Enter-tainer/typstyle (used by `just fmt`)
# Prettier: https://prettier.io (Markdown in `just fmt`)

set shell := ["bash", "-euo", "pipefail", "-c"]

cargo := require("cargo")
typst := require("typst")
typstyle := require("typstyle")
prettier := require("prettier")
ffmpeg := require("ffmpeg")

# ---------------------------------------------------------------------------- #
#                                  CONSTANTS                                   #
# ---------------------------------------------------------------------------- #

wasm-target := "wasm32-unknown-unknown"

# Local `@preview/conch:…` tree before the package is on Universe.

vendor-root := "_vendor"

# ---------------------------------------------------------------------------- #
#                                  RECIPES                                     #
# ---------------------------------------------------------------------------- #

[doc("List all recipes (same as `just --list`).")]
[group("meta")]
default:
    @just --list

[group("build")]
build:
    cd wasm && {{ cargo }} build -p conch-wasm --release --target {{ wasm-target }}
    cp wasm/target/{{ wasm-target }}/release/conch.wasm conch.wasm

[group("build")]
clean:
    cd wasm && {{ cargo }} clean
    rm -f conch.wasm

[group("checks")]
fmt:
    just --fmt --unstable
    cd wasm && {{ cargo }} fmt
    {{ typstyle }} -i .
    {{ prettier }} --write **/*.md

[doc("Run Rust unit tests in wasm/ (shell, parser, ansi).")]
[group("checks")]
test:
    cd wasm && {{ cargo }} test --workspace --all-features

# Symlink this repo as a local preview package (…/preview/conch/0.1.0).
[group("build")]
[private]
_vendor-link:
    mkdir -p "{{ vendor-root }}/preview/conch"
    ln -sfn "$(pwd)" "{{ vendor-root }}/preview/conch/0.1.0"

[group("docs")]
example: build _vendor-link
    {{ typst }} compile --package-path {{ vendor-root }} --root . template/main.typ template/main.pdf

[group("docs")]
watch: build _vendor-link
    {{ typst }} watch --package-path {{ vendor-root }} --root . template/main.typ template/main.pdf

# Universe: thumbnail.png at package root; long edge ≥ 1080 px (typst/packages manifest).
[group("docs")]
thumbnail: build _vendor-link
    {{ typst }} compile --package-path {{ vendor-root }} --root . -f png --pages 1 --ppi 250 template/main.typ thumbnail.png

# PNG frames → palette GIF. Run from repo root; `src` is a Typst file (e.g. `#import "lib.typ"` or `@preview`).

[arg("fps", long="fps", short="f", help="Frames per second in the GIF mux")]
[arg("out", long="out", short="o", help="Output .gif (default: <src> with .gif suffix)")]
[arg("frames_dir", long="frames-dir", help="PNG frame dir; default is a temp directory")]
[arg("src", long="src", help="Typst source (.typ), path from repo root")]
[arg("hold_after_final", long="hold-after-final", help="typst --input conch_hold_after_final=…")]
[arg("hold_after_frame", long="hold-after-frame", help="typst --input conch_hold_after_frame=… (per-line anim)")]
[arg("hold_after_output", long="hold-after-output", help="typst --input conch_hold_after_output=…")]
[arg("hold_final_blink_hold", long="hold-final-blink-hold", help="typst --input conch_hold_final_blink_hold=…")]
[arg("hold_final_cursor_blink", long="hold-final-cursor-blink", help="typst --input conch_hold_final_cursor_blink=…")]
[doc("GIF from Typst. All options are flags (see `just --usage gif`). `hold_*` forwards to `typst --input conch_hold_*`.")]
[group("docs")]
gif src="demo/demo.typ" out="" frames_dir="" fps="10" hold_after_output="" hold_after_final="" hold_final_cursor_blink="" hold_final_blink_hold="" hold_after_frame="": build
    #!/usr/bin/env bash
    set -euo pipefail
    src="{{ src }}"
    out="{{ out }}"
    frames_dir_arg="{{ frames_dir }}"
    fps="{{ fps }}"
    hold_after_output="{{ hold_after_output }}"
    hold_after_final="{{ hold_after_final }}"
    hold_final_cursor_blink="{{ hold_final_cursor_blink }}"
    hold_final_blink_hold="{{ hold_final_blink_hold }}"
    hold_after_frame="{{ hold_after_frame }}"
    # One argv array (bash 3.2 + `set -u` cannot expand an empty `typst_inputs[@]`).
    typst_cmd=(compile --root .)
    [[ -n "${hold_after_output}" ]] && typst_cmd+=(--input "conch_hold_after_output=${hold_after_output}")
    [[ -n "${hold_after_final}" ]] && typst_cmd+=(--input "conch_hold_after_final=${hold_after_final}")
    [[ -n "${hold_final_cursor_blink}" ]] && typst_cmd+=(--input "conch_hold_final_cursor_blink=${hold_final_cursor_blink}")
    [[ -n "${hold_final_blink_hold}" ]] && typst_cmd+=(--input "conch_hold_final_blink_hold=${hold_final_blink_hold}")
    [[ -n "${hold_after_frame}" ]] && typst_cmd+=(--input "conch_hold_after_frame=${hold_after_frame}")
    if [[ -z "${src}" ]]; then
      echo "error: empty src" >&2
      exit 1
    fi
    if [[ ! -f "${src}" ]]; then
      echo "error: not a file: ${src}" >&2
      exit 1
    fi
    if [[ -z "${out}" ]]; then
      out="${src%.typ}.gif"
    fi
    if [[ -z "${frames_dir_arg}" ]]; then
      frames_dir=$(mktemp -d "${TMPDIR:-/tmp}/conch-gif.XXXXXX")
      frames_dir_is_temp=1
    else
      frames_dir="${frames_dir_arg}"
      rm -rf "${frames_dir}"
      mkdir -p "${frames_dir}"
      frames_dir_is_temp=0
    fi
    list=$(mktemp)
    cleanup() {
      rm -f "${list}"
      if [[ "${frames_dir_is_temp:-0}" -eq 1 ]]; then
        rm -rf "${frames_dir}"
      fi
    }
    trap cleanup EXIT
    typst_cmd+=("${src}" "${frames_dir}/f-{0p}.png")
    {{ typst }} "${typst_cmd[@]}"
    root="$(pwd)"
    shopt -s nullglob
    frames=("${frames_dir}"/f-*.png)
    IFS=$'\n'
    sorted=($(printf '%s\n' "${frames[@]}" | LC_ALL=C sort -V))
    unset IFS
    if ((${#sorted[@]} == 0)); then
      echo "error: no PNG frames in ${frames_dir}/" >&2
      exit 1
    fi
    n=${#sorted[@]}
    mkdir -p "$(dirname "${out}")"
    vf_multi="fps=${fps},split[s0][s1];[s0]palettegen[p];[s1][p]paletteuse"
    # `fps` before palette drops to zero frames when n==1; single-page docs need a separate chain.
    vf_single="split[s0][s1];[s0]palettegen[p];[s1][p]paletteuse"
    if ((n == 1)); then
      in0="${sorted[0]}"
      case "${in0}" in
        /*) ;;
        *) in0="${root}/${in0}" ;;
      esac
      {{ ffmpeg }} -y -i "${in0}" -vf "${vf_single}" -loop 0 "${out}"
    else
      frame_s=$(awk -v fps="${fps}" 'BEGIN { printf "%.17g", 1 / fps }')
      echo "ffconcat version 1.0" >> "${list}"
      i=0
      for f in "${sorted[@]}"; do
        i=$((i + 1))
        case "${f}" in
          /*) printf "file '%s'\n" "${f}" >> "${list}" ;;
          *) printf "file '%s'\n" "${root}/${f}" >> "${list}" ;;
        esac
        if ((i < n)); then
          echo "duration ${frame_s}" >> "${list}"
        fi
      done
      {{ ffmpeg }} -y -f concat -safe 0 -i "${list}" \
        -vf "${vf_multi}" \
        -loop 0 "${out}"
    fi
    printf 'wrote %s (%d frames @ %s fps)\n' "${out}" "${n}" "${fps}"

# PNG screenshots + special cases. `{0p}` must stay literal (just would treat `{p}` as a variable).
[group("build")]
demos: build
    for f in demo/*.typ; do \
      [[ "$f" == demo/demo.typ ]] && continue; \
      if [[ "$f" == demo/touying.typ ]]; then \
        just gif --src "$f" -o "${f%.typ}.gif" -f 1; \
      elif [[ "$f" == demo/paginate.typ ]]; then \
        {{ typst }} compile --root . -f png "$f" "${f%.typ}-{0p}.png"; \
      else \
        {{ typst }} compile --root . -f png --pages 1 "$f" "${f%.typ}.png"; \
      fi; \
    done

# Copy package files to a directory for Typst Universe submission.

# Usage: just package /path/to/typst-packages/packages/preview/conch/0.1.0
[arg("dest", help="Target directory (e.g. ../typst-packages/packages/preview/conch/0.1.0)")]
[group("build")]
package dest: build
    #!/usr/bin/env bash
    set -euo pipefail
    dest="{{ dest }}"
    rm -rf "$dest"
    mkdir -p "$dest/src" "$dest/template" "$dest/demo"
    # Core package files
    cp typst.toml lib.typ conch.wasm LICENSE README.md CONTRIBUTING_GUIDE.md thumbnail.png "$dest/"
    cp src/*.typ "$dest/src/"
    cp template/main.typ "$dest/template/"
    # README images (committed to repo but excluded from download bundle via typst.toml)
    cp demo/demo.gif demo/touying.gif demo/frame.png demo/shell.png demo/pipes.png \
       demo/permissions.png demo/script.png demo/themes.png demo/chrome.png \
       demo/paginate-1.png demo/paginate-2.png "$dest/demo/"
    echo "Packaged to $dest"
    echo "Files:"
    find "$dest" -type f | sort | while read -r f; do
      printf "  %s (%s)\n" "${f#$dest/}" "$(du -h "$f" | cut -f1 | tr -d ' ')"
    done

alias f := fmt
