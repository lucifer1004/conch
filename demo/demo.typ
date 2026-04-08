#import "../lib.typ": terminal-per-char

// Source for `just gif` → demo/demo.gif (README quick-start session, per-keystroke frames).
#terminal-per-char(
  hold: (
    after-output: 10,
    after-final: 24,
    final-cursor-blink: true,
    final-blink-hold: 3,
  ),
  user: "demo",
  hostname: "conch",
  height: 350pt,
  width: 560pt,
  files: (
    "hello.txt": "Hello, World!",
    "src/main.rs": "fn main() {\n    println!(\"hi\");\n}",
    "run.sh": (content: "#!/bin/bash\necho 'Hello from $SHELL!'", mode: 644),
  ),
)[```
ls
cat hello.txt
cat src/main.rs
cat run.sh
./run.sh
chmod 755 run.sh
./run.sh
echo "done"
ls
```]
