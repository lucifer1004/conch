use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};

use super::node::{split_path, TreeNode, MAX_SYMLINK_DEPTH};
use super::MemFs;
use crate::error::{VfsError, VfsErrorKind};

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
    /// intermediate components). Returns `Err` if the path does not exist,
    /// a symlink loop / depth limit is exceeded, or a permission check fails.
    pub(crate) fn traverse(&self, path: &str) -> Result<&TreeNode, VfsError> {
        if path.is_empty() {
            return Err(VfsErrorKind::NotFound.into());
        }
        let path = crate::normalize(path);
        self.traverse_with_depth(&path, 0)
    }

    fn traverse_with_depth(&self, path: &str, depth: usize) -> Result<&TreeNode, VfsError> {
        if depth > MAX_SYMLINK_DEPTH {
            return Err(VfsErrorKind::TooManySymlinks.into());
        }
        if path.is_empty() {
            return Err(VfsErrorKind::NotFound.into());
        }
        if path == "/" {
            return Ok(&self.root);
        }

        let components = split_path(path);
        let mut node = &self.root;
        let mut current_dir = String::from("/");

        for (i, component) in components.iter().enumerate() {
            match node {
                TreeNode::Dir { children, .. } => {
                    // Check execute permission on this directory before descending into it.
                    if !self.check_permission(node, 1) {
                        return Err(VfsErrorKind::PermissionDenied.into());
                    }
                    node = children
                        .get(*component)
                        .ok_or_else(|| VfsError::from(VfsErrorKind::NotFound))?;
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
                TreeNode::File { .. } | TreeNode::Symlink { .. } => {
                    return Err(VfsErrorKind::NotADirectory.into());
                }
            }
        }
        if let TreeNode::Symlink { target, .. } = node {
            let symlink_dir = crate::parent(&current_dir).unwrap_or("/");
            let resolved = Self::resolve_symlink_target(target, symlink_dir);
            return self.traverse_with_depth(&resolved, depth + 1);
        }
        Ok(node)
    }

    /// Traverse to `path` WITHOUT following the final symlink component.
    /// Intermediate symlinks in directory components are still followed.
    pub(crate) fn traverse_nofollow(&self, path: &str) -> Result<&TreeNode, VfsError> {
        if path.is_empty() {
            return Err(VfsErrorKind::NotFound.into());
        }
        let path = crate::normalize(path);
        self.traverse_nofollow_with_depth(&path, 0)
    }

    fn traverse_nofollow_with_depth(
        &self,
        path: &str,
        depth: usize,
    ) -> Result<&TreeNode, VfsError> {
        if depth > MAX_SYMLINK_DEPTH {
            return Err(VfsErrorKind::TooManySymlinks.into());
        }
        if path.is_empty() {
            return Err(VfsErrorKind::NotFound.into());
        }
        if path == "/" {
            return Ok(&self.root);
        }

        let components = split_path(path);
        let mut node = &self.root;
        let mut current_dir = String::from("/");

        for (i, component) in components.iter().enumerate() {
            let is_last = i == components.len() - 1;
            match node {
                TreeNode::Dir { children, .. } => {
                    // Check execute permission on this directory before descending into it.
                    if !self.check_permission(node, 1) {
                        return Err(VfsErrorKind::PermissionDenied.into());
                    }
                    node = children
                        .get(*component)
                        .ok_or_else(|| VfsError::from(VfsErrorKind::NotFound))?;
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
                TreeNode::File { .. } | TreeNode::Symlink { .. } => {
                    return Err(VfsErrorKind::NotADirectory.into());
                }
            }
        }
        Ok(node)
    }

    pub(crate) fn traverse_mut(&mut self, path: &str) -> Result<&mut TreeNode, VfsError> {
        if path.is_empty() {
            return Err(VfsErrorKind::NotFound.into());
        }
        let path = crate::normalize(path);
        self.traverse_mut_with_depth(&path, 0)
    }

    fn traverse_mut_with_depth(
        &mut self,
        path: &str,
        depth: usize,
    ) -> Result<&mut TreeNode, VfsError> {
        if depth > MAX_SYMLINK_DEPTH {
            return Err(VfsErrorKind::TooManySymlinks.into());
        }
        if path.is_empty() {
            return Err(VfsErrorKind::NotFound.into());
        }
        if path == "/" {
            return Ok(&mut self.root);
        }

        let resolved_path = self.resolve_path_following_symlinks(path, depth)?;
        if resolved_path == path {
            let mut node = &mut self.root;
            for component in split_path(&resolved_path) {
                match node {
                    TreeNode::Dir { children, .. } => {
                        node = children
                            .get_mut(component)
                            .ok_or_else(|| VfsError::from(VfsErrorKind::NotFound))?;
                    }
                    TreeNode::File { .. } | TreeNode::Symlink { .. } => {
                        return Err(VfsErrorKind::NotADirectory.into());
                    }
                }
            }
            Ok(node)
        } else {
            self.traverse_mut_with_depth(&resolved_path, depth + 1)
        }
    }

    /// Resolve all symlinks in `path` and return the final canonical path,
    /// without returning a reference into the tree (avoids borrow issues).
    pub(crate) fn resolve_path_following_symlinks(
        &self,
        path: &str,
        depth: usize,
    ) -> Result<String, VfsError> {
        if depth > MAX_SYMLINK_DEPTH {
            return Err(VfsErrorKind::TooManySymlinks.into());
        }
        // Normalize only at the entry point (depth == 0) to avoid redundant
        // work on already-resolved recursive calls.
        let normalized;
        let path = if depth == 0 {
            normalized = crate::normalize(path);
            normalized.as_str()
        } else {
            path
        };
        if path.is_empty() {
            return Err(VfsErrorKind::NotFound.into());
        }
        if path == "/" {
            return Ok("/".into());
        }

        let components = split_path(path);
        let mut node = &self.root;
        let mut current_dir = String::from("/");

        for (i, component) in components.iter().enumerate() {
            match node {
                TreeNode::Dir { children, .. } => {
                    node = children
                        .get(*component)
                        .ok_or_else(|| VfsError::from(VfsErrorKind::NotFound))?;
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
                TreeNode::File { .. } | TreeNode::Symlink { .. } => {
                    return Err(VfsErrorKind::NotADirectory.into());
                }
            }
        }
        Ok(current_dir)
    }

    /// Get mutable access to the parent directory's children map and the leaf name.
    ///
    /// Follows symlinks in intermediate components. Uses a raw-pointer trick to
    /// work around Rust's inability to return a mutable reference produced by
    /// chained reborrows.
    pub(crate) fn traverse_parent_mut<'a>(
        &'a mut self,
        path: &str,
    ) -> Option<(&'a mut BTreeMap<String, TreeNode>, String)> {
        let normalized = crate::normalize(path);
        let components = split_path(&normalized);
        if components.is_empty() {
            return None;
        }
        let (parents, leaf) = components.split_at(components.len() - 1);
        let leaf_name = leaf[0].to_string();

        if !parents.is_empty() {
            let raw_parent = alloc::format!("/{}", parents.join("/"));
            let resolved = self.resolve_path_following_symlinks(&raw_parent, 0);
            match resolved {
                Ok(r) if r != raw_parent => {
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
