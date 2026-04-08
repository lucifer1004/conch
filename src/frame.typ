#import "theme.typ": _resolve-font, _resolve-theme

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
  width: auto,
  height: auto,
) = {
  let t = _resolve-theme(theme)
  let f = _resolve-font(font)
  let term-width = if width == auto { 560pt } else { width }
  let term-height = if height == auto { auto } else { height }
  let title-text = if title != none { title } else { "" }

  block(
    fill: t.bg,
    radius: 8pt,
    clip: true,
    width: term-width,
    height: term-height,
    {
      // Title bar
      block(fill: t.title-bg, width: 100%, inset: (x: 12pt, y: 8pt), {
        box(circle(fill: rgb("#ff5f57"), radius: 5pt))
        h(6pt)
        box(circle(fill: rgb("#febc2e"), radius: 5pt))
        h(6pt)
        box(circle(fill: rgb("#28c840"), radius: 5pt))
        h(1fr)
        if title-text != "" {
          text(..f, fill: t.title-fg)[#title-text]
        }
        h(1fr)
        box(width: 42pt)
      })

      // Body
      block(inset: (x: 12pt, y: 10pt), width: 100%, {
        set text(..f, fill: t.fg)
        set par(leading: 0.4em)
        body
      })
    },
  )
}
