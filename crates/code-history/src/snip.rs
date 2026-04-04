//! Message snipping: selectively remove messages from the conversation context.
//!
//! Enabled by the `history_snip` Cargo feature.
//!
//! Snipping is a lighter-weight alternative to full compaction: instead of
//! summarising the whole conversation, individual messages (or ranges) can be
//! excised and replaced by a `Tombstone` marker so their position in the
//! transcript is preserved.
//!
//! Ref: (scattered) query.ts, sessionStorage.ts

use code_types::ids::SessionId;
use code_types::message::{Message, TombstoneMessage};
use uuid::Uuid;

// ── SnipCriteria ──────────────────────────────────────────────────────────────

/// Criteria that determine which messages are eligible for snipping.
#[derive(Debug, Clone)]
pub struct SnipCriteria {
    /// Maximum number of messages to keep (keeps the *newest* N).
    pub max_messages: Option<usize>,
    /// Remove tool-use / tool-result pairs whose combined character count
    /// exceeds this threshold.
    pub large_tool_result_threshold: Option<usize>,
    /// Always keep the most recent N message pairs (user+assistant) intact.
    pub protect_recent_turns: usize,
}

impl Default for SnipCriteria {
    fn default() -> Self {
        Self {
            max_messages: None,
            large_tool_result_threshold: Some(2000),
            protect_recent_turns: 2,
        }
    }
}

// ── SnipResult ────────────────────────────────────────────────────────────────

#[derive(Debug)]
pub struct SnipResult {
    pub messages: Vec<Message>,
    pub snipped_count: usize,
}

// ── snip_messages ─────────────────────────────────────────────────────────────

/// Apply snipping to `messages` according to `criteria`.
///
/// Snipped messages are replaced by `Message::Tombstone` entries so the
/// transcript index is preserved for resume / fork operations.
pub fn snip_messages(messages: Vec<Message>, criteria: SnipCriteria) -> SnipResult {
    let mut result = messages;
    let mut snipped_count = 0usize;
    let protected_tail = criteria.protect_recent_turns * 2;

    // Step 1: truncate to max_messages (from the tail).
    if let Some(max) = criteria.max_messages {
        if result.len() > max {
            let keep_from = result.len() - max;
            // Replace the head with tombstones.
            for msg in &mut result[..keep_from] {
                if !matches!(msg, Message::Tombstone(_)) {
                    let id = msg_uuid(msg);
                    *msg = tombstone(id);
                    snipped_count += 1;
                }
            }
        }
    }

    // Step 2: replace large tool results with tombstones.
    if let Some(threshold) = criteria.large_tool_result_threshold {
        let tail_start = result.len().saturating_sub(protected_tail);
        for msg in &mut result[..tail_start] {
            if is_large_tool_result(msg, threshold) {
                let id = msg_uuid(msg);
                *msg = tombstone(id);
                snipped_count += 1;
            }
        }
    }

    SnipResult { messages: result, snipped_count }
}

// ── helpers ───────────────────────────────────────────────────────────────────

fn tombstone(uuid: Uuid) -> Message {
    Message::Tombstone(TombstoneMessage { uuid })
}

fn msg_uuid(msg: &Message) -> Uuid {
    match msg {
        Message::User(u) => u.uuid,
        Message::Assistant(a) => a.uuid,
        Message::Progress(p) => p.uuid,
        Message::SystemInformational(s) => s.uuid,
        Message::SystemApiError(e) => e.uuid,
        Message::SystemTurnDuration(d) => d.uuid,
        Message::SystemCompactBoundary(b) => b.uuid,
        Message::SystemMemorySaved(m) => m.uuid,
        Message::Tombstone(t) => t.uuid,
        Message::Attachment(a) => a.uuid,
        Message::ToolUseSummary(s) => s.uuid,
        Message::SystemMicrocompactBoundary(b) => b.uuid,
        Message::SystemPermissionRetry(r) => r.uuid,
    }
}

fn is_large_tool_result(msg: &Message, threshold: usize) -> bool {
    use code_types::message::{ContentBlock, ToolResultContent};

    let Message::User(u) = msg else { return false; };
    u.content.iter().any(|b| {
        if let ContentBlock::ToolResult(tr) = b {
            match &tr.content {
                ToolResultContent::Text(t) => t.len() > threshold,
                ToolResultContent::Blocks(bs) => {
                    bs.iter().any(|b| {
                        if let ContentBlock::Text(t) = b {
                            t.text.len() > threshold
                        } else {
                            false
                        }
                    })
                }
            }
        } else {
            false
        }
    })
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use code_types::message::{ContentBlock, TextBlock, ToolResultBlock, ToolResultContent, UserMessage};

    fn make_user(text: &str) -> Message {
        Message::User(UserMessage {
            uuid: Uuid::new_v4(),
            content: vec![ContentBlock::Text(TextBlock { text: text.into(), cache_control: None })],
            is_api_error_message: false,
            agent_id: None,
        })
    }

    fn make_tool_result(text: &str) -> Message {
        Message::User(UserMessage {
            uuid: Uuid::new_v4(),
            content: vec![ContentBlock::ToolResult(ToolResultBlock {
                tool_use_id: "tu_1".into(),
                content: ToolResultContent::Text(text.into()),
                is_error: None,
            })],
            is_api_error_message: false,
            agent_id: None,
        })
    }

    #[test]
    fn snip_large_tool_result() {
        let big = "x".repeat(3000);
        let msgs = vec![make_user("hello"), make_tool_result(&big), make_user("query")];
        let criteria = SnipCriteria {
            large_tool_result_threshold: Some(500),
            protect_recent_turns: 0,
            max_messages: None,
        };
        let result = snip_messages(msgs, criteria);
        assert_eq!(result.snipped_count, 1);
        assert!(matches!(result.messages[1], Message::Tombstone(_)));
    }

    #[test]
    fn max_messages_leaves_tail() {
        let msgs: Vec<Message> = (0..10).map(|i| make_user(&format!("msg {i}"))).collect();
        let criteria = SnipCriteria { max_messages: Some(3), protect_recent_turns: 0, ..Default::default() };
        let result = snip_messages(msgs, criteria);
        // First 7 should be tombstones.
        assert_eq!(result.snipped_count, 7);
        for msg in &result.messages[..7] {
            assert!(matches!(msg, Message::Tombstone(_)));
        }
    }
}
