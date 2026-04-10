use alloc::string::String;
use alloc::vec::Vec;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::entry::Entry;

use super::MemFs;

/// Serialization-friendly snapshot of MemFs state.
#[derive(Serialize, Deserialize)]
struct MemFsSnapshot {
    entries: Vec<(String, Entry)>,
    current_uid: u32,
    current_gid: u32,
    supplementary_gids: Vec<u32>,
    time: u64,
    umask: u16,
}

impl Serialize for MemFs {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        // Use traverse_nofollow so symlinks are serialized as symlinks (not their targets).
        let entries: Vec<(String, Entry)> = self
            .paths()
            .into_iter()
            .filter(|path| path != "/") // skip root dir, it's implicit
            .filter_map(|path| {
                let node = self.traverse_nofollow(&path).ok()?;
                let entry = match node {
                    super::TreeNode::File {
                        content,
                        mode,
                        uid,
                        gid,
                        mtime,
                        ctime,
                        atime,
                    } => Entry::File {
                        content: content.clone(),
                        mode: *mode,
                        uid: *uid,
                        gid: *gid,
                        mtime: *mtime,
                        ctime: *ctime,
                        atime: *atime,
                    },
                    super::TreeNode::Dir {
                        mode,
                        uid,
                        gid,
                        mtime,
                        ctime,
                        atime,
                        ..
                    } => Entry::Dir {
                        mode: *mode,
                        uid: *uid,
                        gid: *gid,
                        mtime: *mtime,
                        ctime: *ctime,
                        atime: *atime,
                    },
                    super::TreeNode::Symlink {
                        target,
                        uid,
                        gid,
                        mtime,
                        ctime,
                        atime,
                    } => Entry::Symlink {
                        target: target.clone(),
                        uid: *uid,
                        gid: *gid,
                        mtime: *mtime,
                        ctime: *ctime,
                        atime: *atime,
                    },
                };
                Some((path, entry))
            })
            .collect();

        let snapshot = MemFsSnapshot {
            entries,
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
        let mut fs = MemFs::new();
        fs.set_umask(snapshot.umask);
        fs.set_current_user(snapshot.current_uid, snapshot.current_gid);
        for gid in &snapshot.supplementary_gids {
            fs.add_supplementary_gid(*gid);
        }

        // Insert entries using insert_raw to preserve timestamps exactly.
        // Paths from iter() are DFS order, so parents always come before children.
        for (path, entry) in snapshot.entries {
            // Ensure parent directories exist before inserting.
            if let Some(parent) = crate::parent(&path) {
                if parent != "/" && !fs.is_dir(parent) {
                    fs.create_dir_all(parent);
                }
            }
            fs.insert_raw(path, entry);
        }

        // Restore the clock to the serialized value (create_dir_all advances it).
        fs.set_time(snapshot.time);

        Ok(fs)
    }
}
