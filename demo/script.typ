#import "../lib.typ": system, terminal

#show: terminal.with(
  user: "demo",
  system: system(
    hostname: "conch",
    files: (
      "setup.sh": (
        content: "#!/bin/bash\n# Build setup script\nmkdir -p build/bin\necho 'conch v0.1.0' > build/VERSION\necho 'Build environment ready!'",
        mode: 755,
      ),
      "src/main.rs": "fn main() {\n    println!(\"Hello!\");\n}",
    ),
  ),
)

```
cat setup.sh
./setup.sh
cat build/VERSION
tree
```
