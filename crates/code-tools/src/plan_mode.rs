//! Plan mode control tools — EnterPlanMode / ExitPlanMode.
//!
//! These tools signal to the TUI layer that the model wants to enter or exit
//! "plan mode" (a mode where the model proposes a plan before executing).
//! In Phase 5 (no TUI), they simply return an acknowledgment.  Phase 9 will
//! wire the TUI state machine to these signals.
//!
//! Ref: src/tools/PlanModeTool/PlanModeTool.ts

use std::path::Path;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use code_permissions::evaluator::ToolCallContext;
use code_types::tool::{ToolInputSchema, ToolResult, ValidationResult};
use serde_json::{json, Value};

use crate::{ok_result, ProgressSender, Tool, ToolContext};

// ── Shared state ──────────────────────────────────────────────────────────────

/// Shared plan-mode flag.  Wrapped in `Arc<Mutex<_>>` so that the TUI layer
/// (Phase 9) can attach a watcher.
#[derive(Clone, Default)]
pub struct PlanModeState(Arc<Mutex<bool>>);

impl PlanModeState {
    pub fn new() -> Self {
        Self(Arc::new(Mutex::new(false)))
    }

    pub fn enter(&self) {
        *self.0.lock().unwrap() = true;
    }

    pub fn exit(&self) {
        *self.0.lock().unwrap() = false;
    }

    pub fn is_active(&self) -> bool {
        *self.0.lock().unwrap()
    }
}

// ── EnterPlanMode ─────────────────────────────────────────────────────────────

pub struct EnterPlanModeTool {
    state: PlanModeState,
}

impl EnterPlanModeTool {
    pub fn new(state: PlanModeState) -> Self {
        Self { state }
    }

    pub fn with_default_state() -> Self {
        Self::new(PlanModeState::new())
    }
}

#[async_trait]
impl Tool for EnterPlanModeTool {
    fn name(&self) -> &str { "EnterPlanMode" }

    fn description(&self) -> &str {
        "Signals that you are entering plan mode. \
        In plan mode you should outline a complete implementation plan and \
        get explicit approval before making any changes."
    }

    fn input_schema(&self) -> ToolInputSchema {
        json!({
            "type": "object",
            "properties": {}
        })
    }

    fn is_read_only(&self, _input: &Value) -> bool { true }

    async fn validate_input(&self, _input: &Value, _ctx: &ToolContext) -> ValidationResult {
        ValidationResult::ok()
    }

    fn permission_context<'a>(&'a self, input: &'a Value, cwd: &'a Path) -> ToolCallContext<'a> {
        ToolCallContext {
            tool_name: self.name(),
            content: None,
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
        self.state.enter();
        ok_result(tool_use_id, "Entered plan mode.")
    }
}

// ── ExitPlanMode ──────────────────────────────────────────────────────────────

pub struct ExitPlanModeTool {
    state: PlanModeState,
}

impl ExitPlanModeTool {
    pub fn new(state: PlanModeState) -> Self {
        Self { state }
    }

    pub fn with_default_state() -> Self {
        Self::new(PlanModeState::new())
    }
}

#[async_trait]
impl Tool for ExitPlanModeTool {
    fn name(&self) -> &str { "ExitPlanMode" }

    fn description(&self) -> &str {
        "Signals that you are exiting plan mode. \
        Only call this after the user has approved your plan."
    }

    fn input_schema(&self) -> ToolInputSchema {
        json!({
            "type": "object",
            "properties": {}
        })
    }

    fn is_read_only(&self, _input: &Value) -> bool { true }

    async fn validate_input(&self, _input: &Value, _ctx: &ToolContext) -> ValidationResult {
        ValidationResult::ok()
    }

    fn permission_context<'a>(&'a self, input: &'a Value, cwd: &'a Path) -> ToolCallContext<'a> {
        ToolCallContext {
            tool_name: self.name(),
            content: None,
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
        self.state.exit();
        ok_result(tool_use_id, "Exited plan mode.")
    }
}
