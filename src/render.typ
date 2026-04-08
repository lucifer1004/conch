#import "ansi.typ": _has-ansi, _render-ansi-raw

#let _render-output(entry, t, f) = {
  if entry.output == "" { return }
  let has-lang = "lang" in entry and entry.lang != none
  let has-ansi = _has-ansi(entry.output)
  if has-lang and not has-ansi {
    show raw.where(block: false): it => {
      set text(..f)
      box(fill: none, inset: 0pt, outset: 0pt, radius: 0pt, stroke: none, it)
    }
    for line in entry.output.split("\n") {
      raw(lang: entry.lang, line)
      linebreak()
    }
  } else if has-ansi {
    _render-ansi-raw(entry.output, t.fg, t.ansi)
    linebreak()
  } else {
    let color = if entry.exit-code != 0 { t.error } else { t.fg }
    for line in entry.output.split("\n") {
      text(fill: color)[#line]
      linebreak()
    }
  }
}

#let _render-prompt-parts(user, hostname, path, t) = {
  let user-host = user + "@" + hostname
  let colon = ":"
  let dollar = "$ "
  text(fill: t.prompt-user)[#user-host]
  text(fill: t.prompt-sym)[#colon]
  text(fill: t.prompt-path)[#path]
  text(fill: t.prompt-sym)[#dollar]
}

#let _render-prompt(entry, t) = {
  _render-prompt-parts(entry.user, entry.hostname, entry.path, t)
  text(fill: t.fg)[#entry.command]
}

/// Caret at the prompt (`0.85em` tall — small enough that line height stays stable when wrap moves it to the next line). Hidden frames omit the box entirely.
#let _cursor-cell(t) = box(
  fill: t.cursor,
  width: 0.5em,
  height: 0.85em,
  baseline: 15%,
)

// Unified frame renderer for shell sessions
#let _render-frame(
  session,
  user,
  hostname,
  t,
  f,
  term-width,
  typing: none,
  show-cursor: true,
  term-height: auto,
  overflow: "clip",
) = {
  let title = user + "@" + hostname

  let title-bar = block(
    fill: t.title-bg,
    width: 100%,
    inset: (x: 12pt, y: 8pt),
    {
      box(circle(fill: rgb("#ff5f57"), radius: 5pt))
      h(6pt)
      box(circle(fill: rgb("#febc2e"), radius: 5pt))
      h(6pt)
      box(circle(fill: rgb("#28c840"), radius: 5pt))
      h(1fr)
      text(..f, fill: t.title-fg)[#title]
      h(1fr)
      box(width: 42pt)
    },
  )

  let body-content = {
    set text(..f, fill: t.fg)
    set par(leading: 0.4em)

    for entry in session.entries {
      _render-prompt(entry, t)
      linebreak()
      _render-output(entry, t, f)
    }

    // Final prompt
    {
      _render-prompt-parts(user, hostname, session.final-path, t)
      if typing != none { text(fill: t.fg)[#typing] }
      if show-cursor {
        _cursor-cell(t)
      }
    }
  }

  if term-height != auto and overflow == "paginate" {
    // Paginate: split content across new pages instead of clipping
    context {
      let title-h = measure(title-bar).height
      let available = term-height - title-h / 2

      let one-line = measure(block({
        set text(..f)
        set par(leading: 0.4em)
        [X]
      })).height
      let two-lines = measure(block({
        set text(..f)
        set par(leading: 0.4em)
        [X]
        linebreak()
        [X]
      })).height
      let line-step = two-lines - one-line
      let lines-per-page = calc.max(1, calc.floor(
        (available - 20pt) / line-step,
      ))

      // Count lines per entry: 1 (prompt) + output lines
      let entries = session.entries
      let line-counts = entries.map(e => {
        let n = if e.output == "" { 0 } else { e.output.split("\n").len() }
        1 + n
      })

      // Group entries into pages via fold
      let pages = if entries.len() == 0 {
        ((items: (), count: 0),)
      } else {
        line-counts
          .zip(entries)
          .fold(
            ((items: (), count: 0),),
            (acc, pair) => {
              let n = pair.at(0)
              let entry = pair.at(1)
              let last = acc.last()
              if last.count + n > lines-per-page and last.items.len() > 0 {
                acc + ((items: (entry,), count: n),)
              } else {
                let updated = (
                  items: last.items + (entry,),
                  count: last.count + n,
                )
                acc.slice(0, acc.len() - 1) + (updated,)
              }
            },
          )
      }

      // Does the final prompt (1 line) still fit on the last page?
      let last-page = pages.last()
      let final-fits = last-page.count + 1 <= lines-per-page

      for (pi, page) in pages.enumerate() {
        if pi > 0 { pagebreak() }
        let is-last = pi == pages.len() - 1

        block(
          fill: t.bg,
          radius: 8pt,
          clip: true,
          width: term-width,
          height: term-height,
          {
            title-bar
            block(width: 100%, height: available, clip: true, {
              block(inset: (x: 12pt, y: 10pt), width: term-width, {
                set text(..f, fill: t.fg)
                set par(leading: 0.4em)
                for entry in page.items {
                  _render-prompt(entry, t)
                  linebreak()
                  _render-output(entry, t, f)
                }
                if is-last and final-fits {
                  _render-prompt-parts(user, hostname, session.final-path, t)
                  if typing != none { text(fill: t.fg)[#typing] }
                  if show-cursor {
                    _cursor-cell(t)
                  }
                }
              })
            })
          },
        )
      }

      if not final-fits {
        pagebreak()
        block(
          fill: t.bg,
          radius: 8pt,
          clip: true,
          width: term-width,
          height: term-height,
          {
            title-bar
            block(width: 100%, height: available, clip: true, {
              block(inset: (x: 12pt, y: 10pt), width: term-width, {
                set text(..f, fill: t.fg)
                set par(leading: 0.4em)
                _render-prompt-parts(user, hostname, session.final-path, t)
                if typing != none { text(fill: t.fg)[#typing] }
                if show-cursor {
                  _cursor-cell(t)
                }
              })
            })
          },
        )
      }
    }
  } else if term-height != auto {
    // Clip: shift old lines off the top like a real terminal
    block(
      fill: t.bg,
      radius: 8pt,
      clip: true,
      width: term-width,
      height: term-height,
      {
        title-bar
        context {
          let title-h = measure(title-bar).height
          let available = term-height - title-h / 2
          let body-block = block(
            inset: (x: 12pt, y: 10pt),
            width: term-width,
            body-content,
          )
          let body-h = measure(body-block).height

          // Measure line-to-line step for snapping to whole lines
          let one-line = measure(block({
            set text(..f)
            set par(leading: 0.4em)
            [X]
          })).height
          let two-lines = measure(block({
            set text(..f)
            set par(leading: 0.4em)
            [X]
            linebreak()
            [X]
          })).height
          let line-step = two-lines - one-line

          block(width: 100%, height: available, clip: true, {
            if body-h > available {
              // Round up to whole lines so no half-letters show at the top
              let overflow = body-h - available
              let lines-to-skip = calc.ceil(overflow / line-step)
              v(-(lines-to-skip * line-step))
            }
            body-block
          })
        }
      },
    )
  } else {
    block(fill: t.bg, radius: 8pt, clip: true, width: term-width, {
      title-bar
      block(inset: (x: 12pt, y: 10pt), width: 100%, body-content)
    })
  }
}
