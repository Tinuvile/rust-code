//! Post-sampling hook: fires a Notification event after each assistant turn.
//!
//! Ref: src/utils/hooks/hookEvents.ts (NotificationHookEvent)

use code_types::message::{AssistantMessage, ContentBlock};

use crate::event::{HookDecision, HookEvent};
use crate::registry::HookRegistry;

/// Fire a `Notification` hook for the first text block in `assistant_msg`.
///
/// Returns `Some(text)` if a hook returned an `InjectMessage` decision,
/// `None` otherwise.
pub async fn run_post_sampling_hooks(
    registry: &HookRegistry,
    assistant_msg: &AssistantMessage,
    session_id: &str,
) -> Option<String> {
    // Extract first text content block.
    let message_text = assistant_msg.content.iter().find_map(|b| {
        if let ContentBlock::Text(t) = b {
            Some(t.text.clone())
        } else {
            None
        }
    })?;

    let event = HookEvent::Notification {
        message: message_text,
        session_id: session_id.to_owned(),
    };

    match registry.dispatch(&event).await {
        HookDecision::InjectMessage { text } => Some(text),
        _ => None,
    }
}
