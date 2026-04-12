//! Wire-format types for the JSON interface between Typst and the WASM shell.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::Str;

pub use bare_vfs::Entry as FsEntry;

/// How `cmd &` behaves.
#[derive(Deserialize, Default, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum BackgroundMode {
    /// Run immediately to completion (default, Typst-safe).
    #[default]
    Sync,
    /// Store as lazy job; execute on `wait`.
    Deferred,
    /// Store as lazy job; step during foreground execution.
    Interleaved,
}

/// Input configuration from Typst
#[derive(Deserialize)]
pub struct Config {
    pub user: String,
    #[serde(default)]
    pub system: Option<SystemSpec>,
    // Legacy flat fields (used when system is None)
    #[serde(default)]
    pub hostname: Option<String>,
    #[serde(default)]
    pub home: Option<String>,
    #[serde(default)]
    pub files: Option<BTreeMap<String, FileSpec>>,
    pub commands: Vec<String>,
    #[serde(default)]
    pub date: Option<String>,
    #[serde(default, rename = "include-files")]
    pub include_files: bool,
    #[serde(default, rename = "background-mode")]
    pub background_mode: BackgroundMode,
}

#[derive(Deserialize, Default)]
pub struct SystemSpec {
    #[serde(default = "default_hostname")]
    pub hostname: String,
    #[serde(default)]
    pub users: Vec<UserSpec>,
    #[serde(default)]
    pub groups: Vec<GroupSpec>,
    #[serde(default)]
    pub files: BTreeMap<String, FileSpec>,
}

fn default_hostname() -> String {
    "conch".to_string()
}

#[derive(Deserialize)]
pub struct UserSpec {
    pub name: String,
    #[serde(default)]
    pub uid: Option<u32>,
    #[serde(default)]
    pub home: Option<String>,
    #[serde(default)]
    pub groups: Vec<String>,
}

#[derive(Deserialize)]
pub struct GroupSpec {
    pub name: String,
    #[serde(default)]
    pub gid: Option<u32>,
    #[serde(default)]
    pub members: Vec<String>,
}

/// A single command's output in the terminal session
#[derive(Serialize)]
pub struct OutputEntry {
    pub user: Str,
    pub hostname: Str,
    pub path: String,
    pub command: String,
    pub output: String,
    #[serde(rename = "exit-code")]
    pub exit_code: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lang: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "first-line")]
    pub first_line: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "last-line")]
    pub last_line: Option<u32>,
    #[serde(
        default,
        skip_serializing_if = "Vec::is_empty",
        rename = "bg-completions"
    )]
    pub bg_completions: Vec<String>,
}

/// Statement range info returned by `analyze_script`.
#[derive(Serialize)]
pub struct StatementRange {
    #[serde(rename = "first-line")]
    pub first_line: u32,
    #[serde(rename = "last-line")]
    pub last_line: u32,
    pub source: String,
}

/// Result of `analyze_script` — structural info about a script.
#[derive(Serialize)]
pub struct ScriptAnalysis {
    pub statements: Vec<StatementRange>,
}

/// A filesystem entry in the output snapshot.
#[derive(Serialize)]
#[serde(tag = "type")]
pub enum FileOutput {
    #[serde(rename = "file")]
    File {
        content: String,
        #[serde(serialize_with = "serialize_mode")]
        mode: u16,
    },
    #[serde(rename = "dir")]
    Dir {
        #[serde(serialize_with = "serialize_mode")]
        mode: u16,
    },
    #[serde(rename = "symlink")]
    Symlink { target: String },
}

fn serialize_mode<S: serde::Serializer>(mode: &u16, s: S) -> Result<S::Ok, S::Error> {
    s.serialize_str(&format!("{:o}", mode))
}

/// Full session output returned to Typst
#[derive(Serialize)]
pub struct SessionOutput {
    pub entries: Vec<OutputEntry>,
    #[serde(rename = "final-path")]
    pub final_path: String,
    #[serde(rename = "final-user")]
    pub final_user: String,
    #[serde(rename = "final-hostname")]
    pub final_hostname: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub files: Option<BTreeMap<String, FileOutput>>,
}

/// Deserialization helper: accept either a string or {content, mode} object
#[derive(Deserialize)]
#[serde(untagged)]
pub enum FileSpec {
    Content(String),
    WithMode { content: String, mode: u16 },
}
