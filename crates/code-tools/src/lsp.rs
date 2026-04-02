//! LSP tool stubs — language-server integration.
//!
//! Full LSP integration (hover, go-to-definition, references, diagnostics)
//! depends on a language-server management layer planned for a later phase.
//!
//! This module defines `LspHoverTool` and `LspDefinitionTool` as stubs.
//! Each tool returns a "not available" error until the LSP layer is wired up.
//! `is_enabled()` returns `false` so they are excluded from the default registry.
//!
//! Ref: src/tools/LSPTool/LSPTool.ts

use std::path::Path;

use async_trait::async_trait;
use code_permissions::evaluator::ToolCallContext;
use code_types::tool::{ToolInputSchema, ToolResult, ValidationResult};
use serde_json::{json, Value};

use crate::{error_result, ProgressSender, Tool, ToolContext};

// ── LspHoverTool ──────────────────────────────────────────────────────────────

pub struct LspHoverTool;

#[async_trait]
impl Tool for LspHoverTool {
    fn name(&self) -> &str { "LspHover" }

    fn description(&self) -> &str {
        "Get hover information (type, documentation) for a symbol at a specific \
        position in a source file via the Language Server Protocol."
    }

    fn input_schema(&self) -> ToolInputSchema {
        json!({
            "type": "object",
            "properties": {
                "file_path": {
                    "type": "string",
                    "description": "Absolute path to the source file"
                },
                "line": {
                    "type": "number",
                    "description": "0-based line number"
                },
                "character": {
                    "type": "number",
                    "description": "0-based character offset on the line"
                }
            },
            "required": ["file_path", "line", "character"]
        })
    }

    fn is_read_only(&self, _input: &Value) -> bool { true }
    fn is_enabled(&self) -> bool { false }

    async fn validate_input(&self, _input: &Value, _ctx: &ToolContext) -> ValidationResult {
        ValidationResult::ok()
    }

    fn permission_context<'a>(&'a self, input: &'a Value, cwd: &'a Path) -> ToolCallContext<'a> {
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
        _input: Value,
        _ctx: &ToolContext,
        _progress: Option<&ProgressSender>,
    ) -> ToolResult {
        error_result(
            tool_use_id,
            "LSP integration is not yet available (planned for a later phase).",
        )
    }
}

// ── LspDefinitionTool ─────────────────────────────────────────────────────────

pub struct LspDefinitionTool;

#[async_trait]
impl Tool for LspDefinitionTool {
    fn name(&self) -> &str { "LspDefinition" }

    fn description(&self) -> &str {
        "Jump to the definition of a symbol at a specific position in a source file."
    }

    fn input_schema(&self) -> ToolInputSchema {
        json!({
            "type": "object",
            "properties": {
                "file_path": { "type": "string", "description": "Absolute path to the source file" },
                "line": { "type": "number", "description": "0-based line number" },
                "character": { "type": "number", "description": "0-based character offset" }
            },
            "required": ["file_path", "line", "character"]
        })
    }

    fn is_read_only(&self, _input: &Value) -> bool { true }
    fn is_enabled(&self) -> bool { false }

    async fn validate_input(&self, _input: &Value, _ctx: &ToolContext) -> ValidationResult {
        ValidationResult::ok()
    }

    fn permission_context<'a>(&'a self, input: &'a Value, cwd: &'a Path) -> ToolCallContext<'a> {
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
        _input: Value,
        _ctx: &ToolContext,
        _progress: Option<&ProgressSender>,
    ) -> ToolResult {
        error_result(
            tool_use_id,
            "LSP integration is not yet available (planned for a later phase).",
        )
    }
}
