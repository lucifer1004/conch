#import "../lib.typ": terminal-frame

#set page(width: 620pt, height: auto, margin: 0.3in)

#let demo-body = [
  #text(fill: rgb("#50fa7b"))[demo\@conch]#text(fill: white)[:~\$ ]echo "Hello!" \
  Hello!
]

#grid(
  columns: (1fr, 1fr),
  gutter: 12pt,
  ..for name in ("macos", "windows", "windows-terminal", "gnome", "plain") {
    (
      [
        #align(center)[#text(size: 11pt, weight: "bold")[#name]]
        #v(4pt)
        #terminal-frame(
          title: "demo@conch",
          theme: "dracula",
          chrome: name,
          width: 280pt,
        )[
          #demo-body
        ]
      ],
    )
  }
)
