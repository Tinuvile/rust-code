//! Progress reporting channel for streaming tool output to the TUI.
//!
//! Ref: src/services/tools/StreamingToolExecutor.ts

use code_types::tool::ToolProgress;
use tokio::sync::mpsc;

/// Sender half of the progress channel.
pub type ProgressSender = mpsc::UnboundedSender<ToolProgress>;

/// Receiver half of the progress channel.
pub type ProgressReceiver = mpsc::UnboundedReceiver<ToolProgress>;

/// Create an unbounded channel pair for tool progress streaming.
///
/// The sender is passed into `Tool::call()`; the receiver is held by the
/// TUI layer (Phase 9) to display live output.
pub fn progress_channel() -> (ProgressSender, ProgressReceiver) {
    mpsc::unbounded_channel()
}

/// Emit a progress event.  No-ops if `sender` is `None`.
pub fn emit_progress(
    sender: Option<&ProgressSender>,
    tool_use_id: impl Into<String>,
    tool_name: impl Into<String>,
    data: serde_json::Value,
) {
    if let Some(tx) = sender {
        let _ = tx.send(ToolProgress {
            tool_use_id: tool_use_id.into(),
            tool_name: tool_name.into(),
            data,
        });
    }
}
