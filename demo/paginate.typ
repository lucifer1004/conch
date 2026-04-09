#import "../lib.typ": system, terminal

#show: terminal.with(
  user: "demo",
  system: system(
    hostname: "conch",
    files: (
      "hello.txt": "Hello, World!",
      "data.csv": "name,age,city\nalice,30,paris\nbob,25,london\ncharlie,35,tokyo",
      "src/main.rs": "fn main() {\n    println!(\"Hello from conch!\");\n}",
      "README.md": "# Conch\nA shell simulator for Typst.\nPowered by Rust + WASM.",
    ),
  ),
  height: 300pt,
  overflow: "paginate",
)

```
ls
cat src/main.rs
cat data.csv
grep -n conch README.md
echo "Welcome to conch!"
tree
ls -la
```
