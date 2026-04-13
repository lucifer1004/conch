// Demo: Typst function plugin — custom commands without compiling WASM.
//
// Define commands as plain Typst functions that receive (args, stdin, files)
// and return (stdout, exit-code). Great for mocks and simple tools.

#import "../lib.typ": system, terminal

// A cowsay-like command
#let cowsay(args, stdin, files) = {
  let msg = if stdin != "" { stdin.trim("\n", at: end) } else { args.join(" ") }
  (
    stdout: " "
      + "_" * (msg.len() + 2)
      + "\n< "
      + msg
      + " >\n "
      + "-" * (msg.len() + 2)
      + "\n        \\   ^__^\n         \\  (oo)\\_______\n            (__)\\       )\\/\\\n                ||----w |\n                ||     ||\n",
    exit-code: 0,
  )
}

// A rot13 cipher command
#let rot13(args, stdin, files) = {
  let text = if stdin != "" { stdin } else { args.join(" ") + "\n" }
  let result = text
    .codepoints()
    .map(c => {
      let code = c.to-unicode()
      if code >= 65 and code <= 90 {
        str.from-unicode(calc.rem(code - 65 + 13, 26) + 65)
      } else if code >= 97 and code <= 122 {
        str.from-unicode(calc.rem(code - 97 + 13, 26) + 97)
      } else {
        c
      }
    })
    .join()
  (stdout: result, exit-code: 0)
}

#show: terminal.with(
  system: system(
    files: ("secret.txt": "Hello from conch!\n"),
    plugins: (
      ("cowsay", cowsay),
      ("rot13", rot13),
    ),
  ),
  user: "demo",
)

```
cowsay "Plugins work!"
echo "Hello World" | rot13
cat secret.txt | rot13
```
