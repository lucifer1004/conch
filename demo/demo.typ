#import "../lib.typ": system, terminal-per-char

// Source for `just gif` → demo/demo.gif (README quick-start session, per-keystroke frames).
// Showcases: syntax highlighting, permissions, scripts, users/groups, keyboard corrections,
// pipes, sudo, symlinks, and scrolling (terminal height triggers top clipping).
#terminal-per-char(
  hold: (
    after-output: 10,
    after-final: 24,
    final-cursor-blink: true,
    final-blink-hold: 3,
  ),
  user: "demo",
  system: system(
    hostname: "conch",
    users: (
      (name: "alice", groups: ("sudo",)),
    ),
    groups: (
      (name: "sudo"),
    ),
    files: (
      "src/main.rs": "fn main() {\n    println!(\"Hello, conch!\");\n}",
      "app.py": "import sys\n\ndef greet(name):\n    print(f\"Hi {name}!\")\n\ngreet(sys.argv[1])",
      "secret.txt": (content: "TOP SECRET: launch codes", mode: 000),
      "deploy.sh": (
        content: "#!/bin/bash\nmkdir -p dist\necho 'v1.0' > dist/VERSION\necho 'Deploy complete!'",
        mode: 755,
      ),
      "data.csv": "name,role,level\nalice,admin,5\nbob,dev,3\ncharlie,dev,4",
      "/root/flag.txt": (content: "CTF{conch_shell_master}", mode: 600),
    ),
  ),
  height: 350pt,
  width: 560pt,
)[```
ls -la
cat src/main.rs
cat app.py
cat secret.txt
chmod 644 secret.txt
cat secert\x7f\x7f\x7fret.txt
./deploy.sh
cat dist/VERSION
ln -s src/main.rs link.rs
cat data.csv | sort -t, -k3 -rn | head -n 2
useradd bob
su alice
whoami
id
sudo cat /root/flag.txt
tree
eco\x1b[Dh\x1b[F "All features!"
```]
