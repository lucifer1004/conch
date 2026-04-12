use super::*;
use crate::types::Config;

mod arrays;
mod builtins_new;
mod expand;
mod fs;
mod glob;
mod inspect;
mod nav;
mod pipeline;
mod process;
mod script;
mod shell;
mod text;
mod transform;
mod user;

pub fn shell_with_files(files: serde_json::Value) -> Shell {
    let v = serde_json::json!({
        "user": "u",
        "system": {
            "hostname": "h",
            "users": [{"name": "u", "home": "/home/u"}],
            "files": files,
        },
        "commands": [],
    });
    let c: Config = match serde_json::from_value(v) {
        Ok(c) => c,
        Err(e) => {
            assert!(
                e.to_string().is_empty(),
                "shell_with_files: config parse failed: {e}"
            );
            return shell();
        }
    };
    let mut s = Shell::new(&c);
    s.color = false;
    s
}

pub fn shell() -> Shell {
    shell_with_files(serde_json::json!({}))
}

pub fn shell_with_bg_mode(mode: &str) -> Shell {
    let v = serde_json::json!({
        "user": "u",
        "system": {
            "hostname": "h",
            "users": [{"name": "u", "home": "/home/u"}],
            "files": {},
        },
        "commands": [],
        "background-mode": mode,
    });
    let c: Config = serde_json::from_value(v).expect("bg mode config should parse");
    let mut s = Shell::new(&c);
    s.color = false;
    s
}
