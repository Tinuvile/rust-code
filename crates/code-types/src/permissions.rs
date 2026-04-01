//! Permission type definitions.
//!
//! Extracted into a standalone module to break import cycles, mirroring
//! the approach in the original TypeScript codebase.
//!
//! Ref: src/types/permissions.ts

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ── Permission Modes ─────────────────────────────────────────────────────────

/// External permission modes — the user-addressable set exposed via CLI / settings.
///
/// Ref: src/types/permissions.ts EXTERNAL_PERMISSION_MODES
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum ExternalPermissionMode {
    #[default]
    Default,
    AcceptEdits,
    BypassPermissions,
    DontAsk,
    Plan,
}

/// Full permission mode union including internal-only variants.
///
/// `Auto` requires the `transcript_classifier` Cargo feature.
/// `Bubble` is used by subagents to propagate decisions upward.
///
/// Ref: src/types/permissions.ts PermissionMode
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum PermissionMode {
    #[default]
    Default,
    AcceptEdits,
    BypassPermissions,
    DontAsk,
    Plan,
    /// Only available with `transcript_classifier` feature.
    Auto,
    /// Internal: subagent bubbles decision to parent.
    Bubble,
}

impl From<ExternalPermissionMode> for PermissionMode {
    fn from(m: ExternalPermissionMode) -> Self {
        match m {
            ExternalPermissionMode::Default => Self::Default,
            ExternalPermissionMode::AcceptEdits => Self::AcceptEdits,
            ExternalPermissionMode::BypassPermissions => Self::BypassPermissions,
            ExternalPermissionMode::DontAsk => Self::DontAsk,
            ExternalPermissionMode::Plan => Self::Plan,
        }
    }
}

// ── Permission Behaviors ─────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PermissionBehavior {
    Allow,
    Deny,
    Ask,
}

// ── Permission Rules ─────────────────────────────────────────────────────────

/// Where a permission rule originated.
///
/// Ref: src/types/permissions.ts PermissionRuleSource
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PermissionRuleSource {
    UserSettings,
    ProjectSettings,
    LocalSettings,
    FlagSettings,
    PolicySettings,
    CliArg,
    Command,
    Session,
}

/// Specifies which tool (and optionally what content) a rule applies to.
///
/// Ref: src/types/permissions.ts PermissionRuleValue
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionRuleValue {
    pub tool_name: String,
    /// Optional glob / pattern (e.g. `"git *"` for BashTool, a file path for FileEditTool).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rule_content: Option<String>,
}

/// A single permission rule with source, behavior, and target.
///
/// Ref: src/types/permissions.ts PermissionRule
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionRule {
    pub source: PermissionRuleSource,
    pub rule_behavior: PermissionBehavior,
    pub rule_value: PermissionRuleValue,
}

/// Per-source lists of rule content strings.
/// Maps source → list of rule values (tool names / glob patterns).
///
/// Ref: src/types/permissions.ts ToolPermissionRulesBySource
pub type ToolPermissionRulesBySource = HashMap<PermissionRuleSource, Vec<String>>;

// ── Permission Updates ────────────────────────────────────────────────────────

/// Where a permission update should be persisted.
///
/// Ref: src/types/permissions.ts PermissionUpdateDestination
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PermissionUpdateDestination {
    UserSettings,
    ProjectSettings,
    LocalSettings,
    Session,
    CliArg,
}

/// An additional directory added to the permission scope.
///
/// Ref: src/types/permissions.ts AdditionalWorkingDirectory
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdditionalWorkingDirectory {
    pub path: String,
    pub source: PermissionRuleSource,
}

/// Mutations to the permission configuration.
///
/// Ref: src/types/permissions.ts PermissionUpdate
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum PermissionUpdate {
    AddRules {
        destination: PermissionUpdateDestination,
        rules: Vec<PermissionRuleValue>,
        behavior: PermissionBehavior,
    },
    ReplaceRules {
        destination: PermissionUpdateDestination,
        rules: Vec<PermissionRuleValue>,
        behavior: PermissionBehavior,
    },
    RemoveRules {
        destination: PermissionUpdateDestination,
        rules: Vec<PermissionRuleValue>,
        behavior: PermissionBehavior,
    },
    SetMode {
        destination: PermissionUpdateDestination,
        mode: ExternalPermissionMode,
    },
    AddDirectories {
        destination: PermissionUpdateDestination,
        directories: Vec<String>,
    },
    RemoveDirectories {
        destination: PermissionUpdateDestination,
        directories: Vec<String>,
    },
}

// ── Permission Decisions & Results ────────────────────────────────────────────

/// Why a particular permission decision was made.
///
/// Ref: src/types/permissions.ts PermissionDecisionReason
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum PermissionDecisionReason {
    Rule { rule: PermissionRule },
    Mode { mode: PermissionMode },
    Hook {
        hook_name: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        hook_source: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        reason: Option<String>,
    },
    AsyncAgent { reason: String },
    SandboxOverride { reason: SandboxOverrideReason },
    Classifier { classifier: String, reason: String },
    WorkingDir { reason: String },
    SafetyCheck { reason: String, classifier_approvable: bool },
    Other { reason: String },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SandboxOverrideReason {
    ExcludedCommand,
    DangerouslyDisableSandbox,
}

/// Result when permission is granted.
///
/// Ref: src/types/permissions.ts PermissionAllowDecision
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct PermissionAllowDecision {
    /// Optionally replace the tool's input (hooks may modify it).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_input: Option<serde_json::Value>,
    pub user_modified: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub decision_reason: Option<PermissionDecisionReason>,
}

/// Result when the user should be prompted.
///
/// Ref: src/types/permissions.ts PermissionAskDecision
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionAskDecision {
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_input: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub decision_reason: Option<PermissionDecisionReason>,
    #[serde(default)]
    pub suggestions: Vec<PermissionUpdate>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blocked_path: Option<String>,
}

/// Result when permission is denied.
///
/// Ref: src/types/permissions.ts PermissionDenyDecision
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionDenyDecision {
    pub message: String,
    pub decision_reason: PermissionDecisionReason,
}

/// The outcome of a single permission check.
///
/// Ref: src/types/permissions.ts PermissionDecision
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "behavior", rename_all = "lowercase")]
pub enum PermissionDecision {
    Allow(PermissionAllowDecision),
    Ask(PermissionAskDecision),
    Deny(PermissionDenyDecision),
}

/// Extended result that also allows "passthrough" (delegate to next evaluator).
///
/// Ref: src/types/permissions.ts PermissionResult
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "behavior", rename_all = "lowercase")]
pub enum PermissionResult {
    Allow(PermissionAllowDecision),
    Ask(PermissionAskDecision),
    Deny(PermissionDenyDecision),
    Passthrough {
        message: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        decision_reason: Option<PermissionDecisionReason>,
        #[serde(default)]
        suggestions: Vec<PermissionUpdate>,
        #[serde(skip_serializing_if = "Option::is_none")]
        blocked_path: Option<String>,
    },
}

// ── Tool Permission Context ───────────────────────────────────────────────────

/// Immutable snapshot of the permission configuration passed into every tool.
///
/// In Rust, immutability is the default; no DeepImmutable wrapper is needed.
///
/// Ref: src/types/permissions.ts ToolPermissionContext
#[derive(Debug, Clone)]
pub struct ToolPermissionContext {
    pub mode: PermissionMode,
    pub additional_working_directories: Vec<AdditionalWorkingDirectory>,
    pub always_allow_rules: ToolPermissionRulesBySource,
    pub always_deny_rules: ToolPermissionRulesBySource,
    pub always_ask_rules: ToolPermissionRulesBySource,
    pub is_bypass_permissions_mode_available: bool,
    pub stripped_dangerous_rules: Option<ToolPermissionRulesBySource>,
    /// True for background agents that cannot show UI prompts.
    pub should_avoid_permission_prompts: bool,
    /// True for coordinator workers: wait for automated checks before dialog.
    pub await_automated_checks_before_dialog: bool,
    /// Mode before plan-mode entry, restored on exit.
    pub pre_plan_mode: Option<PermissionMode>,
}

impl Default for ToolPermissionContext {
    fn default() -> Self {
        Self {
            mode: PermissionMode::Default,
            additional_working_directories: Vec::new(),
            always_allow_rules: HashMap::new(),
            always_deny_rules: HashMap::new(),
            always_ask_rules: HashMap::new(),
            is_bypass_permissions_mode_available: false,
            stripped_dangerous_rules: None,
            should_avoid_permission_prompts: false,
            await_automated_checks_before_dialog: false,
            pre_plan_mode: None,
        }
    }
}
