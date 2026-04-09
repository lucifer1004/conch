use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::fmt;

use crate::entry::EntryRef;
use crate::metadata::Metadata;

/// Maximum symlink resolution depth to prevent infinite loops.
pub(crate) const MAX_SYMLINK_DEPTH: usize = 40;

#[derive(Debug, Clone)]
pub(crate) enum TreeNode {
    File {
        content: Vec<u8>,
        mode: u16,
        uid: u32,
        gid: u32,
        mtime: u64,
        ctime: u64,
    },
    Dir {
        mode: u16,
        children: BTreeMap<String, TreeNode>,
        uid: u32,
        gid: u32,
        mtime: u64,
        ctime: u64,
    },
    Symlink {
        target: String,
        uid: u32,
        gid: u32,
        mtime: u64,
        ctime: u64,
    },
}

impl TreeNode {
    pub(crate) fn is_dir(&self) -> bool {
        matches!(self, TreeNode::Dir { .. })
    }

    pub(crate) fn is_file(&self) -> bool {
        matches!(self, TreeNode::File { .. })
    }

    pub(crate) fn is_symlink(&self) -> bool {
        matches!(self, TreeNode::Symlink { .. })
    }

    pub(crate) fn mode(&self) -> u16 {
        match self {
            TreeNode::File { mode, .. } | TreeNode::Dir { mode, .. } => *mode,
            TreeNode::Symlink { .. } => 0o777,
        }
    }

    pub(crate) fn mode_mut(&mut self) -> Option<&mut u16> {
        match self {
            TreeNode::File { mode, .. } | TreeNode::Dir { mode, .. } => Some(mode),
            TreeNode::Symlink { .. } => None,
        }
    }

    /// Returns (uid, gid, mode) for permission checking.
    pub(crate) fn ownership_and_mode(&self) -> (u32, u32, u16) {
        match self {
            TreeNode::File { uid, gid, mode, .. } => (*uid, *gid, *mode),
            TreeNode::Dir { uid, gid, mode, .. } => (*uid, *gid, *mode),
            TreeNode::Symlink { uid, gid, .. } => (*uid, *gid, 0o777),
        }
    }

    pub(crate) fn mtime(&self) -> u64 {
        match self {
            TreeNode::File { mtime, .. }
            | TreeNode::Dir { mtime, .. }
            | TreeNode::Symlink { mtime, .. } => *mtime,
        }
    }

    pub(crate) fn as_entry_ref(&self) -> EntryRef<'_> {
        match self {
            TreeNode::File {
                content,
                mode,
                uid,
                gid,
                mtime,
                ctime,
            } => EntryRef::File {
                content,
                mode: *mode,
                uid: *uid,
                gid: *gid,
                mtime: *mtime,
                ctime: *ctime,
            },
            TreeNode::Dir {
                mode,
                uid,
                gid,
                mtime,
                ctime,
                ..
            } => EntryRef::Dir {
                mode: *mode,
                uid: *uid,
                gid: *gid,
                mtime: *mtime,
                ctime: *ctime,
            },
            TreeNode::Symlink {
                target,
                uid,
                gid,
                mtime,
                ctime,
            } => EntryRef::Symlink {
                target,
                uid: *uid,
                gid: *gid,
                mtime: *mtime,
                ctime: *ctime,
            },
        }
    }

    pub(crate) fn to_metadata(&self) -> Metadata {
        match self {
            TreeNode::File {
                content,
                mode,
                uid,
                gid,
                mtime,
                ctime,
            } => Metadata::new(true, content.len(), *mode, *uid, *gid, *mtime, *ctime),
            TreeNode::Dir {
                mode,
                uid,
                gid,
                mtime,
                ctime,
                ..
            } => Metadata::new(false, 0, *mode, *uid, *gid, *mtime, *ctime),
            TreeNode::Symlink {
                uid,
                gid,
                mtime,
                ctime,
                ..
            } => Metadata::new_symlink(0o777, *uid, *gid, *mtime, *ctime),
        }
    }

    /// Collect all paths via DFS.
    pub(crate) fn collect_paths(&self, prefix: &str, out: &mut Vec<String>) {
        out.push(prefix.to_string());
        if let TreeNode::Dir { children, .. } = self {
            for (name, child) in children {
                let child_path = if prefix == "/" {
                    alloc::format!("/{}", name)
                } else {
                    alloc::format!("{}/{}", prefix, name)
                };
                child.collect_paths(&child_path, out);
            }
        }
    }

    /// Pretty-print the tree for Display.
    pub(crate) fn fmt_tree(
        &self,
        f: &mut fmt::Formatter<'_>,
        name: &str,
        prefix: &str,
        is_last: bool,
    ) -> fmt::Result {
        let connector = if is_last { "└── " } else { "├── " };
        let suffix = if self.is_dir() { "/" } else { "" };
        writeln!(f, "{}{}{}{}", prefix, connector, name, suffix)?;
        if let TreeNode::Dir { children, .. } = self {
            let child_prefix = if is_last {
                alloc::format!("{}    ", prefix)
            } else {
                alloc::format!("{}│   ", prefix)
            };
            let count = children.len();
            for (i, (child_name, child)) in children.iter().enumerate() {
                child.fmt_tree(f, child_name, &child_prefix, i == count - 1)?;
            }
        }
        Ok(())
    }

    /// Collect all `(path, EntryRef)` pairs via DFS.
    pub(crate) fn collect_entries<'a>(
        &'a self,
        prefix: &str,
        out: &mut Vec<(String, EntryRef<'a>)>,
    ) {
        out.push((prefix.to_string(), self.as_entry_ref()));
        if let TreeNode::Dir { children, .. } = self {
            for (name, child) in children {
                let child_path = if prefix == "/" {
                    alloc::format!("/{}", name)
                } else {
                    alloc::format!("{}/{}", prefix, name)
                };
                child.collect_entries(&child_path, out);
            }
        }
    }
}

/// Split an absolute path into components. "/" → empty vec, "/a/b" → ["a", "b"].
pub(crate) fn split_path(path: &str) -> Vec<&str> {
    path.split('/').filter(|s| !s.is_empty()).collect()
}
