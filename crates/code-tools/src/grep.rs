//! GrepTool — search file contents using ripgrep.
//!
//! Ref: src/tools/GrepTool/GrepTool.ts

use std::path::Path;
use std::time::SystemTime;

use async_trait::async_trait;
use code_permissions::evaluator::ToolCallContext;
use code_types::tool::{ToolInputSchema, ToolResult, ToolResultPayload, ValidationResult};
use serde::Deserialize;
use serde_json::{json, Value};
use tokio::process::Command;

use crate::{error_result, ProgressSender, Tool, ToolContext};

// ── Input ─────────────────────────────────────────────────────────────────────

#[derive(Deserialize, Default)]
#[serde(rename_all = "snake_case")]
struct GrepInput {
    pattern: String,
    path: Option<String>,
    glob: Option<String>,
    output_mode: Option<OutputMode>,
    #[serde(rename = "-B")]
    before: Option<u32>,
    #[serde(rename = "-A")]
    after: Option<u32>,
    #[serde(rename = "-C")]
    context_flag: Option<u32>,
    context: Option<u32>,
    #[serde(rename = "-n")]
    show_line_numbers: Option<bool>,
    #[serde(rename = "-i")]
    case_insensitive: Option<bool>,
    #[serde(rename = "type")]
    file_type: Option<String>,
    head_limit: Option<usize>,
    offset: Option<usize>,
    multiline: Option<bool>,
}

#[derive(Deserialize, Default, PartialEq)]
#[serde(rename_all = "snake_case")]
enum OutputMode {
    #[default]
    FilesWithMatches,
    Content,
    Count,
}

// ── Tool ─────────────────────────────────────────────────────────────────────

pub struct GrepTool;

#[async_trait]
impl Tool for GrepTool {
    fn name(&self) -> &str {
        "Grep"
    }

    fn description(&self) -> &str {
        "A powerful search tool built on ripgrep. \
        Supports full regex syntax. \
        Filter files with glob parameter (e.g., \"*.js\") or type parameter (e.g., \"js\"). \
        Output modes: \"content\" shows matching lines, \"files_with_matches\" shows only file paths (default), \
        \"count\" shows match counts."
    }

    fn input_schema(&self) -> ToolInputSchema {
        json!({
            "type": "object",
            "properties": {
                "pattern": { "type": "string", "description": "The regular expression pattern to search for" },
                "path": { "type": "string", "description": "File or directory to search in. Defaults to current working directory." },
                "glob": { "type": "string", "description": "Glob pattern to filter files (e.g. \"*.js\", \"*.{ts,tsx}\")" },
                "output_mode": {
                    "type": "string",
                    "enum": ["content", "files_with_matches", "count"],
                    "description": "Output mode (default: files_with_matches)"
                },
                "-B": { "type": "number", "description": "Lines to show before each match (requires output_mode: content)" },
                "-A": { "type": "number", "description": "Lines to show after each match (requires output_mode: content)" },
                "-C": { "type": "number", "description": "Lines to show before and after each match" },
                "context": { "type": "number", "description": "Alias for -C" },
                "-n": { "type": "boolean", "description": "Show line numbers (default: true for content mode)" },
                "-i": { "type": "boolean", "description": "Case insensitive search" },
                "type": { "type": "string", "description": "File type to search (e.g. js, py, rust, go)" },
                "head_limit": { "type": "number", "description": "Limit output to first N lines/entries (default: 250)" },
                "offset": { "type": "number", "description": "Skip first N entries" },
                "multiline": { "type": "boolean", "description": "Enable multiline mode" }
            },
            "required": ["pattern"]
        })
    }

    fn is_read_only(&self, _input: &Value) -> bool {
        true
    }

    async fn validate_input(&self, input: &Value, _ctx: &ToolContext) -> ValidationResult {
        match input.get("pattern").and_then(|v| v.as_str()) {
            Some(p) if !p.is_empty() => ValidationResult::ok(),
            Some(_) => ValidationResult::err("pattern must not be empty", 1),
            None => ValidationResult::err("pattern is required", 1),
        }
    }

    fn permission_context<'a>(
        &'a self,
        input: &'a Value,
        cwd: &'a Path,
    ) -> ToolCallContext<'a> {
        ToolCallContext {
            tool_name: self.name(),
            content: input.get("path").and_then(|v| v.as_str()),
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
        let parsed: GrepInput = match serde_json::from_value(input) {
            Ok(v) => v,
            Err(e) => return error_result(tool_use_id, format!("Invalid input: {e}")),
        };

        let start = std::time::Instant::now();

        // Resolve search path.
        let search_path = match parsed.path {
            Some(ref p) => {
                let expanded = expand_tilde(p);
                if expanded.is_absolute() { expanded } else { ctx.cwd.join(&expanded) }
            }
            None => ctx.cwd.clone(),
        };

        let mode = parsed.output_mode.unwrap_or_default();
        let head_limit = parsed.head_limit.unwrap_or(250);
        let offset = parsed.offset.unwrap_or(0);
        let context_lines = parsed.context_flag.or(parsed.context);

        // Build rg arguments.
        let mut args: Vec<String> = vec!["--no-heading".into()];

        match mode {
            OutputMode::FilesWithMatches => args.push("--files-with-matches".into()),
            OutputMode::Count => args.push("--count".into()),
            OutputMode::Content => {
                // Line numbers default to true in content mode.
                if parsed.show_line_numbers.unwrap_or(true) {
                    args.push("-n".into());
                }
                if let Some(c) = context_lines {
                    args.push(format!("-C{c}"));
                } else {
                    if let Some(b) = parsed.before { args.push(format!("-B{b}")); }
                    if let Some(a) = parsed.after { args.push(format!("-A{a}")); }
                }
            }
        }

        if parsed.case_insensitive.unwrap_or(false) {
            args.push("-i".into());
        }

        if parsed.multiline.unwrap_or(false) {
            args.push("-U".into());
            args.push("--multiline-dotall".into());
        }

        if let Some(ref glob) = parsed.glob {
            args.push(format!("--glob={glob}"));
        }

        if let Some(ref t) = parsed.file_type {
            args.push(format!("--type={t}"));
        }

        args.push("-e".into());
        args.push(parsed.pattern.clone());
        args.push("--".into());
        args.push(search_path.to_string_lossy().into_owned());

        // Spawn rg.
        let output = match Command::new("rg").args(&args).output().await {
            Ok(o) => o,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return error_result(
                    tool_use_id,
                    "ripgrep (rg) not found in PATH. Please install ripgrep: https://github.com/BurntSushi/ripgrep",
                );
            }
            Err(e) => return error_result(tool_use_id, format!("Failed to run rg: {e}")),
        };

        // Exit code 1 = no matches (not an error); 2 = real error.
        if output.status.code() == Some(2) {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return error_result(tool_use_id, format!("rg error: {stderr}"));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let all_lines: Vec<&str> = stdout.lines().collect();
        let total = all_lines.len();

        // Apply offset and head_limit.
        let start_idx = offset.min(total);
        let end_idx = (start_idx + head_limit).min(total);
        let slice = &all_lines[start_idx..end_idx];
        let applied_limit = slice.len() < total.saturating_sub(start_idx);

        let duration_ms = start.elapsed().as_millis() as u64;

        match mode {
            OutputMode::Content => {
                let content_text = slice.join("\n");
                ToolResult {
                    tool_use_id: tool_use_id.to_owned(),
                    content: ToolResultPayload::Text(content_text),
                    is_error: false,
                    was_truncated: false,
                }
            }
            OutputMode::FilesWithMatches => {
                // Make paths relative to cwd, sort by mtime.
                let mut files: Vec<String> = slice
                    .iter()
                    .map(|l| make_relative(l.trim(), &ctx.cwd))
                    .collect();

                // Sort by mtime descending (best effort — skip on error).
                sort_by_mtime(&ctx.cwd, &mut files).await;

                ToolResult {
                    tool_use_id: tool_use_id.to_owned(),
                    content: ToolResultPayload::Json(json!({
                        "filenames": files,
                        "num_files": files.len(),
                        "duration_ms": duration_ms,
                        "applied_limit": applied_limit,
                        "applied_offset": offset,
                    })),
                    is_error: false,
                    was_truncated: false,
                }
            }
            OutputMode::Count => {
                ToolResult {
                    tool_use_id: tool_use_id.to_owned(),
                    content: ToolResultPayload::Json(json!({
                        "counts": slice.iter().map(|l| l.to_string()).collect::<Vec<_>>(),
                        "duration_ms": duration_ms,
                    })),
                    is_error: false,
                    was_truncated: false,
                }
            }
        }
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn expand_tilde(path: &str) -> std::path::PathBuf {
    if path.starts_with("~/") || path == "~" {
        if let Some(home) = dirs_next::home_dir() {
            return home.join(&path[2..]);
        }
    }
    std::path::PathBuf::from(path)
}

fn make_relative(path: &str, cwd: &Path) -> String {
    let p = Path::new(path);
    if let Ok(rel) = p.strip_prefix(cwd) {
        return rel.to_string_lossy().into_owned();
    }
    path.to_owned()
}

async fn sort_by_mtime(cwd: &Path, files: &mut Vec<String>) {
    let mut with_time: Vec<(String, SystemTime)> = Vec::with_capacity(files.len());
    for f in files.iter() {
        let full = if Path::new(f).is_absolute() {
            Path::new(f).to_path_buf()
        } else {
            cwd.join(f)
        };
        let mtime = tokio::fs::metadata(&full)
            .await
            .ok()
            .and_then(|m| m.modified().ok())
            .unwrap_or(SystemTime::UNIX_EPOCH);
        with_time.push((f.clone(), mtime));
    }
    with_time.sort_by(|a, b| b.1.cmp(&a.1));
    *files = with_time.into_iter().map(|(f, _)| f).collect();
}
