//! Demo conch plugin: `upper` uppercases text, `lower` lowercases text.
//!
//! Shows the minimal structure for a conch WASM plugin using wasm-minimal-protocol.
//! Input JSON: `{args: [...], stdin: "...", files: {name: content}}`
//! Output JSON: `{stdout: "...", exit-code: 0}`

use wasm_minimal_protocol::*;

initiate_protocol!();

#[wasm_func]
pub fn execute(input: &[u8]) -> Vec<u8> {
    let v: serde_json::Value = match serde_json::from_slice(input) {
        Ok(v) => v,
        Err(_) => return br#"{"stdout":"","exit-code":1}"#.to_vec(),
    };

    let stdin = v["stdin"].as_str().unwrap_or("");
    let args: Vec<&str> = v["args"]
        .as_array()
        .map(|a| a.iter().filter_map(|x| x.as_str()).collect())
        .unwrap_or_default();

    // Determine input text: stdin (from pipe) > file arg > literal args
    let text = if !stdin.is_empty() {
        stdin.to_string()
    } else if let Some(files) = v["files"].as_object() {
        // Try to read the first file-like arg
        args.iter()
            .find_map(|a| files.get(*a).and_then(|v| v.as_str()))
            .map(|s| s.to_string())
            .unwrap_or_else(|| args.join(" ") + "\n")
    } else {
        args.join(" ") + "\n"
    };

    // Check first arg for mode flag, default to uppercase
    let lowercase = args.first().is_some_and(|a| *a == "-l" || *a == "--lower");
    let result = if lowercase {
        text.to_lowercase()
    } else {
        text.to_uppercase()
    };

    serde_json::to_vec(&serde_json::json!({
        "stdout": result,
        "exit-code": 0,
    }))
    .unwrap_or_default()
}
