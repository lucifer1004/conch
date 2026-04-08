use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

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

/// Virtual filesystem entry
#[derive(Clone)]
pub enum FsEntry {
    File { content: String, mode: u16 },
    Dir { mode: u16 },
}

impl FsEntry {
    pub fn file(content: String) -> Self {
        FsEntry::File {
            content,
            mode: 0o644,
        }
    }

    pub fn file_with_mode(content: String, mode: u16) -> Self {
        FsEntry::File { content, mode }
    }

    pub fn dir() -> Self {
        FsEntry::Dir { mode: 0o755 }
    }

    pub fn is_dir(&self) -> bool {
        matches!(self, FsEntry::Dir { .. })
    }

    pub fn is_file(&self) -> bool {
        matches!(self, FsEntry::File { .. })
    }

    pub fn content(&self) -> Option<&str> {
        match self {
            FsEntry::File { content, .. } => Some(content),
            _ => None,
        }
    }

    pub fn mode(&self) -> u16 {
        match self {
            FsEntry::File { mode, .. } | FsEntry::Dir { mode, .. } => *mode,
        }
    }

    /// Check if owner has read permission
    pub fn is_readable(&self) -> bool {
        self.mode() & 0o400 != 0
    }

    /// Check if owner has write permission
    pub fn is_writable(&self) -> bool {
        self.mode() & 0o200 != 0
    }

    /// Format mode as Unix permission string (e.g., "rwxr-xr-x")
    pub fn mode_string(&self) -> String {
        Self::format_mode(self.mode())
    }

    /// Format a raw mode number as Unix permission string
    pub fn format_mode(m: u16) -> String {
        let mut s = String::with_capacity(9);
        for shift in [6, 3, 0] {
            let bits = (m >> shift) & 0o7;
            s.push(if bits & 4 != 0 { 'r' } else { '-' });
            s.push(if bits & 2 != 0 { 'w' } else { '-' });
            s.push(if bits & 1 != 0 { 'x' } else { '-' });
        }
        s
    }
}

/// Deserialization helper: accept either a string or {content, mode} object
#[derive(Deserialize)]
#[serde(untagged)]
pub enum FileSpec {
    Content(String),
    WithMode { content: String, mode: u16 },
}
