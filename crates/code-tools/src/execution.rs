//! Tool execution pipeline.
//!
//! Runs a single tool use block through the full 7-step pipeline:
//!
//! 1. Look up tool in registry (unknown tool → error result).
//! 2. Validate required fields from the input schema.
//! 3. Call `tool.validate_input()` — tool-specific semantic checks.
//! 4. Run pre-tool hook (`hook_runner.pre_tool()`).
//! 5. Evaluate permission (`PermissionEvaluator::evaluate()`).
//! 6. Call `tool.call()` with possibly-updated input.
//! 7. Run post-tool hook; persist large results via `result_storage`.
//!
//! Ref: src/services/tools/toolExecution.ts (checkPermissionsAndCallTool)

use code_permissions::denial_tracking::DenialTrackingState;
use code_permissions::evaluator::PermissionEvaluator;
use code_types::message::ToolUseBlock;
use code_types::permissions::PermissionDecision;
use code_types::tool::{ToolResult, ToolResultPayload, ValidationResult};
use tracing::debug;

use crate::hooks_stub::{PostToolHookResult, PreToolHookResult, ToolHookRunner};
use crate::progress::ProgressSender;
use crate::registry::ToolRegistry;
use crate::result_storage::maybe_persist_result;
use crate::{error_result, ToolContext};

/// Execute a single tool use block through the full pipeline.
///
/// Returns a `ToolResult` in all cases — errors are surfaced as
/// `is_error: true` results rather than propagated as `Err`.
pub async fn execute_tool(
    tool_use: &ToolUseBlock,
    registry: &ToolRegistry,
    ctx: &ToolContext,
    evaluator: &PermissionEvaluator,
    denial_state: &mut DenialTrackingState,
    hook_runner: &dyn ToolHookRunner,
    progress: Option<&ProgressSender>,
) -> ToolResult {
    let id = &tool_use.id;
    let name = &tool_use.name;
    let input = &tool_use.input;

    debug!("execute_tool: id={id} name={name}");

    // ── Step 1: Look up tool ──────────────────────────────────────────────────
    let tool = match registry.get(name) {
        Some(t) => t,
        None => {
            return error_result(id, format!("Unknown tool: {name}"));
        }
    };

    if !tool.is_enabled() {
        return error_result(id, format!("Tool '{name}' is not available in the current environment."));
    }

    // ── Step 2: Schema validation (required fields) ───────────────────────────
    if let Err(msg) = validate_required_fields(input, &tool.input_schema()) {
        return error_result(id, msg);
    }

    // ── Step 3: Tool-specific semantic validation ─────────────────────────────
    match tool.validate_input(input, ctx).await {
        ValidationResult::Ok => {}
        ValidationResult::Err { message, .. } => {
            return error_result(id, message);
        }
    }

    // ── Step 4: Pre-tool hook ─────────────────────────────────────────────────
    let mut effective_input = input.clone();
    match hook_runner.pre_tool(name, &effective_input).await {
        PreToolHookResult::Continue => {}
        PreToolHookResult::Block { reason } => {
            return error_result(id, format!("Blocked by hook: {reason}"));
        }
        PreToolHookResult::ModifyInput { new_input } => {
            effective_input = new_input;
        }
    }

    // ── Step 5: Permission evaluation ────────────────────────────────────────
    let call_ctx = tool.permission_context(&effective_input, &ctx.cwd);
    let decision = evaluator.evaluate(&call_ctx, &ctx.permission_ctx, denial_state);

    let final_input = match decision {
        PermissionDecision::Allow(allow) => {
            // Increment allow counter in denial state.
            denial_state.record_allow();
            // Use updated input if the permission system modified it.
            allow.updated_input.unwrap_or(effective_input)
        }
        PermissionDecision::Ask(ask) => {
            // In non-interactive mode (no TUI), treat Ask as Deny.
            // Phase 9 (TUI) will intercept Ask decisions before they reach here.
            denial_state.record_denial();
            return ToolResult {
                tool_use_id: id.clone(),
                content: ToolResultPayload::Text(ask.message),
                is_error: true,
                was_truncated: false,
            };
        }
        PermissionDecision::Deny(deny) => {
            denial_state.record_denial();
            return ToolResult {
                tool_use_id: id.clone(),
                content: ToolResultPayload::Text(format!(
                    "Permission denied for '{}': {}",
                    name, deny.message
                )),
                is_error: true,
                was_truncated: false,
            };
        }
    };

    // ── Step 6: Execute the tool ──────────────────────────────────────────────
    let mut result = tool.call(id, final_input.clone(), ctx, progress).await;

    // ── Step 7: Post-tool hook + result storage ───────────────────────────────
    match hook_runner.post_tool(name, &final_input, &result).await {
        PostToolHookResult::Unchanged => {}
        PostToolHookResult::ModifyResult { new_result } => {
            result = new_result;
        }
    }

    // Persist large results to disk.
    if let ToolResultPayload::Text(ref text) = result.content {
        let (final_text, was_truncated) =
            maybe_persist_result(text.clone(), id, name, &ctx.session_dir).await;
        if was_truncated {
            result.content = ToolResultPayload::Text(final_text);
            result.was_truncated = true;
        }
    }

    result
}

// ── Schema field validation ───────────────────────────────────────────────────

/// Check that all fields listed in `required` are present in `input`.
///
/// This is a lightweight substitute for a full JSON Schema validator.
fn validate_required_fields(
    input: &serde_json::Value,
    schema: &serde_json::Value,
) -> Result<(), String> {
    let required = match schema.get("required").and_then(|v| v.as_array()) {
        Some(arr) => arr,
        None => return Ok(()),
    };

    for field in required {
        let name = match field.as_str() {
            Some(s) => s,
            None => continue,
        };
        if input.get(name).is_none() {
            return Err(format!("Required field '{name}' is missing."));
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn missing_required_field() {
        let schema = json!({ "required": ["foo", "bar"] });
        let input = json!({ "foo": 1 });
        assert!(validate_required_fields(&input, &schema).is_err());
    }

    #[test]
    fn all_required_fields_present() {
        let schema = json!({ "required": ["foo", "bar"] });
        let input = json!({ "foo": 1, "bar": 2 });
        assert!(validate_required_fields(&input, &schema).is_ok());
    }

    #[test]
    fn no_required_is_ok() {
        let schema = json!({ "properties": {} });
        assert!(validate_required_fields(&json!({}), &schema).is_ok());
    }
}
