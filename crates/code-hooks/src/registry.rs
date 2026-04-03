//! Central hook dispatch registry.
//!
//! `HookRegistry` wraps a `HookMap` and provides a single `dispatch` entry
//! point that fans out to all registered hooks for an event and aggregates
//! their decisions.
//!
//! Decision priority rules:
//! - First `Block` decision short-circuits further hook execution.
//! - All `InjectMessage` texts are accumulated and joined with `\n`.
//! - Any other non-`Continue` decision wins immediately.
//! - If all hooks return `Continue`, `Continue` is returned.
//!
//! Ref: src/utils/hooks/AsyncHookRegistry.ts

use std::sync::Arc;

use code_config::settings::HooksSettings;

use crate::config::{resolve_hooks, HookMap, ResolvedHook};
use crate::event::{event_name, HookDecision, HookEvent};
use crate::executor_http::run_http_hook;
use crate::executor_prompt::run_prompt_hook;
use crate::executor_shell::run_shell_hook;

// ── HookRegistry ──────────────────────────────────────────────────────────────

/// Shared, cloneable hook registry.
#[derive(Clone)]
pub struct HookRegistry {
    hooks: Arc<HookMap>,
}

impl HookRegistry {
    /// Create a registry from a pre-resolved `HookMap`.
    pub fn new(hooks: HookMap) -> Self {
        Self { hooks: Arc::new(hooks) }
    }

    /// Build a registry directly from optional `HooksSettings`.
    pub fn from_settings(settings: Option<&HooksSettings>) -> Self {
        Self::new(resolve_hooks(settings))
    }

    /// Return `true` if there are any hooks registered for the given event.
    pub fn has_hooks_for(&self, event: &HookEvent) -> bool {
        self.hooks.contains_key(event_name(event))
    }

    /// Dispatch `event` to all matching hooks and return the aggregate decision.
    ///
    /// Hooks are executed sequentially in registration order.
    pub async fn dispatch(&self, event: &HookEvent) -> HookDecision {
        let name = event_name(event);
        let hooks = match self.hooks.get(name) {
            Some(h) if !h.is_empty() => h,
            _ => return HookDecision::Continue,
        };

        let mut inject_texts: Vec<String> = Vec::new();

        for hook in hooks.iter() {
            let decision = match hook {
                ResolvedHook::Shell { command } => run_shell_hook(command, event).await,
                ResolvedHook::Http { url } => run_http_hook(url, event).await,
                ResolvedHook::Prompt { text } => run_prompt_hook(text, event),
            };

            match decision {
                HookDecision::Block { reason } => {
                    return HookDecision::Block { reason };
                }
                HookDecision::InjectMessage { text } => {
                    inject_texts.push(text);
                }
                HookDecision::ModifyInput { new_input } => {
                    return HookDecision::ModifyInput { new_input };
                }
                HookDecision::Continue => {}
            }
        }

        if !inject_texts.is_empty() {
            return HookDecision::InjectMessage { text: inject_texts.join("\n") };
        }

        HookDecision::Continue
    }
}

impl std::fmt::Debug for HookRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HookRegistry")
            .field("event_count", &self.hooks.len())
            .finish()
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ResolvedHook;

    fn registry_with(event: &str, hooks: Vec<ResolvedHook>) -> HookRegistry {
        let mut map = HookMap::new();
        map.insert(event.to_owned(), hooks);
        HookRegistry::new(map)
    }

    #[tokio::test]
    async fn empty_registry_returns_continue() {
        let registry = HookRegistry::new(HookMap::new());
        let ev = HookEvent::Setup { session_id: "s".to_owned() };
        assert!(matches!(registry.dispatch(&ev).await, HookDecision::Continue));
    }

    #[tokio::test]
    async fn prompt_hook_injects_message() {
        let registry = registry_with(
            "setup",
            vec![ResolvedHook::Prompt { text: "hello".to_owned() }],
        );
        let ev = HookEvent::Setup { session_id: "s".to_owned() };
        let decision = registry.dispatch(&ev).await;
        assert!(matches!(decision, HookDecision::InjectMessage { .. }));
        if let HookDecision::InjectMessage { text } = decision {
            assert_eq!(text, "hello");
        }
    }

    #[tokio::test]
    async fn multiple_inject_messages_joined() {
        let registry = registry_with(
            "setup",
            vec![
                ResolvedHook::Prompt { text: "line1".to_owned() },
                ResolvedHook::Prompt { text: "line2".to_owned() },
            ],
        );
        let ev = HookEvent::Setup { session_id: "s".to_owned() };
        let decision = registry.dispatch(&ev).await;
        if let HookDecision::InjectMessage { text } = decision {
            assert_eq!(text, "line1\nline2");
        } else {
            panic!("expected InjectMessage");
        }
    }
}
