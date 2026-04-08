#import "../lib.typ": terminal-frame

#set page(width: 1040pt, height: auto, margin: 0.3in)

#let demo-body = [
  #text(fill: rgb("#50fa7b"))[demo\@conch]#text(fill: white)[:~\$ ]ls \
  hello.txt  #text(fill: rgb("#bd93f9"), weight: "bold")[src/] \
  #text(fill: rgb("#50fa7b"))[demo\@conch]#text(fill: white)[:~\$ ]echo "Hello!" \
  Hello!
]

#grid(
  columns: (1fr, 1fr),
  gutter: 16pt,
  ..for name in (
    "dracula",
    "catppuccin",
    "monokai",
    "retro",
    "solarized",
    "gruvbox",
  ) {
    (
      [
        #align(center)[#text(size: 11pt, weight: "bold")[#name]]
        #v(4pt)
        #terminal-frame(title: "demo@conch", theme: name, width: 480pt)[
          #text(fill: rgb("#50fa7b"))[demo\@conch]#text(fill: white)[:~\$ ]ls \
          hello.txt  src/ \
          #text(fill: rgb("#50fa7b"))[demo\@conch]#text(fill: white)[:~\$ ]echo "Hello!" \
          Hello!
        ]
      ],
    )
  }
)
