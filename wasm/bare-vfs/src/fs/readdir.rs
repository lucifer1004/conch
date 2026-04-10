use alloc::collections::btree_map;
use alloc::collections::BTreeMap;
use alloc::string::String;

use crate::dir::DirEntry;

use super::inode::{Inode, InodeKind};

/// Iterator over directory entries, yielding [`DirEntry`] values.
#[derive(Debug)]
pub struct ReadDirIter<'a> {
    inodes: &'a BTreeMap<u64, Inode>,
    inner: btree_map::Iter<'a, String, u64>,
}

impl<'a> ReadDirIter<'a> {
    pub(crate) fn new(
        inodes: &'a BTreeMap<u64, Inode>,
        iter: btree_map::Iter<'a, String, u64>,
    ) -> Self {
        ReadDirIter {
            inodes,
            inner: iter,
        }
    }
}

impl<'a> Iterator for ReadDirIter<'a> {
    type Item = DirEntry;

    fn next(&mut self) -> Option<Self::Item> {
        let (name, ino) = self.inner.next()?;
        let inode = self.inodes.get(ino)?; // returns None if dangling, ending iteration
        Some(DirEntry {
            name: name.clone(),
            is_dir: inode.is_dir(),
            is_symlink: inode.is_symlink(),
            mode: inode.mode(),
            mtime: inode.mtime(),
            size: match &inode.kind {
                InodeKind::File { content } => content.len(),
                _ => 0,
            },
            ino: *ino,
        })
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.inner.size_hint()
    }
}

impl<'a> ExactSizeIterator for ReadDirIter<'a> {}
