use super::*;

#[test]
fn useradd_creates_user_and_home() {
    let mut s = shell();
    let (out, code, _) = s.run_line("useradd alice");
    assert_eq!(code, 0, "useradd failed: {:?}", out);
    assert!(
        s.users.get_user_by_name("alice").is_some(),
        "alice not in UserDb"
    );
    assert!(s.fs.is_dir("/home/alice"), "home dir not created");
}

#[test]
fn adduser_alias_works() {
    let mut s = shell();
    let (out, code, _) = s.run_line("adduser bob");
    assert_eq!(code, 0, "adduser failed: {:?}", out);
    assert!(s.users.get_user_by_name("bob").is_some());
}

#[test]
fn userdel_removes_user() {
    let mut s = shell();
    s.run_line("useradd charlie");
    let (out, code, _) = s.run_line("userdel charlie");
    assert_eq!(code, 0, "userdel failed: {:?}", out);
    assert!(
        s.users.get_user_by_name("charlie").is_none(),
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
        s.users.get_group_by_name("devs").is_some(),
        "devs group not found"
    );
}

#[test]
fn addgroup_alias_works() {
    let mut s = shell();
    let (out, code, _) = s.run_line("addgroup ops");
    assert_eq!(code, 0, "addgroup failed: {:?}", out);
    assert!(s.users.get_group_by_name("ops").is_some());
}

#[test]
fn usermod_adds_to_group() {
    let mut s = shell();
    s.run_line("useradd eve");
    s.run_line("groupadd staff");
    let (out, code, _) = s.run_line("usermod -aG staff eve");
    assert_eq!(code, 0, "usermod failed: {:?}", out);
    let grp = s.users.get_group_by_name("staff").unwrap();
    assert!(
        grp.members.contains(&"eve".to_string()),
        "eve not in staff group"
    );
}

#[test]
fn su_switches_user() {
    let mut s = shell();
    s.run_line("useradd frank");
    let (out, code, _) = s.run_line("su frank");
    assert_eq!(code, 0, "su failed: {:?}", out);
    assert_eq!(s.user, "frank");
    let frank = s.users.get_user_by_name("frank").unwrap();
    assert_eq!(s.fs.current_uid(), frank.uid);
}

#[test]
fn su_dash_changes_home() {
    let mut s = shell();
    s.run_line("useradd grace");
    let (out, code, _) = s.run_line("su - grace");
    assert_eq!(code, 0, "su - failed: {:?}", out);
    assert_eq!(s.user, "grace");
    assert_eq!(s.cwd, "/home/grace");
}

#[test]
fn su_without_args_becomes_root() {
    let mut s = shell();
    let (out, code, _) = s.run_line("su");
    assert_eq!(code, 0, "su (no args) failed: {:?}", out);
    assert_eq!(s.user, "root");
    assert_eq!(s.fs.current_uid(), 0);
}

#[test]
fn sudo_runs_as_root() {
    let mut s = shell();
    // sudo whoami should run as root and return "root"
    let (out, code, _) = s.run_line("sudo whoami");
    assert_eq!(code, 0, "sudo whoami failed: {:?}", out);
    assert_eq!(out.trim(), "root");
}

#[test]
fn sudo_restores_user() {
    let mut s = shell();
    let original_uid = s.fs.current_uid();
    let original_user = s.user.clone();
    s.run_line("sudo whoami");
    assert_eq!(
        s.fs.current_uid(),
        original_uid,
        "uid not restored after sudo"
    );
    assert_eq!(s.user, original_user, "user not restored after sudo");
}

#[test]
fn passwd_succeeds() {
    let mut s = shell();
    let (out, code, _) = s.run_line("passwd");
    assert_eq!(code, 0, "passwd failed: {:?}", out);
    assert!(
        out.contains("password updated successfully"),
        "got: {:?}",
        out
    );
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
    let henry = s.users.get_user_by_name("henry").unwrap();
    let henry_uid = henry.uid;
    let entry = s.fs.get("/home/u/file.txt").unwrap();
    assert_eq!(entry.uid(), henry_uid, "file uid not updated");
}

// su to nonexistent user
#[test]
fn su_nonexistent_user_fails() {
    let mut s = shell();
    let (out, code, _) = s.run_line("su nobody");
    assert_ne!(code, 0);
    assert!(
        out.contains("does not exist") || out.contains("no such") || out.contains("unknown"),
        "got {:?}",
        out
    );
}

// sudo with no args
#[test]
fn sudo_no_args_fails() {
    let mut s = shell();
    let (_, code, _) = s.run_line("sudo");
    assert_ne!(code, 0);
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
    assert_ne!(code, 0);
}

// usermod nonexistent group
#[test]
fn usermod_nonexistent_group_fails() {
    let mut s = shell();
    let (_, code, _) = s.run_line("usermod -aG nogroup u");
    assert_ne!(code, 0);
}

// useradd duplicate user
#[test]
fn useradd_duplicate_fails() {
    let mut s = shell();
    s.run_line("useradd alice");
    let (_, code, _) = s.run_line("useradd alice");
    assert_ne!(code, 0);
}

// deluser nonexistent
#[test]
fn deluser_nonexistent_fails() {
    let mut s = shell();
    let (_, code, _) = s.run_line("deluser ghost");
    assert_ne!(code, 0);
}

// su preserves environment after exit (run commands after su)
#[test]
fn su_then_whoami() {
    let mut s = shell();
    s.run_line("useradd alice");
    s.run_line("su alice");
    let (out, code, _) = s.run_line("whoami");
    assert_eq!(code, 0);
    assert_eq!(out, "alice");
}

#[test]
fn deluser_removes_existing() {
    let mut s = shell();
    s.run_line("useradd bob");
    let (_, code, _) = s.run_line("deluser bob");
    assert_eq!(code, 0);
    // Verify user is gone — id or su should fail
    let (_, c2, _) = s.run_line("su bob");
    assert_ne!(c2, 0);
}

#[test]
fn passwd_with_username() {
    let mut s = shell();
    let (out, code, _) = s.run_line("passwd u");
    assert_eq!(code, 0);
    assert!(
        out.contains("updated") || out.contains("success"),
        "got {:?}",
        out
    );
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
    assert_eq!(entry2.output, "alice");
}
