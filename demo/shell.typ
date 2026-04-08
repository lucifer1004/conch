#import "../lib.typ": terminal

#show: terminal.with(
  user: "demo",
  hostname: "conch",
  files: (
    "hello.txt": "Hello, World!",
    "data.csv": "name,age,city\nalice,30,paris\nbob,25,london\ncharlie,35,tokyo",
    "src/main.rs": "fn main() {\n    println!(\"Hello from conch!\");\n}",
    "README.md": "# Conch\nA shell simulator for Typst.\nPowered by Rust + WASM.",
  ),
)

```
ls
cat src/main.rs
cat data.csv | cut -d , -f 1,3 | sort
grep -n conch README.md
echo "Welcome to $SHELL!"
tree
ls -la
```
