use alloc::collections::BTreeMap;
use alloc::string::String;

use super::node::{split_path, TreeNode, MAX_SYMLINK_DEPTH};
use super::MemFs;

impl MemFs {
    /// Permission check: `mask` is 3-bit (bit2=read, bit1=write, bit0=execute).
    /// Root (uid==0) always passes.
    pub(crate) fn check_permission(&self, node: &TreeNode, mask: u16) -> bool {
        if self.current_uid == 0 {
            return true;
        }
        let (uid, gid, mode) = node.ownership_and_mode();
        if self.current_uid == uid {
            return mode & (mask << 6) != 0;
        }
        if self.current_gid == gid || self.supplementary_gids.contains(&gid) {
            return mode & (mask << 3) != 0;
        }
        mode & mask != 0
    }

    // -- Internal traversal -------------------------------------------------

    /// Resolve a symlink target to an absolute path given the directory
    /// containing the symlink.
    fn resolve_symlink_target(target: &str, symlink_dir: &str) -> String {
        if target.starts_with('/') {
            crate::normalize(target)
        } else {
            let base = alloc::format!("{}/{}", symlink_dir, target);
            crate::normalize(&base)
        }
    }

    /// Traverse to `path`, following symlinks transparently (including in
    /// intermediate components). Returns `None` if the path does not exist
    /// or a symlink loop / depth limit is exceeded.
    pub(crate) fn traverse(&self, path: &str) -> Option<&TreeNode> {
        self.traverse_with_depth(path, 0)
    }

    fn traverse_with_depth(&self, path: &str, depth: usize) -> Option<&TreeNode> {
        if depth > MAX_SYMLINK_DEPTH {
            return None;
        }
        if path.is_empty() {
            return None;
        }
        if path == "/" {
            return Some(&self.root);
        }

        let components = split_path(path);
        let mut node = &self.root;
        let mut current_dir = String::from("/");

        for (i, component) in components.iter().enumerate() {
            match node {
                TreeNode::Dir { children, .. } => {
                    node = children.get(*component)?;
                    if current_dir == "/" {
                        current_dir = alloc::format!("/{}", component);
                    } else {
                        current_dir = alloc::format!("{}/{}", current_dir, component);
                    }
                    if let TreeNode::Symlink { target, .. } = node {
                        let symlink_dir = crate::parent(&current_dir).unwrap_or("/");
                        let resolved = Self::resolve_symlink_target(target, symlink_dir);
                        let remaining = &components[i + 1..];
                        let full_path = if remaining.is_empty() {
                            resolved
                        } else {
                            let suffix = remaining.join("/");
                            alloc::format!("{}/{}", resolved, suffix)
                        };
                        return self.traverse_with_depth(&full_path, depth + 1);
                    }
                }
                TreeNode::File { .. } | TreeNode::Symlink { .. } => return None,
            }
        }
        if let TreeNode::Symlink { target, .. } = node {
            let symlink_dir = crate::parent(&current_dir).unwrap_or("/");
            let resolved = Self::resolve_symlink_target(target, symlink_dir);
            return self.traverse_with_depth(&resolved, depth + 1);
        }
        Some(node)
    }

    /// Traverse to `path` WITHOUT following the final symlink component.
    /// Intermediate symlinks in directory components are still followed.
    pub(crate) fn traverse_nofollow(&self, path: &str) -> Option<&TreeNode> {
        self.traverse_nofollow_with_depth(path, 0)
    }

    fn traverse_nofollow_with_depth(&self, path: &str, depth: usize) -> Option<&TreeNode> {
        if depth > MAX_SYMLINK_DEPTH {
            return None;
        }
        if path.is_empty() {
            return None;
        }
        if path == "/" {
            return Some(&self.root);
        }

        let components = split_path(path);
        let mut node = &self.root;
        let mut current_dir = String::from("/");

        for (i, component) in components.iter().enumerate() {
            let is_last = i == components.len() - 1;
            match node {
                TreeNode::Dir { children, .. } => {
                    node = children.get(*component)?;
                    if current_dir == "/" {
                        current_dir = alloc::format!("/{}", component);
                    } else {
                        current_dir = alloc::format!("{}/{}", current_dir, component);
                    }
                    if !is_last {
                        if let TreeNode::Symlink { target, .. } = node {
                            let symlink_dir = crate::parent(&current_dir).unwrap_or("/");
                            let resolved = Self::resolve_symlink_target(target, symlink_dir);
                            let remaining = &components[i + 1..];
                            let full_path = if remaining.is_empty() {
                                resolved
                            } else {
                                let suffix = remaining.join("/");
                                alloc::format!("{}/{}", resolved, suffix)
                            };
                            return self.traverse_nofollow_with_depth(&full_path, depth + 1);
                        }
                    }
                }
                TreeNode::File { .. } | TreeNode::Symlink { .. } => return None,
            }
        }
        Some(node)
    }

    pub(crate) fn traverse_mut(&mut self, path: &str) -> Option<&mut TreeNode> {
        self.traverse_mut_with_depth(path, 0)
    }

    fn traverse_mut_with_depth(&mut self, path: &str, depth: usize) -> Option<&mut TreeNode> {
        if depth > MAX_SYMLINK_DEPTH {
            return None;
        }
        if path.is_empty() {
            return None;
        }
        if path == "/" {
            return Some(&mut self.root);
        }

        let resolved_path = self.resolve_path_following_symlinks(path, depth)?;
        if resolved_path == path {
            let mut node = &mut self.root;
            for component in split_path(&resolved_path) {
                match node {
                    TreeNode::Dir { children, .. } => {
                        node = children.get_mut(component)?;
                    }
                    TreeNode::File { .. } | TreeNode::Symlink { .. } => return None,
                }
            }
            Some(node)
        } else {
            self.traverse_mut_with_depth(&resolved_path, depth + 1)
        }
    }

    /// Resolve all symlinks in `path` and return the final canonical path,
    /// without returning a reference into the tree (avoids borrow issues).
    fn resolve_path_following_symlinks(&self, path: &str, depth: usize) -> Option<String> {
        if depth > MAX_SYMLINK_DEPTH {
            return None;
        }
        if path.is_empty() {
            return None;
        }
        if path == "/" {
            return Some("/".into());
        }

        let components = split_path(path);
        let mut node = &self.root;
        let mut current_dir = String::from("/");

        for (i, component) in components.iter().enumerate() {
            match node {
                TreeNode::Dir { children, .. } => {
                    node = children.get(*component)?;
                    if current_dir == "/" {
                        current_dir = alloc::format!("/{}", component);
                    } else {
                        current_dir = alloc::format!("{}/{}", current_dir, component);
                    }
                    if let TreeNode::Symlink { target, .. } = node {
                        let symlink_dir = crate::parent(&current_dir).unwrap_or("/");
                        let resolved = Self::resolve_symlink_target(target, symlink_dir);
                        let remaining = &components[i + 1..];
                        let full_path = if remaining.is_empty() {
                            resolved
                        } else {
                            let suffix = remaining.join("/");
                            alloc::format!("{}/{}", resolved, suffix)
                        };
                        return self.resolve_path_following_symlinks(&full_path, depth + 1);
                    }
                }
                TreeNode::File { .. } | TreeNode::Symlink { .. } => return None,
            }
        }
        Some(current_dir)
    }

    /// Get mutable access to the parent directory's children map and the leaf name.
    ///
    /// Follows symlinks in intermediate components. Uses a raw-pointer trick to
    /// work around Rust's inability to return a mutable reference produced by
    /// chained reborrows.
    pub(crate) fn traverse_parent_mut<'a>(
        &'a mut self,
        path: &'a str,
    ) -> Option<(&'a mut BTreeMap<String, TreeNode>, &'a str)> {
        let components = split_path(path);
        if components.is_empty() {
            return None;
        }
        let (parents, leaf) = components.split_at(components.len() - 1);
        let leaf_name = leaf[0];

        if !parents.is_empty() {
            let raw_parent = alloc::format!("/{}", parents.join("/"));
            let resolved = self.resolve_path_following_symlinks(&raw_parent, 0);
            match resolved {
                Some(r) if r != raw_parent => {
                    let full = alloc::format!("{}/{}", r, leaf_name);
                    let rp_components = split_path(&r);
                    let mut node: *mut TreeNode = &mut self.root;
                    for component in &rp_components {
                        unsafe {
                            match &mut *node {
                                TreeNode::Dir { children, .. } => {
                                    node = children.get_mut(*component)? as *mut TreeNode;
                                }
                                TreeNode::File { .. } | TreeNode::Symlink { .. } => return None,
                            }
                        }
                    }
                    let _ = full;
                    unsafe {
                        match &mut *node {
                            TreeNode::Dir { children, .. } => return Some((children, leaf_name)),
                            TreeNode::File { .. } | TreeNode::Symlink { .. } => return None,
                        }
                    }
                }
                _ => {}
            }
        }

        let mut node: *mut TreeNode = &mut self.root;
        for component in parents {
            unsafe {
                match &mut *node {
                    TreeNode::Dir { children, .. } => {
                        node = children.get_mut(*component)? as *mut TreeNode;
                    }
                    TreeNode::File { .. } | TreeNode::Symlink { .. } => return None,
                }
            }
        }
        unsafe {
            match &mut *node {
                TreeNode::Dir { children, .. } => Some((children, leaf_name)),
                TreeNode::File { .. } | TreeNode::Symlink { .. } => None,
            }
        }
    }
}
