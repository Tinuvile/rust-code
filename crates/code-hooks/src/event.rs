//! Hook event types and decision types.
//!
//! `HookEvent` is serialized as JSON and written to hook executor stdin.
//! `HookDecision` is expected back on stdout.
//!
//! Ref: src/utils/hooks/hookEvents.ts

use serde::{Deserialize, Serialize};
use serde_json::Value;

// ── HookEvent ─────────────────────────────────────────────────────────────────

/// Events fired at specific points in the session lifecycle.
///
/// Serializes with a `"event"` discriminant field in snake_case.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum HookEvent {
    /// Fired before a tool call is executed.
    PreToolUse {
        tool_name: String,
        input: Value,
        session_id: String,
    },
    /// Fired after a successful tool call.
    PostToolUse {
        tool_name: String,
        input: Value,
        result: Value,
        session_id: String,
    },
    /// Fired when a tool call fails with an error.
    PostToolUseFailure {
        tool_name: String,
        input: Value,
        error: String,
        session_id: String,
    },
    /// Fired when permission was denied for a tool call.
    PermissionDenied {
        tool_name: String,
        reason: String,
        session_id: String,
    },
    /// Fired for assistant notification messages.
    Notification {
        message: String,
        session_id: String,
    },
    /// Fired when a new session starts.
    SessionStart {
        session_id: String,
        cwd: String,
        model: String,
    },
    /// Fired during session setup (after SessionStart).
    Setup {
        session_id: String,
    },
}

// ── HookDecision ──────────────────────────────────────────────────────────────

/// Decision returned by a hook executor.
///
/// Serializes with a `"decision"` discriminant field in snake_case.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "decision", rename_all = "snake_case")]
pub enum HookDecision {
    /// Allow the action to proceed unchanged.
    Continue,
    /// Abort the action with a reason shown to the user.
    Block { reason: String },
    /// Replace the tool input with `new_input` (PreToolUse only).
    ModifyInput { new_input: Value },
    /// Inject a message into the conversation on behalf of the hook.
    InjectMessage { text: String },
}

impl Default for HookDecision {
    fn default() -> Self {
        Self::Continue
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Return the snake_case event name string for a `HookEvent`.
///
/// Matches the `"event"` tag values produced by `serde`.
pub fn event_name(event: &HookEvent) -> &'static str {
    match event {
        HookEvent::PreToolUse { .. } => "pre_tool_use",
        HookEvent::PostToolUse { .. } => "post_tool_use",
        HookEvent::PostToolUseFailure { .. } => "post_tool_use_failure",
        HookEvent::PermissionDenied { .. } => "permission_denied",
        HookEvent::Notification { .. } => "notification",
        HookEvent::SessionStart { .. } => "session_start",
        HookEvent::Setup { .. } => "setup",
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn event_serializes_with_tag() {
        let ev = HookEvent::PreToolUse {
            tool_name: "bash".to_owned(),
            input: json!({"command": "ls"}),
            session_id: "s1".to_owned(),
        };
        let s = serde_json::to_string(&ev).unwrap();
        assert!(s.contains("\"event\":\"pre_tool_use\""));
        assert!(s.contains("\"tool_name\":\"bash\""));
    }

    #[test]
    fn decision_default_is_continue() {
        let d = HookDecision::default();
        assert!(matches!(d, HookDecision::Continue));
    }

    #[test]
    fn event_name_returns_correct_string() {
        let ev = HookEvent::Setup { session_id: "s".to_owned() };
        assert_eq!(event_name(&ev), "setup");
    }

    #[test]
    fn decision_roundtrip() {
        let d = HookDecision::Block { reason: "not allowed".to_owned() };
        let s = serde_json::to_string(&d).unwrap();
        let back: HookDecision = serde_json::from_str(&s).unwrap();
        assert!(matches!(back, HookDecision::Block { .. }));
    }
}
