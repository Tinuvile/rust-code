//! GlobTool — find files by pattern.
//!
//! Ref: src/tools/GlobTool/GlobTool.ts

use std::path::{Path, PathBuf};
use std::time::SystemTime;

use async_trait::async_trait;
use code_permissions::evaluator::ToolCallContext;
use code_types::tool::{ToolInputSchema, ToolResult, ToolResultPayload, ValidationResult};
use globset::{Glob, GlobSetBuilder};
use serde::Deserialize;
use serde_json::{json, Value};
use walkdir::WalkDir;

use crate::{error_result, ProgressSender, Tool, ToolContext};

// ── VCS directories to skip ───────────────────────────────────────────────────

const VCS_DIRS: &[&str] = &[".git", ".svn", ".hg", ".bzr", "_darcs", "CVS"];

// ── Input ─────────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct GlobInput {
    pattern: String,
    path: Option<String>,
}

// ── Tool ─────────────────────────────────────────────────────────────────────

pub struct GlobTool;

#[async_trait]
impl Tool for GlobTool {
    fn name(&self) -> &str {
        "Glob"
    }

    fn description(&self) -> &str {
        "Fast file pattern matching tool that works with any codebase size. \
        Supports glob patterns like \"**/*.js\" or \"src/**/*.ts\". \
        Returns matching file paths sorted by modification time. \
        Use this tool when you need to find files by name patterns."
    }

    fn input_schema(&self) -> ToolInputSchema {
        json!({
            "type": "object",
            "properties": {
                "pattern": {
                    "type": "string",
                    "description": "The glob pattern to match files against"
                },
                "path": {
                    "type": "string",
                    "description": "The directory to search in. Defaults to the current working directory."
                }
            },
            "required": ["pattern"]
        })
    }

    fn is_read_only(&self, _input: &Value) -> bool {
        true
    }

    async fn validate_input(&self, input: &Value, _ctx: &ToolContext) -> ValidationResult {
        let pattern = match input.get("pattern").and_then(|v| v.as_str()) {
            Some(p) => p,
            None => return ValidationResult::err("pattern is required", 1),
        };
        if pattern.is_empty() {
            return ValidationResult::err("pattern must not be empty", 1);
        }
        // Validate glob compiles.
        if let Err(e) = Glob::new(pattern) {
            return ValidationResult::err(format!("Invalid glob pattern: {e}"), 1);
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
        let parsed: GlobInput = match serde_json::from_value(input) {
            Ok(v) => v,
            Err(e) => return error_result(tool_use_id, format!("Invalid input: {e}")),
        };

        let start = std::time::Instant::now();

        // Resolve search directory.
        let search_dir = match parsed.path {
            Some(ref p) => {
                let expanded = expand_tilde(p);
                if expanded.is_absolute() {
                    expanded
                } else {
                    ctx.cwd.join(&expanded)
                }
            }
            None => ctx.cwd.clone(),
        };

        if !search_dir.exists() {
            return error_result(
                tool_use_id,
                format!("Directory does not exist: {}", search_dir.display()),
            );
        }

        // Build glob set.
        let glob = match Glob::new(&parsed.pattern) {
            Ok(g) => g,
            Err(e) => return error_result(tool_use_id, format!("Invalid glob pattern: {e}")),
        };
        let mut builder = GlobSetBuilder::new();
        builder.add(glob);
        let glob_set = match builder.build() {
            Ok(g) => g,
            Err(e) => return error_result(tool_use_id, format!("Glob build error: {e}")),
        };

        let max_results = ctx.glob_limits.max_results.unwrap_or(100);

        // Walk and collect matching files with their mtimes.
        let mut matches: Vec<(PathBuf, SystemTime)> = Vec::new();
        let mut truncated = false;

        'walk: for entry in WalkDir::new(&search_dir)
            .follow_links(false)
            .into_iter()
            .filter_entry(|e| !is_vcs_dir(e))
        {
            let entry = match entry {
                Ok(e) => e,
                Err(_) => continue,
            };

            // Only match files, not directories.
            if !entry.file_type().is_file() {
                continue;
            }

            // Match against the glob using a path relative to search_dir.
            let rel = match entry.path().strip_prefix(&search_dir) {
                Ok(r) => r,
                Err(_) => entry.path(),
            };

            if glob_set.is_match(rel) {
                if matches.len() >= max_results {
                    truncated = true;
                    break 'walk;
                }
                let mtime = entry
                    .metadata()
                    .ok()
                    .and_then(|m| m.modified().ok())
                    .unwrap_or(SystemTime::UNIX_EPOCH);
                matches.push((entry.into_path(), mtime));
            }
        }

        // Sort by modification time, newest first.
        matches.sort_by(|a, b| b.1.cmp(&a.1));

        let filenames: Vec<String> = matches
            .iter()
            .map(|(p, _)| p.to_string_lossy().into_owned())
            .collect();

        let duration_ms = start.elapsed().as_millis() as u64;
        let num_files = filenames.len();

        ToolResult {
            tool_use_id: tool_use_id.to_owned(),
            content: ToolResultPayload::Json(json!({
                "filenames": filenames,
                "num_files": num_files,
                "truncated": truncated,
                "duration_ms": duration_ms,
            })),
            is_error: false,
            was_truncated: false,
        }
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn is_vcs_dir(entry: &walkdir::DirEntry) -> bool {
    if entry.file_type().is_dir() {
        if let Some(name) = entry.file_name().to_str() {
            return VCS_DIRS.contains(&name);
        }
    }
    false
}

fn expand_tilde(path: &str) -> PathBuf {
    if path.starts_with("~/") || path == "~" {
        if let Some(home) = dirs_next::home_dir() {
            return home.join(&path[2..]);
        }
    }
    PathBuf::from(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ToolContext;
    use code_types::permissions::ToolPermissionContext;

    fn ctx(dir: &Path) -> ToolContext {
        ToolContext {
            cwd: dir.to_path_buf(),
            session_id: "test".into(),
            session_dir: dir.to_path_buf(),
            permission_ctx: ToolPermissionContext::default(),
            file_reading_limits: Default::default(),
            glob_limits: Default::default(),
        }
    }

    #[tokio::test]
    async fn finds_toml_files() {
        let tool = GlobTool;
        let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let input = json!({ "pattern": "*.toml" });
        let result = tool.call("id1", input, &ctx(&dir), None).await;
        assert!(!result.is_error);
        if let ToolResultPayload::Json(v) = result.content {
            let files = v["filenames"].as_array().unwrap();
            assert!(!files.is_empty());
            assert!(files.iter().any(|f| f.as_str().unwrap().ends_with(".toml")));
        }
    }
}
