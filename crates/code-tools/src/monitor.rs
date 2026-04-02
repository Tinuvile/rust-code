//! MonitorTool — report agent resource usage and health metrics.
//!
//! Enabled by feature `monitor_tool`.
//! Returns real-time metrics about the current session: token usage, tool call
//! counts, elapsed time, and (on Unix) process memory.
//!
//! Ref: src/tools/MonitorTool/MonitorTool.ts

use std::path::Path;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use code_permissions::evaluator::ToolCallContext;
use code_types::tool::{ToolInputSchema, ToolResult, ValidationResult};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::{ok_result, ProgressSender, Tool, ToolContext};

// ── Metrics ───────────────────────────────────────────────────────────────────

/// Cumulative session metrics updated by the tool execution layer.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct SessionMetrics {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub tool_calls: u64,
    pub errors: u64,
    pub session_start: u64, // Unix seconds
}

impl SessionMetrics {
    pub fn new() -> Self {
        Self {
            session_start: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            ..Default::default()
        }
    }
}

/// Thread-safe handle to session metrics, shared with the execution layer.
#[derive(Clone, Default)]
pub struct MetricsHandle(Arc<Mutex<SessionMetrics>>);

impl MetricsHandle {
    pub fn new() -> Self {
        Self(Arc::new(Mutex::new(SessionMetrics::new())))
    }

    pub fn snapshot(&self) -> SessionMetrics {
        self.0.lock().unwrap().clone()
    }

    pub fn record_tool_call(&self) {
        let mut m = self.0.lock().unwrap();
        m.tool_calls += 1;
    }

    pub fn record_error(&self) {
        let mut m = self.0.lock().unwrap();
        m.errors += 1;
    }

    pub fn add_tokens(&self, input: u64, output: u64) {
        let mut m = self.0.lock().unwrap();
        m.input_tokens += input;
        m.output_tokens += output;
    }
}

// ── MonitorTool ───────────────────────────────────────────────────────────────

pub struct MonitorTool {
    metrics: MetricsHandle,
}

impl MonitorTool {
    pub fn new(metrics: MetricsHandle) -> Self {
        Self { metrics }
    }

    pub fn with_default_metrics() -> Self {
        Self::new(MetricsHandle::new())
    }
}

#[async_trait]
impl Tool for MonitorTool {
    fn name(&self) -> &str { "Monitor" }

    fn description(&self) -> &str {
        "Return current session metrics: token usage, tool call counts, and elapsed time."
    }

    fn input_schema(&self) -> ToolInputSchema {
        json!({ "type": "object", "properties": {} })
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
        let snap = self.metrics.snapshot();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let elapsed = now.saturating_sub(snap.session_start);

        let report = json!({
            "input_tokens": snap.input_tokens,
            "output_tokens": snap.output_tokens,
            "total_tokens": snap.input_tokens + snap.output_tokens,
            "tool_calls": snap.tool_calls,
            "errors": snap.errors,
            "elapsed_seconds": elapsed,
        });

        ok_result(
            tool_use_id,
            serde_json::to_string_pretty(&report).unwrap_or_default(),
        )
    }
}
