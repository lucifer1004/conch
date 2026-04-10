use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use super::inode::{Inode, InodeKind};
use super::MemFs;
use crate::path::split_path;

// ---------------------------------------------------------------------------
// Snapshot types
// ---------------------------------------------------------------------------

/// Serialization-friendly snapshot of a single inode.
#[derive(Serialize, Deserialize)]
enum InodeSnapshot {
    File {
        content: Vec<u8>,
        mode: u16,
        uid: u32,
        gid: u32,
        mtime: u64,
        ctime: u64,
        atime: u64,
        nlink: u64,
    },
    Dir {
        mode: u16,
        uid: u32,
        gid: u32,
        mtime: u64,
        ctime: u64,
        atime: u64,
        nlink: u64,
    },
    Symlink {
        target: String,
        uid: u32,
        gid: u32,
        mtime: u64,
        ctime: u64,
        atime: u64,
        nlink: u64,
    },
}

/// Serialization-friendly snapshot of MemFs state.
///
/// Preserves the inode table directly so that hard links (multiple paths
/// pointing to the same inode) survive a serialization round-trip.
#[derive(Serialize, Deserialize)]
struct MemFsSnapshot {
    /// Map of inode number -> serialized inode data.
    inodes: Vec<(u64, InodeSnapshot)>,
    /// Map of path -> inode number (for all non-root directory entries).
    paths: Vec<(String, u64)>,
    /// Scalar fields.
    next_ino: u64,
    root_ino: u64,
    current_uid: u32,
    current_gid: u32,
    supplementary_gids: Vec<u32>,
    time: u64,
    umask: u16,
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn inode_to_snapshot(inode: &Inode) -> InodeSnapshot {
    match &inode.kind {
        InodeKind::File { content } => InodeSnapshot::File {
            content: content.clone(),
            mode: inode.mode,
            uid: inode.uid,
            gid: inode.gid,
            mtime: inode.mtime,
            ctime: inode.ctime,
            atime: inode.atime,
            nlink: inode.nlink,
        },
        InodeKind::Dir { .. } => InodeSnapshot::Dir {
            mode: inode.mode,
            uid: inode.uid,
            gid: inode.gid,
            mtime: inode.mtime,
            ctime: inode.ctime,
            atime: inode.atime,
            nlink: inode.nlink,
        },
        InodeKind::Symlink { target } => InodeSnapshot::Symlink {
            target: target.clone(),
            uid: inode.uid,
            gid: inode.gid,
            mtime: inode.mtime,
            ctime: inode.ctime,
            atime: inode.atime,
            nlink: inode.nlink,
        },
    }
}

fn snapshot_to_inode(snap: InodeSnapshot) -> Inode {
    match snap {
        InodeSnapshot::File {
            content,
            mode,
            uid,
            gid,
            mtime,
            ctime,
            atime,
            nlink,
        } => Inode {
            kind: InodeKind::File { content },
            mode,
            uid,
            gid,
            mtime,
            ctime,
            atime,
            nlink,
        },
        InodeSnapshot::Dir {
            mode,
            uid,
            gid,
            mtime,
            ctime,
            atime,
            nlink,
        } => Inode {
            kind: InodeKind::Dir {
                children: BTreeMap::new(),
            },
            mode,
            uid,
            gid,
            mtime,
            ctime,
            atime,
            nlink,
        },
        InodeSnapshot::Symlink {
            target,
            uid,
            gid,
            mtime,
            ctime,
            atime,
            nlink,
        } => Inode {
            kind: InodeKind::Symlink { target },
            mode: 0o777,
            uid,
            gid,
            mtime,
            ctime,
            atime,
            nlink,
        },
    }
}

/// Walk the directory tree collecting (path, ino) pairs for all entries
/// (excluding the root itself).
fn collect_path_ino_pairs(
    inodes: &BTreeMap<u64, Inode>,
    ino: u64,
    prefix: &str,
    out: &mut Vec<(String, u64)>,
) {
    if let Some(inode) = inodes.get(&ino) {
        if let InodeKind::Dir { children } = &inode.kind {
            for (name, child_ino) in children {
                let child_path = if prefix == "/" {
                    alloc::format!("/{}", name)
                } else {
                    alloc::format!("{}/{}", prefix, name)
                };
                out.push((child_path.clone(), *child_ino));
                collect_path_ino_pairs(inodes, *child_ino, &child_path, out);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Serialize / Deserialize impls
// ---------------------------------------------------------------------------

impl Serialize for MemFs {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        // Collect all inodes
        let inode_snapshots: Vec<(u64, InodeSnapshot)> = self
            .inodes
            .iter()
            .map(|(ino, inode)| (*ino, inode_to_snapshot(inode)))
            .collect();

        // Collect path -> ino mappings (excluding root itself)
        let mut path_ino_pairs = Vec::new();
        collect_path_ino_pairs(&self.inodes, self.root_ino, "/", &mut path_ino_pairs);

        let snapshot = MemFsSnapshot {
            inodes: inode_snapshots,
            paths: path_ino_pairs,
            next_ino: self.next_ino,
            root_ino: self.root_ino,
            current_uid: self.current_uid(),
            current_gid: self.current_gid(),
            supplementary_gids: self.supplementary_gids().to_vec(),
            time: self.time(),
            umask: self.umask(),
        };
        snapshot.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for MemFs {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let snapshot = MemFsSnapshot::deserialize(deserializer)?;

        // Rebuild the inode table
        let mut inodes = BTreeMap::new();
        for (ino, snap) in snapshot.inodes {
            inodes.insert(ino, snapshot_to_inode(snap));
        }

        // Rebuild directory children maps from the path -> ino pairs.
        // Paths are in DFS order (parent before children), so we can process
        // them sequentially.
        for (path, child_ino) in &snapshot.paths {
            let components = split_path(path);
            if components.is_empty() {
                continue;
            }
            let leaf = components.last().unwrap().to_string();

            // Find the parent inode number by walking the path
            let parent_ino = if components.len() == 1 {
                snapshot.root_ino
            } else {
                let mut ino = snapshot.root_ino;
                for comp in &components[..components.len() - 1] {
                    if let Some(inode) = inodes.get(&ino) {
                        if let InodeKind::Dir { children } = &inode.kind {
                            if let Some(next) = children.get(*comp) {
                                ino = *next;
                            } else {
                                break;
                            }
                        }
                    }
                }
                ino
            };

            // Insert into parent's children
            if let Some(parent) = inodes.get_mut(&parent_ino) {
                if let InodeKind::Dir { children } = &mut parent.kind {
                    children.insert(leaf, *child_ino);
                }
            }
        }

        // Validate root_ino before constructing MemFs
        let root_inode = inodes
            .get(&snapshot.root_ino)
            .ok_or_else(|| serde::de::Error::custom("root inode missing from inode table"))?;
        if !root_inode.is_dir() {
            return Err(serde::de::Error::custom("root inode is not a directory"));
        }

        let fs = MemFs::from_raw_parts(
            inodes,
            snapshot.next_ino,
            snapshot.root_ino,
            snapshot.current_uid,
            snapshot.current_gid,
            snapshot.supplementary_gids,
            snapshot.time,
            snapshot.umask,
        );

        Ok(fs)
    }
}
