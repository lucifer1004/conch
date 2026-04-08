#import "../lib.typ": terminal

#show: terminal.with(
  user: "demo",
  hostname: "conch",
  files: (
    "public.txt": "Hello, World!",
    "secret.txt": (content: "top secret data", mode: 000),
    "readonly.txt": (content: "do not modify", mode: 444),
    "writeonly.log": (content: "", mode: 200),
    "setup.sh": (content: "#!/bin/bash\necho 'Ready!'", mode: 644),
  ),
)

```
ls -la
cat public.txt
cat secret.txt
echo "overwrite" > readonly.txt
chmod 755 setup.sh
./setup.sh
```
