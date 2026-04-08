#import "theme.typ": _resolve-font, _resolve-theme
#import "chrome.typ": _resolve-chrome

// =========================================================================
// Layer 1: Terminal frame — a reusable themed terminal window
// =========================================================================

/// Render content inside a terminal window frame.
/// No shell, no WASM — just the visual chrome around arbitrary content.
///
/// ```typst
/// #terminal-frame(title: "my-app", theme: "dracula")[
///   #text(fill: green)[$ cargo build]
///   Compiling my-app v0.1.0
///   Finished release target
/// ]
/// ```
#let terminal-frame(
  body,
  title: none,
  theme: "dracula",
  font: auto,
  chrome: "macos",
  width: auto,
  height: auto,
) = {
  let t = _resolve-theme(theme)
  let f = _resolve-font(font)
  let c = _resolve-chrome(chrome)
  let term-width = if width == auto { 560pt } else { width }
  let term-height = if height == auto { auto } else { height }
  let title-text = if title != none { title } else { "" }
  let title-bar = (c.bar)(title-text, t, f)

  block(
    fill: t.bg,
    radius: c.radius,
    clip: true,
    width: term-width,
    height: term-height,
    {
      if title-bar != none { title-bar }

      // Body
      block(inset: (x: 12pt, y: 6pt), width: 100%, {
        set text(..f, fill: t.fg)
        set par(leading: 0.4em)
        body
      })
    },
  )
}
