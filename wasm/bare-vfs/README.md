# bare-vfs

A minimal, synchronous, in-memory virtual filesystem for `no_std` and `wasm32-unknown-unknown`.

- **Zero dependencies** -- pure Rust, `alloc` only
- **Unix semantics** -- symlinks, permissions (owner/group/other), `chown`
- **Trie-backed** -- `BTreeMap` tree with O(depth) lookup and sorted iteration
- **Symlink-aware** -- transparent following, relative resolution, loop detection (depth 40)

## Quick Start

```rust
use bare_vfs::MemFs;

let mut fs = MemFs::new();

fs.create_dir_all("/src/bin");
fs.write("/src/main.rs", b"fn main() {}");

assert!(fs.is_file("/src/main.rs"));
assert_eq!(fs.read_to_string("/src/main.rs").unwrap(), "fn main() {}");

for entry in fs.read_dir("/src").unwrap() {
    println!("{} (dir={})", entry.name, entry.is_dir);
}
```

## Features

| Feature | Default | Description |
|---------|---------|-------------|
| `std`   | off     | Adds `FileHandle` with `Read` + `Seek` support |

Enable with:

```toml
[dependencies]
bare-vfs = { version = "0.1", features = ["std"] }
```

## API Overview

### Filesystem

```rust
let mut fs = MemFs::new();

// Write / read
fs.write("/hello.txt", b"world");
fs.write_with_mode("/secret.txt", b"data", 0o600);
fs.append("/hello.txt", b"!")?;
let bytes = fs.read("/hello.txt")?;          // &[u8]
let text  = fs.read_to_string("/hello.txt")?; // &str

// Directories
fs.create_dir("/a")?;
fs.create_dir_all("/a/b/c");
let entries = fs.read_dir("/a")?;  // Vec<DirEntry>, sorted by name

// Copy / rename / remove
fs.copy("/hello.txt", "/hello2.txt")?;
fs.rename("/hello2.txt", "/moved.txt")?;
fs.remove("/moved.txt");
fs.remove_dir_all("/a")?;

// Metadata
let meta = fs.metadata("/hello.txt")?;
assert!(meta.is_file());
assert_eq!(meta.len(), 6);
```

### Symlinks

```rust
fs.write("/target.txt", b"content");
fs.symlink("/target.txt", "/link.txt")?;

// Most operations follow symlinks transparently
assert_eq!(fs.read("/link.txt")?, fs.read("/target.txt")?);

// Inspect without following
assert!(fs.is_symlink("/link.txt"));
let target = fs.read_link("/link.txt")?;      // "/target.txt"
let meta   = fs.symlink_metadata("/link.txt")?; // metadata of the link itself
```

### Permissions & Ownership

```rust
fs.write_with_mode("/app.sh", b"#!/bin/sh", 0o755);

// Switch user context (default is uid=0 root, which bypasses checks)
fs.set_current_user(1000, 1000);
fs.add_supplementary_gid(100);

fs.set_mode("/app.sh", 0o700)?;
fs.chown("/app.sh", 1000, 1000)?;

// Permission checks apply to read, append, copy, set_mode
let result = fs.read("/app.sh"); // Ok -- owner has read permission
```

### Iteration

```rust
// All paths (DFS order)
for path in fs.paths() {
    println!("{path}");
}

// All entries with metadata
for (path, entry) in fs.iter() {
    match entry {
        bare_vfs::EntryRef::File { content, mode, .. } => { /* ... */ }
        bare_vfs::EntryRef::Dir { mode, .. } => { /* ... */ }
        bare_vfs::EntryRef::Symlink { target, .. } => { /* ... */ }
    }
}
```

### File Handles (requires `std`)

```rust
use std::io::{Read, Seek, SeekFrom};

let mut handle = fs.open("/hello.txt")?;
let mut buf = String::new();
handle.read_to_string(&mut buf)?;
handle.seek(SeekFrom::Start(0))?;
```

## Path Handling

All paths are normalized automatically -- `.` and `..` segments are resolved:

```rust
use bare_vfs::normalize;

assert_eq!(normalize("/a/b/../c/./d"), "/a/c/d");
```

## Use Cases

- **WASM runtimes** -- filesystem abstraction without host OS access
- **Testing** -- deterministic, isolated filesystem for unit tests
- **Sandboxing** -- restrict file operations to an in-memory tree
- **Embedded** -- `no_std` compatible, no heap beyond `alloc`

## Thread Safety

`MemFs` is **not `Sync`**. For concurrent access, wrap in an external mutex.

## License

MIT OR Apache-2.0
