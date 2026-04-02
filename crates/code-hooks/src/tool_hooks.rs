//! Tool hook types and no-op runner.
//!
//! Phase 6 provides the type definitions and a no-op `ToolHookRunner`.
//! Phase 8 will replace the no-op with real shell/HTTP hook dispatch.
//!
//! These types mirror the Phase 5 `hooks_stub` in `code-tools` but live in
//! `code-hooks` so that the hooks crate owns the hook system.
//!
//! Ref: src/utils/hooks/hookEvents.ts (PreToolUseHookEvent, PostToolUseHookEvent)

use async_trait::async_trait;
use serde_json::Value;

use code_types::tool::ToolResult;

// ── Pre-tool hook ─────────────────────────────────────────────────────────────

/// Outcome of a `pre_tool` hook invocation.
pub enum PreToolOutcome {
    /// Allow the tool call to proceed.
    Continue,
    /// Block the tool call with an error message.
    Block { reason: String },
    /// Replace the tool's input with a modified version.
    ModifyInput { new_input: Value },
}

// ── Post-tool hook ────────────────────────────────────────────────────────────

/// Outcome of a `post_tool` hook invocation.
pub enum PostToolOutcome {
    /// Leave the result unchanged.
    Unchanged,
    /// Replace the result with a modified version.
    ModifyResult { new_result: ToolResult },
}

// ── ToolHookRunner trait ──────────────────────────────────────────────────────

/// Trait for running pre/post tool hooks.
///
/// `NoopToolHookRunner` is the Phase 6 implementation.
/// Phase 8 introduces `ShellToolHookRunner` and `HttpToolHookRunner`.
#[async_trait]
pub trait ToolHookRunner: Send + Sync {
    /// Called immediately before tool execution.
    async fn pre_tool(&self, tool_name: &str, input: &Value) -> PreToolOutcome {
        let _ = (tool_name, input);
        PreToolOutcome::Continue
    }

    /// Called immediately after tool execution.
    async fn post_tool(
        &self,
        tool_name: &str,
        input: &Value,
        result: &ToolResult,
    ) -> PostToolOutcome {
        let _ = (tool_name, input, result);
        PostToolOutcome::Unchanged
    }
}

// ── No-op implementation ──────────────────────────────────────────────────────

/// Phase 6 stub — passes through all tool calls without modification.
pub struct NoopToolHookRunner;

#[async_trait]
impl ToolHookRunner for NoopToolHookRunner {}
