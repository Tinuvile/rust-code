//! WebSearchTool — search the web via Anthropic's search API.
//!
//! Requires the `ANTHROPIC_SEARCH_KEY` environment variable.
//! Returns a list of search results with titles, URLs, and snippets.
//!
//! Ref: src/tools/WebSearchTool/WebSearchTool.ts

use std::path::Path;

use async_trait::async_trait;
use code_permissions::evaluator::ToolCallContext;
use code_types::tool::{ToolInputSchema, ToolResult, ToolResultPayload, ValidationResult};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::{error_result, ProgressSender, Tool, ToolContext};

const SEARCH_API_URL: &str = "https://api.search.anthropic.com/v1/search";

#[derive(Deserialize)]
struct WebSearchInput {
    query: String,
    #[allow(dead_code)]
    num_results: Option<u32>,
    #[allow(dead_code)]
    offset: Option<u32>,
}

pub struct WebSearchTool;

#[async_trait]
impl Tool for WebSearchTool {
    fn name(&self) -> &str { "WebSearch" }

    fn description(&self) -> &str {
        "Searches the web and returns relevant results. \
        Returns titles, URLs, and snippets for each result. \
        Use this when you need current information not available in training data."
    }

    fn input_schema(&self) -> ToolInputSchema {
        json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "The search query"
                },
                "num_results": {
                    "type": "number",
                    "description": "Number of results to return (default: 5)"
                },
                "offset": {
                    "type": "number",
                    "description": "Result offset for pagination (default: 0)"
                }
            },
            "required": ["query"]
        })
    }

    fn is_read_only(&self, _input: &Value) -> bool { true }

    async fn validate_input(&self, input: &Value, _ctx: &ToolContext) -> ValidationResult {
        match input.get("query").and_then(|v| v.as_str()) {
            Some(q) if !q.is_empty() => ValidationResult::ok(),
            _ => ValidationResult::err("query is required and must not be empty", 1),
        }
    }

    fn permission_context<'a>(&'a self, input: &'a Value, cwd: &'a Path) -> ToolCallContext<'a> {
        ToolCallContext {
            tool_name: self.name(),
            content: input.get("query").and_then(|v| v.as_str()),
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
        let parsed: WebSearchInput = match serde_json::from_value(input) {
            Ok(v) => v,
            Err(e) => return error_result(tool_use_id, format!("Invalid input: {e}")),
        };

        let api_key = match std::env::var("ANTHROPIC_SEARCH_KEY")
            .or_else(|_| std::env::var("ANTHROPIC_API_KEY"))
        {
            Ok(k) => k,
            Err(_) => {
                return error_result(
                    tool_use_id,
                    "WebSearch requires ANTHROPIC_SEARCH_KEY environment variable.",
                )
            }
        };

        let num_results = parsed.num_results.unwrap_or(5);
        let offset = parsed.offset.unwrap_or(0);

        let client = match reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(20))
            .build()
        {
            Ok(c) => c,
            Err(e) => return error_result(tool_use_id, format!("HTTP client error: {e}")),
        };

        let body = json!({
            "query": parsed.query,
            "num_results": num_results,
            "offset": offset,
        });

        let response = match client
            .post(SEARCH_API_URL)
            .header("x-api-key", &api_key)
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .await
        {
            Ok(r) => r,
            Err(e) => return error_result(tool_use_id, format!("Search request failed: {e}")),
        };

        let status = response.status();
        let json_body: Value = match response.json().await {
            Ok(v) => v,
            Err(e) => return error_result(tool_use_id, format!("Failed to parse search response: {e}")),
        };

        if !status.is_success() {
            let msg = json_body
                .get("error")
                .and_then(|e| e.get("message"))
                .and_then(|m| m.as_str())
                .unwrap_or("Unknown error");
            return error_result(tool_use_id, format!("Search API error ({}): {msg}", status.as_u16()));
        }

        ToolResult {
            tool_use_id: tool_use_id.to_owned(),
            content: ToolResultPayload::Json(json_body),
            is_error: false,
            was_truncated: false,
        }
    }
}
