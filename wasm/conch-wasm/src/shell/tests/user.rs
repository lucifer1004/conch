use super::*;

#[test]
fn useradd_creates_user_and_home() {
    let mut s = shell();
    let (out, code, _) = s.run_line("useradd alice");
    assert_eq!(code, 0, "useradd failed: {:?}", out);
    assert!(
        s.ident.users.get_user_by_name("alice").is_some(),
        "alice not in UserDb"
    );
    assert!(s.fs.is_dir("/home/alice"), "home dir not created");
}

#[test]
fn adduser_alias_works() {
    let mut s = shell();
    let (out, code, _) = s.run_line("adduser bob");
    assert_eq!(code, 0, "adduser failed: {:?}", out);
    assert!(s.ident.users.get_user_by_name("bob").is_some());
}

#[test]
fn userdel_removes_user() {
    let mut s = shell();
    s.run_line("useradd charlie");
    let (out, code, _) = s.run_line("userdel charlie");
    assert_eq!(code, 0, "userdel failed: {:?}", out);
    assert!(
        s.ident.users.get_user_by_name("charlie").is_none(),
        "charlie still in UserDb"
    );
}

#[test]
fn userdel_with_r_removes_home() {
    let mut s = shell();
    s.run_line("useradd dave");
    assert!(s.fs.is_dir("/home/dave"));
    let (out, code, _) = s.run_line("userdel -r dave");
    assert_eq!(code, 0, "userdel -r failed: {:?}", out);
    assert!(!s.fs.is_dir("/home/dave"), "home dir still exists");
}

#[test]
fn groupadd_creates_group() {
    let mut s = shell();
    let (out, code, _) = s.run_line("groupadd devs");
    assert_eq!(code, 0, "groupadd failed: {:?}", out);
    assert!(
        s.ident.users.get_group_by_name("devs").is_some(),
        "devs group not found"
    );
}

#[test]
fn addgroup_alias_works() {
    let mut s = shell();
    let (out, code, _) = s.run_line("addgroup ops");
    assert_eq!(code, 0, "addgroup failed: {:?}", out);
    assert!(s.ident.users.get_group_by_name("ops").is_some());
}

#[test]
fn usermod_adds_to_group() {
    let mut s = shell();
    s.run_line("useradd eve");
    s.run_line("groupadd staff");
    let (out, code, _) = s.run_line("usermod -aG staff eve");
    assert_eq!(code, 0, "usermod failed: {:?}", out);
    let grp = s.ident.users.get_group_by_name("staff");
    assert!(grp.is_some(), "staff group not found");
    if let Some(g) = grp {
        assert!(
            g.members.contains(&"eve".to_string()),
            "eve not in staff group"
        );
    }
}

#[test]
fn su_switches_user() {
    let mut s = shell();
    s.run_line("useradd frank");
    let (out, code, _) = s.run_line("su frank");
    assert_eq!(code, 0, "su failed: {:?}", out);
    assert_eq!(s.ident.user, "frank");
    let frank = s.ident.users.get_user_by_name("frank");
    assert!(frank.is_some(), "frank not in UserDb");
    if let Some(f) = frank {
        assert_eq!(s.fs.current_uid(), f.uid);
    }
}

#[test]
fn su_dash_changes_home() {
    let mut s = shell();
    s.run_line("useradd grace");
    let (out, code, _) = s.run_line("su - grace");
    assert_eq!(code, 0, "su - failed: {:?}", out);
    assert_eq!(s.ident.user, "grace");
    assert_eq!(s.cwd, "/home/grace");
}

#[test]
fn su_without_args_becomes_root() {
    let mut s = shell();
    let (out, code, _) = s.run_line("su");
    assert_eq!(code, 0, "su (no args) failed: {:?}", out);
    assert_eq!(s.ident.user, "root");
    assert_eq!(s.fs.current_uid(), 0);
}

#[test]
fn sudo_runs_as_root() {
    let mut s = shell();
    // sudo whoami should run as root and return "root"
    let (out, code, _) = s.run_line("sudo whoami");
    assert_eq!(code, 0, "sudo whoami failed: {:?}", out);
    assert_eq!(out, "root\n");
}

#[test]
fn sudo_restores_user() {
    let mut s = shell();
    let original_uid = s.fs.current_uid();
    let original_user = s.ident.user.clone();
    s.run_line("sudo whoami");
    assert_eq!(
        s.fs.current_uid(),
        original_uid,
        "uid not restored after sudo"
    );
    assert_eq!(s.ident.user, original_user, "user not restored after sudo");
}

#[test]
fn passwd_succeeds() {
    let mut s = shell();
    let (out, code, _) = s.run_line("passwd");
    assert_eq!(code, 0, "passwd failed: {:?}", out);
    assert_eq!(out, "passwd: password updated successfully\n");
}

#[test]
fn id_shows_all_groups() {
    let mut s = shell();
    s.run_line("groupadd extra");
    s.run_line("usermod -aG extra u");
    let (out, code, _) = s.run_line("id");
    assert_eq!(code, 0);
    assert!(out.contains("uid="), "missing uid: {:?}", out);
    assert!(out.contains("gid="), "missing gid: {:?}", out);
    assert!(out.contains("groups="), "missing groups: {:?}", out);
    assert!(
        out.contains("extra"),
        "extra group not in id output: {:?}",
        out
    );
}

#[test]
fn chown_accepts_username() {
    let mut s = shell();
    s.run_line("useradd henry");
    s.run_line("touch /home/u/file.txt");
    let (out, code, _) = s.run_line("sudo chown henry /home/u/file.txt");
    assert_eq!(code, 0, "chown with username failed: {:?}", out);
    let henry = s.ident.users.get_user_by_name("henry");
    assert!(henry.is_some(), "henry not in UserDb");
    let henry_uid = henry.map(|h| h.uid).unwrap_or(0);
    let entry = s.fs.get("/home/u/file.txt");
    assert!(entry.is_some(), "file.txt not found");
    if let Some(e) = entry {
        assert_eq!(e.uid(), henry_uid, "file uid not updated");
    }
}

// su to nonexistent user
#[test]
fn su_nonexistent_user_fails() {
    let mut s = shell();
    let (out, code, _) = s.run_line("su nobody");
    assert_eq!(code, 1);
    assert!(out.contains("does not exist"), "got {:?}", out);
}

// sudo with no args
#[test]
fn sudo_no_args_fails() {
    let mut s = shell();
    let (_, code, _) = s.run_line("sudo");
    assert_eq!(code, 1);
}

// sudo nonexistent command
#[test]
fn sudo_nonexistent_command() {
    let mut s = shell();
    let (out, code, _) = s.run_line("sudo nosuchcmd");
    assert_eq!(code, 127);
    assert!(out.contains("not found"), "got {:?}", out);
}

// usermod on nonexistent user
#[test]
fn usermod_nonexistent_user_fails() {
    let mut s = shell();
    s.run_line("groupadd devs");
    let (_, code, _) = s.run_line("usermod -aG devs nobody");
    assert_eq!(code, 6);
}

// usermod nonexistent group
#[test]
fn usermod_nonexistent_group_fails() {
    let mut s = shell();
    let (_, code, _) = s.run_line("usermod -aG nogroup u");
    assert_eq!(code, 6);
}

// useradd duplicate user
#[test]
fn useradd_duplicate_fails() {
    let mut s = shell();
    s.run_line("useradd alice");
    let (_, code, _) = s.run_line("useradd alice");
    assert_eq!(code, 9);
}

// deluser nonexistent
#[test]
fn deluser_nonexistent_fails() {
    let mut s = shell();
    let (_, code, _) = s.run_line("deluser ghost");
    assert_eq!(code, 6);
}

// su preserves environment after exit (run commands after su)
#[test]
fn su_then_whoami() {
    let mut s = shell();
    s.run_line("useradd alice");
    s.run_line("su alice");
    let (out, code, _) = s.run_line("whoami");
    assert_eq!(code, 0);
    assert_eq!(out, "alice\n");
}

#[test]
fn deluser_removes_existing() {
    let mut s = shell();
    s.run_line("useradd bob");
    let (_, code, _) = s.run_line("deluser bob");
    assert_eq!(code, 0);
    // Verify user is gone — id or su should fail
    let (_, c2, _) = s.run_line("su bob");
    assert_eq!(c2, 1);
}

#[test]
fn passwd_with_username() {
    let mut s = shell();
    let (out, code, _) = s.run_line("passwd u");
    assert_eq!(code, 0);
    assert_eq!(out, "passwd: password updated successfully\n");
}

#[test]
fn su_entry_shows_pre_switch_user() {
    let mut s = shell();
    s.run_line("useradd alice");
    // The "su alice" entry should still show "u" (the pre-switch user)
    let entry = s.execute("su alice");
    assert_eq!(entry.user, "u", "su entry should show original user");
    assert_eq!(entry.exit_code, 0);
    // But the next entry shows "alice"
    let entry2 = s.execute("whoami");
    assert_eq!(entry2.user, "alice", "post-su entry should show new user");
    assert_eq!(entry2.output, "alice\n");
}

#[test]
fn su_installs_supplementary_groups() {
    let mut s = shell();
    s.run_line("sudo useradd alice");
    s.run_line("sudo groupadd shared");
    s.run_line("sudo usermod -aG shared alice");
    // Create a file readable only by group shared (0640)
    s.run_line("sudo touch /tmp/groupfile");
    s.run_line("sudo chown alice:shared /tmp/groupfile");
    s.run_line("sudo chmod 640 /tmp/groupfile");
    // Switch to alice — should have supplementary group access
    s.run_line("su alice");
    let (out, code, _) = s.run_line("cat /tmp/groupfile");
    assert_eq!(code, 0, "alice should read via group access: {out}");
}

#[test]
fn set_current_user_clears_supplementary_groups() {
    let mut s = shell();
    s.run_line("sudo useradd alice");
    s.run_line("sudo groupadd team");
    s.run_line("sudo usermod -aG team alice");
    s.run_line("su alice");
    // alice has 'team' group. Now switch to a different user via su
    s.run_line("su - root");
    // root should NOT have alice's 'team' supplementary group
    let (out, _, _) = s.run_line("id");
    assert!(
        !out.contains("(team)"),
        "root should not inherit alice's groups: {out}"
    );
}

#[test]
fn useradd_after_system_user_no_gid_collision() -> Result<(), String> {
    // Regression: add_user_with_ids previously failed to advance next_gid, causing
    // a subsequent useradd to reuse a system user's primary GID and corrupt their group name.
    let v = serde_json::json!({
        "user": "u",
        "system": {
            "hostname": "h",
            "users": [
                {"name": "u", "home": "/home/u"},
                {"name": "alice", "home": "/home/alice"}
            ],
            "files": {},
        },
        "commands": [],
    });
    let c: crate::types::Config =
        serde_json::from_value(v).map_err(|e| format!("invalid config: {}", e))?;
    let mut s = crate::shell::Shell::new(&c);
    // Create bob after system users alice and u
    s.run_line("sudo useradd bob");
    // alice's group should still be "alice", not "bob"
    s.run_line("su alice");
    let (out, _, _) = s.run_line("id");
    // id output: "uid=1001(alice) gid=1001(alice) groups=1001(alice)"
    // The key check: gid must show "(alice)" not "(bob)"
    let gid_part = out
        .split_whitespace()
        .find(|s| s.starts_with("gid="))
        .unwrap_or("");
    assert_eq!(gid_part, "gid=1001(alice)", "got full id: {out}");
    Ok(())
}

#[test]
fn useradd_with_shell_flag() {
    let mut s = shell();
    s.run_line("useradd -s /bin/zsh alice");
    let user = s.ident.users.get_user_by_name("alice");
    assert!(user.is_some(), "alice not created");
    assert_eq!(user.unwrap().shell, "/bin/zsh");
}

#[test]
fn useradd_default_shell() {
    let mut s = shell();
    s.run_line("useradd bob");
    let user = s.ident.users.get_user_by_name("bob");
    assert!(user.is_some(), "bob not created");
    assert_eq!(user.unwrap().shell, "/bin/sh");
}

#[test]
fn usermod_without_a_replaces_groups() {
    let mut s = shell();
    s.run_line("useradd alice");
    s.run_line("groupadd grp1");
    s.run_line("groupadd grp2");
    s.run_line("groupadd grp3");
    // Append alice to grp1 and grp2
    s.run_line("usermod -aG grp1 alice");
    s.run_line("usermod -aG grp2 alice");
    let grp1 = s.ident.users.get_group_by_name("grp1").unwrap();
    assert!(grp1.members.contains(&"alice".to_string()));
    let grp2 = s.ident.users.get_group_by_name("grp2").unwrap();
    assert!(grp2.members.contains(&"alice".to_string()));
    // Now replace groups with only grp3 (no -a flag)
    s.run_line("usermod -G grp3 alice");
    let grp1 = s.ident.users.get_group_by_name("grp1").unwrap();
    assert!(
        !grp1.members.contains(&"alice".to_string()),
        "alice should have been removed from grp1"
    );
    let grp2 = s.ident.users.get_group_by_name("grp2").unwrap();
    assert!(
        !grp2.members.contains(&"alice".to_string()),
        "alice should have been removed from grp2"
    );
    let grp3 = s.ident.users.get_group_by_name("grp3").unwrap();
    assert!(
        grp3.members.contains(&"alice".to_string()),
        "alice should be in grp3"
    );
}

#[test]
fn usermod_without_a_keeps_primary_group() {
    let mut s = shell();
    s.run_line("useradd alice");
    s.run_line("groupadd extra");
    s.run_line("usermod -aG extra alice");
    // Replace with empty group list — primary group should survive
    s.run_line("groupadd newgrp");
    s.run_line("usermod -G newgrp alice");
    // alice's primary group ("alice") should still be intact
    let alice = s.ident.users.get_user_by_name("alice").unwrap();
    let primary_gid = alice.gid;
    let primary_group = s.ident.users.get_group_by_gid(primary_gid);
    assert!(primary_group.is_some(), "primary group should still exist");
}

// ---------------------------------------------------------------------------
// 2C.11: sudo -u USER
// ---------------------------------------------------------------------------

#[test]
fn sudo_u_runs_as_specified_user() {
    let mut s = shell();
    s.run_line("useradd alice");
    let (out, code, _) = s.run_line("sudo -u alice whoami");
    assert_eq!(code, 0);
    assert_eq!(out, "alice\n");
}

#[test]
fn sudo_u_restores_original_user() {
    let mut s = shell();
    s.run_line("useradd alice");
    let original_user = s.ident.user.clone();
    s.run_line("sudo -u alice whoami");
    assert_eq!(
        s.ident.user, original_user,
        "user should be restored after sudo -u"
    );
}

#[test]
fn sudo_u_unknown_user_fails() {
    let mut s = shell();
    let (out, code, _) = s.run_line("sudo -u nobody whoami");
    assert_eq!(code, 1);
    assert!(out.contains("unknown user"), "got: {}", out);
}

// ---------------------------------------------------------------------------
// Feature 5: su -c COMMAND
// ---------------------------------------------------------------------------

#[test]
fn su_c_runs_command_as_user() {
    let mut s = shell();
    s.run_line("useradd alice");
    let (out, code, _) = s.run_line("su -c 'whoami' alice");
    assert_eq!(code, 0);
    assert_eq!(out, "alice\n");
}

#[test]
fn su_c_runs_as_root_by_default() {
    let mut s = shell();
    let (out, code, _) = s.run_line("su -c 'whoami'");
    assert_eq!(code, 0);
    assert_eq!(out, "root\n");
}

#[test]
fn su_c_restores_user() {
    let mut s = shell();
    s.run_line("useradd alice");
    let orig_user = s.ident.user.clone();
    s.run_line("su -c 'whoami' alice");
    assert_eq!(s.ident.user, orig_user, "su -c should restore user after");
}
