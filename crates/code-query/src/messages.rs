//! Message normalization for API calls.
//!
//! The internal `Vec<Message>` contains UI-only variants (system messages,
//! progress, tombstones, attachments) that must be stripped before being sent
//! to the Anthropic API.  Consecutive messages from the same role are also
//! merged to satisfy the API's alternating-role constraint.
//!
//! Ref: src/utils/messages.ts (normalizeMessagesForAPI, mergeMessages)

use code_types::message::{
    ApiMessage, ApiRole, AssistantMessage, ContentBlock, Message, ToolResultBlock,
    ToolResultContent, ToolUseBlock, UserMessage,
};

// ── Public API ────────────────────────────────────────────────────────────────

/// Convert an internal `Message` slice to the API-wire format.
///
/// Steps:
///  1. Strip all UI-only variants (system messages, progress, tombstones, etc.)
///  2. Convert `Message::User` → `ApiMessage { role: User, content }`
///     and `Message::Assistant` → `ApiMessage { role: Assistant, content }`,
///     inserting tool results from the subsequent `User` turn into the correct block.
///  3. Merge consecutive messages with the same role (API requirement).
///
/// Ref: src/utils/messages.ts normalizeMessagesForAPI
pub fn normalize_messages_for_api(messages: &[Message]) -> Vec<ApiMessage> {
    // Step 1: filter to User + Assistant only.
    let api_messages: Vec<ApiMessage> = messages
        .iter()
        .filter(|m| !m.is_ui_only())
        .filter_map(to_api_message)
        .collect();

    // Step 2: merge consecutive same-role messages.
    merge_consecutive_roles(api_messages)
}

/// Extract all `ToolUseBlock`s from an assistant message.
pub fn extract_tool_use_blocks(msg: &AssistantMessage) -> Vec<ToolUseBlock> {
    msg.content
        .iter()
        .filter_map(|b| {
            if let ContentBlock::ToolUse(t) = b {
                Some(t.clone())
            } else {
                None
            }
        })
        .collect()
}

/// Build a `UserMessage` containing `ToolResult` blocks from `ToolResult` list.
pub fn tool_results_message(results: &[code_types::tool::ToolResult]) -> UserMessage {
    let content = results
        .iter()
        .map(|r| {
            let content = match &r.content {
                code_types::tool::ToolResultPayload::Text(s) => {
                    ToolResultContent::Text(s.clone())
                }
                code_types::tool::ToolResultPayload::Json(v) => {
                    ToolResultContent::Text(v.to_string())
                }
            };
            ContentBlock::ToolResult(ToolResultBlock {
                tool_use_id: r.tool_use_id.clone(),
                content,
                is_error: if r.is_error { Some(true) } else { None },
            })
        })
        .collect();

    UserMessage::new(content)
}

/// Return `true` if the message's stop reason indicates more tool calls should be processed.
pub fn is_tool_use_stop(msg: &AssistantMessage) -> bool {
    msg.stop_reason.as_deref() == Some("tool_use")
}

// ── Internal helpers ──────────────────────────────────────────────────────────

fn to_api_message(msg: &Message) -> Option<ApiMessage> {
    match msg {
        Message::User(u) => Some(ApiMessage {
            role: ApiRole::User,
            content: u.content.clone(),
        }),
        Message::Assistant(a) => Some(ApiMessage {
            role: ApiRole::Assistant,
            content: a.content.clone(),
        }),
        _ => None, // UI-only variants already filtered out above
    }
}

fn merge_consecutive_roles(mut messages: Vec<ApiMessage>) -> Vec<ApiMessage> {
    if messages.is_empty() {
        return messages;
    }

    let mut merged: Vec<ApiMessage> = Vec::with_capacity(messages.len());
    let mut drain = messages.drain(..);
    let mut current = drain.next().unwrap();

    for next in drain {
        if roles_match(&current.role, &next.role) {
            // Same role: merge content blocks.
            current.content.extend(next.content);
        } else {
            merged.push(current);
            current = next;
        }
    }
    merged.push(current);
    merged
}

fn roles_match(a: &ApiRole, b: &ApiRole) -> bool {
    matches!((a, b), (ApiRole::User, ApiRole::User) | (ApiRole::Assistant, ApiRole::Assistant))
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn user_msg(text: &str) -> Message {
        Message::User(UserMessage::text(text))
    }

    fn assistant_msg(text: &str) -> Message {
        use code_types::message::{TextBlock, TokenUsage};
        Message::Assistant(AssistantMessage {
            uuid: Uuid::new_v4(),
            content: vec![ContentBlock::Text(TextBlock { text: text.into(), cache_control: None })],
            model: "claude-sonnet".into(),
            stop_reason: Some("end_turn".into()),
            usage: TokenUsage::default(),
            agent_id: None,
        })
    }

    #[test]
    fn normalizes_and_strips_ui_only() {
        use code_types::message::SystemInformationalMessage;
        let msgs = vec![
            user_msg("hello"),
            Message::SystemInformational(SystemInformationalMessage {
                uuid: Uuid::new_v4(),
                content: "info".into(),
                level: code_types::message::SystemMessageLevel::Info,
            }),
            assistant_msg("hi"),
        ];
        let api = normalize_messages_for_api(&msgs);
        assert_eq!(api.len(), 2);
        assert!(matches!(api[0].role, ApiRole::User));
        assert!(matches!(api[1].role, ApiRole::Assistant));
    }

    #[test]
    fn merges_consecutive_user_messages() {
        let msgs = vec![user_msg("a"), user_msg("b"), assistant_msg("c")];
        let api = normalize_messages_for_api(&msgs);
        assert_eq!(api.len(), 2);
        assert_eq!(api[0].content.len(), 2);
    }
}
