//! FileReadTool — read files from the filesystem.
//!
//! Ref: src/tools/FileReadTool/FileReadTool.ts

use std::path::Path;

use async_trait::async_trait;
use code_permissions::evaluator::ToolCallContext;
use code_types::tool::{ToolInputSchema, ToolResult, ToolResultPayload, ValidationResult};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::{error_result, ProgressSender, Tool, ToolContext};

// ── Blocked device files ──────────────────────────────────────────────────────

const BLOCKED_DEVICES: &[&str] = &[
    "/dev/zero",
    "/dev/random",
    "/dev/urandom",
    "/dev/stdin",
    "/dev/tty",
    "/dev/full",
    "/dev/null",
];

// ── Common binary extensions (not exhaustive, but covers the common cases) ────

const BINARY_EXTENSIONS: &[&str] = &[
    "exe", "dll", "so", "dylib", "pyd", "pyc", "class", "jar", "war", "ear",
    "bin", "obj", "o", "a", "lib", "out",
    "zip", "tar", "gz", "bz2", "xz", "7z", "rar", "zst",
    "png", "jpg", "jpeg", "gif", "bmp", "ico", "tiff", "webp", "avif",
    "mp3", "mp4", "wav", "ogg", "flac", "aac", "mkv", "avi", "mov",
    "pdf", "doc", "docx", "xls", "xlsx", "ppt", "pptx",
    "wasm", "node",
];

// ── Input ─────────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct FileReadInput {
    file_path: String,
    offset: Option<usize>,
    limit: Option<usize>,
}

// ── Tool ─────────────────────────────────────────────────────────────────────

pub struct FileReadTool;

#[async_trait]
impl Tool for FileReadTool {
    fn name(&self) -> &str {
        "Read"
    }

    fn description(&self) -> &str {
        "Reads a file from the local filesystem. You can access any file directly by using this \
        tool. Assume this tool is able to read all files on the machine. If the User provides a \
        path to a file assume that path is valid.\n\n\
        By default, it reads up to 2000 lines starting from the beginning of the file. \
        When you already know which part of the file you need, only read that part.\n\n\
        Results are returned using cat -n format, with line numbers starting at 1."
    }

    fn input_schema(&self) -> ToolInputSchema {
        json!({
            "type": "object",
            "properties": {
                "file_path": {
                    "type": "string",
                    "description": "The absolute path to the file to read"
                },
                "offset": {
                    "type": "number",
                    "description": "The line number to start reading from (1-indexed). Only provide if reading a large file."
                },
                "limit": {
                    "type": "number",
                    "description": "The number of lines to read. Only provide if the file is too large to read at once."
                }
            },
            "required": ["file_path"]
        })
    }

    fn is_read_only(&self, _input: &Value) -> bool {
        true
    }

    async fn validate_input(&self, input: &Value, _ctx: &ToolContext) -> ValidationResult {
        let file_path = match input.get("file_path").and_then(|v| v.as_str()) {
            Some(p) => p,
            None => return ValidationResult::err("file_path is required", 1),
        };

        if file_path.is_empty() {
            return ValidationResult::err("file_path must not be empty", 1);
        }

        // Reject UNC paths on Windows.
        if file_path.starts_with("\\\\") {
            return ValidationResult::err("UNC paths are not supported", 1);
        }

        // Reject known infinite/blocking device files.
        for blocked in BLOCKED_DEVICES {
            if file_path == *blocked {
                return ValidationResult::err(
                    format!("Reading '{file_path}' is not allowed (blocked device file)"),
                    1,
                );
            }
        }

        // Reject binary file extensions.
        if let Some(ext) = std::path::Path::new(file_path)
            .extension()
            .and_then(|e| e.to_str())
        {
            if BINARY_EXTENSIONS.contains(&ext.to_lowercase().as_str()) {
                return ValidationResult::err(
                    format!(
                        "Reading binary files (.{ext}) is not supported. \
                         Try using BashTool with 'xxd' or 'strings' to inspect binary content."
                    ),
                    1,
                );
            }
        }

        ValidationResult::ok()
    }

    fn permission_context<'a>(
        &'a self,
        input: &'a Value,
        cwd: &'a Path,
    ) -> ToolCallContext<'a> {
        ToolCallContext {
            tool_name: self.name(),
            content: input.get("file_path").and_then(|v| v.as_str()),
            input: Some(input),
            is_read_only: true,
            cwd,
        }
    }

    async fn call(
        &self,
        tool_use_id: &str,
        input: Value,
        ctx: &ToolContext,
        _progress: Option<&ProgressSender>,
    ) -> ToolResult {
        let parsed: FileReadInput = match serde_json::from_value(input) {
            Ok(v) => v,
            Err(e) => return error_result(tool_use_id, format!("Invalid input: {e}")),
        };

        let path = resolve_path(&parsed.file_path, &ctx.cwd);

        // Check file size limit before reading.
        match tokio::fs::metadata(&path).await {
            Ok(meta) => {
                if let Some(max) = ctx.file_reading_limits.max_size_bytes {
                    if meta.len() as usize > max {
                        return error_result(
                            tool_use_id,
                            format!(
                                "File is too large to read ({} bytes, limit is {} bytes). \
                                 Use offset and limit parameters to read a portion of the file.",
                                meta.len(),
                                max
                            ),
                        );
                    }
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                let suggestions = find_similar_files(&parsed.file_path, &ctx.cwd).await;
                let mut msg = format!("File not found: {}", path.display());
                if !suggestions.is_empty() {
                    msg.push_str("\n\nDid you mean one of these?");
                    for s in &suggestions {
                        msg.push_str(&format!("\n  - {s}"));
                    }
                }
                return error_result(tool_use_id, msg);
            }
            Err(e) => {
                return error_result(tool_use_id, format!("Cannot access file: {e}"));
            }
        }

        let content = match tokio::fs::read_to_string(&path).await {
            Ok(c) => c,
            Err(e) => {
                return error_result(tool_use_id, format!("Error reading file: {e}"));
            }
        };

        let lines: Vec<&str> = content.lines().collect();
        let total_lines = lines.len();

        // Apply offset (1-based) and limit.
        let offset = parsed.offset.unwrap_or(1).saturating_sub(1); // convert to 0-based
        let limit = parsed.limit.unwrap_or(2000);

        let end = (offset + limit).min(total_lines);
        let slice = if offset < total_lines {
            &lines[offset..end]
        } else {
            &lines[0..0]
        };

        // Format with line numbers (cat -n style: right-justified, tab-separated).
        let numbered: String = slice
            .iter()
            .enumerate()
            .map(|(i, line)| format!("{:>6}\t{line}\n", offset + i + 1))
            .collect();

        ToolResult {
            tool_use_id: tool_use_id.to_owned(),
            content: ToolResultPayload::Text(numbered),
            is_error: false,
            was_truncated: false,
        }
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn resolve_path(file_path: &str, cwd: &Path) -> std::path::PathBuf {
    let expanded = expand_tilde(file_path);
    if expanded.is_absolute() {
        expanded
    } else {
        cwd.join(&expanded)
    }
}

fn expand_tilde(path: &str) -> std::path::PathBuf {
    if path.starts_with("~/") || path == "~" {
        if let Some(home) = dirs_next::home_dir() {
            return home.join(&path[2..]);
        }
    }
    std::path::PathBuf::from(path)
}

/// Find files in `cwd` whose names are similar to the given path (simple prefix match).
async fn find_similar_files(file_path: &str, cwd: &Path) -> Vec<String> {
    let name = Path::new(file_path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_lowercase();

    if name.is_empty() {
        return vec![];
    }

    let prefix: String = name.chars().take(3).collect();
    let mut results = Vec::new();

    if let Ok(mut dir) = tokio::fs::read_dir(cwd).await {
        while let Ok(Some(entry)) = dir.next_entry().await {
            if let Some(n) = entry.file_name().to_str() {
                if n.to_lowercase().starts_with(&prefix) {
                    results.push(cwd.join(n).to_string_lossy().into_owned());
                    if results.len() >= 5 {
                        break;
                    }
                }
            }
        }
    }

    results
}
