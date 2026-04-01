//! Settings schema and layered merge.
//!
//! Implements the `SettingsJson` struct that matches the settings.json schema
//! and a layered merge of all setting sources in priority order:
//! MDM > enterprise (managed-settings) > remote-managed > user settings > project settings
//!
//! Ref: src/utils/settings/types.ts SettingsSchema
//! Ref: src/utils/settings/settings.ts (merge logic)

use std::collections::HashMap;
use serde::{Deserialize, Serialize};

use code_types::permissions::ExternalPermissionMode;

// ── Permission rules within settings ─────────────────────────────────────────

/// The `permissions` block inside a settings.json file.
///
/// Ref: src/utils/settings/types.ts PermissionsSchema
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SettingsPermissions {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allow: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deny: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ask: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_mode: Option<ExternalPermissionMode>,
    /// `"disable"` to prevent users from enabling bypass-permissions mode.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disable_bypass_permissions_mode: Option<String>,
    /// Additional working directories granted access.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub additional_directories: Option<Vec<String>>,
}

// ── Hook configuration (simplified) ──────────────────────────────────────────

/// A single hook entry — either a shell command string or an HTTP URL.
///
/// Ref: src/schemas/hooks.ts HookCommand
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum HookCommand {
    Bash { command: String },
    Http { url: String },
}

/// The `hooks` block in settings.json.
///
/// Keys are event names: PreToolUse, PostToolUse, Notification, etc.
///
/// Ref: src/schemas/hooks.ts HooksSchema
pub type HooksSettings = HashMap<String, Vec<HookCommand>>;

// ── MCP server config ─────────────────────────────────────────────────────────

/// Configuration for a single MCP server.
///
/// Ref: src/services/mcp/types.ts McpServerConfig
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum McpServerConfig {
    Stdio(StdioMcpServer),
    Http(HttpMcpServer),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StdioMcpServer {
    #[serde(rename = "type", default = "default_stdio")]
    pub kind: String,
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub env: Option<HashMap<String, String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
}

fn default_stdio() -> String { "stdio".to_owned() }

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HttpMcpServer {
    #[serde(rename = "type")]
    pub kind: String, // "http" | "sse"
    pub url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub headers: Option<HashMap<String, String>>,
}

// ── Main SettingsJson ─────────────────────────────────────────────────────────

/// The full settings.json schema.
///
/// All fields are optional (settings can be partial).
/// Unknown fields are preserved via `#[serde(flatten)]`.
///
/// Priority when merged: MDM > managed-settings > remote-managed > user > project.
///
/// Ref: src/utils/settings/types.ts SettingsSchema
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SettingsJson {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key_helper: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub aws_credential_export: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub aws_auth_refresh: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gcp_auth_refresh: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub env: Option<HashMap<String, String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub permissions: Option<SettingsPermissions>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hooks: Option<HooksSettings>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mcp_servers: Option<HashMap<String, McpServerConfig>>,
    /// Number of days to retain transcripts (0 = disabled).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cleanup_period_days: Option<u32>,
    /// Whether to respect .gitignore in file pickers.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub respect_gitignore: Option<bool>,
    /// Tools the model is always allowed to use without confirmation.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_tools: Option<Vec<String>>,
    /// Disable specific tools entirely.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disabled_tools: Option<Vec<String>>,
    /// Disable specific MCP server connections.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disabled_mcp_servers: Option<Vec<String>>,
    /// Opt-in list for built-in MCP servers that default to disabled.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled_mcp_servers: Option<Vec<String>>,
    /// Model override.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// Custom system prompt appended to the default.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub append_system_prompt: Option<String>,
    /// Maximum number of tokens the model may use per turn.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    /// Whether auto-compact is enabled.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auto_compact_enabled: Option<bool>,
    /// Whether to include attribution trailers in commits.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub include_co_authors: Option<bool>,
    /// Additional unknown fields preserved for forward-compatibility.
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

// ── Source priority levels ────────────────────────────────────────────────────

/// The ordered list of settings sources, from highest to lowest priority.
///
/// The `#[repr(u8)]` discriminants are the sort keys: 0 = highest priority.
///
/// Ref: src/utils/settings/constants.ts SettingSource
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u8)]
pub enum SettingSource {
    /// Machine-level MDM policy (highest).
    Mdm = 0,
    /// Managed file on disk (`managed-settings.json` + drop-ins).
    ManagedFile = 1,
    /// Remote-managed settings fetched from API.
    RemoteManaged = 2,
    /// User-level `~/.claude/settings.json`.
    User = 3,
    /// Project-level `.claude/settings.json`.
    Project = 4,
    /// Local project override `.claude/settings.local.json` (lowest).
    LocalProject = 5,
}

// ── Merge helpers ─────────────────────────────────────────────────────────────

/// Merge `overlay` on top of `base`, returning a new merged `SettingsJson`.
///
/// For scalar `Option` fields: overlay wins if `Some`.
/// For collection fields (Vec, HashMap): overlay appends / extends.
///
/// Ref: src/utils/settings/settings.ts settingsMergeCustomizer
pub fn merge_settings(base: SettingsJson, overlay: SettingsJson) -> SettingsJson {
    SettingsJson {
        api_key_helper: overlay.api_key_helper.or(base.api_key_helper),
        aws_credential_export: overlay.aws_credential_export.or(base.aws_credential_export),
        aws_auth_refresh: overlay.aws_auth_refresh.or(base.aws_auth_refresh),
        gcp_auth_refresh: overlay.gcp_auth_refresh.or(base.gcp_auth_refresh),
        env: merge_opt_map(base.env, overlay.env),
        permissions: merge_permissions(base.permissions, overlay.permissions),
        hooks: merge_opt_map(base.hooks, overlay.hooks),
        mcp_servers: merge_opt_map(base.mcp_servers, overlay.mcp_servers),
        cleanup_period_days: overlay.cleanup_period_days.or(base.cleanup_period_days),
        respect_gitignore: overlay.respect_gitignore.or(base.respect_gitignore),
        allowed_tools: merge_opt_vec(base.allowed_tools, overlay.allowed_tools),
        disabled_tools: merge_opt_vec(base.disabled_tools, overlay.disabled_tools),
        disabled_mcp_servers: merge_opt_vec(base.disabled_mcp_servers, overlay.disabled_mcp_servers),
        enabled_mcp_servers: merge_opt_vec(base.enabled_mcp_servers, overlay.enabled_mcp_servers),
        model: overlay.model.or(base.model),
        append_system_prompt: overlay.append_system_prompt.or(base.append_system_prompt),
        max_tokens: overlay.max_tokens.or(base.max_tokens),
        auto_compact_enabled: overlay.auto_compact_enabled.or(base.auto_compact_enabled),
        include_co_authors: overlay.include_co_authors.or(base.include_co_authors),
        extra: {
            let mut m = base.extra;
            m.extend(overlay.extra);
            m
        },
    }
}

/// Merge layered settings in priority order (first = highest priority).
///
/// The function folds from lowest-priority upward so that each higher-priority
/// source can override lower ones.
pub fn merge_all(sources: Vec<(SettingSource, SettingsJson)>) -> SettingsJson {
    // Sort lowest → highest priority so we fold higher onto lower.
    let mut sorted = sources;
    // Sort highest ordinal (lowest priority) first so the fold applies
    // higher-priority sources last and they win via `merge_settings`.
    sorted.sort_by(|(a, _), (b, _)| b.cmp(a));
    sorted.into_iter().fold(SettingsJson::default(), |acc, (_, s)| merge_settings(acc, s))
}

fn merge_opt_vec<T>(base: Option<Vec<T>>, overlay: Option<Vec<T>>) -> Option<Vec<T>> {
    match (base, overlay) {
        (None, None) => None,
        (Some(b), None) => Some(b),
        (None, Some(o)) => Some(o),
        (Some(mut b), Some(o)) => {
            b.extend(o);
            Some(b)
        }
    }
}

fn merge_opt_map<K, V>(
    base: Option<HashMap<K, V>>,
    overlay: Option<HashMap<K, V>>,
) -> Option<HashMap<K, V>>
where
    K: Eq + std::hash::Hash,
{
    match (base, overlay) {
        (None, None) => None,
        (Some(b), None) => Some(b),
        (None, Some(o)) => Some(o),
        (Some(mut b), Some(o)) => {
            b.extend(o);
            Some(b)
        }
    }
}

fn merge_permissions(
    base: Option<SettingsPermissions>,
    overlay: Option<SettingsPermissions>,
) -> Option<SettingsPermissions> {
    match (base, overlay) {
        (None, None) => None,
        (Some(b), None) => Some(b),
        (None, Some(o)) => Some(o),
        (Some(b), Some(o)) => Some(SettingsPermissions {
            allow: merge_opt_vec(b.allow, o.allow),
            deny: merge_opt_vec(b.deny, o.deny),
            ask: merge_opt_vec(b.ask, o.ask),
            default_mode: o.default_mode.or(b.default_mode),
            disable_bypass_permissions_mode: o
                .disable_bypass_permissions_mode
                .or(b.disable_bypass_permissions_mode),
            additional_directories: merge_opt_vec(
                b.additional_directories,
                o.additional_directories,
            ),
        }),
    }
}


// ── Parsing ───────────────────────────────────────────────────────────────────

/// Parse a settings.json file from a byte slice, returning the result and any
/// parse warnings/errors (which are non-fatal — unknown fields are preserved).
pub fn parse_settings(bytes: &[u8]) -> anyhow::Result<SettingsJson> {
    // Strip UTF-8 BOM if present.
    let bytes = bytes.strip_prefix(b"\xef\xbb\xbf").unwrap_or(bytes);
    let settings: SettingsJson = serde_json::from_slice(bytes)?;
    Ok(settings)
}

/// Load settings from a file path, returning `None` if the file does not exist.
pub async fn load_settings_file(path: &std::path::Path) -> anyhow::Result<Option<SettingsJson>> {
    match tokio::fs::read(path).await {
        Ok(bytes) => Ok(Some(parse_settings(&bytes)?)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.into()),
    }
}
