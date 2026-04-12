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
//! assert_eq!(fs.read_to_string("/src/main.rs"), Ok("fn main() {}"));
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
#[cfg(feature = "std")]
mod open_options;
mod path;

pub use dir::DirEntry;
pub use entry::{Entry, EntryRef};
pub use error::{VfsError, VfsErrorKind};
pub use fs::{AccessMode, MemFs, ReadDirIter, Walk};
#[cfg(feature = "std")]
pub use handle::FileHandle;
pub use metadata::Metadata;
#[cfg(feature = "std")]
pub use open_options::OpenOptions;
pub use path::{normalize, parent, validate};
