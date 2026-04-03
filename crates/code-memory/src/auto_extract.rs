//! Background memory auto-extraction (stub).
//!
//! In a full implementation this would fork a sub-agent that inspects the
//! conversation and writes new memory entries to the memdir.  For Phase 8
//! this is a no-op stub: the returned handle's receiver is immediately closed.
//!
//! Ref: src/services/extractMemories/

use std::path::Path;

use code_types::message::Message;

// ── Handle ────────────────────────────────────────────────────────────────────

/// A handle returned when background memory extraction is requested.
///
/// The `result_rx` receiver will immediately return `Err(RecvError)` because
/// no sender is retained — this is the correct no-op behaviour for the stub.
pub struct AutoExtractHandle {
    /// Completion channel.  Will never yield `Ok(_)` in the stub.
    pub result_rx: tokio::sync::oneshot::Receiver<anyhow::Result<String>>,
}

// ── Trigger ───────────────────────────────────────────────────────────────────

/// Request background memory extraction (stub — does nothing).
///
/// Returns a handle whose receiver channel is immediately closed.
pub fn trigger_auto_extract(
    _conversation: &[Message],
    _cwd: &Path,
) -> AutoExtractHandle {
    // Create a channel then immediately drop the sender so the receiver
    // is closed without ever sending a value.
    let (tx, rx) = tokio::sync::oneshot::channel::<anyhow::Result<String>>();
    drop(tx);
    AutoExtractHandle { result_rx: rx }
}
