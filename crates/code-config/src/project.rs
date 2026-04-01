//! Per-project configuration stored in `<project>/.claude/settings.json`
//! and the per-path project record inside `~/.claude/config.json`.
//!
//! Ref: src/utils/config.ts ProjectConfig

use std::collections::HashMap;
use serde::{Deserialize, Serialize};

use crate::settings::McpServerConfig;

// ── Worktree session ──────────────────────────────────────────────────────────

/// Persisted state for an active worktree session.
///
/// Ref: src/utils/config.ts ProjectConfig.activeWorktreeSession
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActiveWorktreeSession {
    pub original_cwd: String,
    pub worktree_path: String,
    pub worktree_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub original_branch: Option<String>,
    pub session_id: String,
    #[serde(default)]
    pub hook_based: bool,
}

// ── Per-model usage stats ─────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelUsageStats {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_input_tokens: u64,
    pub cache_creation_input_tokens: u64,
    pub web_search_requests: u64,
    pub cost_usd: f64,
}

// ── ProjectConfig ─────────────────────────────────────────────────────────────

/// Project-level config record (stored per-path in `~/.claude/config.json`).
///
/// Ref: src/utils/config.ts ProjectConfig
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ProjectConfig {
    #[serde(default)]
    pub allowed_tools: Vec<String>,
    #[serde(default)]
    pub mcp_context_uris: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mcp_servers: Option<HashMap<String, McpServerConfig>>,

    // Metrics from the last session (used for analytics/display).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_api_duration: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_cost: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_session_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_model_usage: Option<HashMap<String, ModelUsageStats>>,

    // Trust / onboarding state.
    #[serde(default)]
    pub has_trust_dialog_accepted: bool,
    #[serde(default)]
    pub has_completed_project_onboarding: bool,
    #[serde(default)]
    pub project_onboarding_seen_count: u32,

    /// `true` once the user has approved external includes in code.md.
    #[serde(default)]
    pub has_code_md_external_includes_approved: bool,
    /// `true` once the "external includes" warning banner has been shown.
    #[serde(default)]
    pub has_code_md_external_includes_warning_shown: bool,

    // MCP server approval (kept for backward compat with older clients).
    #[serde(default)]
    pub enabled_mcpjson_servers: Vec<String>,
    #[serde(default)]
    pub disabled_mcpjson_servers: Vec<String>,
    #[serde(default)]
    pub enable_all_project_mcp_servers: bool,

    /// Disable specific MCP servers (all scopes).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disabled_mcp_servers: Option<Vec<String>>,
    /// Opt-in for built-in MCP servers that default to disabled.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled_mcp_servers: Option<Vec<String>>,

    /// Active worktree session, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active_worktree_session: Option<ActiveWorktreeSession>,

    /// Spawn mode for remote-control multi-session.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remote_control_spawn_mode: Option<String>,
}

// ── I/O helpers ───────────────────────────────────────────────────────────────

/// Return the path to the project settings file for a given project root.
///
/// Ref: src/utils/config.ts getProjectConfigPath
pub fn project_settings_path(project_root: &std::path::Path) -> std::path::PathBuf {
    project_root.join(".claude").join("settings.json")
}

/// Return the path to the local (gitignored) project settings file.
pub fn project_local_settings_path(project_root: &std::path::Path) -> std::path::PathBuf {
    project_root.join(".claude").join("settings.local.json")
}

/// Load project-level settings from `<root>/.claude/settings.json`.
///
/// Returns `None` if the file does not exist.
pub async fn load_project_settings(
    project_root: &std::path::Path,
) -> anyhow::Result<Option<crate::settings::SettingsJson>> {
    crate::settings::load_settings_file(&project_settings_path(project_root)).await
}

/// Load the local project settings from `<root>/.claude/settings.local.json`.
pub async fn load_project_local_settings(
    project_root: &std::path::Path,
) -> anyhow::Result<Option<crate::settings::SettingsJson>> {
    crate::settings::load_settings_file(&project_local_settings_path(project_root)).await
}
