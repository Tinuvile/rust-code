//! Resolve hook configuration from settings into executable hook descriptors.
//!
//! Ref: src/utils/hooks/hookConfig.ts

use std::collections::HashMap;

use code_config::settings::{HookCommand, HooksSettings};

// ── ResolvedHook ──────────────────────────────────────────────────────────────

/// A single resolved hook ready for execution.
#[derive(Debug, Clone)]
pub enum ResolvedHook {
    /// Shell command executed via `sh -c` / `cmd /C`.
    Shell { command: String },
    /// HTTP endpoint called with a POST of the event JSON.
    Http { url: String },
    /// A literal text injected directly into the conversation.
    Prompt { text: String },
}

// ── HookMap ───────────────────────────────────────────────────────────────────

/// Map from event-name strings to their ordered list of hooks.
pub type HookMap = HashMap<String, Vec<ResolvedHook>>;

// ── resolve_hooks ─────────────────────────────────────────────────────────────

/// Convert a `HooksSettings` map into a resolved `HookMap`.
///
/// `HookCommand::Bash { command }` → `Shell`, `HookCommand::Http { url }` → `Http`.
pub fn resolve_hooks(settings: Option<&HooksSettings>) -> HookMap {
    let mut map = HookMap::new();

    let Some(hooks) = settings else {
        return map;
    };

    for (event_name, commands) in hooks {
        let resolved: Vec<ResolvedHook> = commands
            .iter()
            .map(|cmd| match cmd {
                HookCommand::Bash { command } => ResolvedHook::Shell {
                    command: command.clone(),
                },
                HookCommand::Http { url } => ResolvedHook::Http { url: url.clone() },
            })
            .collect();

        if !resolved.is_empty() {
            map.insert(event_name.clone(), resolved);
        }
    }

    map
}

// ── hooks_for_event ───────────────────────────────────────────────────────────

/// Return the hooks registered for a given event name (empty slice if none).
pub fn hooks_for_event<'a>(map: &'a HookMap, event_name: &str) -> &'a [ResolvedHook] {
    map.get(event_name).map(|v| v.as_slice()).unwrap_or(&[])
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn none_settings_returns_empty_map() {
        let map = resolve_hooks(None);
        assert!(map.is_empty());
    }

    #[test]
    fn bash_command_resolved_as_shell() {
        let mut settings = HooksSettings::new();
        settings.insert(
            "pre_tool_use".to_owned(),
            vec![HookCommand::Bash { command: "echo hi".to_owned() }],
        );
        let map = resolve_hooks(Some(&settings));
        let hooks = hooks_for_event(&map, "pre_tool_use");
        assert_eq!(hooks.len(), 1);
        assert!(matches!(hooks[0], ResolvedHook::Shell { .. }));
    }

    #[test]
    fn http_command_resolved_as_http() {
        let mut settings = HooksSettings::new();
        settings.insert(
            "notification".to_owned(),
            vec![HookCommand::Http { url: "http://localhost/hook".to_owned() }],
        );
        let map = resolve_hooks(Some(&settings));
        let hooks = hooks_for_event(&map, "notification");
        assert!(matches!(hooks[0], ResolvedHook::Http { .. }));
    }

    #[test]
    fn unknown_event_returns_empty_slice() {
        let map = HookMap::new();
        assert!(hooks_for_event(&map, "no_such_event").is_empty());
    }
}
