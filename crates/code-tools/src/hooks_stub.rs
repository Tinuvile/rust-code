//! Hook runner trait and no-op implementation.
//!
//! Phase 5 uses `NoopHookRunner` everywhere.  Phase 8 (`code-hooks`) will
//! provide a real implementation that reads hook configurations from
//! `settings.json` and executes shell / HTTP / prompt hooks.
//!
//! Ref: src/utils/hooks/, src/services/tools/toolHooks.ts

use async_trait::async_trait;
use code_types::tool::ToolResult;

// ── Hook results ──────────────────────────────────────────────────────────────

/// What the pre-tool hook wants the execution pipeline to do.
#[derive(Debug)]
pub enum PreToolHookResult {
    /// Allow the tool call to proceed with the original (or modified) input.
    Continue,
    /// Block the tool call entirely and return this error message.
    Block { reason: String },
    /// Replace the tool's input before execution.
    ModifyInput { new_input: serde_json::Value },
}

/// What the post-tool hook wants the execution pipeline to do.
#[derive(Debug)]
pub enum PostToolHookResult {
    /// Leave the result unchanged.
    Unchanged,
    /// Replace the result entirely.
    ModifyResult { new_result: ToolResult },
}

// ── Hook runner trait ─────────────────────────────────────────────────────────

/// Minimal hook interface injected into the execution pipeline.
///
/// The trait is object-safe so it can be passed as `&dyn ToolHookRunner`.
#[async_trait]
pub trait ToolHookRunner: Send + Sync {
    /// Called before schema validation (can short-circuit with Block or
    /// replace the input with ModifyInput).
    async fn pre_tool(
        &self,
        tool_name: &str,
        input: &serde_json::Value,
    ) -> PreToolHookResult;

    /// Called after a successful `tool.call()` (can replace the result).
    async fn post_tool(
        &self,
        tool_name: &str,
        input: &serde_json::Value,
        result: &ToolResult,
    ) -> PostToolHookResult;
}

// ── No-op implementation ──────────────────────────────────────────────────────

/// Phase 5 no-op hook runner — passes everything through unchanged.
pub struct NoopHookRunner;

#[async_trait]
impl ToolHookRunner for NoopHookRunner {
    async fn pre_tool(
        &self,
        _tool_name: &str,
        _input: &serde_json::Value,
    ) -> PreToolHookResult {
        PreToolHookResult::Continue
    }

    async fn post_tool(
        &self,
        _tool_name: &str,
        _input: &serde_json::Value,
        _result: &ToolResult,
    ) -> PostToolHookResult {
        PostToolHookResult::Unchanged
    }
}
