//! McpTool — dynamically-created wrapper for MCP (Model Context Protocol) tools.
//!
//! MCP server integration is implemented in Phase 7.  This module provides:
//!   1. The `McpTool` struct, which wraps a single MCP server tool and makes it
//!      available via the standard `Tool` trait.
//!   2. A `McpCallFn` type alias for the async callback that phases 7+ will
//!      provide to actually invoke the MCP tool over the transport.
//!
//! In Phase 5 (no MCP transport), every call returns an informational error.
//!
//! Ref: src/tools/MCPTool/MCPTool.ts

use std::path::Path;
use std::sync::Arc;

use async_trait::async_trait;
use code_permissions::evaluator::ToolCallContext;
use code_types::tool::{ToolInputSchema, ToolResult};
use serde_json::Value;

use crate::{error_result, ProgressSender, Tool, ToolContext};

// ── Callback type ─────────────────────────────────────────────────────────────

/// Async function type for dispatching calls to an MCP transport.
///
/// Phase 7 will construct `McpTool` instances that carry a real implementation.
/// `tool_use_id`, `tool_name`, and `input` are forwarded to the transport.
pub type McpCallFn = Arc<
    dyn Fn(String, String, Value) -> std::pin::Pin<Box<dyn std::future::Future<Output = ToolResult> + Send>>
        + Send
        + Sync,
>;

// ── Tool ─────────────────────────────────────────────────────────────────────

/// A single MCP server tool surfaced through the standard `Tool` trait.
///
/// One `McpTool` instance is created per (server, tool_name) pair.
pub struct McpTool {
    /// Canonical name, e.g. `"mcp__myserver__my_tool"`.
    pub tool_name: String,
    /// Human-readable description forwarded from the MCP server.
    pub tool_description: String,
    /// JSON Schema forwarded from the MCP server.
    pub schema: ToolInputSchema,
    /// Whether the MCP server declares this tool as read-only.
    pub read_only: bool,
    /// Optional async dispatch function (None = Phase 5 stub).
    pub call_fn: Option<McpCallFn>,
}

impl McpTool {
    /// Create a Phase-5 stub that returns a "not available" error on call.
    pub fn stub(
        name: impl Into<String>,
        description: impl Into<String>,
        schema: ToolInputSchema,
    ) -> Self {
        Self {
            tool_name: name.into(),
            tool_description: description.into(),
            schema,
            read_only: false,
            call_fn: None,
        }
    }
}

#[async_trait]
impl Tool for McpTool {
    fn name(&self) -> &str { &self.tool_name }

    fn description(&self) -> &str { &self.tool_description }

    fn input_schema(&self) -> ToolInputSchema { self.schema.clone() }

    fn is_read_only(&self, _input: &Value) -> bool { self.read_only }

    fn permission_context<'a>(&'a self, input: &'a Value, cwd: &'a Path) -> ToolCallContext<'a> {
        ToolCallContext {
            tool_name: self.name(),
            content: None,
            input: Some(input),
            is_read_only: self.read_only,
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
        match &self.call_fn {
            Some(f) => f(tool_use_id.to_owned(), self.tool_name.clone(), input).await,
            None => error_result(
                tool_use_id,
                format!(
                    "MCP tool '{}' is not available: MCP transport not initialized (Phase 7).",
                    self.tool_name
                ),
            ),
        }
    }
}
