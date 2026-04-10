use alloc::collections::btree_map;
use alloc::string::String;

use crate::dir::DirEntry;

use super::node::TreeNode;

/// Iterator over directory entries, yielding [`DirEntry`] values.
#[derive(Debug)]
pub struct ReadDirIter<'a> {
    inner: btree_map::Iter<'a, String, TreeNode>,
}

impl<'a> ReadDirIter<'a> {
    pub(crate) fn new(iter: btree_map::Iter<'a, String, TreeNode>) -> Self {
        ReadDirIter { inner: iter }
    }
}

impl<'a> Iterator for ReadDirIter<'a> {
    type Item = DirEntry;

    fn next(&mut self) -> Option<Self::Item> {
        let (name, node) = self.inner.next()?;
        Some(DirEntry {
            name: name.clone(),
            is_dir: node.is_dir(),
            is_symlink: node.is_symlink(),
            mode: node.mode(),
            mtime: node.mtime(),
            size: match node {
                TreeNode::File { content, .. } => content.len(),
                _ => 0,
            },
        })
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.inner.size_hint()
    }
}

impl<'a> ExactSizeIterator for ReadDirIter<'a> {}
