#import "../lib.typ": terminal

#show: terminal.with(
  user: "demo",
  hostname: "conch",
  files: (
    "names.txt": "charlie\nalice\nbob\nalice\ncharlie\ncharlie\nbob",
    "log.txt": "INFO: server started\nERROR: connection lost\nINFO: reconnected\nWARN: slow query\nERROR: timeout\nINFO: recovered",
  ),
)

```
cat names.txt | sort | uniq -c | sort -rn
cat log.txt | grep ERROR
echo "build ok" > status.txt && cat status.txt
seq 1 5 | tr '\n' ' '
cat log.txt | grep -c INFO
```
