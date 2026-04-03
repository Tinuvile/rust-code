//! Prompt hook executor: injects a static text message into the conversation.
//!
//! Ref: src/utils/hooks/execAgentHook.ts (prompt variant)

use crate::event::{HookDecision, HookEvent};

/// Execute a prompt hook by injecting `text` as a conversation message.
///
/// The `_event` parameter is accepted for API consistency but not used.
pub fn run_prompt_hook(text: &str, _event: &HookEvent) -> HookDecision {
    HookDecision::InjectMessage { text: text.to_owned() }
}
