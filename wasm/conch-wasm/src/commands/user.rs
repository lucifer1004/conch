use crate::shell::Shell;

impl Shell {
    /// `useradd [-m] [-d HOME] [-s SHELL] [-u UID] [-g GID] NAME` — create a new user account and home directory.
    pub fn cmd_useradd(&mut self, args: &[String]) -> (String, i32) {
        let mut home_opt: Option<String> = None;
        let mut shell_opt: Option<String> = None;
        let mut uid_opt: Option<u32> = None;
        let mut gid_opt: Option<u32> = None;
        let mut username: Option<String> = None;

        let mut i = 0;
        while i < args.len() {
            match args[i].as_str() {
                "-m" => {
                    i += 1;
                }
                "-d" if i + 1 < args.len() => {
                    home_opt = Some(args[i + 1].clone());
                    i += 2;
                }
                "-s" if i + 1 < args.len() => {
                    shell_opt = Some(args[i + 1].clone());
                    i += 2;
                }
                "-u" if i + 1 < args.len() => {
                    uid_opt = args[i + 1].parse::<u32>().ok();
                    i += 2;
                }
                "-g" if i + 1 < args.len() => {
                    gid_opt = self.ident.users.resolve_gid(&args[i + 1]);
                    i += 2;
                }
                s if !s.starts_with('-') => {
                    username = Some(s.to_string());
                    i += 1;
                }
                _ => {
                    i += 1;
                }
            }
        }

        let name = match username {
            Some(n) => n,
            None => return ("useradd: no username specified".to_string(), 1),
        };

        if self.ident.users.get_user_by_name(&name).is_some() {
            return (format!("useradd: user '{}' already exists", name), 9);
        }

        let home = home_opt.unwrap_or_else(|| format!("/home/{}", name));

        let uid = match (uid_opt, gid_opt) {
            (Some(uid), Some(gid)) => self.ident.users.add_user_with_ids(&name, uid, gid, &home),
            (Some(uid), None) => {
                let gid = self.ident.users.add_group(&name);
                self.ident.users.add_user_with_ids(&name, uid, gid, &home)
            }
            (None, Some(_gid)) => {
                // auto uid; for this sim just use auto gid as well
                self.ident.users.add_user(&name, &home)
            }
            (None, None) => self.ident.users.add_user(&name, &home),
        };

        if let Some(shell) = shell_opt {
            self.ident.users.set_user_shell(&name, shell);
        }

        // Create home directory with correct ownership.
        // Temporarily run as root so we can write to /home (owned by root).
        let saved_uid = self.fs.current_uid();
        let saved_gid = self.fs.current_gid();
        self.fs.set_current_user(0, 0);
        let _ = self.fs.create_dir_all(&home);
        let _ = self.fs.chown(
            &home,
            uid,
            self.ident
                .users
                .get_user_by_uid(uid)
                .map(|u| u.gid)
                .unwrap_or(uid),
        );
        self.fs.set_current_user(saved_uid, saved_gid);

        (String::new(), 0)
    }

    /// `groupadd [-g GID] NAME` — create a new group, optionally with an explicit GID.
    pub fn cmd_groupadd(&mut self, args: &[String]) -> (String, i32) {
        let mut groupname: Option<String> = None;
        let mut gid_opt: Option<u32> = None;

        let mut i = 0;
        while i < args.len() {
            match args[i].as_str() {
                "-g" if i + 1 < args.len() => {
                    gid_opt = args[i + 1].parse::<u32>().ok();
                    i += 2;
                }
                s if !s.starts_with('-') => {
                    groupname = Some(s.to_string());
                    i += 1;
                }
                _ => {
                    i += 1;
                }
            }
        }

        let name = match groupname {
            Some(n) => n,
            None => return ("groupadd: no group name specified".to_string(), 1),
        };

        if self.ident.users.get_group_by_name(&name).is_some() {
            return (format!("groupadd: group '{}' already exists", name), 9);
        }

        if let Some(gid) = gid_opt {
            self.ident.users.add_group_with_id(&name, gid);
        } else {
            self.ident.users.add_group(&name);
        }

        (String::new(), 0)
    }

    /// `userdel [-r] NAME` — remove a user account; `-r` also deletes the home directory.
    pub fn cmd_userdel(&mut self, args: &[String]) -> (String, i32) {
        let mut remove_home = false;
        let mut username: Option<String> = None;

        for arg in args {
            match arg.as_str() {
                "-r" => remove_home = true,
                s if !s.starts_with('-') => username = Some(s.to_string()),
                _ => {}
            }
        }

        let name = match username {
            Some(n) => n,
            None => return ("userdel: no username specified".to_string(), 1),
        };

        let entry = match self.ident.users.remove_user(&name) {
            Some(e) => e,
            None => return (format!("userdel: user '{}' does not exist", name), 6),
        };

        if remove_home {
            // Temporarily run as root to remove home dir from /home (owned by root).
            let saved_uid = self.fs.current_uid();
            let saved_gid = self.fs.current_gid();
            self.fs.set_current_user(0, 0);
            let _ = self.fs.remove_dir_all(&entry.home);
            self.fs.set_current_user(saved_uid, saved_gid);
        }

        (String::new(), 0)
    }

    /// `usermod [-a] [-G GROUP,...] NAME` — modify a user account; `-aG` appends supplementary groups.
    pub fn cmd_usermod(&mut self, args: &[String]) -> (String, i32) {
        // Parse -a -G group user
        let mut append = false;
        let mut groups: Vec<String> = Vec::new();
        let mut username: Option<String> = None;

        let mut i = 0;
        while i < args.len() {
            let arg = args[i].as_str();
            // Handle combined flags like -aG and -Ga
            if arg.starts_with('-') && arg.len() > 1 {
                let flags = &arg[1..];
                if flags.contains('a') {
                    append = true;
                }
                if flags.contains('G') && i + 1 < args.len() {
                    groups = args[i + 1].split(',').map(|s| s.to_string()).collect();
                    i += 2;
                    continue;
                }
                i += 1;
            } else if !arg.starts_with('-') {
                username = Some(arg.to_string());
                i += 1;
            } else {
                i += 1;
            }
        }

        let name = match username {
            Some(n) => n,
            None => return ("usermod: no username specified".to_string(), 1),
        };

        if self.ident.users.get_user_by_name(&name).is_none() {
            return (format!("usermod: user '{}' does not exist", name), 6);
        }

        if !append && !groups.is_empty() {
            self.ident
                .users
                .remove_user_from_supplementary_groups(&name);
        }

        for group in &groups {
            if let Err(e) = self.ident.users.add_user_to_group(&name, group) {
                return (e, 6);
            }
        }

        (String::new(), 0)
    }

    /// `su [-] [-l] [-c CMD] [USER]` — switch to another user identity; `-c CMD` runs a single command and restores the caller.
    pub fn cmd_su(&mut self, args: &[String]) -> (String, i32) {
        let mut login_shell = false;
        let mut target_user: Option<String> = None;
        let mut run_command: Option<String> = None;

        let mut i = 0;
        while i < args.len() {
            match args[i].as_str() {
                "-" | "-l" => {
                    login_shell = true;
                    i += 1;
                }
                "-c" => {
                    if i + 1 < args.len() {
                        run_command = Some(args[i + 1].clone());
                        i += 2;
                    } else {
                        i += 1;
                    }
                }
                s if !s.starts_with('-') => {
                    target_user = Some(s.to_string());
                    i += 1;
                }
                _ => {
                    i += 1;
                }
            }
        }

        let uname = target_user.unwrap_or_else(|| "root".to_string());

        let (uid, gid, home) = match self.ident.users.get_user_by_name(&uname) {
            Some(u) => (u.uid, u.gid, u.home.clone()),
            None => return (format!("su: user {} does not exist", uname), 1),
        };

        // su -c COMMAND: run command as target user, then restore identity
        if let Some(cmd) = run_command {
            let saved_uid = self.fs.current_uid();
            let saved_gid = self.fs.current_gid();
            let saved_groups: Vec<u32> = self.fs.supplementary_gids().to_vec();
            let saved_user = self.ident.user.clone();

            let sup_gids: Vec<u32> = self
                .ident
                .users
                .user_groups(&uname)
                .iter()
                .map(|g| g.gid)
                .filter(|&g| g != gid)
                .collect();
            self.fs.set_identity(uid, gid, &sup_gids);
            self.ident.user = uname.clone().into();
            self.vars.env.insert("USER".into(), uname.clone());

            let (output, code, _) = self.run_line(&cmd);

            // Restore original identity
            self.fs.set_identity(saved_uid, saved_gid, &saved_groups);
            self.ident.user = saved_user;
            self.vars
                .env
                .insert("USER".into(), self.ident.user.to_string());

            return (output, code);
        }

        // Install full identity: uid, gid, and supplementary groups
        let sup_gids: Vec<u32> = self
            .ident
            .users
            .user_groups(&uname)
            .iter()
            .map(|g| g.gid)
            .filter(|&g| g != gid)
            .collect();
        self.fs.set_identity(uid, gid, &sup_gids);
        self.ident.user = uname.clone().into();
        self.vars.env.insert("USER".into(), uname.clone());

        if login_shell {
            self.cwd = home.clone().into();
            self.ident.home = home.clone().into();
            self.vars.env.insert("HOME".into(), home.clone());
            self.vars.env.insert("PWD".into(), home);
        }

        (String::new(), 0)
    }

    /// `sudo [-u USER] CMD [ARGS...]` — run a command as root (or as `USER` with `-u`), restoring the caller's identity afterward.
    pub fn cmd_sudo(&mut self, args: &[String]) -> (String, i32) {
        if args.is_empty() {
            return ("sudo: no command specified".to_string(), 1);
        }

        let saved_uid = self.fs.current_uid();
        let saved_gid = self.fs.current_gid();
        let saved_groups: Vec<u32> = self.fs.supplementary_gids().to_vec();
        let saved_user = self.ident.user.clone();

        // Parse -u USER flag
        let mut target_user: Option<String> = None;
        let mut cmd_start = 0;
        {
            let mut i = 0;
            while i < args.len() {
                match args[i].as_str() {
                    "-u" if i + 1 < args.len() => {
                        target_user = Some(args[i + 1].clone());
                        i += 2;
                        cmd_start = i;
                    }
                    s if s.starts_with('-') => {
                        i += 1;
                        cmd_start = i;
                    }
                    _ => {
                        cmd_start = i;
                        break;
                    }
                }
            }
        }

        let cmd_args = &args[cmd_start..];
        if cmd_args.is_empty() {
            return ("sudo: no command specified".to_string(), 1);
        }

        // Resolve target identity
        if let Some(ref uname) = target_user {
            match self.ident.users.get_user_by_name(uname) {
                Some(u) => {
                    let sup_gids: Vec<u32> = self
                        .ident
                        .users
                        .user_groups(uname)
                        .iter()
                        .map(|g| g.gid)
                        .filter(|&g| g != u.gid)
                        .collect();
                    self.fs.set_identity(u.uid, u.gid, &sup_gids);
                    self.ident.user = uname.clone().into();
                }
                None => {
                    return (format!("sudo: unknown user: {}", uname), 1);
                }
            }
        } else {
            // Default: elevate to root
            self.fs.set_identity(0, 0, &[]);
            self.ident.user = "root".into();
        }

        let cmd = &cmd_args[0];
        let rest = &cmd_args[1..];
        let line = if rest.is_empty() {
            cmd.clone()
        } else {
            format!("{} {}", cmd, rest.join(" "))
        };

        let (output, code, _) = self.run_line(&line);

        // Restore full identity
        self.fs.set_identity(saved_uid, saved_gid, &saved_groups);
        self.ident.user = saved_user;

        (output, code)
    }

    /// `passwd [USER]` — stub: always reports a successful password update.
    pub fn cmd_passwd(&mut self, _args: &[String]) -> (String, i32) {
        ("passwd: password updated successfully".to_string(), 0)
    }
}
