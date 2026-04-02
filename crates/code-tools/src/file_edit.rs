//! FileEditTool — perform exact string replacements in files.
//!
//! Ref: src/tools/FileEditTool/FileEditTool.ts

use std::path::Path;

use async_trait::async_trait;
use code_permissions::evaluator::ToolCallContext;
use code_types::tool::{ToolInputSchema, ToolResult, ValidationResult};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::{error_result, ok_result, ProgressSender, Tool, ToolContext};

// ── Anti-sanitization table ───────────────────────────────────────────────────
//
// The model sometimes receives XML-escaped tool output that it then mirrors
// back in an edit.  We un-escape those sequences before matching.
//
// Ref: src/tools/FileEditTool/FileEditTool.ts DESANITIZATIONS

const DESANITIZATIONS: &[(&str, &str)] = &[
    ("<fnr>", "<function_results>"),
    ("<n>", "<name>"),
    ("</n>", "</name>"),
    ("<o>", "<output>"),
    ("</o>", "</output>"),
    ("<e>", "<error>"),
    ("</e>", "</error>"),
    ("<s>", "<system>"),
    ("</s>", "</system>"),
    ("<r>", "<result>"),
    ("</r>", "</result>"),
    ("\n\nH:", "\n\nHuman:"),
    ("\n\nA:", "\n\nAssistant:"),
];

// ── Input ─────────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct FileEditInput {
    file_path: String,
    old_string: String,
    new_string: String,
    replace_all: Option<bool>,
}

// ── Resolved match variant ────────────────────────────────────────────────────

/// Which normalization strategy found the match.
#[derive(Debug, Clone)]
enum MatchVariant {
    Exact,
    NormalizedQuotes { normalized_old: String },
    Desanitized { desanitized_old: String },
}

// ── Tool ─────────────────────────────────────────────────────────────────────

pub struct FileEditTool;

#[async_trait]
impl Tool for FileEditTool {
    fn name(&self) -> &str {
        "Edit"
    }

    fn description(&self) -> &str {
        "Performs exact string replacements in files.\n\n\
        Usage:\n\
        - You must use the Read tool at least once before editing. \
          This tool will error if you attempt an edit without reading the file.\n\
        - When editing text from Read tool output, ensure you preserve the exact indentation.\n\
        - ALWAYS prefer editing existing files. NEVER write new files unless explicitly required.\n\
        - The edit will FAIL if old_string is not unique in the file.\n\
        - Use replace_all for replacing and renaming strings across the file."
    }

    fn input_schema(&self) -> ToolInputSchema {
        json!({
            "type": "object",
            "properties": {
                "file_path": {
                    "type": "string",
                    "description": "The absolute path to the file to modify"
                },
                "old_string": {
                    "type": "string",
                    "description": "The text to replace"
                },
                "new_string": {
                    "type": "string",
                    "description": "The text to replace it with (must be different from old_string)"
                },
                "replace_all": {
                    "type": "boolean",
                    "description": "Replace all occurrences of old_string (default: false)"
                }
            },
            "required": ["file_path", "old_string", "new_string"]
        })
    }

    fn is_read_only(&self, _input: &Value) -> bool {
        false
    }

    async fn validate_input(&self, input: &Value, _ctx: &ToolContext) -> ValidationResult {
        let file_path = match input.get("file_path").and_then(|v| v.as_str()) {
            Some(p) => p,
            None => return ValidationResult::err("file_path is required", 1),
        };
        let old_string = match input.get("old_string").and_then(|v| v.as_str()) {
            Some(s) => s,
            None => return ValidationResult::err("old_string is required", 1),
        };
        let new_string = match input.get("new_string").and_then(|v| v.as_str()) {
            Some(s) => s,
            None => return ValidationResult::err("new_string is required", 1),
        };
        let replace_all = input
            .get("replace_all")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        if old_string == new_string {
            return ValidationResult::err(
                "No changes: old_string and new_string are identical.",
                1,
            );
        }

        if file_path.is_empty() {
            return ValidationResult::err("file_path must not be empty", 1);
        }

        // Validate the match now (read file during validate phase).
        let path = resolve_path(file_path, std::path::Path::new("."));
        let content = match std::fs::read_to_string(&path) {
            Ok(c) => c,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                // Allow creating a new file when old_string is empty.
                if old_string.is_empty() {
                    return ValidationResult::ok();
                }
                return ValidationResult::err(format!("File not found: {file_path}"), 2);
            }
            Err(e) => return ValidationResult::err(format!("Cannot read file: {e}"), 2),
        };

        match find_match(&content, old_string) {
            None => ValidationResult::err(
                format!(
                    "The old_string was not found in the file. \
                     Make sure old_string exactly matches a portion of the file content."
                ),
                3,
            ),
            Some(_variant) => {
                if !replace_all {
                    let count = count_occurrences(&content, old_string);
                    if count > 1 {
                        return ValidationResult::err(
                            format!(
                                "old_string matches {count} occurrences in the file. \
                                 Use replace_all: true to replace all occurrences, \
                                 or make old_string more specific to target one location."
                            ),
                            4,
                        );
                    }
                }
                ValidationResult::ok()
            }
        }
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
            is_read_only: false,
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
        let parsed: FileEditInput = match serde_json::from_value(input) {
            Ok(v) => v,
            Err(e) => return error_result(tool_use_id, format!("Invalid input: {e}")),
        };

        let path = resolve_path(&parsed.file_path, &ctx.cwd);
        let replace_all = parsed.replace_all.unwrap_or(false);

        // Read the file.
        let content = if path.exists() {
            match tokio::fs::read_to_string(&path).await {
                Ok(c) => c,
                Err(e) => return error_result(tool_use_id, format!("Cannot read file: {e}")),
            }
        } else if parsed.old_string.is_empty() {
            // Creating a new file.
            String::new()
        } else {
            return error_result(tool_use_id, format!("File not found: {}", path.display()));
        };

        // Resolve the actual old_string variant to use.
        let (actual_old, actual_new) = match find_match(&content, &parsed.old_string) {
            Some(MatchVariant::Exact) => (parsed.old_string.clone(), parsed.new_string.clone()),
            Some(MatchVariant::NormalizedQuotes { normalized_old }) => {
                let normalized_new = normalize_quotes(&parsed.new_string);
                (normalized_old, normalized_new)
            }
            Some(MatchVariant::Desanitized { desanitized_old }) => {
                (desanitized_old, parsed.new_string.clone())
            }
            None => {
                return error_result(
                    tool_use_id,
                    "The old_string was not found in the file.",
                );
            }
        };

        // Perform the replacement.
        let new_content = if replace_all {
            content.replace(&actual_old, &actual_new)
        } else {
            content.replacen(&actual_old, &actual_new, 1)
        };

        // Strip trailing whitespace from lines (not for .md/.mdx files).
        let is_markdown = path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| matches!(e, "md" | "mdx"))
            .unwrap_or(false);

        let final_content = if is_markdown {
            new_content
        } else {
            strip_trailing_whitespace(&new_content)
        };

        // Write back.
        if let Some(parent) = path.parent() {
            if let Err(e) = tokio::fs::create_dir_all(parent).await {
                return error_result(tool_use_id, format!("Cannot create directory: {e}"));
            }
        }

        if let Err(e) = tokio::fs::write(&path, &final_content).await {
            return error_result(tool_use_id, format!("Cannot write file: {e}"));
        }

        ok_result(
            tool_use_id,
            format!("The file {} has been edited successfully.", path.display()),
        )
    }
}

// ── Match finding ─────────────────────────────────────────────────────────────

/// Try to find `old_string` in `content` using three strategies:
/// 1. Exact substring search.
/// 2. Curly-quote normalization.
/// 3. Anti-sanitization of XML-escaped sequences.
fn find_match(content: &str, old_string: &str) -> Option<MatchVariant> {
    // Strategy 1: exact.
    if content.contains(old_string) {
        return Some(MatchVariant::Exact);
    }

    // Strategy 2: normalize curly quotes in both content and old_string.
    let norm_content = normalize_quotes(content);
    let norm_old = normalize_quotes(old_string);
    if norm_old != old_string && norm_content.contains(&norm_old) {
        return Some(MatchVariant::NormalizedQuotes { normalized_old: norm_old });
    }

    // Strategy 3: anti-sanitization.
    let mut desanitized = old_string.to_owned();
    for (from, to) in DESANITIZATIONS {
        desanitized = desanitized.replace(from, to);
    }
    if desanitized != old_string && content.contains(&desanitized) {
        return Some(MatchVariant::Desanitized { desanitized_old: desanitized });
    }

    None
}

/// Count occurrences of `needle` in `haystack` (non-overlapping).
fn count_occurrences(haystack: &str, needle: &str) -> usize {
    if needle.is_empty() {
        return 0;
    }
    let mut count = 0;
    let mut start = 0;
    while let Some(pos) = haystack[start..].find(needle) {
        count += 1;
        start += pos + needle.len();
    }
    count
}

/// Replace curly/smart quotes with straight ASCII equivalents.
fn normalize_quotes(s: &str) -> String {
    s.replace('\u{2018}', "'")   // left single quotation mark
     .replace('\u{2019}', "'")   // right single quotation mark
     .replace('\u{201C}', "\"")  // left double quotation mark
     .replace('\u{201D}', "\"")  // right double quotation mark
     .replace('\u{2032}', "'")   // prime
     .replace('\u{2033}', "\"")  // double prime
}

/// Strip trailing whitespace from every line.
fn strip_trailing_whitespace(content: &str) -> String {
    let ends_with_newline = content.ends_with('\n');
    let mut result: String = content
        .lines()
        .map(|l| l.trim_end())
        .collect::<Vec<_>>()
        .join("\n");
    if ends_with_newline {
        result.push('\n');
    }
    result
}

// ── Path helper ───────────────────────────────────────────────────────────────

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

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_match_found() {
        let content = "hello world\nfoo bar\n";
        assert!(matches!(find_match(content, "hello world"), Some(MatchVariant::Exact)));
    }

    #[test]
    fn curly_quote_match() {
        let content = "it's a test";
        let old = "it\u{2019}s a test"; // curly apostrophe
        assert!(matches!(
            find_match(content, old),
            Some(MatchVariant::NormalizedQuotes { .. })
        ));
    }

    #[test]
    fn desanitization_match() {
        let content = "<function_results>ok</function_results>";
        let old = "<fnr>ok</function_results>";
        assert!(matches!(
            find_match(content, old),
            Some(MatchVariant::Desanitized { .. })
        ));
    }

    #[test]
    fn no_match_returns_none() {
        assert!(find_match("abc", "xyz").is_none());
    }

    #[test]
    fn count_occurrences_works() {
        assert_eq!(count_occurrences("aababab", "ab"), 3);
        assert_eq!(count_occurrences("hello", "xyz"), 0);
    }

    #[test]
    fn strip_trailing_whitespace_keeps_newline() {
        let input = "line1   \nline2  \n";
        let output = strip_trailing_whitespace(input);
        assert_eq!(output, "line1\nline2\n");
    }

    #[test]
    fn identical_old_new_fails_validate() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let tool = FileEditTool;
        let input = json!({
            "file_path": "/tmp/test.txt",
            "old_string": "hello",
            "new_string": "hello"
        });
        let ctx = ToolContext {
            cwd: std::path::PathBuf::from("/tmp"),
            session_id: "t".into(),
            session_dir: std::path::PathBuf::from("/tmp"),
            permission_ctx: code_types::permissions::ToolPermissionContext::default(),
            file_reading_limits: Default::default(),
            glob_limits: Default::default(),
        };
        let result = rt.block_on(tool.validate_input(&input, &ctx));
        assert!(!result.is_ok());
    }
}
