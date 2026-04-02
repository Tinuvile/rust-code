//! WebFetchTool — fetch a URL and return its content as text.
//!
//! HTML is stripped to plain text via regex; other content types are
//! returned as-is. Content is truncated to `MAX_CONTENT_CHARS`.
//!
//! Ref: src/tools/WebFetchTool/WebFetchTool.ts

use std::path::Path;

use async_trait::async_trait;
use code_permissions::evaluator::ToolCallContext;
use code_types::tool::{ToolInputSchema, ToolResult, ToolResultPayload, ValidationResult};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::{error_result, ProgressSender, Tool, ToolContext};

const MAX_CONTENT_CHARS: usize = 100_000;

#[derive(Deserialize)]
struct WebFetchInput {
    url: String,
    #[allow(dead_code)]
    prompt: Option<String>,
    max_length: Option<usize>,
}

pub struct WebFetchTool;

#[async_trait]
impl Tool for WebFetchTool {
    fn name(&self) -> &str { "WebFetch" }

    fn description(&self) -> &str {
        "Fetches content from a URL and returns it as text. \
        HTML pages are converted to plain text. \
        Use this to read documentation, web pages, or any URL-accessible content."
    }

    fn input_schema(&self) -> ToolInputSchema {
        json!({
            "type": "object",
            "properties": {
                "url": {
                    "type": "string",
                    "description": "The URL to fetch"
                },
                "prompt": {
                    "type": "string",
                    "description": "Optional description of what you're looking for"
                },
                "max_length": {
                    "type": "number",
                    "description": "Maximum characters to return (default: 100000)"
                }
            },
            "required": ["url"]
        })
    }

    fn is_read_only(&self, _input: &Value) -> bool { true }

    async fn validate_input(&self, input: &Value, _ctx: &ToolContext) -> ValidationResult {
        let url = match input.get("url").and_then(|v| v.as_str()) {
            Some(u) if !u.is_empty() => u,
            _ => return ValidationResult::err("url is required and must not be empty", 1),
        };
        if !url.starts_with("http://") && !url.starts_with("https://") {
            return ValidationResult::err("url must start with http:// or https://", 1);
        }
        ValidationResult::ok()
    }

    fn permission_context<'a>(&'a self, input: &'a Value, cwd: &'a Path) -> ToolCallContext<'a> {
        ToolCallContext {
            tool_name: self.name(),
            content: input.get("url").and_then(|v| v.as_str()),
            input: Some(input),
            is_read_only: true,
            cwd,
        }
    }

    async fn call(
        &self,
        tool_use_id: &str,
        input: Value,
        _ctx: &ToolContext,
        _progress: Option<&ProgressSender>,
    ) -> ToolResult {
        let parsed: WebFetchInput = match serde_json::from_value(input) {
            Ok(v) => v,
            Err(e) => return error_result(tool_use_id, format!("Invalid input: {e}")),
        };
        let max_len = parsed.max_length.unwrap_or(MAX_CONTENT_CHARS);

        let client = match reqwest::Client::builder()
            .user_agent("Mozilla/5.0 (compatible; ClaudeCode/1.0)")
            .timeout(std::time::Duration::from_secs(30))
            .build()
        {
            Ok(c) => c,
            Err(e) => return error_result(tool_use_id, format!("HTTP client error: {e}")),
        };

        let response = match client.get(&parsed.url).send().await {
            Ok(r) => r,
            Err(e) => return error_result(tool_use_id, format!("Failed to fetch {}: {e}", parsed.url)),
        };

        let status = response.status();
        if !status.is_success() {
            return error_result(tool_use_id, format!("HTTP {} for {}", status.as_u16(), parsed.url));
        }

        let is_html = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .map(|ct| ct.contains("text/html"))
            .unwrap_or(false);

        let body = match response.text().await {
            Ok(b) => b,
            Err(e) => return error_result(tool_use_id, format!("Failed to read response: {e}")),
        };

        let text = if is_html { html_to_text(&body) } else { body };

        let truncated = text.chars().count() > max_len;
        let mut output: String = text.chars().take(max_len).collect();
        if truncated {
            output.push_str(&format!("\n\n[Content truncated at {max_len} characters]"));
        }

        ToolResult {
            tool_use_id: tool_use_id.to_owned(),
            content: ToolResultPayload::Text(output),
            is_error: false,
            was_truncated: truncated,
        }
    }
}

fn html_to_text(html: &str) -> String {
    // Remove script/style blocks.
    let re_ss = regex::Regex::new(r"(?is)<(script|style)[^>]*>.*?</(script|style)>").unwrap();
    let s = re_ss.replace_all(html, " ");
    // Strip tags.
    let re_tags = regex::Regex::new(r"<[^>]+>").unwrap();
    let s = re_tags.replace_all(&s, " ");
    // Decode common entities.
    let s = s.replace("&amp;", "&").replace("&lt;", "<").replace("&gt;", ">")
             .replace("&quot;", "\"").replace("&#39;", "'").replace("&nbsp;", " ");
    // Collapse whitespace.
    let re_nl = regex::Regex::new(r"\n{3,}").unwrap();
    let s = re_nl.replace_all(&s, "\n\n");
    let re_sp = regex::Regex::new(r"[ \t]+").unwrap();
    let s = re_sp.replace_all(&s, " ");
    s.trim().to_owned()
}
