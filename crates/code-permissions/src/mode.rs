//! Permission mode helpers — display strings, cycling, and status-bar labels.
//!
//! Extension traits for `PermissionMode` and `ExternalPermissionMode`.
//!
//! Ref: src/types/permissions.ts (mode labels)
//!      src/components/PermissionModeIndicator.tsx

use code_types::permissions::{ExternalPermissionMode, PermissionMode};

// ── PermissionMode extension ──────────────────────────────────────────────────

pub trait PermissionModeExt {
    fn short_label(&self) -> &'static str;
    fn description(&self) -> &'static str;
    fn is_non_interactive(&self) -> bool;
    fn is_read_only_mode(&self) -> bool;
    fn is_bypass(&self) -> bool;
    fn cycle_next(&self) -> ExternalPermissionMode;
}

impl PermissionModeExt for PermissionMode {
    /// Short label shown in the TUI status bar.
    fn short_label(&self) -> &'static str {
        match self {
            PermissionMode::Default => "default",
            PermissionMode::AcceptEdits => "accept-edits",
            PermissionMode::BypassPermissions => "bypass",
            PermissionMode::DontAsk => "dont-ask",
            PermissionMode::Plan => "plan",
            PermissionMode::Auto => "auto",
            PermissionMode::Bubble => "bubble",
        }
    }

    /// Full human-readable description.
    fn description(&self) -> &'static str {
        match self {
            PermissionMode::Default => "Default — ask before non-read-only tools",
            PermissionMode::AcceptEdits => "AcceptEdits — auto-approve file edits",
            PermissionMode::BypassPermissions => "BypassPermissions — allow all tools",
            PermissionMode::DontAsk => "DontAsk — allow all, never prompt",
            PermissionMode::Plan => "Plan — read-only tools only",
            PermissionMode::Auto => "Auto — classifier-driven decisions",
            PermissionMode::Bubble => "Bubble — delegate to parent agent",
        }
    }

    /// Returns `true` when the mode means "never prompt the user interactively".
    fn is_non_interactive(&self) -> bool {
        matches!(
            self,
            PermissionMode::BypassPermissions | PermissionMode::DontAsk | PermissionMode::Bubble
        )
    }

    /// Returns `true` when only read-only tools are permitted.
    fn is_read_only_mode(&self) -> bool {
        matches!(self, PermissionMode::Plan)
    }

    /// Returns `true` when the mode bypasses all permission checks.
    fn is_bypass(&self) -> bool {
        matches!(
            self,
            PermissionMode::BypassPermissions | PermissionMode::DontAsk
        )
    }

    /// Returns the next external mode in the cycle shown in the TUI
    /// (Default → AcceptEdits → Plan → Default).
    ///
    /// Ref: src/components/PermissionModeToggle.tsx
    fn cycle_next(&self) -> ExternalPermissionMode {
        match self {
            PermissionMode::Default => ExternalPermissionMode::AcceptEdits,
            PermissionMode::AcceptEdits => ExternalPermissionMode::Plan,
            _ => ExternalPermissionMode::Default,
        }
    }
}

// ── ExternalPermissionMode extension ─────────────────────────────────────────

pub trait ExternalPermissionModeExt {
    fn short_label(&self) -> &'static str;
    fn from_cli_str(s: &str) -> Option<ExternalPermissionMode>;
}

impl ExternalPermissionModeExt for ExternalPermissionMode {
    /// Short label for the external mode.
    fn short_label(&self) -> &'static str {
        match self {
            ExternalPermissionMode::Default => "default",
            ExternalPermissionMode::AcceptEdits => "accept-edits",
            ExternalPermissionMode::BypassPermissions => "bypass",
            ExternalPermissionMode::DontAsk => "dont-ask",
            ExternalPermissionMode::Plan => "plan",
        }
    }

    /// Parse from a CLI string (case-insensitive).
    fn from_cli_str(s: &str) -> Option<ExternalPermissionMode> {
        match s.to_lowercase().as_str() {
            "default" => Some(ExternalPermissionMode::Default),
            "accept-edits" | "acceptedits" => Some(ExternalPermissionMode::AcceptEdits),
            "bypass-permissions" | "bypasspermissions" | "bypass" => {
                Some(ExternalPermissionMode::BypassPermissions)
            }
            "dont-ask" | "dontask" => Some(ExternalPermissionMode::DontAsk),
            "plan" => Some(ExternalPermissionMode::Plan),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cycle_wraps() {
        let m = PermissionMode::Plan;
        assert_eq!(m.cycle_next(), ExternalPermissionMode::Default);
    }

    #[test]
    fn bypass_is_bypass() {
        assert!(PermissionMode::BypassPermissions.is_bypass());
        assert!(!PermissionMode::Default.is_bypass());
    }

    #[test]
    fn parse_cli_str() {
        assert_eq!(
            ExternalPermissionMode::from_cli_str("accept-edits"),
            Some(ExternalPermissionMode::AcceptEdits)
        );
        assert_eq!(ExternalPermissionMode::from_cli_str("garbage"), None);
    }
}
