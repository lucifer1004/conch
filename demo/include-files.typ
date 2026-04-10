#import "../lib.typ": execute, system, terminal-block

// Demonstrate `include-files`: execute commands, then extract the
// resulting filesystem state for use in the Typst document.

#set page(width: 600pt, height: auto, margin: 24pt)
#set text(font: "New Computer Modern", size: 10pt)

= Filesystem Extraction Demo

Run shell commands and capture generated files.

== Terminal Session

#terminal-block(
  system: system(files: (
    "data.csv": "name,score\nalice,95\nbob,82\ncharlie,91",
  )),
  user: "demo",
  width: 540pt,
)[```
cat data.csv
sort -t, -k2 -rn data.csv > ranked.csv
head -n 2 ranked.csv > top2.csv
wc -l ranked.csv
```]

== Extracted Files

#let result = execute(
  system: system(files: (
    "data.csv": "name,score\nalice,95\nbob,82\ncharlie,91",
  )),
  user: "demo",
  commands: (
    "sort -t, -k2 -rn data.csv > ranked.csv",
    "head -n 2 ranked.csv > top2.csv",
  ),
  include-files: true,
)

The `include-files` option returns every file, directory, and symlink
after execution. Here are the files in the home directory:

#let home-files = {
  let pairs = ()
  for (path, entry) in result.files {
    if entry.type == "file" and path.starts-with("/home") {
      pairs += ((path: path, entry: entry),)
    }
  }
  pairs.sorted(key: p => p.path)
}

#table(
  columns: (1fr, 2fr, auto),
  align: (left, left, center),
  table.header[*File*][*Content*][*Mode*],
  ..{
    let cells = ()
    for p in home-files {
      cells += (
        raw(p.path.split("/").last()),
        raw(p.entry.content.trim(), block: true),
        raw(str(p.entry.mode)),
      )
    }
    cells
  },
)

== Inline Extraction

You can also use extracted content directly in prose:

#let ranked = result.files.at("/home/demo/ranked.csv", default: none)
#if ranked != none [
  The top scorer is *#ranked.content.split("\n").at(0).split(",").at(0)* with a
  score of *#ranked.content.split("\n").at(0).split(",").at(1)*.
]
