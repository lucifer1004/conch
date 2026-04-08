// Touying integration demo — terminal animation inside presentation slides.
// Requires: @preview/touying (install via typst package manager).
//
// Compile:  typst compile demo/touying.typ demo/touying.pdf --root .

#import "@preview/touying:0.7.1": *
#import "../lib.typ": terminal-block, terminal-frames

#import themes.simple: (
  centered-slide, empty-slide, focus-slide, simple-theme, slide, title-slide,
)
#show: simple-theme.with(aspect-ratio: "16-9")


// =========================================================================

#title-slide[
  = Conch + Touying
  A terminal simulator inside presentation slides.
]

// =========================================================================
// Static terminal embedded in a slide
// =========================================================================

== Static Terminal

#slide[
  #terminal-block(
    user: "demo",
    hostname: "conch",
    width: 480pt,
    height: 200pt,
    files: ("hello.txt": "Hello from conch!"),
  )[```
  ls
  cat hello.txt
  ```]
]

// =========================================================================
// Animated terminal — per-line, one subslide per command step
// =========================================================================

#let line-frames = terminal-frames(
  mode: "per-line",
  user: "demo",
  hostname: "conch",
  width: 480pt,
  height: 200pt,
  files: (
    "hello.txt": "Hello, World!",
    "src/main.rs": "fn main() {\n    println!(\"hi\");\n}",
  ),
  commands: ("ls", "cat hello.txt", "cat src/main.rs", "echo done"),
)

== Per-Line Animation

#slide(repeat: line-frames.len(), self => [
  #line-frames.at(self.subslide - 1)
])

// =========================================================================
// Key-frames — fewer subslides, only meaningful moments
// =========================================================================

#let key-frames = terminal-frames(
  mode: "key-frames",
  user: "demo",
  hostname: "conch",
  width: 480pt,
  height: 200pt,
  files: ("data.csv": "name,age\nalice,30\nbob,25"),
  commands: ("ls", "cat data.csv", "echo done"),
)

== Key-Frames

#slide(repeat: key-frames.len(), self => [
  #key-frames.at(self.subslide - 1)
])

// =========================================================================

#focus-slide[
  #alternatives[Works with any touying theme.][Any chrome style.][Any color theme.]
]
