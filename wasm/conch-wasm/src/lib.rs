//! Conch WASM — a sandboxed shell interpreter compiled to WebAssembly.
//!
//! Accepts a JSON configuration (user, filesystem seed, commands) and returns
//! structured output entries suitable for rendering terminal sessions in Typst.

pub mod ansi;
mod commands;
pub mod keyline;
mod plugin_host;
pub mod script;
mod shell;
mod str_type;
mod types;
mod userdb;

pub(crate) use str_type::Str;

use shell::Shell;
use types::*;
use wasm_minimal_protocol::*;

initiate_protocol!();

/// Execute a shell session: parse JSON config, run commands, return JSON output.
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

    // Join all commands into a single script for proper multi-line support
    let script = config.commands.join("\n");

    match crate::script::parse_script(&script) {
        Ok(ast) => {
            let (results, _final_flow) = shell.interpret_stmts_collecting(&ast.stmts, &script);

            // Map per-statement results to OutputEntry objects
            for r in results {
                // Handle __CLEAR__
                if r.output.last().map(|s| s.as_str()) == Some("__CLEAR__") {
                    entries.clear();
                    continue;
                }

                let mut output = r.output;

                // Top-level control flow errors (bare return/break/continue)
                if let Some(msg) = crate::script::interp::top_level_flow_error(&r.flow) {
                    output.push(msg);
                }

                entries.push(OutputEntry {
                    user: r.user,
                    hostname: r.hostname,
                    path: r.display_path,
                    command: r.command_source,
                    output: output.concat(),
                    exit_code: r.exit_code,
                    lang: r.lang,
                    first_line: Some(r.first_line),
                    last_line: Some(r.last_line),
                    bg_completions: r.bg_completions,
                    delegate: r.delegate,
                });
            }

            // Fire EXIT trap at session end
            let trap_out = shell.fire_exit_trap();
            if !trap_out.is_empty() {
                entries.push(OutputEntry {
                    user: shell.ident.user.clone(),
                    hostname: shell.ident.hostname.clone(),
                    path: shell.display_path(),
                    command: "trap EXIT".to_string(),
                    output: trap_out,
                    exit_code: 0,
                    lang: None,
                    first_line: None,
                    last_line: None,
                    bg_completions: Vec::new(),
                    delegate: None,
                });
            }
        }
        Err(e) => {
            entries.push(OutputEntry {
                user: shell.ident.user.clone(),
                hostname: shell.ident.hostname.clone(),
                path: shell.display_path(),
                command: script.clone(),
                output: format!("conch: {}", e),
                exit_code: 2,
                lang: None,
                first_line: None,
                last_line: None,
                bg_completions: Vec::new(),
                delegate: None,
            });
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
        final_user: shell.ident.user.to_string(),
        final_hostname: shell.ident.hostname.to_string(),
        files,
    };

    serde_json::to_vec(&out).unwrap_or_default()
}

/// Analyze a script to get statement ranges without executing.
/// Returns JSON: `{"statements": [{"first-line": N, "last-line": M, "source": "..."}]}`
#[wasm_func]
pub fn analyze_script(input: &[u8]) -> Vec<u8> {
    let script = match std::str::from_utf8(input) {
        Ok(s) => s,
        Err(e) => {
            return format!(r#"{{"error":"invalid UTF-8: {}"}}"#, e).into_bytes();
        }
    };
    match crate::script::parse_script(script) {
        Ok(ast) => {
            let statements: Vec<StatementRange> = ast
                .stmts
                .iter()
                .map(|stmt| {
                    let span = stmt.span();
                    let source = script
                        .get(span.start_byte as usize..span.end_byte as usize)
                        .unwrap_or("")
                        .to_string();
                    StatementRange {
                        first_line: span.start_line,
                        last_line: span.end_line,
                        source,
                    }
                })
                .collect();
            let analysis = ScriptAnalysis { statements };
            serde_json::to_vec(&analysis).unwrap_or_default()
        }
        Err(e) => format!(r#"{{"error":"{}"}}"#, e).into_bytes(),
    }
}

/// Process a keyline input string and return per-keystroke buffer states as JSON.
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

/// Register an external WASM plugin for a command name.
/// Input format: 4 bytes (name length LE) + name bytes + WASM module bytes.
#[wasm_func]
pub fn register_plugin(input: &[u8]) -> Vec<u8> {
    if input.len() < 4 {
        return br#"{"error":"input too short"}"#.to_vec();
    }
    let name_len = u32::from_le_bytes([input[0], input[1], input[2], input[3]]) as usize;
    if input.len() < 4 + name_len {
        return br#"{"error":"name length exceeds input"}"#.to_vec();
    }
    let name = match std::str::from_utf8(&input[4..4 + name_len]) {
        Ok(s) => s,
        Err(_) => return br#"{"error":"invalid UTF-8 name"}"#.to_vec(),
    };
    let wasm_bytes = input[4 + name_len..].to_vec();
    plugin_host::register(name, wasm_bytes);
    br#"{"ok":true}"#.to_vec()
}

#[wasm_func]
pub fn version() -> Vec<u8> {
    env!("CARGO_PKG_VERSION").as_bytes().to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn analyze_script_single_command() -> Result<(), String> {
        let result = analyze_script(b"echo hello");
        let v: serde_json::Value =
            serde_json::from_slice(&result).map_err(|e| format!("json parse: {}", e))?;
        let stmts = v["statements"]
            .as_array()
            .ok_or("missing statements array")?;
        assert_eq!(stmts.len(), 1);
        assert_eq!(stmts[0]["first-line"], 0);
        assert_eq!(stmts[0]["last-line"], 0);
        assert_eq!(stmts[0]["source"], "echo hello");
        Ok(())
    }

    #[test]
    fn analyze_script_multiline_if() -> Result<(), String> {
        let result = analyze_script(b"if true; then\n  echo hello\nfi\necho done");
        let v: serde_json::Value =
            serde_json::from_slice(&result).map_err(|e| format!("json parse: {}", e))?;
        let stmts = v["statements"]
            .as_array()
            .ok_or("missing statements array")?;
        assert_eq!(stmts.len(), 2);
        assert_eq!(stmts[0]["first-line"], 0);
        assert_eq!(stmts[0]["last-line"], 2);
        let src0 = stmts[0]["source"].as_str().ok_or("missing source")?;
        assert!(src0.contains("if true"));
        assert!(src0.contains("fi"));
        assert_eq!(stmts[1]["first-line"], 3);
        assert_eq!(stmts[1]["last-line"], 3);
        assert_eq!(stmts[1]["source"], "echo done");
        Ok(())
    }

    #[test]
    fn analyze_script_for_loop() -> Result<(), String> {
        let result = analyze_script(b"for x in a b c; do\n  echo $x\ndone");
        let v: serde_json::Value =
            serde_json::from_slice(&result).map_err(|e| format!("json parse: {}", e))?;
        let stmts = v["statements"]
            .as_array()
            .ok_or("missing statements array")?;
        assert_eq!(stmts.len(), 1);
        assert_eq!(stmts[0]["first-line"], 0);
        assert_eq!(stmts[0]["last-line"], 2);
        Ok(())
    }

    #[test]
    fn analyze_script_parse_error() -> Result<(), String> {
        let result = analyze_script(b"if true; then");
        let v: serde_json::Value =
            serde_json::from_slice(&result).map_err(|e| format!("json parse: {}", e))?;
        assert!(v.get("error").is_some());
        Ok(())
    }

    #[test]
    fn execute_multiline_if_single_entry() -> Result<(), String> {
        let input = br#"{"user":"u","system":{"hostname":"h","users":[{"name":"u","home":"/home/u"}]},"commands":["if true; then","  echo hello","fi"]}"#;
        let raw = execute(input);
        let v: serde_json::Value =
            serde_json::from_slice(&raw).map_err(|e| format!("json parse: {}", e))?;
        let entries = v["entries"].as_array().ok_or("missing entries array")?;
        assert_eq!(entries.len(), 1, "multi-line if should be one entry");
        assert_eq!(entries[0]["output"], "hello\n");
        let cmd = entries[0]["command"]
            .as_str()
            .ok_or("missing command field")?;
        assert!(
            cmd.contains("if true"),
            "command should contain if: {}",
            cmd
        );
        assert!(entries[0].get("first-line").is_some());
        Ok(())
    }

    #[test]
    fn execute_malformed_json_returns_error_field() -> Result<(), String> {
        let out = execute(b"not-json");
        let v: serde_json::Value =
            serde_json::from_slice(&out).map_err(|e| format!("json parse: {}", e))?;
        assert!(v.get("error").and_then(|e| e.as_str()).is_some());
        assert_eq!(v["entries"].as_array().map(|a| a.len()), Some(0));
        Ok(())
    }

    #[test]
    fn execute_su_updates_final_user() -> Result<(), String> {
        let input = br#"{"user":"demo","system":{"hostname":"h","users":[{"name":"demo","home":"/home/demo"},{"name":"alice","home":"/home/alice"}]},"commands":["su alice"]}"#;
        let raw = execute(input);
        let v: serde_json::Value =
            serde_json::from_slice(&raw).map_err(|e| format!("json parse: {}", e))?;
        assert_eq!(v["final-user"].as_str(), Some("alice"));
        assert_eq!(v["final-hostname"].as_str(), Some("h"));
        Ok(())
    }

    #[test]
    fn execute_final_user_unchanged_without_su() -> Result<(), String> {
        let input = br#"{"user":"demo","system":{"hostname":"h","users":[{"name":"demo","home":"/home/demo"}]},"commands":["echo hi"]}"#;
        let raw = execute(input);
        let v: serde_json::Value =
            serde_json::from_slice(&raw).map_err(|e| format!("json parse: {}", e))?;
        assert_eq!(v["final-user"].as_str(), Some("demo"));
        assert_eq!(v["final-hostname"].as_str(), Some("h"));
        Ok(())
    }

    #[test]
    fn execute_clear_drops_prior_entries() -> Result<(), String> {
        let input = br#"{"user":"u","system":{"hostname":"h","users":[{"name":"u","home":"/home/u"}]},"commands":["echo first","clear","echo second"]}"#;
        let raw = execute(input);
        let v: serde_json::Value =
            serde_json::from_slice(&raw).map_err(|e| format!("json parse: {}", e))?;
        let entries = v["entries"].as_array().ok_or("missing entries array")?;
        assert_eq!(entries.len(), 3, "expected all three commands");
        let cmd = entries[2]["command"]
            .as_str()
            .ok_or("missing command field")?;
        assert!(cmd.contains("second"));
        Ok(())
    }

    #[test]
    fn execute_include_files_returns_filesystem() -> Result<(), String> {
        let input = br#"{"user":"u","system":{"hostname":"h","users":[{"name":"u","home":"/home/u"}],"files":{"note.txt":"hello"}},"commands":["echo world > out.txt","ln -s note.txt link.txt","mkdir sub"],"include-files":true}"#;
        let raw = execute(input);
        let v: serde_json::Value =
            serde_json::from_slice(&raw).map_err(|e| format!("json parse: {}", e))?;
        let files = v["files"].as_object().ok_or("files should be present")?;
        // Check file
        let out = &files["/home/u/out.txt"];
        assert_eq!(out["type"].as_str(), Some("file"));
        assert_eq!(out["content"].as_str(), Some("world\n"));
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
        Ok(())
    }

    #[test]
    fn execute_without_include_files_omits_files() -> Result<(), String> {
        let input = br#"{"user":"u","system":{"hostname":"h","users":[{"name":"u","home":"/home/u"}]},"commands":["echo hi"]}"#;
        let raw = execute(input);
        let v: serde_json::Value =
            serde_json::from_slice(&raw).map_err(|e| format!("json parse: {}", e))?;
        assert!(
            v.get("files").is_none(),
            "files should be absent when not requested"
        );
        Ok(())
    }

    #[test]
    fn execute_sync_bg_no_bg_completions() -> Result<(), String> {
        let input = br#"{"user":"u","system":{"hostname":"h","users":[{"name":"u","home":"/home/u"}]},"commands":["echo hi &","echo done"]}"#;
        let raw = execute(input);
        let v: serde_json::Value =
            serde_json::from_slice(&raw).map_err(|e| format!("json parse: {}", e))?;
        let entries = v["entries"].as_array().ok_or("missing entries")?;
        // In sync mode, bg-completions should not appear (empty = skipped)
        for entry in entries {
            assert!(
                entry.get("bg-completions").is_none(),
                "sync mode should not have bg-completions: {:?}",
                entry
            );
        }
        Ok(())
    }

    #[test]
    fn execute_interleaved_bg_completions() -> Result<(), String> {
        let input = br#"{"user":"u","system":{"hostname":"h","users":[{"name":"u","home":"/home/u"}]},"commands":["echo bg &","echo fg"],"background-mode":"interleaved"}"#;
        let raw = execute(input);
        let v: serde_json::Value =
            serde_json::from_slice(&raw).map_err(|e| format!("json parse: {}", e))?;
        let entries = v["entries"].as_array().ok_or("missing entries")?;
        // The second entry (echo fg) should have bg-completions from the bg job
        let has_bg = entries.iter().any(|e| {
            e.get("bg-completions")
                .and_then(|v| v.as_array())
                .map(|a| !a.is_empty())
                .unwrap_or(false)
        });
        assert!(
            has_bg,
            "interleaved mode should produce bg-completions: {:?}",
            entries
        );
        Ok(())
    }

    #[test]
    fn execute_deferred_bg_runs_on_wait() -> Result<(), String> {
        let input = br#"{"user":"u","system":{"hostname":"h","users":[{"name":"u","home":"/home/u"}]},"commands":["echo hello > /tmp/out.txt &","wait","cat /tmp/out.txt"],"background-mode":"deferred","include-files":true}"#;
        let raw = execute(input);
        let v: serde_json::Value =
            serde_json::from_slice(&raw).map_err(|e| format!("json parse: {}", e))?;
        let entries = v["entries"].as_array().ok_or("missing entries")?;
        // cat /tmp/out.txt should show "hello"
        let last = entries.last().ok_or("no entries")?;
        assert_eq!(
            last["output"].as_str(),
            Some("hello\n"),
            "deferred bg should run on wait: {:?}",
            entries
        );
        Ok(())
    }

    #[test]
    fn execute_cat_propagates_lang_hint() -> Result<(), String> {
        let input = br#"{"user":"u","system":{"hostname":"h","users":[{"name":"u","home":"/home/u"}],"files":{"main.rs":"fn main() {}"}},"commands":["cat main.rs"]}"#;
        let raw = execute(input);
        let v: serde_json::Value =
            serde_json::from_slice(&raw).map_err(|e| format!("json parse: {}", e))?;
        let entries = v["entries"].as_array().ok_or("missing entries")?;
        assert_eq!(entries.len(), 1);
        assert_eq!(
            entries[0]["lang"].as_str(),
            Some("rust"),
            "cat main.rs should set lang to rust: {:?}",
            entries[0]
        );
        Ok(())
    }

    #[test]
    fn execute_cat_no_lang_for_unknown_ext() -> Result<(), String> {
        let input = br#"{"user":"u","system":{"hostname":"h","users":[{"name":"u","home":"/home/u"}],"files":{"data.txt":"hello"}},"commands":["cat data.txt"]}"#;
        let raw = execute(input);
        let v: serde_json::Value =
            serde_json::from_slice(&raw).map_err(|e| format!("json parse: {}", e))?;
        let entries = v["entries"].as_array().ok_or("missing entries")?;
        assert!(
            entries[0].get("lang").is_none() || entries[0]["lang"].is_null(),
            "cat data.txt should not set lang: {:?}",
            entries[0]
        );
        Ok(())
    }

    #[test]
    fn execute_history_records_commands() -> Result<(), String> {
        let input = br#"{"user":"u","system":{"hostname":"h","users":[{"name":"u","home":"/home/u"}]},"commands":["echo first","echo second","history"]}"#;
        let raw = execute(input);
        let v: serde_json::Value =
            serde_json::from_slice(&raw).map_err(|e| format!("json parse: {}", e))?;
        let entries = v["entries"].as_array().ok_or("missing entries")?;
        assert_eq!(entries.len(), 3);
        let hist_output = entries[2]["output"]
            .as_str()
            .ok_or("missing history output")?;
        assert!(
            hist_output.contains("echo first"),
            "history should contain 'echo first': {}",
            hist_output
        );
        assert!(
            hist_output.contains("echo second"),
            "history should contain 'echo second': {}",
            hist_output
        );
        assert!(
            hist_output.contains("history"),
            "history should contain 'history' itself: {}",
            hist_output
        );
        Ok(())
    }

    #[test]
    fn execute_errexit_stops_on_failure() -> Result<(), String> {
        let input = br#"{"user":"u","system":{"hostname":"h","users":[{"name":"u","home":"/home/u"}]},"commands":["set -e","false","echo should_not_run"]}"#;
        let raw = execute(input);
        let v: serde_json::Value =
            serde_json::from_slice(&raw).map_err(|e| format!("json parse: {}", e))?;
        let entries = v["entries"].as_array().ok_or("missing entries")?;
        // "set -e" and "false" should run, but "echo should_not_run" should NOT
        let has_should_not = entries.iter().any(|e| {
            e["output"]
                .as_str()
                .unwrap_or("")
                .contains("should_not_run")
        });
        assert!(
            !has_should_not,
            "errexit should stop after 'false': {:?}",
            entries
        );
        Ok(())
    }

    #[test]
    fn execute_exit_trap_fires() -> Result<(), String> {
        let input = br#"{"user":"u","system":{"hostname":"h","users":[{"name":"u","home":"/home/u"}]},"commands":["trap 'echo goodbye' EXIT","echo hello"]}"#;
        let raw = execute(input);
        let v: serde_json::Value =
            serde_json::from_slice(&raw).map_err(|e| format!("json parse: {}", e))?;
        let entries = v["entries"].as_array().ok_or("missing entries")?;
        let all_output: String = entries
            .iter()
            .filter_map(|e| e["output"].as_str())
            .collect();
        assert!(
            all_output.contains("goodbye"),
            "EXIT trap should fire at session end: {:?}",
            entries
        );
        Ok(())
    }

    #[test]
    fn execute_external_command_produces_delegate() -> Result<(), String> {
        let input = br#"{"user":"u","system":{"hostname":"h","users":[{"name":"u","home":"/home/u"}]},"commands":["jq '.name'"],"external-commands":["jq"]}"#;
        let raw = execute(input);
        let v: serde_json::Value =
            serde_json::from_slice(&raw).map_err(|e| format!("json parse: {}", e))?;
        let entries = v["entries"].as_array().ok_or("missing entries")?;
        assert_eq!(entries.len(), 1);
        let d = &entries[0]["delegate"];
        assert!(!d.is_null(), "should have delegate: {:?}", entries[0]);
        assert_eq!(d["command"].as_str(), Some("jq"));
        assert_eq!(d["args"][0].as_str(), Some(".name"));
        Ok(())
    }

    #[test]
    fn execute_external_in_pipeline_gets_stdin() -> Result<(), String> {
        let input = br#"{"user":"u","system":{"hostname":"h","users":[{"name":"u","home":"/home/u"}],"files":{"data.json":"{\"name\":\"alice\"}"}},"commands":["cat data.json | jq '.name'"],"external-commands":["jq"]}"#;
        let raw = execute(input);
        let v: serde_json::Value =
            serde_json::from_slice(&raw).map_err(|e| format!("json parse: {}", e))?;
        let entries = v["entries"].as_array().ok_or("missing entries")?;
        assert_eq!(entries.len(), 1);
        let d = &entries[0]["delegate"];
        assert!(!d.is_null(), "should have delegate: {:?}", entries[0]);
        assert_eq!(d["command"].as_str(), Some("jq"));
        let stdin = d["stdin"].as_str().unwrap_or("");
        assert!(
            stdin.contains("alice"),
            "delegate stdin should contain cat output: {}",
            stdin
        );
        Ok(())
    }

    #[test]
    fn execute_unknown_command_without_external_gives_127() -> Result<(), String> {
        let input = br#"{"user":"u","system":{"hostname":"h","users":[{"name":"u","home":"/home/u"}]},"commands":["jq '.name'"]}"#;
        let raw = execute(input);
        let v: serde_json::Value =
            serde_json::from_slice(&raw).map_err(|e| format!("json parse: {}", e))?;
        let entries = v["entries"].as_array().ok_or("missing entries")?;
        assert_eq!(entries[0]["exit-code"].as_i64(), Some(127));
        assert!(
            entries[0].get("delegate").is_none() || entries[0]["delegate"].is_null(),
            "should NOT have delegate without external-commands"
        );
        Ok(())
    }
}
