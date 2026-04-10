use alloc::collections::btree_map;
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;

use crate::entry::EntryRef;

use super::inode::{Inode, InodeKind};

/// A shared empty BTreeMap used by `Walk::empty()`. This must be `static`
/// (not `const`) because the `Walk` struct holds a `&'a` reference to it,
/// and a `const` value has no fixed address to borrow from.
static EMPTY_INODES: BTreeMap<u64, Inode> = BTreeMap::new();

/// Depth-first iterator over filesystem entries.
///
/// Yields `(path, entry_ref)` pairs in DFS order (parent before children,
/// children sorted by name).
#[derive(Debug)]
pub struct Walk<'a> {
    inodes: &'a BTreeMap<u64, Inode>,
    /// Stack of (path_prefix, children_iterator) frames.
    stack: Vec<(String, btree_map::Iter<'a, String, u64>)>,
    /// The next item to yield (set when we first encounter a node).
    pending: Option<(String, EntryRef<'a>)>,
}

impl<'a> Walk<'a> {
    /// Create a new Walk starting at the given inode with the given path prefix.
    pub(crate) fn new(inodes: &'a BTreeMap<u64, Inode>, ino: u64, prefix: String) -> Self {
        let inode = match inodes.get(&ino) {
            Some(i) => i,
            None => {
                return Walk {
                    inodes,
                    stack: Vec::new(),
                    pending: None,
                }
            }
        };
        let entry_ref = inode.as_entry_ref();
        let mut walk = Walk {
            inodes,
            stack: Vec::new(),
            pending: Some((prefix.clone(), entry_ref)),
        };
        if let InodeKind::Dir { children } = &inode.kind {
            walk.stack.push((prefix, children.iter()));
        }
        walk
    }

    /// Create an empty Walk that yields nothing.
    pub(crate) fn empty() -> Self {
        Walk {
            inodes: &EMPTY_INODES,
            stack: Vec::new(),
            pending: None,
        }
    }
}

impl<'a> Iterator for Walk<'a> {
    type Item = (String, EntryRef<'a>);

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(item) = self.pending.take() {
            return Some(item);
        }

        while let Some((parent_path, iter)) = self.stack.last_mut() {
            if let Some((name, child_ino)) = iter.next() {
                let child_path = if parent_path == "/" {
                    alloc::format!("/{}", name)
                } else {
                    alloc::format!("{}/{}", parent_path, name)
                };

                if let Some(child_inode) = self.inodes.get(child_ino) {
                    let entry_ref = child_inode.as_entry_ref();

                    if let InodeKind::Dir { children } = &child_inode.kind {
                        self.stack.push((child_path.clone(), children.iter()));
                    }

                    return Some((child_path, entry_ref));
                }
            } else {
                self.stack.pop();
            }
        }

        None
    }
}
