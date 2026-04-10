# bare-vfs

A minimal, synchronous, in-memory virtual filesystem for `no_std` and `wasm32-unknown-unknown`.

- **Inode-based** -- Unix-style inode table with real `nlink` tracking
- **Hard links** -- multiple names for the same file, shared content and metadata
- **Unix permissions** -- owner/group/other rwx, setuid/setgid/sticky, umask
- **Symlink-aware** -- transparent following, relative resolution, loop detection
- **Serializable** -- `serde` feature for snapshot/restore (preserves hard links)
- **Zero unsafe** -- no `unsafe` code anywhere in the crate
- **`no_std`** -- pure Rust with `alloc` only, zero external dependencies by default

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

| Feature | Default | Description                                           |
| ------- | ------- | ----------------------------------------------------- |
| `std`   | off     | `FileHandle` (Read/Write/Seek), `OpenOptions` builder |
| `serde` | off     | Serialize/Deserialize for `MemFs` and public types    |

```toml
[dependencies]
bare-vfs = { version = "0.1", features = ["std", "serde"] }
```

## API Overview

### Files and Directories

```rust
let mut fs = MemFs::new();

// Write / read
fs.write("/hello.txt", "world");
fs.write_with_mode("/secret.txt", b"data", 0o600);
fs.append("/hello.txt", b"!")?;
fs.truncate("/hello.txt", 5)?;
let bytes = fs.read("/hello.txt")?;           // &[u8]
let text  = fs.read_to_string("/hello.txt")?; // &str

// Directories
fs.create_dir("/a")?;
fs.create_dir_all("/a/b/c");  // follows symlinks in intermediate components
let entries = fs.read_dir("/a")?;  // Vec<DirEntry>, sorted by name

// Copy / rename / remove
fs.copy("/hello.txt", "/hello2.txt")?;
fs.rename("/hello2.txt", "/moved.txt")?;
fs.remove("/moved.txt");
fs.remove_dir_all("/a")?;
```

### Hard Links

```rust
fs.write("/original.txt", "shared content");
fs.hard_link("/original.txt", "/alias.txt")?;

// Both names share the same inode
assert_eq!(fs.metadata("/original.txt")?.ino(),
           fs.metadata("/alias.txt")?.ino());
assert_eq!(fs.metadata("/original.txt")?.nlink(), 2);

// Mutations through one name are visible through the other
fs.append("/original.txt", b" updated")?;
assert_eq!(fs.read_to_string("/alias.txt")?, "shared content updated");

// Removing one name preserves the other
fs.remove("/original.txt");
assert_eq!(fs.read_to_string("/alias.txt")?, "shared content updated");
```

### Symlinks

```rust
fs.write("/target.txt", b"content");
fs.symlink("/target.txt", "/link.txt")?;

// Most operations follow symlinks transparently
assert_eq!(fs.read("/link.txt")?, fs.read("/target.txt")?);

// Inspect without following
assert!(fs.is_symlink("/link.txt"));
let target = fs.read_link("/link.txt")?;        // "/target.txt"
let meta   = fs.symlink_metadata("/link.txt")?;  // metadata of the link itself
```

### Permissions and Ownership

```rust
fs.write_with_mode("/app.sh", b"#!/bin/sh", 0o755);

// Switch user context (default is uid=0 root, which bypasses all checks)
fs.set_current_user(1000, 1000);
fs.add_supplementary_gid(100);

// chmod requires file owner or root
fs.set_mode("/app.sh", 0o700)?;

// chown: root can change uid+gid; owner can change gid to own groups
fs.chown("/app.sh", 1000, 100)?;

// Permission checks apply to read, write, append, copy, chmod, readdir, traversal
let result = fs.read("/app.sh"); // Ok -- owner has read permission

// Explicit permission testing
use bare_vfs::AccessMode;
fs.access("/app.sh", AccessMode::R_OK | AccessMode::X_OK)?;
```

### Umask

```rust
fs.set_umask(0o077);
fs.write("/private.txt", "secret");
assert_eq!(fs.metadata("/private.txt")?.mode(), 0o600);  // 0o644 & !0o077
```

### Timestamps

Timestamps auto-increment on each mutation (monotonic counter, not wall clock):

```rust
fs.write("/a", "first");
fs.write("/b", "second");
assert!(fs.metadata("/a")?.mtime() < fs.metadata("/b")?.mtime());

// Override with real timestamps if needed
fs.set_time(1_700_000_000);
```

### Metadata

```rust
let meta = fs.metadata("/app.sh")?;
meta.is_file();    // true
meta.len();        // content size in bytes
meta.mode();       // Unix permission bits
meta.uid();        // owner user ID
meta.gid();        // owner group ID
meta.ino();        // inode number
meta.nlink();      // hard link count
meta.mtime();      // last modification time
meta.ctime();      // last metadata change time
meta.atime();      // last access time
```

### Iteration

```rust
// Lazy DFS iterator (no allocation until consumed)
for (path, entry) in fs.walk() {
    println!("{path}");
}

// Subtree only
for (path, entry) in fs.walk_prefix("/src") {
    // only entries under /src
}

// Lazy directory listing
for entry in fs.read_dir_iter("/src")? {
    println!("{} ({}B)", entry.name, entry.size);
}

// Collecting variants (allocate Vec)
let all_paths = fs.paths();
let all_entries = fs.iter();
```

### File Handles (requires `std`)

```rust
use std::io::{Read, Write, Seek, SeekFrom};

// Read-only convenience
let mut handle = fs.open("/hello.txt")?;
let mut buf = String::new();
handle.read_to_string(&mut buf)?;

// OpenOptions builder for full control
use bare_vfs::OpenOptions;
let mut handle = OpenOptions::new()
    .write(true)
    .create(true)
    .mode(0o755)
    .open(&mut fs, "/script.sh")?;
handle.write_all(b"#!/bin/sh\necho hello")?;
fs.commit("/script.sh", handle);  // persist changes back
```

### Serialization (requires `serde`)

```rust
// Snapshot entire filesystem (preserves hard links, timestamps, permissions)
let json = serde_json::to_string(&fs)?;
let restored: MemFs = serde_json::from_str(&json)?;
```

### Path Utilities

```rust
use bare_vfs::{normalize, parent, validate};

assert_eq!(normalize("/a/b/../c/./d"), "/a/c/d");
assert_eq!(parent("/a/b/c"), Some("/a/b"));
assert!(validate("/a/b").is_ok());
assert!(validate("relative").is_err());

// Resolve all symlinks
let canon = fs.canonical_path("/link/to/somewhere")?;
```

### Display

```rust
println!("{}", fs);
// /
// ├── src/
// │   └── main.rs
// └── hello.txt
```

## Use Cases

- **WASM runtimes** -- filesystem abstraction without host OS access
- **Testing** -- deterministic, isolated filesystem for unit tests
- **Sandboxing** -- restrict file operations to an in-memory tree
- **Embedded** -- `no_std` compatible, no heap beyond `alloc`

## Thread Safety

`MemFs` is `Clone` but **not `Sync`**. For concurrent access, wrap in an external mutex.

## License

MIT OR Apache-2.0
