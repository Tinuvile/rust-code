//! Session lifecycle hook helpers.
//!
//! Fires `SessionStart` then `Setup` events and collects any injected messages.
//!
//! Ref: src/utils/hooks/hookEvents.ts (SessionStartHookEvent, SetupHookEvent)

use crate::event::{HookDecision, HookEvent};
use crate::registry::HookRegistry;

/// Fire `SessionStart` and `Setup` hooks for a new session.
///
/// Returns any texts from `InjectMessage` decisions (in order: SessionStart
/// then Setup).  `Block` decisions are logged at warn level but are non-fatal.
pub async fn run_session_start_hooks(
    registry: &HookRegistry,
    session_id: &str,
    cwd: &str,
    model: &str,
) -> Vec<String> {
    let mut texts: Vec<String> = Vec::new();

    // SessionStart
    let start_event = HookEvent::SessionStart {
        session_id: session_id.to_owned(),
        cwd: cwd.to_owned(),
        model: model.to_owned(),
    };
    match registry.dispatch(&start_event).await {
        HookDecision::InjectMessage { text } => texts.push(text),
        HookDecision::Block { reason } => {
            tracing::warn!("session_start hook blocked: {reason}");
        }
        _ => {}
    }

    // Setup
    let setup_event = HookEvent::Setup { session_id: session_id.to_owned() };
    match registry.dispatch(&setup_event).await {
        HookDecision::InjectMessage { text } => texts.push(text),
        HookDecision::Block { reason } => {
            tracing::warn!("setup hook blocked: {reason}");
        }
        _ => {}
    }

    texts
}
