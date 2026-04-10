pub mod ansi;
mod commands;
pub mod keyline;
mod parser;
mod shell;
mod types;
mod userdb;

use shell::Shell;
use types::*;
use wasm_minimal_protocol::*;

initiate_protocol!();

#[wasm_func]
pub fn execute(input: &[u8]) -> Vec<u8> {
    let config: Config = match serde_json::from_slice(input) {
        Ok(c) => c,
        Err(e) => {
            return format!(r#"{{"entries":[],"final-path":"~","error":"{}"}}"#, e).into_bytes();
        }
    };

    let mut shell = Shell::new(&config);
    let mut entries: Vec<OutputEntry> = Vec::new();

    for line in &config.commands {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let entry = shell.execute(trimmed);
        if entry.output == "__CLEAR__" {
            entries.clear();
        } else {
            entries.push(entry);
        }
    }

    let files = if config.include_files {
        use bare_vfs::EntryRef;
        let mut map = std::collections::BTreeMap::new();
        for (path, entry) in shell.fs.walk() {
            if path == "/" {
                continue;
            }
            let output = match entry {
                EntryRef::File { content, mode, .. } => {
                    match std::str::from_utf8(content) {
                        Ok(s) => FileOutput::File {
                            content: s.to_string(),
                            mode,
                        },
                        Err(_) => continue, // skip binary files
                    }
                }
                EntryRef::Dir { mode, .. } => FileOutput::Dir { mode },
                EntryRef::Symlink { target, .. } => FileOutput::Symlink {
                    target: target.to_string(),
                },
            };
            map.insert(path, output);
        }
        Some(map)
    } else {
        None
    };

    let out = SessionOutput {
        entries,
        final_path: shell.display_path(),
        final_user: shell.user.clone(),
        final_hostname: shell.hostname.clone(),
        files,
    };

    serde_json::to_vec(&out).unwrap_or_default()
}

#[wasm_func]
pub fn process_keyline(input: &[u8]) -> Vec<u8> {
    let line = match std::str::from_utf8(input) {
        Ok(s) => s,
        Err(e) => {
            return format!(
                r#"[{{"text":"","cursor":0,"event":"error: invalid UTF-8 at byte {}"}}]"#,
                e.valid_up_to()
            )
            .into_bytes();
        }
    };
    let states = keyline::process(line);
    serde_json::to_vec(&states).unwrap_or_default()
}

/// Process keyline input with history for Up/Down arrow navigation.
/// Input: JSON `{"input": "...", "history": ["cmd1", "cmd2"]}`
#[wasm_func]
pub fn process_keyline_with_history(input: &[u8]) -> Vec<u8> {
    #[derive(serde::Deserialize)]
    struct Input {
        input: String,
        #[serde(default)]
        history: Vec<String>,
    }
    let parsed: Input = match serde_json::from_slice(input) {
        Ok(v) => v,
        Err(e) => {
            return format!(r#"[{{"text":"","cursor":0,"event":"error: {}"}}]"#, e).into_bytes();
        }
    };
    let states = keyline::process_with_history(&parsed.input, &parsed.history);
    serde_json::to_vec(&states).unwrap_or_default()
}

#[wasm_func]
pub fn version() -> Vec<u8> {
    env!("CARGO_PKG_VERSION").as_bytes().to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn execute_malformed_json_returns_error_field() {
        let out = execute(b"not-json");
        let v: serde_json::Value = serde_json::from_slice(&out).expect("valid JSON error body");
        assert!(v.get("error").and_then(|e| e.as_str()).is_some());
        assert_eq!(v["entries"].as_array().map(|a| a.len()), Some(0));
    }

    #[test]
    fn execute_su_updates_final_user() {
        let input = br#"{"user":"demo","system":{"hostname":"h","users":[{"name":"demo","home":"/home/demo"},{"name":"alice","home":"/home/alice"}]},"commands":["su alice"]}"#;
        let raw = execute(input);
        let v: serde_json::Value = serde_json::from_slice(&raw).unwrap();
        assert_eq!(v["final-user"].as_str(), Some("alice"));
        assert_eq!(v["final-hostname"].as_str(), Some("h"));
    }

    #[test]
    fn execute_final_user_unchanged_without_su() {
        let input = br#"{"user":"demo","system":{"hostname":"h","users":[{"name":"demo","home":"/home/demo"}]},"commands":["echo hi"]}"#;
        let raw = execute(input);
        let v: serde_json::Value = serde_json::from_slice(&raw).unwrap();
        assert_eq!(v["final-user"].as_str(), Some("demo"));
        assert_eq!(v["final-hostname"].as_str(), Some("h"));
    }

    #[test]
    fn execute_clear_drops_prior_entries() {
        let input = br#"{"user":"u","system":{"hostname":"h","users":[{"name":"u","home":"/home/u"}]},"commands":["echo first","clear","echo second"]}"#;
        let raw = execute(input);
        let v: serde_json::Value = serde_json::from_slice(&raw).unwrap();
        let entries = v["entries"].as_array().unwrap();
        assert_eq!(entries.len(), 1, "expected only post-clear command");
        assert!(entries[0]["command"].as_str().unwrap().contains("second"));
    }

    #[test]
    fn execute_include_files_returns_filesystem() {
        let input = br#"{"user":"u","system":{"hostname":"h","users":[{"name":"u","home":"/home/u"}],"files":{"note.txt":"hello"}},"commands":["echo world > out.txt","ln -s note.txt link.txt","mkdir sub"],"include-files":true}"#;
        let raw = execute(input);
        let v: serde_json::Value = serde_json::from_slice(&raw).unwrap();
        let files = v["files"].as_object().expect("files should be present");
        // Check file
        let out = &files["/home/u/out.txt"];
        assert_eq!(out["type"].as_str(), Some("file"));
        assert_eq!(out["content"].as_str(), Some("world"));
        // Check symlink
        let link = &files["/home/u/link.txt"];
        assert_eq!(link["type"].as_str(), Some("symlink"));
        assert_eq!(link["target"].as_str(), Some("note.txt"));
        // Check directory
        let sub = &files["/home/u/sub"];
        assert_eq!(sub["type"].as_str(), Some("dir"));
        // Check seeded file
        let note = &files["/home/u/note.txt"];
        assert_eq!(note["content"].as_str(), Some("hello"));
    }

    #[test]
    fn execute_without_include_files_omits_files() {
        let input = br#"{"user":"u","system":{"hostname":"h","users":[{"name":"u","home":"/home/u"}]},"commands":["echo hi"]}"#;
        let raw = execute(input);
        let v: serde_json::Value = serde_json::from_slice(&raw).unwrap();
        assert!(
            v.get("files").is_none(),
            "files should be absent when not requested"
        );
    }
}
