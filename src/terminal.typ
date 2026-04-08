#import "theme.typ": _resolve-font, _resolve-theme
#import "session.typ": _execute-session, _parse-commands, _process-keyline
#import "render.typ": _render-frame

// --- Public shell functions ---

// Defaults for `hold` on animation entrypoints; callers may pass a partial dict (`defaults + hold`).
#let _hold-per-line-defaults = (after-frame: 0)
#let _hold-per-char-defaults = (
  after-output: 0,
  after-final: 0,
  final-cursor-blink: false,
  final-blink-hold: 2,
)

// `typst compile --input conch_hold_*=…` overrides `hold:` in source (last wins). Key names are stable for scripts / `just gif`.
#let _parse-bool-input(s) = {
  if s == "0" or s == "false" or s == "no" or s == "off" { return false }
  if s == "1" or s == "true" or s == "yes" or s == "on" { return true }
  false
}

#let _hold-input-patch-char() = {
  let o = (:)
  let a = sys.inputs.at("conch_hold_after_output", default: none)
  if a != none { o.insert("after-output", int(a)) }
  let b = sys.inputs.at("conch_hold_after_final", default: none)
  if b != none { o.insert("after-final", int(b)) }
  let c = sys.inputs.at("conch_hold_final_cursor_blink", default: none)
  if c != none { o.insert("final-cursor-blink", _parse-bool-input(c)) }
  let d = sys.inputs.at("conch_hold_final_blink_hold", default: none)
  if d != none { o.insert("final-blink-hold", int(d)) }
  o
}

#let _hold-input-patch-line() = {
  let o = (:)
  let a = sys.inputs.at("conch_hold_after_frame", default: none)
  if a != none { o.insert("after-frame", int(a)) }
  o
}

/// Render a terminal session as an embeddable block — no page settings,
/// composable with surrounding content.
///
/// ````typst
/// = Build Log
///
/// #terminal-block(user: "ci", files: (...))[```
/// cargo build --release
/// cargo test
/// ```]
/// ````
#let terminal-block(
  body,
  user: "user",
  hostname: "conch",
  theme: "dracula",
  font: auto,
  width: auto,
  height: auto,
  files: (:),
  show-cursor: true,
  overflow: "clip",
) = {
  let commands = _parse-commands(body)
  let session = _execute-session(user, hostname, files, commands)
  let t = _resolve-theme(theme)
  let f = _resolve-font(font)
  let term-width = if width == auto { 560pt } else { width }
  let term-height = if height == auto { auto } else { height }
  _render-frame(
    session,
    user,
    hostname,
    t,
    f,
    term-width,
    show-cursor: show-cursor,
    term-height: term-height,
    overflow: overflow,
  )
}

/// Render a full terminal session as a standalone page.
/// Intended as a show rule: `#show: terminal.with(...)`.
/// Sets page dimensions automatically.
#let terminal(
  body,
  user: "user",
  hostname: "conch",
  theme: "dracula",
  font: auto,
  width: auto,
  height: auto,
  files: (:),
  show-cursor: true,
  overflow: "clip",
) = {
  set page(height: auto, width: auto, margin: 0.5in)
  terminal-block(
    body,
    user: user,
    hostname: hostname,
    theme: theme,
    font: font,
    width: width,
    height: height,
    files: files,
    show-cursor: show-cursor,
    overflow: overflow,
  )
}

/// Per-line animation: one frame per command execution.
#let terminal-per-line(
  body,
  user: "user",
  hostname: "conch",
  theme: "dracula",
  font: auto,
  width: auto,
  height: auto,
  files: (:),
  overflow: "clip",
  /// Extra duplicate pages per animation step (PNG/GIF/video frame pacing). Pass a partial dict; merges with defaults, then `sys.inputs` (`conch_hold_after_frame`, …).
  hold: (:),
) = {
  set page(height: auto, width: auto, margin: 0.5in)
  let h = _hold-per-line-defaults + hold + _hold-input-patch-line()
  let commands = _parse-commands(body)
  let t = _resolve-theme(theme)
  let f = _resolve-font(font)
  let term-width = if width == auto { 560pt } else { width }
  let term-height = if height == auto { auto } else { height }
  let run-cmds = commands.slice(0, commands.len() - 1)
  let last-cmd = commands.last()

  for i in range(run-cmds.len() + 1) {
    let executed = run-cmds.slice(0, i)
    let session = _execute-session(user, hostname, files, executed)
    let typing = if i < run-cmds.len() { run-cmds.at(i) } else { last-cmd }
    _render-frame(
      session,
      user,
      hostname,
      t,
      f,
      term-width,
      typing: typing,
      term-height: term-height,
      overflow: overflow,
    )
    for _ in range(h.after-frame) {
      pagebreak()
      _render-frame(
        session,
        user,
        hostname,
        t,
        f,
        term-width,
        typing: typing,
        term-height: term-height,
        overflow: overflow,
      )
    }
    if i < run-cmds.len() { pagebreak() }
  }
}

/// Per-char animation: typing effect, one frame per keystroke.
#let terminal-per-char(
  body,
  user: "user",
  hostname: "conch",
  theme: "dracula",
  font: auto,
  width: auto,
  height: auto,
  files: (:),
  overflow: "clip",
  /// Frame pacing for sequence/GIF export. Pass a partial dict; merges with defaults, then `sys.inputs` (`conch_hold_after_output`, …) so CLI can override.
  hold: (:),
) = {
  set page(height: auto, width: auto, margin: 0.5in)
  let h = _hold-per-char-defaults + hold + _hold-input-patch-char()
  let commands = _parse-commands(body)
  let t = _resolve-theme(theme)
  let f = _resolve-font(font)
  let term-width = if width == auto { 560pt } else { width }
  let term-height = if height == auto { auto } else { height }
  let run-cmds = commands.slice(0, commands.len() - 1)
  let last-cmd = commands.last()
  let first-frame = true

  // Pre-process: resolve key events for lines with \x escapes
  let cmd-data = run-cmds.map(cmd => {
    if cmd.contains("\\x") {
      let states = _process-keyline(cmd)
      let final-text = if states.len() > 0 { states.last().text } else { "" }
      (final: final-text, states: states)
    } else {
      (final: cmd, states: none)
    }
  })
  let exec-cmds = cmd-data.map(c => c.final)

  let last-data = if last-cmd.contains("\\x") {
    let states = _process-keyline(last-cmd)
    let final-text = if states.len() > 0 { states.last().text } else { "" }
    (final: final-text, states: states)
  } else {
    (final: last-cmd, states: none)
  }

  for i in range(cmd-data.len()) {
    let info = cmd-data.at(i)
    let executed = exec-cmds.slice(0, i)
    let session = _execute-session(user, hostname, files, executed)

    if info.states != none {
      // Keyline path: animate each buffer state with cursor position
      for state in info.states {
        if not first-frame { pagebreak() }
        first-frame = false
        _render-frame(
          session,
          user,
          hostname,
          t,
          f,
          term-width,
          typing: state.text,
          cursor-pos: state.cursor,
          term-height: term-height,
          overflow: overflow,
        )
      }
    } else {
      // Classic path: animate character by character
      let cmd-chars = info.final.clusters()
      for j in range(cmd-chars.len() + 1) {
        if not first-frame { pagebreak() }
        first-frame = false
        let partial = cmd-chars.slice(0, j).join()
        _render-frame(
          session,
          user,
          hostname,
          t,
          f,
          term-width,
          typing: partial,
          term-height: term-height,
          overflow: overflow,
        )
      }
    }

    pagebreak()
    let session-after = _execute-session(user, hostname, files, exec-cmds.slice(
      0,
      i + 1,
    ))
    _render-frame(
      session-after,
      user,
      hostname,
      t,
      f,
      term-width,
      typing: "",
      term-height: term-height,
      overflow: overflow,
    )
    for _ in range(h.after-output) {
      pagebreak()
      _render-frame(
        session-after,
        user,
        hostname,
        t,
        f,
        term-width,
        typing: "",
        term-height: term-height,
        overflow: overflow,
      )
    }
  }

  {
    let session-final = _execute-session(user, hostname, files, exec-cmds)

    if last-data.states != none {
      for state in last-data.states {
        pagebreak()
        _render-frame(
          session-final,
          user,
          hostname,
          t,
          f,
          term-width,
          typing: state.text,
          cursor-pos: state.cursor,
          term-height: term-height,
          overflow: overflow,
        )
      }
    } else {
      let cmd-chars = last-cmd.clusters()
      for j in range(cmd-chars.len() + 1) {
        pagebreak()
        let partial = cmd-chars.slice(0, j).join()
        _render-frame(
          session-final,
          user,
          hostname,
          t,
          f,
          term-width,
          typing: partial,
          term-height: term-height,
          overflow: overflow,
        )
      }
    }

    for k in range(h.after-final) {
      pagebreak()
      let show-cursor = if h.final-cursor-blink {
        let half-period = calc.max(1, h.final-blink-hold)
        let phase = calc.floor(k / half-period)
        calc.rem(phase, 2) == 0
      } else {
        true
      }
      _render-frame(
        session-final,
        user,
        hostname,
        t,
        f,
        term-width,
        typing: last-data.final,
        show-cursor: show-cursor,
        term-height: term-height,
        overflow: overflow,
      )
    }
  }
}
