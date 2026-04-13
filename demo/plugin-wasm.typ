// Demo: WASM plugin — a custom `upper` command compiled to WASM
// and executed inside conch via the embedded wasmi interpreter.
//
// The plugin uppercases any text it receives (from stdin or args).
// It works in any pipeline position, just like a built-in command.

#import "../lib.typ": system, terminal

#let upper-wasm = read("demo-plugin.wasm", encoding: none)

#show: terminal.with(
  system: system(
    files: (
      "hello.txt": "Hello, World!\n",
      "names.txt": "alice\nbob\ncharlie\n",
    ),
    wasm-plugins: (("upper", upper-wasm),),
  ),
  user: "demo",
)

```
cat hello.txt
cat hello.txt | upper
echo "conch plugins" | upper
cat names.txt | upper | head -2
upper hello world
```
