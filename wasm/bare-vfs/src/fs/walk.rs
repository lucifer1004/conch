use alloc::collections::btree_map;
use alloc::string::String;
use alloc::vec::Vec;

use crate::entry::EntryRef;

use super::node::TreeNode;

/// Depth-first iterator over filesystem entries.
///
/// Yields `(path, entry_ref)` pairs in DFS order (parent before children,
/// children sorted by name).
#[derive(Debug)]
pub struct Walk<'a> {
    /// Stack of (path_prefix, children_iterator) frames.
    /// When we enter a directory, we push its children iterator onto the stack.
    stack: Vec<(String, btree_map::Iter<'a, String, TreeNode>)>,
    /// The next item to yield (set when we first encounter a node).
    pending: Option<(String, EntryRef<'a>)>,
}

impl<'a> Walk<'a> {
    /// Create a new Walk starting at the given node with the given path prefix.
    pub(crate) fn new(root: &'a TreeNode, prefix: String) -> Self {
        let entry_ref = root.as_entry_ref();
        let mut walk = Walk {
            stack: Vec::new(),
            pending: Some((prefix.clone(), entry_ref)),
        };
        // If the root is a directory, push its children onto the stack.
        if let TreeNode::Dir { children, .. } = root {
            walk.stack.push((prefix, children.iter()));
        }
        walk
    }

    /// Create an empty Walk that yields nothing.
    pub(crate) fn empty() -> Self {
        Walk {
            stack: Vec::new(),
            pending: None,
        }
    }
}

impl<'a> Iterator for Walk<'a> {
    type Item = (String, EntryRef<'a>);

    fn next(&mut self) -> Option<Self::Item> {
        // If we have a pending item from initialization or a previous step, yield it.
        if let Some(item) = self.pending.take() {
            return Some(item);
        }

        // Pop from the stack until we find the next child.
        while let Some((parent_path, iter)) = self.stack.last_mut() {
            if let Some((name, node)) = iter.next() {
                let child_path = if parent_path == "/" {
                    alloc::format!("/{}", name)
                } else {
                    alloc::format!("{}/{}", parent_path, name)
                };
                let entry_ref = node.as_entry_ref();

                // If this child is a directory, push its children for later traversal.
                if let TreeNode::Dir { children, .. } = node {
                    self.stack.push((child_path.clone(), children.iter()));
                }

                return Some((child_path, entry_ref));
            } else {
                // This directory's children are exhausted; pop it.
                self.stack.pop();
            }
        }

        None
    }
}
