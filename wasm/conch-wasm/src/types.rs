use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

// Re-export bare_vfs types for use across the crate
pub use bare_vfs::Entry as FsEntry;

/// Input configuration from Typst
#[derive(Deserialize)]
pub struct Config {
    pub user: String,
    pub hostname: String,
    pub home: String,
    pub files: BTreeMap<String, FileSpec>,
    pub commands: Vec<String>,
    #[serde(default)]
    pub date: Option<String>,
}

/// A single command's output in the terminal session
#[derive(Serialize)]
pub struct OutputEntry {
    pub user: String,
    pub hostname: String,
    pub path: String,
    pub command: String,
    pub output: String,
    #[serde(rename = "exit-code")]
    pub exit_code: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lang: Option<String>,
}

/// Full session output returned to Typst
#[derive(Serialize)]
pub struct SessionOutput {
    pub entries: Vec<OutputEntry>,
    #[serde(rename = "final-path")]
    pub final_path: String,
}

/// Deserialization helper: accept either a string or {content, mode} object
#[derive(Deserialize)]
#[serde(untagged)]
pub enum FileSpec {
    Content(String),
    WithMode { content: String, mode: u16 },
}
