//! Micro-compaction: truncate oversized tool-result blocks in place.
//!
//! Large tool outputs (e.g. file reads, shell output) can dominate the context
//! window without contributing useful information to the model.  This module
//! truncates any `ToolResultContent::Text` that exceeds the threshold, adding
//! a prepended notice so the model knows data was omitted.
//!
//! Ref: src/services/compact/microCompact.ts

use code_types::message::{ContentBlock, Message, ToolResultContent};

// ── Threshold ─────────────────────────────────────────────────────────────────

/// Text length (in chars) above which a single tool result is truncated.
pub const MICRO_COMPACT_THRESHOLD_CHARS: usize = 8_000;

// ── Per-result helper ─────────────────────────────────────────────────────────

/// If the tool-result text exceeds the threshold, return a truncated
/// `ToolResultContent` with a leading notice; otherwise return `None`.
///
/// Only `ToolResultContent::Text` is considered; `Blocks` variants are left
/// unchanged (they may contain images, etc.).
pub fn maybe_micro_compact_result(content: &ToolResultContent) -> Option<ToolResultContent> {
    let text = match content {
        ToolResultContent::Text(t) => t,
        ToolResultContent::Blocks(_) => return None,
    };

    if text.len() <= MICRO_COMPACT_THRESHOLD_CHARS {
        return None;
    }

    let truncated = &text[..MICRO_COMPACT_THRESHOLD_CHARS];
    let notice = format!("[Output truncated to {} chars]\n", MICRO_COMPACT_THRESHOLD_CHARS);
    let new_text = format!("{notice}{truncated}");
    Some(ToolResultContent::Text(new_text))
}

// ── Bulk walker ───────────────────────────────────────────────────────────────

/// Walk every `Message::User` content block and truncate oversized
/// `ToolResult` text in place.
///
/// Returns the number of tool-result blocks that were truncated.
pub fn micro_compact_messages(messages: &mut Vec<Message>) -> usize {
    let mut count = 0;

    for msg in messages.iter_mut() {
        let content = match msg {
            Message::User(u) => &mut u.content,
            _ => continue,
        };

        for block in content.iter_mut() {
            let tr = match block {
                ContentBlock::ToolResult(tr) => tr,
                _ => continue,
            };

            if let Some(new_content) = maybe_micro_compact_result(&tr.content) {
                tr.content = new_content;
                count += 1;
            }
        }
    }

    count
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use code_types::message::{ContentBlock, ToolResultBlock, ToolResultContent, UserMessage};
    use uuid::Uuid;

    fn make_tool_result_msg(text: impl Into<String>) -> Message {
        Message::User(UserMessage {
            uuid: Uuid::new_v4(),
            content: vec![ContentBlock::ToolResult(ToolResultBlock {
                tool_use_id: "t1".to_owned(),
                content: ToolResultContent::Text(text.into()),
                is_error: None,
            })],
            is_api_error_message: false,
            agent_id: None,
        })
    }

    #[test]
    fn short_result_unchanged() {
        let content = ToolResultContent::Text("short".to_owned());
        assert!(maybe_micro_compact_result(&content).is_none());
    }

    #[test]
    fn long_result_truncated() {
        let long = "x".repeat(MICRO_COMPACT_THRESHOLD_CHARS + 100);
        let content = ToolResultContent::Text(long);
        let result = maybe_micro_compact_result(&content).expect("should truncate");
        match result {
            ToolResultContent::Text(t) => {
                assert!(t.starts_with("[Output truncated to"));
                assert!(t.len() <= MICRO_COMPACT_THRESHOLD_CHARS + 50); // notice + truncated text
            }
            _ => panic!("expected Text"),
        }
    }

    #[test]
    fn micro_compact_messages_counts_truncated() {
        let long = "y".repeat(MICRO_COMPACT_THRESHOLD_CHARS + 1);
        let mut messages = vec![
            make_tool_result_msg("short"),
            make_tool_result_msg(long),
        ];
        let n = micro_compact_messages(&mut messages);
        assert_eq!(n, 1);
    }

    #[test]
    fn micro_compact_messages_modifies_in_place() {
        let long = "z".repeat(MICRO_COMPACT_THRESHOLD_CHARS + 500);
        let mut messages = vec![make_tool_result_msg(long)];
        micro_compact_messages(&mut messages);

        if let Message::User(u) = &messages[0] {
            if let ContentBlock::ToolResult(tr) = &u.content[0] {
                if let ToolResultContent::Text(t) = &tr.content {
                    assert!(t.starts_with("[Output truncated to"));
                    return;
                }
            }
        }
        panic!("message not modified in place");
    }
}
