//! MCP configuration loading.
//!
//! Resolves the set of MCP servers to connect for a session by merging
//! global, project, and CLI-provided configurations.
//!
//! Ref: src/services/mcp/config.ts

use std::collections::{HashMap, HashSet};

use code_config::settings::{McpServerConfig, SettingsJson};

// ── McpSessionConfig ──────────────────────────────────────────────────────────

/// Resolved set of MCP server configurations for a single session.
#[derive(Debug, Clone, Default)]
pub struct McpSessionConfig {
    /// Ordered list of (server_name, config) to connect.
    pub servers: Vec<(String, McpServerConfig)>,
    /// Names of disabled servers (for informational use; already filtered out).
    pub disabled: HashSet<String>,
}

impl McpSessionConfig {
    /// Build from a fully-merged [`SettingsJson`].
    ///
    /// The caller is responsible for merging settings layers (global → project
    /// → user → MDM); this function only reads the already-merged result.
    pub fn from_settings(settings: &SettingsJson) -> Self {
        let disabled: HashSet<String> = settings
            .disabled_mcp_servers
            .as_deref()
            .unwrap_or(&[])
            .iter()
            .cloned()
            .collect();

        let servers = settings
            .mcp_servers
            .as_ref()
            .map(|map| {
                let mut entries: Vec<(String, McpServerConfig)> = map
                    .iter()
                    .filter(|(name, _)| !disabled.contains(*name))
                    .map(|(name, cfg)| (name.clone(), cfg.clone()))
                    .collect();
                // Deterministic ordering by server name.
                entries.sort_by(|a, b| a.0.cmp(&b.0));
                entries
            })
            .unwrap_or_default();

        Self { servers, disabled }
    }

    /// Build from separate global, project, and CLI-provided server maps.
    ///
    /// Priority: CLI overrides > project > global.
    /// Any name in `disabled` is excluded from the final list.
    pub fn resolve(
        global_servers: &HashMap<String, McpServerConfig>,
        project_servers: &HashMap<String, McpServerConfig>,
        cli_servers: &HashMap<String, McpServerConfig>,
        disabled: &[String],
    ) -> Self {
        let disabled_set: HashSet<String> = disabled.iter().cloned().collect();

        // Merge: later entries win (global → project → cli).
        let mut merged: HashMap<String, McpServerConfig> = HashMap::new();
        for (k, v) in global_servers {
            merged.insert(k.clone(), v.clone());
        }
        for (k, v) in project_servers {
            merged.insert(k.clone(), v.clone());
        }
        for (k, v) in cli_servers {
            merged.insert(k.clone(), v.clone());
        }

        let mut servers: Vec<(String, McpServerConfig)> = merged
            .into_iter()
            .filter(|(name, _)| !disabled_set.contains(name))
            .collect();
        servers.sort_by(|a, b| a.0.cmp(&b.0));

        Self {
            servers,
            disabled: disabled_set,
        }
    }

    /// Returns `true` if no servers are configured.
    pub fn is_empty(&self) -> bool {
        self.servers.is_empty()
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use code_config::settings::{HttpMcpServer, StdioMcpServer};

    fn stdio(cmd: &str) -> McpServerConfig {
        McpServerConfig::Stdio(StdioMcpServer {
            kind: "stdio".to_owned(),
            command: cmd.to_owned(),
            args: vec![],
            env: None,
            cwd: None,
        })
    }

    fn http(url: &str) -> McpServerConfig {
        McpServerConfig::Http(HttpMcpServer {
            kind: "http".to_owned(),
            url: url.to_owned(),
            headers: None,
        })
    }

    #[test]
    fn from_settings_filters_disabled() {
        let mut settings = SettingsJson::default();
        let mut servers = HashMap::new();
        servers.insert("alpha".to_owned(), stdio("alpha-server"));
        servers.insert("beta".to_owned(), stdio("beta-server"));
        settings.mcp_servers = Some(servers);
        settings.disabled_mcp_servers = Some(vec!["beta".to_owned()]);

        let cfg = McpSessionConfig::from_settings(&settings);
        assert_eq!(cfg.servers.len(), 1);
        assert_eq!(cfg.servers[0].0, "alpha");
        assert!(cfg.disabled.contains("beta"));
    }

    #[test]
    fn resolve_priority_cli_wins() {
        let global: HashMap<_, _> = [("s".to_owned(), stdio("global"))].into();
        let project: HashMap<_, _> = [("s".to_owned(), stdio("project"))].into();
        let cli: HashMap<_, _> = [("s".to_owned(), http("http://cli"))].into();

        let cfg = McpSessionConfig::resolve(&global, &project, &cli, &[]);
        assert_eq!(cfg.servers.len(), 1);
        // CLI value should win — it's an Http variant.
        assert!(matches!(cfg.servers[0].1, McpServerConfig::Http(_)));
    }

    #[test]
    fn resolve_disabled_excluded() {
        let global: HashMap<_, _> = [
            ("a".to_owned(), stdio("a")),
            ("b".to_owned(), stdio("b")),
        ]
        .into();
        let cfg = McpSessionConfig::resolve(&global, &Default::default(), &Default::default(), &["b".to_owned()]);
        assert_eq!(cfg.servers.len(), 1);
        assert_eq!(cfg.servers[0].0, "a");
    }
}
