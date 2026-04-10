use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};

use super::inode::{Inode, InodeKind, MAX_SYMLINK_DEPTH};
use super::MemFs;
use crate::error::{VfsError, VfsErrorKind};
use crate::path::split_path;

impl MemFs {
    /// Permission check: `mask` is 3-bit (bit2=read, bit1=write, bit0=execute).
    /// Root (uid==0) always passes.
    pub(crate) fn check_permission(&self, inode: &Inode, mask: u16) -> bool {
        if self.current_uid == 0 {
            return true;
        }
        let (uid, gid, mode) = inode.ownership_and_mode();
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
    /// intermediate components). Returns `(ino, &Inode)`.
    pub(crate) fn traverse(&self, path: &str) -> Result<(u64, &Inode), VfsError> {
        if path.is_empty() {
            return Err(VfsErrorKind::NotFound.into());
        }
        let path = crate::normalize(path);
        self.traverse_with_depth(&path, 0)
    }

    fn traverse_with_depth(&self, path: &str, depth: usize) -> Result<(u64, &Inode), VfsError> {
        if depth > MAX_SYMLINK_DEPTH {
            return Err(VfsErrorKind::TooManySymlinks.into());
        }
        if path.is_empty() {
            return Err(VfsErrorKind::NotFound.into());
        }
        if path == "/" {
            return Ok((self.root_ino, self.inodes.get(&self.root_ino).unwrap()));
        }

        let components = split_path(path);
        let mut current_ino = self.root_ino;
        let mut current_dir = String::from("/");

        for (i, component) in components.iter().enumerate() {
            let inode = self
                .inodes
                .get(&current_ino)
                .ok_or(VfsError::from(VfsErrorKind::NotFound))?;

            // Must be a directory to descend
            let children = match &inode.kind {
                InodeKind::Dir { children } => {
                    // Check execute permission on this directory
                    if !self.check_permission(inode, 1) {
                        return Err(VfsErrorKind::PermissionDenied.into());
                    }
                    children
                }
                _ => return Err(VfsErrorKind::NotADirectory.into()),
            };

            let child_ino = *children
                .get(*component)
                .ok_or(VfsError::from(VfsErrorKind::NotFound))?;
            let child = self
                .inodes
                .get(&child_ino)
                .ok_or(VfsError::from(VfsErrorKind::NotFound))?;

            // Update current_dir
            current_dir = if current_dir == "/" {
                alloc::format!("/{}", component)
            } else {
                alloc::format!("{}/{}", current_dir, component)
            };

            // Handle symlinks
            if let InodeKind::Symlink { target } = &child.kind {
                let symlink_dir = crate::parent(&current_dir).unwrap_or("/");
                let resolved = Self::resolve_symlink_target(target, symlink_dir);
                let remaining = &components[i + 1..];
                let full_path = if remaining.is_empty() {
                    resolved
                } else {
                    alloc::format!("{}/{}", resolved, remaining.join("/"))
                };
                return self.traverse_with_depth(&full_path, depth + 1);
            }

            current_ino = child_ino;
        }

        // Final: if symlink, follow it
        let final_inode = self.inodes.get(&current_ino).unwrap();
        if let InodeKind::Symlink { target } = &final_inode.kind {
            let symlink_dir = crate::parent(&current_dir).unwrap_or("/");
            let resolved = Self::resolve_symlink_target(target, symlink_dir);
            return self.traverse_with_depth(&resolved, depth + 1);
        }

        Ok((current_ino, self.inodes.get(&current_ino).unwrap()))
    }

    /// Traverse to `path` WITHOUT following the final symlink component.
    /// Intermediate symlinks in directory components are still followed.
    pub(crate) fn traverse_nofollow(&self, path: &str) -> Result<(u64, &Inode), VfsError> {
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
    ) -> Result<(u64, &Inode), VfsError> {
        if depth > MAX_SYMLINK_DEPTH {
            return Err(VfsErrorKind::TooManySymlinks.into());
        }
        if path.is_empty() {
            return Err(VfsErrorKind::NotFound.into());
        }
        if path == "/" {
            return Ok((self.root_ino, self.inodes.get(&self.root_ino).unwrap()));
        }

        let components = split_path(path);
        let mut current_ino = self.root_ino;
        let mut current_dir = String::from("/");

        for (i, component) in components.iter().enumerate() {
            let is_last = i == components.len() - 1;
            let inode = self
                .inodes
                .get(&current_ino)
                .ok_or(VfsError::from(VfsErrorKind::NotFound))?;

            let children = match &inode.kind {
                InodeKind::Dir { children } => {
                    if !self.check_permission(inode, 1) {
                        return Err(VfsErrorKind::PermissionDenied.into());
                    }
                    children
                }
                _ => return Err(VfsErrorKind::NotADirectory.into()),
            };

            let child_ino = *children
                .get(*component)
                .ok_or(VfsError::from(VfsErrorKind::NotFound))?;
            let child = self
                .inodes
                .get(&child_ino)
                .ok_or(VfsError::from(VfsErrorKind::NotFound))?;

            current_dir = if current_dir == "/" {
                alloc::format!("/{}", component)
            } else {
                alloc::format!("{}/{}", current_dir, component)
            };

            if !is_last {
                if let InodeKind::Symlink { target } = &child.kind {
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

            current_ino = child_ino;
        }

        Ok((current_ino, self.inodes.get(&current_ino).unwrap()))
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
        let mut current_ino = self.root_ino;
        let mut current_dir = String::from("/");

        for (i, component) in components.iter().enumerate() {
            let inode = self
                .inodes
                .get(&current_ino)
                .ok_or(VfsError::from(VfsErrorKind::NotFound))?;
            match &inode.kind {
                InodeKind::Dir { children } => {
                    let child_ino = *children
                        .get(*component)
                        .ok_or(VfsError::from(VfsErrorKind::NotFound))?;
                    let child = self
                        .inodes
                        .get(&child_ino)
                        .ok_or(VfsError::from(VfsErrorKind::NotFound))?;

                    if current_dir == "/" {
                        current_dir = alloc::format!("/{}", component);
                    } else {
                        current_dir = alloc::format!("{}/{}", current_dir, component);
                    }

                    if let InodeKind::Symlink { target } = &child.kind {
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

                    current_ino = child_ino;
                }
                _ => {
                    return Err(VfsErrorKind::NotADirectory.into());
                }
            }
        }
        Ok(current_dir)
    }

    /// Get mutable access to the parent directory's children map and the leaf name.
    ///
    /// Follows symlinks in intermediate components. First resolves the parent
    /// path using shared references to obtain the parent inode number, then
    /// performs a single `get_mut` to return the mutable children map.
    pub(crate) fn traverse_parent_mut<'a>(
        &'a mut self,
        path: &str,
    ) -> Option<(&'a mut BTreeMap<String, u64>, String)> {
        let normalized = crate::normalize(path);
        let components = split_path(&normalized);
        if components.is_empty() {
            return None; // root has no parent
        }
        let leaf_name = components.last().unwrap().to_string();

        // Build parent path
        let parent_path = if components.len() == 1 {
            "/".to_string()
        } else {
            alloc::format!("/{}", components[..components.len() - 1].join("/"))
        };

        // Resolve parent using shared refs to get the inode number
        let parent_ino = match self.resolve_path_following_symlinks(&parent_path, 0) {
            Ok(resolved) => {
                // Walk the resolved path to find the final inode number
                let resolved_components = split_path(&resolved);
                let mut ino = self.root_ino;
                for comp in &resolved_components {
                    let inode = self.inodes.get(&ino)?;
                    match &inode.kind {
                        InodeKind::Dir { children } => {
                            ino = *children.get(*comp)?;
                        }
                        _ => return None,
                    }
                }
                ino
            }
            Err(_) => return None,
        };

        // Single mutable access
        let parent_inode = self.inodes.get_mut(&parent_ino)?;
        match &mut parent_inode.kind {
            InodeKind::Dir { children } => Some((children, leaf_name)),
            _ => None,
        }
    }
}
