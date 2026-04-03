//! Group messages into logical runs for display and compact boundary detection.
//!
//! Ref: src/services/compact/compact.ts (message grouping)

use code_types::message::{ContentBlock, Message};

// ── RunKind ───────────────────────────────────────────────────────────────────

/// The semantic kind of a run of messages.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunKind {
    /// Human-turn messages (not pure tool results).
    User,
    /// Model response messages.
    Assistant,
    /// User-role messages whose content is entirely `ToolResult` blocks.
    ToolResult,
    /// UI-only system messages (never sent to the API).
    System,
}

// ── MessageRun ────────────────────────────────────────────────────────────────

/// A contiguous sequence of messages sharing the same `RunKind`.
#[derive(Debug, Clone)]
pub struct MessageRun {
    pub kind: RunKind,
    pub messages: Vec<Message>,
}

// ── Classification ────────────────────────────────────────────────────────────

/// Classify a single message into a `RunKind`.
pub fn classify_message(msg: &Message) -> RunKind {
    match msg {
        Message::User(u) => {
            // A user message is a ToolResult run if ALL its content blocks
            // are ToolResult blocks (and there is at least one).
            if !u.content.is_empty()
                && u.content
                    .iter()
                    .all(|b| matches!(b, ContentBlock::ToolResult(_)))
            {
                RunKind::ToolResult
            } else {
                RunKind::User
            }
        }
        Message::Assistant(_) => RunKind::Assistant,
        // Everything else is UI-only.
        _ => RunKind::System,
    }
}

// ── Grouping ──────────────────────────────────────────────────────────────────

/// Group messages into contiguous `MessageRun` slices of the same `RunKind`.
///
/// Consecutive messages with the same kind are merged into a single run.
pub fn group_messages(messages: &[Message]) -> Vec<MessageRun> {
    let mut runs: Vec<MessageRun> = Vec::new();

    for msg in messages {
        let kind = classify_message(msg);
        match runs.last_mut() {
            Some(run) if run.kind == kind => {
                run.messages.push(msg.clone());
            }
            _ => {
                runs.push(MessageRun {
                    kind,
                    messages: vec![msg.clone()],
                });
            }
        }
    }

    runs
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use code_types::message::{
        AssistantMessage, ContentBlock, TextBlock, ToolResultBlock, ToolResultContent, UserMessage,
    };
    use uuid::Uuid;

    fn user_text(text: &str) -> Message {
        Message::User(UserMessage {
            uuid: Uuid::new_v4(),
            content: vec![ContentBlock::Text(TextBlock { text: text.to_owned(), cache_control: None })],
            is_api_error_message: false,
            agent_id: None,
        })
    }

    fn tool_result_msg(id: &str) -> Message {
        Message::User(UserMessage {
            uuid: Uuid::new_v4(),
            content: vec![ContentBlock::ToolResult(ToolResultBlock {
                tool_use_id: id.to_owned(),
                content: ToolResultContent::Text("ok".to_owned()),
                is_error: None,
            })],
            is_api_error_message: false,
            agent_id: None,
        })
    }

    fn assistant_msg() -> Message {
        Message::Assistant(AssistantMessage {
            uuid: Uuid::new_v4(),
            content: vec![],
            model: "claude-3".to_owned(),
            stop_reason: Some("end_turn".to_owned()),
            usage: Default::default(),
            agent_id: None,
        })
    }

    #[test]
    fn empty_input_returns_empty_runs() {
        assert!(group_messages(&[]).is_empty());
    }

    #[test]
    fn consecutive_same_kind_merged() {
        let msgs = vec![user_text("a"), user_text("b")];
        let runs = group_messages(&msgs);
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].messages.len(), 2);
        assert_eq!(runs[0].kind, RunKind::User);
    }

    #[test]
    fn different_kinds_split_into_runs() {
        let msgs = vec![user_text("hi"), assistant_msg(), tool_result_msg("t1")];
        let runs = group_messages(&msgs);
        assert_eq!(runs.len(), 3);
        assert_eq!(runs[0].kind, RunKind::User);
        assert_eq!(runs[1].kind, RunKind::Assistant);
        assert_eq!(runs[2].kind, RunKind::ToolResult);
    }

    #[test]
    fn classify_tool_result_user_message() {
        assert_eq!(classify_message(&tool_result_msg("x")), RunKind::ToolResult);
    }

    #[test]
    fn classify_regular_user_message() {
        assert_eq!(classify_message(&user_text("hello")), RunKind::User);
    }
}
