//! A minimal, synchronous, in-memory virtual filesystem.
//!
//! `bare-vfs` provides a [`MemFs`] backed by a `BTreeMap<String, Entry>` that works
//! everywhere Rust compiles — including `wasm32-unknown-unknown` and `no_std` targets.
//!
//! Files store raw bytes (`Vec<u8>`), with convenience methods for UTF-8 string
//! access. Enable the `std` feature for [`FileHandle`] with `Read` + `Seek`.
//!
//! # Quick start
//!
//! ```
//! use bare_vfs::MemFs;
//!
//! let mut fs = MemFs::new();
//! fs.create_dir_all("/src/bin");
//! fs.write("/src/main.rs", "fn main() {}");
//!
//! assert!(fs.is_file("/src/main.rs"));
//! assert_eq!(fs.read_to_string("/src/main.rs").unwrap(), "fn main() {}");
//! ```

#![no_std]
extern crate alloc;

#[cfg(feature = "std")]
extern crate std;

mod dir;
mod entry;
mod error;
mod fs;
#[cfg(feature = "std")]
mod handle;
mod metadata;
mod path;

pub use dir::DirEntry;
pub use entry::{Entry, EntryRef};
pub use error::VfsError;
pub use fs::MemFs;
#[cfg(feature = "std")]
pub use handle::FileHandle;
pub use metadata::Metadata;
pub use path::{normalize, parent};
