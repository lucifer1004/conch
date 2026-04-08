#import "@preview/conch:0.1.0": terminal

#show: terminal.with(
  user: "lucifer1004",
  hostname: "conch",
  height: 300pt,
  files: (
    "greet.sh": (
      content: "#!/bin/bash\n# A greeting script\necho \"Hello from $USER!\"\nls | head -n 3\necho \"Done.\"",
      mode: 755,
    ),
    "setup.sh": "#!/bin/bash\nmkdir -p build\necho 'ready' > build/status.txt\necho 'Build environment ready.'",
    "src/main.typ": "#set page(width: 210mm)\nHello from Typst!",
    "README.md": "# Conch\nA shell simulator for Typst.",
  ),
)

```
ls -la
cat greet.sh
./greet.sh
chmod 755 setup.sh
bash setup.sh
cat build/status.txt
tree
```
