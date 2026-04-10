use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::fmt;

use crate::entry::{Entry, EntryRef};
use crate::metadata::Metadata;

/// Maximum symlink resolution depth to prevent infinite loops.
pub(crate) const MAX_SYMLINK_DEPTH: usize = 40;

#[derive(Debug, Clone)]
pub(crate) enum InodeKind {
    File { content: Vec<u8> },
    Dir { children: BTreeMap<String, u64> },
    Symlink { target: String },
}

#[derive(Debug, Clone)]
pub(crate) struct Inode {
    pub(crate) kind: InodeKind,
    pub(crate) mode: u16,
    pub(crate) uid: u32,
    pub(crate) gid: u32,
    pub(crate) mtime: u64,
    pub(crate) ctime: u64,
    pub(crate) atime: u64,
    pub(crate) nlink: u64,
}

impl Inode {
    pub(crate) fn is_dir(&self) -> bool {
        matches!(self.kind, InodeKind::Dir { .. })
    }

    pub(crate) fn is_file(&self) -> bool {
        matches!(self.kind, InodeKind::File { .. })
    }

    pub(crate) fn is_symlink(&self) -> bool {
        matches!(self.kind, InodeKind::Symlink { .. })
    }

    pub(crate) fn mode(&self) -> u16 {
        match &self.kind {
            InodeKind::File { .. } | InodeKind::Dir { .. } => self.mode,
            InodeKind::Symlink { .. } => 0o777,
        }
    }

    pub(crate) fn mode_mut(&mut self) -> Option<&mut u16> {
        match &self.kind {
            InodeKind::File { .. } | InodeKind::Dir { .. } => Some(&mut self.mode),
            InodeKind::Symlink { .. } => None,
        }
    }

    /// Returns (uid, gid, mode) for permission checking.
    pub(crate) fn ownership_and_mode(&self) -> (u32, u32, u16) {
        (self.uid, self.gid, self.mode())
    }

    pub(crate) fn mtime(&self) -> u64 {
        self.mtime
    }

    #[allow(dead_code)]
    pub(crate) fn atime(&self) -> u64 {
        self.atime
    }

    pub(crate) fn as_entry_ref(&self) -> EntryRef<'_> {
        match &self.kind {
            InodeKind::File { content } => EntryRef::File {
                content,
                mode: self.mode,
                uid: self.uid,
                gid: self.gid,
                mtime: self.mtime,
                ctime: self.ctime,
                atime: self.atime,
            },
            InodeKind::Dir { .. } => EntryRef::Dir {
                mode: self.mode,
                uid: self.uid,
                gid: self.gid,
                mtime: self.mtime,
                ctime: self.ctime,
                atime: self.atime,
            },
            InodeKind::Symlink { target } => EntryRef::Symlink {
                target,
                uid: self.uid,
                gid: self.gid,
                mtime: self.mtime,
                ctime: self.ctime,
                atime: self.atime,
            },
        }
    }

    pub(crate) fn to_metadata(&self, ino: u64) -> Metadata {
        match &self.kind {
            InodeKind::File { content } => Metadata::new(
                true,
                content.len(),
                self.mode,
                self.uid,
                self.gid,
                self.mtime,
                self.ctime,
                self.atime,
                self.nlink,
                ino,
            ),
            InodeKind::Dir { .. } => Metadata::new(
                false, 0, self.mode, self.uid, self.gid, self.mtime, self.ctime, self.atime,
                self.nlink, ino,
            ),
            InodeKind::Symlink { .. } => Metadata::new_symlink(
                0o777, self.uid, self.gid, self.mtime, self.ctime, self.atime, self.nlink, ino,
            ),
        }
    }

    /// Convert an owned Inode into an Entry (consumes self).
    #[allow(dead_code)]
    pub(crate) fn into_entry(self) -> Entry {
        match self.kind {
            InodeKind::File { content } => Entry::File {
                content,
                mode: self.mode,
                uid: self.uid,
                gid: self.gid,
                mtime: self.mtime,
                ctime: self.ctime,
                atime: self.atime,
            },
            InodeKind::Dir { .. } => Entry::Dir {
                mode: self.mode,
                uid: self.uid,
                gid: self.gid,
                mtime: self.mtime,
                ctime: self.ctime,
                atime: self.atime,
            },
            InodeKind::Symlink { target } => Entry::Symlink {
                target,
                uid: self.uid,
                gid: self.gid,
                mtime: self.mtime,
                ctime: self.ctime,
                atime: self.atime,
            },
        }
    }

    /// Create an Entry from a borrow (clones content).
    pub(crate) fn to_entry(&self) -> Entry {
        match &self.kind {
            InodeKind::File { content } => Entry::File {
                content: content.clone(),
                mode: self.mode,
                uid: self.uid,
                gid: self.gid,
                mtime: self.mtime,
                ctime: self.ctime,
                atime: self.atime,
            },
            InodeKind::Dir { .. } => Entry::Dir {
                mode: self.mode,
                uid: self.uid,
                gid: self.gid,
                mtime: self.mtime,
                ctime: self.ctime,
                atime: self.atime,
            },
            InodeKind::Symlink { target } => Entry::Symlink {
                target: target.clone(),
                uid: self.uid,
                gid: self.gid,
                mtime: self.mtime,
                ctime: self.ctime,
                atime: self.atime,
            },
        }
    }
}

/// Collect all paths via DFS from the inode table.
pub(crate) fn collect_paths(
    inodes: &BTreeMap<u64, Inode>,
    ino: u64,
    prefix: &str,
    out: &mut Vec<String>,
) {
    out.push(prefix.to_string());
    if let Some(inode) = inodes.get(&ino) {
        if let InodeKind::Dir { children } = &inode.kind {
            for (name, child_ino) in children {
                let child_path = if prefix == "/" {
                    alloc::format!("/{}", name)
                } else {
                    alloc::format!("{}/{}", prefix, name)
                };
                collect_paths(inodes, *child_ino, &child_path, out);
            }
        }
    }
}

/// Collect all `(path, EntryRef)` pairs via DFS from the inode table.
pub(crate) fn collect_entries<'a>(
    inodes: &'a BTreeMap<u64, Inode>,
    ino: u64,
    prefix: &str,
    out: &mut Vec<(String, EntryRef<'a>)>,
) {
    if let Some(inode) = inodes.get(&ino) {
        out.push((prefix.to_string(), inode.as_entry_ref()));
        if let InodeKind::Dir { children } = &inode.kind {
            for (name, child_ino) in children {
                let child_path = if prefix == "/" {
                    alloc::format!("/{}", name)
                } else {
                    alloc::format!("{}/{}", prefix, name)
                };
                collect_entries(inodes, *child_ino, &child_path, out);
            }
        }
    }
}

/// Pretty-print the tree for Display.
pub(crate) fn fmt_tree(
    inodes: &BTreeMap<u64, Inode>,
    ino: u64,
    f: &mut fmt::Formatter<'_>,
    name: &str,
    prefix: &str,
    is_last: bool,
) -> fmt::Result {
    let inode = match inodes.get(&ino) {
        Some(i) => i,
        None => return Ok(()),
    };
    let connector = if is_last {
        "\u{2514}\u{2500}\u{2500} "
    } else {
        "\u{251c}\u{2500}\u{2500} "
    };
    let suffix = if inode.is_dir() { "/" } else { "" };
    writeln!(f, "{}{}{}{}", prefix, connector, name, suffix)?;
    if let InodeKind::Dir { children } = &inode.kind {
        let child_prefix = if is_last {
            alloc::format!("{}    ", prefix)
        } else {
            alloc::format!("{}\u{2502}   ", prefix)
        };
        let count = children.len();
        for (i, (child_name, child_ino)) in children.iter().enumerate() {
            fmt_tree(
                inodes,
                *child_ino,
                f,
                child_name,
                &child_prefix,
                i == count - 1,
            )?;
        }
    }
    Ok(())
}
