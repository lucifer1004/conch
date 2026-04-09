use super::*;
use crate::types::Config;

mod core;
mod fs;
mod inspect;
mod nav;
mod script;
mod text;
mod transform;
mod user;

pub fn shell_with_files(files: serde_json::Value) -> Shell {
    let v = serde_json::json!({
        "user": "u",
        "hostname": "h",
        "home": "/home/u",
        "files": files,
        "commands": [],
    });
    let c: Config = serde_json::from_value(v).unwrap();
    Shell::new(&c)
}

pub fn shell() -> Shell {
    shell_with_files(serde_json::json!({}))
}
