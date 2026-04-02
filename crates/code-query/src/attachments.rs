//! Attachment message handling.
//!
//! `AttachmentMessage` variants carry CLAUDE.md / memory content that is
//! injected into the system prompt rather than sent as regular messages.
//! This module extracts them and formats them for inclusion.
//!
//! Ref: src/utils/messages.ts (getSystemPromptAttachments)

use code_types::message::{AttachmentMessage, AttachmentType, Message};

/// Collect all attachment messages from a conversation.
pub fn collect_attachments(messages: &[Message]) -> Vec<&AttachmentMessage> {
    messages
        .iter()
        .filter_map(|m| {
            if let Message::Attachment(a) = m {
                Some(a)
            } else {
                None
            }
        })
        .collect()
}

/// Format all attachments into a single string suitable for inclusion in the
/// system prompt.  Each section is labelled with its attachment type.
pub fn format_attachments_for_system_prompt(messages: &[Message]) -> String {
    let attachments = collect_attachments(messages);
    if attachments.is_empty() {
        return String::new();
    }

    let mut out = String::new();
    for a in attachments {
        let label = match &a.attachment_type {
            AttachmentType::Memory => "<memory>",
            AttachmentType::CodeMd => "<claude_md>",
            AttachmentType::NestedMemory => "<nested_memory>",
            AttachmentType::Skill => "<skill>",
        };
        let close = match &a.attachment_type {
            AttachmentType::Memory => "</memory>",
            AttachmentType::CodeMd => "</claude_md>",
            AttachmentType::NestedMemory => "</nested_memory>",
            AttachmentType::Skill => "</skill>",
        };
        out.push_str(label);
        if let Some(path) = &a.path {
            out.push_str(&format!(" path=\"{path}\""));
        }
        out.push('>');
        out.push('\n');
        out.push_str(&a.content);
        out.push('\n');
        out.push_str(close);
        out.push('\n');
    }
    out
}
