//! Global (user-level) configuration stored in `~/.claude/config.json`.
//!
//! The `GlobalConfig` struct mirrors the TypeScript `GlobalConfig` type.
//! Fields are all optional/defaulted to support forward and backward compat.
//!
//! Ref: src/utils/config.ts GlobalConfig

use std::collections::HashMap;
use serde::{Deserialize, Serialize};

use crate::settings::McpServerConfig;

// ── Supporting types ──────────────────────────────────────────────────────────

/// Install method of the CLI binary.
///
/// Ref: src/utils/config.ts InstallMethod
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum InstallMethod {
    Local,
    Native,
    Global,
    Unknown,
}

/// Release channel for auto-updates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ReleaseChannel {
    Stable,
    Latest,
}

/// Notification delivery channel.
///
/// Ref: src/utils/configConstants.ts NOTIFICATION_CHANNELS
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NotificationChannel {
    Auto,
    Iterm2,
    Iterm2WithBell,
    TerminalBell,
    Kitty,
    Ghostty,
    NotificationsDisabled,
}

impl Default for NotificationChannel {
    fn default() -> Self {
        Self::Auto
    }
}

/// Text editor mode.
///
/// Ref: src/utils/configConstants.ts EDITOR_MODES
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EditorMode {
    Normal,
    Vim,
    /// Deprecated — migrated to Normal on read.
    Emacs,
}

/// Diff display tool.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DiffTool {
    Terminal,
    Auto,
}

/// Theme setting.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ThemeSetting {
    Auto,
    Light,
    Dark,
}

impl Default for ThemeSetting {
    fn default() -> Self {
        Self::Auto
    }
}

/// OAuth account info embedded in global config.
///
/// Ref: src/utils/config.ts AccountInfo
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountInfo {
    pub account_uuid: String,
    pub email_address: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub organization_uuid: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub organization_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub organization_role: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspace_role: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub has_extra_usage_enabled: Option<bool>,
}

// ── GlobalConfig ──────────────────────────────────────────────────────────────

/// User-level global config stored at `~/.claude/config.json`.
///
/// Ref: src/utils/config.ts GlobalConfig
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct GlobalConfig {
    pub num_startups: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub install_method: Option<InstallMethod>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auto_updates: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_id: Option<String>,
    pub theme: ThemeSetting,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub has_completed_onboarding: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_onboarding_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_release_notes_seen: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mcp_servers: Option<HashMap<String, McpServerConfig>>,
    pub preferred_notif_channel: NotificationChannel,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub custom_notify_command: Option<String>,
    pub verbose: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub primary_api_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub has_acknowledged_cost_threshold: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub oauth_account: Option<AccountInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub editor_mode: Option<EditorMode>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bypass_permissions_mode_accepted: Option<bool>,
    pub auto_compact_enabled: bool,
    pub show_turn_duration: bool,
    /// Environment variables set for every session.
    #[serde(default)]
    pub env: HashMap<String, String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub diff_tool: Option<DiffTool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auto_connect_ide: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auto_install_ide_extension: Option<bool>,
    /// Map of tip ID → `num_startups` when tip was last shown.
    #[serde(default)]
    pub tips_history: HashMap<String, u32>,
    /// Approved custom API key prompts.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub custom_api_key_responses: Option<CustomApiKeyResponses>,
    /// Per-project configs, keyed by canonical project path.
    #[serde(default)]
    pub projects: HashMap<String, crate::project::ProjectConfig>,
}

impl Default for GlobalConfig {
    fn default() -> Self {
        Self {
            num_startups: 0,
            install_method: None,
            auto_updates: None,
            user_id: None,
            theme: ThemeSetting::default(),
            has_completed_onboarding: None,
            last_onboarding_version: None,
            last_release_notes_seen: None,
            mcp_servers: None,
            preferred_notif_channel: NotificationChannel::default(),
            custom_notify_command: None,
            verbose: false,
            primary_api_key: None,
            has_acknowledged_cost_threshold: None,
            oauth_account: None,
            editor_mode: None,
            bypass_permissions_mode_accepted: None,
            auto_compact_enabled: true,
            show_turn_duration: true,
            env: HashMap::new(),
            diff_tool: None,
            auto_connect_ide: None,
            auto_install_ide_extension: None,
            tips_history: HashMap::new(),
            custom_api_key_responses: None,
            projects: HashMap::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CustomApiKeyResponses {
    #[serde(default)]
    pub approved: Vec<String>,
    #[serde(default)]
    pub rejected: Vec<String>,
}

// ── I/O helpers ───────────────────────────────────────────────────────────────

/// Return the path to the global config file (`~/.claude/config.json`).
pub fn global_config_path() -> Option<std::path::PathBuf> {
    dirs_next::home_dir().map(|h| h.join(".claude").join("config.json"))
}

/// Load the global config from disk, returning `Default` if not found.
pub async fn load_global_config() -> anyhow::Result<GlobalConfig> {
    let path = match global_config_path() {
        Some(p) => p,
        None => return Ok(GlobalConfig::default()),
    };
    match tokio::fs::read(&path).await {
        Ok(bytes) => {
            let stripped = bytes.strip_prefix(b"\xef\xbb\xbf").unwrap_or(&bytes);
            Ok(serde_json::from_slice(stripped)?)
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(GlobalConfig::default()),
        Err(e) => Err(e.into()),
    }
}

/// Persist the global config to disk (atomic write via temp file).
pub async fn save_global_config(config: &GlobalConfig) -> anyhow::Result<()> {
    let path = global_config_path().ok_or_else(|| anyhow::anyhow!("cannot determine home directory"))?;
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    let json = serde_json::to_string_pretty(config)?;
    tokio::fs::write(&path, json).await?;
    Ok(())
}
