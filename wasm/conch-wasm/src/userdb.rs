use std::collections::BTreeMap;

pub struct UserEntry {
    pub uid: u32,
    pub name: String,
    pub gid: u32,
    pub home: String,
    pub shell: String,
}

pub struct GroupEntry {
    pub gid: u32,
    pub name: String,
    pub members: Vec<String>,
}

pub struct UserDb {
    users: BTreeMap<u32, UserEntry>,
    groups: BTreeMap<u32, GroupEntry>,
    name_to_uid: BTreeMap<String, u32>,
    name_to_gid: BTreeMap<String, u32>,
    next_uid: u32,
    next_gid: u32,
}

impl UserDb {
    pub fn new() -> Self {
        UserDb {
            users: BTreeMap::new(),
            groups: BTreeMap::new(),
            name_to_uid: BTreeMap::new(),
            name_to_gid: BTreeMap::new(),
            next_uid: 1000,
            next_gid: 1000,
        }
    }

    /// Add root user (uid=0, gid=0, home=/root)
    pub fn add_root(&mut self) {
        let root_group = GroupEntry {
            gid: 0,
            name: "root".to_string(),
            members: vec!["root".to_string()],
        };
        self.groups.insert(0, root_group);
        self.name_to_gid.insert("root".to_string(), 0);

        let root_user = UserEntry {
            uid: 0,
            name: "root".to_string(),
            gid: 0,
            home: "/root".to_string(),
            shell: "/bin/sh".to_string(),
        };
        self.users.insert(0, root_user);
        self.name_to_uid.insert("root".to_string(), 0);
    }

    /// Add a regular user with auto-assigned uid, create primary group with same name.
    pub fn add_user(&mut self, name: &str, home: &str) -> u32 {
        let uid = self.next_uid;
        self.next_uid += 1;
        let gid = self.add_group(name);
        self.add_user_with_ids(name, uid, gid, home)
    }

    /// Add a user with specific uid/gid.
    pub fn add_user_with_ids(&mut self, name: &str, uid: u32, gid: u32, home: &str) -> u32 {
        // Ensure the primary group exists
        if self.groups.get(&gid).is_none() {
            let group = GroupEntry {
                gid,
                name: name.to_string(),
                members: vec![name.to_string()],
            };
            self.groups.insert(gid, group);
            self.name_to_gid.insert(name.to_string(), gid);
        } else if let Some(g) = self.groups.get_mut(&gid) {
            if !g.members.contains(&name.to_string()) {
                g.members.push(name.to_string());
            }
        }

        let entry = UserEntry {
            uid,
            name: name.to_string(),
            gid,
            home: home.to_string(),
            shell: "/bin/sh".to_string(),
        };
        self.users.insert(uid, entry);
        self.name_to_uid.insert(name.to_string(), uid);

        // Update next_uid if needed
        if uid >= self.next_uid {
            self.next_uid = uid + 1;
        }
        uid
    }

    /// Return the next available uid without consuming it.
    pub fn next_available_uid(&self) -> u32 {
        self.next_uid
    }

    /// Add a group. Returns the gid.
    pub fn add_group(&mut self, name: &str) -> u32 {
        if let Some(&existing) = self.name_to_gid.get(name) {
            return existing;
        }
        let gid = self.next_gid;
        self.next_gid += 1;
        let group = GroupEntry {
            gid,
            name: name.to_string(),
            members: Vec::new(),
        };
        self.groups.insert(gid, group);
        self.name_to_gid.insert(name.to_string(), gid);
        gid
    }

    /// Add a group with a specific gid. Returns the gid.
    pub fn add_group_with_id(&mut self, name: &str, gid: u32) -> u32 {
        if let Some(&existing) = self.name_to_gid.get(name) {
            return existing;
        }
        let group = GroupEntry {
            gid,
            name: name.to_string(),
            members: Vec::new(),
        };
        self.groups.insert(gid, group);
        self.name_to_gid.insert(name.to_string(), gid);
        if gid >= self.next_gid {
            self.next_gid = gid + 1;
        }
        gid
    }

    /// Add user to a supplementary group.
    pub fn add_user_to_group(&mut self, username: &str, groupname: &str) -> Result<(), String> {
        if !self.name_to_uid.contains_key(username) {
            return Err(format!("usermod: user '{}' does not exist", username));
        }
        let gid = match self.name_to_gid.get(groupname) {
            Some(&g) => g,
            None => return Err(format!("usermod: group '{}' does not exist", groupname)),
        };
        if let Some(g) = self.groups.get_mut(&gid) {
            if !g.members.contains(&username.to_string()) {
                g.members.push(username.to_string());
            }
        }
        Ok(())
    }

    /// Remove a user by name.
    pub fn remove_user(&mut self, name: &str) -> Option<UserEntry> {
        let uid = self.name_to_uid.remove(name)?;
        let entry = self.users.remove(&uid)?;
        // Remove from all groups
        for g in self.groups.values_mut() {
            g.members.retain(|m| m != name);
        }
        Some(entry)
    }

    pub fn get_user_by_name(&self, name: &str) -> Option<&UserEntry> {
        let uid = self.name_to_uid.get(name)?;
        self.users.get(uid)
    }

    pub fn get_user_by_uid(&self, uid: u32) -> Option<&UserEntry> {
        self.users.get(&uid)
    }

    pub fn get_group_by_name(&self, name: &str) -> Option<&GroupEntry> {
        let gid = self.name_to_gid.get(name)?;
        self.groups.get(gid)
    }

    pub fn get_group_by_gid(&self, gid: u32) -> Option<&GroupEntry> {
        self.groups.get(&gid)
    }

    /// Try parse as number first, then lookup by name.
    pub fn resolve_uid(&self, name_or_id: &str) -> Option<u32> {
        if let Ok(n) = name_or_id.parse::<u32>() {
            return Some(n);
        }
        self.name_to_uid.get(name_or_id).copied()
    }

    pub fn resolve_gid(&self, name_or_id: &str) -> Option<u32> {
        if let Ok(n) = name_or_id.parse::<u32>() {
            return Some(n);
        }
        self.name_to_gid.get(name_or_id).copied()
    }

    pub fn uid_to_name(&self, uid: u32) -> String {
        self.users
            .get(&uid)
            .map(|u| u.name.clone())
            .unwrap_or_else(|| uid.to_string())
    }

    pub fn gid_to_name(&self, gid: u32) -> String {
        self.groups
            .get(&gid)
            .map(|g| g.name.clone())
            .unwrap_or_else(|| gid.to_string())
    }

    /// Get all groups a user belongs to (primary + supplementary).
    pub fn user_groups(&self, username: &str) -> Vec<&GroupEntry> {
        let primary_gid = self.get_user_by_name(username).map(|u| u.gid);

        self.groups
            .values()
            .filter(|g| {
                g.members.contains(&username.to_string())
                    || primary_gid.map_or(false, |pgid| g.gid == pgid)
            })
            .collect()
    }
}
