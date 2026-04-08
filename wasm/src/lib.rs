pub mod ansi;
mod commands;
mod parser;
mod shell;
mod types;

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

    let out = SessionOutput {
        entries,
        final_path: shell.display_path(),
    };

    serde_json::to_vec(&out).unwrap_or_default()
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
    fn execute_clear_drops_prior_entries() {
        let input = br#"{"user":"u","hostname":"h","home":"/home/u","files":{},"commands":["echo first","clear","echo second"]}"#;
        let raw = execute(input);
        let v: serde_json::Value = serde_json::from_slice(&raw).unwrap();
        let entries = v["entries"].as_array().unwrap();
        assert_eq!(entries.len(), 1, "expected only post-clear command");
        assert!(entries[0]["command"].as_str().unwrap().contains("second"));
    }
}
