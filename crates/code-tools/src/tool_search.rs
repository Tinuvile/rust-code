//! ToolSearchTool — fetch full schemas for deferred tools on demand.
//!
//! Some tools are "deferred" — their full input schema is not sent to the model
//! in every request to save tokens.  When the model needs to call such a tool,
//! it first calls `ToolSearch` to retrieve the schema, then calls the real tool.
//!
//! This tool operates against the live `ToolRegistry`, so it returns accurate
//! schemas for whatever tools are registered in the current session.
//!
//! Ref: src/tools/ToolSearchTool/ToolSearchTool.ts

use std::path::Path;
use std::sync::Arc;

use async_trait::async_trait;
use code_permissions::evaluator::ToolCallContext;
use code_types::tool::{ToolInputSchema, ToolResult, ValidationResult};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::{error_result, ok_result, ProgressSender, Tool, ToolContext};
use crate::registry::ToolRegistry;

// ── Input ─────────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct ToolSearchInput {
    query: String,
    max_results: Option<usize>,
}

// ── Tool ─────────────────────────────────────────────────────────────────────

pub struct ToolSearchTool {
    registry: Arc<ToolRegistry>,
}

impl ToolSearchTool {
    pub fn new(registry: Arc<ToolRegistry>) -> Self {
        Self { registry }
    }
}

#[async_trait]
impl Tool for ToolSearchTool {
    fn name(&self) -> &str { "ToolSearch" }

    fn description(&self) -> &str {
        "Search the available tools and return their full schemas. \
        Use this before calling an unfamiliar tool to retrieve its exact parameter schema."
    }

    fn input_schema(&self) -> ToolInputSchema {
        json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "Search query — matches tool names and descriptions"
                },
                "max_results": {
                    "type": "number",
                    "description": "Maximum number of results to return (default: 5)"
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
        let parsed: ToolSearchInput = match serde_json::from_value(input) {
            Ok(v) => v,
            Err(e) => return error_result(tool_use_id, format!("Invalid input: {e}")),
        };

        let max = parsed.max_results.unwrap_or(5).min(50);
        let query_lower = parsed.query.to_lowercase();

        // Score tools by query relevance (substring match in name or description).
        let mut matches: Vec<Value> = self
            .registry
            .all()
            .filter(|t| {
                let name_lc = t.name().to_lowercase();
                let desc_lc = t.description().to_lowercase();
                name_lc.contains(&query_lower) || desc_lc.contains(&query_lower)
            })
            .take(max)
            .map(|t| {
                json!({
                    "name": t.name(),
                    "description": t.description(),
                    "input_schema": t.input_schema(),
                })
            })
            .collect();

        // Sort alphabetically for determinism.
        matches.sort_by(|a, b| {
            a["name"].as_str().unwrap_or("").cmp(b["name"].as_str().unwrap_or(""))
        });

        let count = matches.len();
        let result = json!({
            "query": parsed.query,
            "results": matches,
            "total": count,
        });

        ok_result(
            tool_use_id,
            serde_json::to_string_pretty(&result).unwrap_or_default(),
        )
    }
}
