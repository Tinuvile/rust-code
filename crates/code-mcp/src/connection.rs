//! MCP connection lifecycle management.
//!
//! `McpConnection` wraps a single MCP server: it builds the transport, runs
//! the initialize handshake, caches tool definitions, and handles reconnection.
//!
//! `McpConnectionManager` holds all server connections for a session.
//!
//! Ref: src/services/mcp/MCPConnectionManager.tsx,
//!      src/utils/hooks/useManageMCPConnections.ts

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::Context;
use code_config::settings::{HttpMcpServer, McpServerConfig, StdioMcpServer};
use tokio::sync::{Mutex, RwLock};
use tracing::{debug, info, warn};

use crate::client::McpClient;
use crate::transport::McpToolDef;
use crate::transport_http::HttpTransport;
use crate::transport_sse::SseTransport;
use crate::transport_stdio::StdioTransport;

// ── ConnectionState ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum ConnectionState {
    Disconnected,
    Connecting,
    Connected,
    Reconnecting { attempt: u32 },
    Failed(String),
}

// ── McpConnection ─────────────────────────────────────────────────────────────

/// A single MCP server connection with lifecycle management.
pub struct McpConnection {
    pub server_name: String,
    config: McpServerConfig,
    /// The live client (None when disconnected).
    client: Option<Arc<Mutex<McpClient>>>,
    state: Arc<Mutex<ConnectionState>>,
    /// Cached tool definitions from the last successful `list_tools`.
    tools: Arc<RwLock<Vec<McpToolDef>>>,
}

impl McpConnection {
    pub fn new(server_name: impl Into<String>, config: McpServerConfig) -> Self {
        Self {
            server_name: server_name.into(),
            config,
            client: None,
            state: Arc::new(Mutex::new(ConnectionState::Disconnected)),
            tools: Arc::new(RwLock::new(vec![])),
        }
    }

    /// Connect, initialize, and populate the tool cache.
    pub async fn connect(&mut self) -> anyhow::Result<()> {
        *self.state.lock().await = ConnectionState::Connecting;

        let transport: Box<dyn crate::transport::McpTransport> =
            build_transport(&self.server_name, &self.config)?;

        let mut client = McpClient::new(transport, &self.server_name);
        client
            .initialize()
            .await
            .with_context(|| format!("failed to initialize mcp server '{}'", self.server_name))?;

        let tools = client
            .list_tools()
            .await
            .with_context(|| {
                format!("failed to list tools for mcp server '{}'", self.server_name)
            })?;

        *self.tools.write().await = tools;
        *self.state.lock().await = ConnectionState::Connected;

        let client_arc = Arc::new(Mutex::new(client));
        self.client = Some(Arc::clone(&client_arc));

        // Spawn a watcher that refreshes the tool list on notification.
        spawn_tool_list_watcher(
            &self.server_name,
            Arc::clone(&client_arc),
            Arc::clone(&self.tools),
        );

        info!(server = %self.server_name, "mcp server connected");
        Ok(())
    }

    /// Disconnect and reset state.
    pub async fn disconnect(&mut self) {
        if let Some(client) = self.client.take() {
            let _ = client.lock().await;
            // Transport close is best-effort.
        }
        *self.state.lock().await = ConnectionState::Disconnected;
        debug!(server = %self.server_name, "mcp server disconnected");
    }

    /// Get a reference to the live client, if connected.
    pub fn client(&self) -> Option<Arc<Mutex<McpClient>>> {
        self.client.as_ref().cloned()
    }

    /// Get the cached tool definitions.
    pub async fn tool_defs(&self) -> Vec<McpToolDef> {
        self.tools.read().await.clone()
    }

    /// Current connection state.
    pub async fn state(&self) -> ConnectionState {
        self.state.lock().await.clone()
    }
}

// ── Transport builder ─────────────────────────────────────────────────────────

fn build_transport(
    server_name: &str,
    config: &McpServerConfig,
) -> anyhow::Result<Box<dyn crate::transport::McpTransport>> {
    match config {
        McpServerConfig::Stdio(cfg) => build_stdio_transport(server_name, cfg),
        McpServerConfig::Http(cfg) => build_http_transport(server_name, cfg),
    }
}

fn build_stdio_transport(
    server_name: &str,
    cfg: &StdioMcpServer,
) -> anyhow::Result<Box<dyn crate::transport::McpTransport>> {
    // StdioTransport::spawn is async; we need to block here because
    // build_transport is sync and called from an async context.
    // We use tokio::task::block_in_place so we can await within it.
    let transport = tokio::task::block_in_place(|| {
        tokio::runtime::Handle::current().block_on(async {
            StdioTransport::spawn(
                &cfg.command,
                &cfg.args,
                cfg.env.as_ref(),
                cfg.cwd.as_deref(),
            )
            .await
        })
    })
    .with_context(|| {
        format!(
            "failed to spawn mcp server '{}' (command: {})",
            server_name, cfg.command
        )
    })?;
    Ok(Box::new(transport))
}

fn build_http_transport(
    server_name: &str,
    cfg: &HttpMcpServer,
) -> anyhow::Result<Box<dyn crate::transport::McpTransport>> {
    if cfg.kind == "sse" {
        // SSE: use the same URL for both POST and SSE stream.
        let transport = SseTransport::new(
            &cfg.url,
            &cfg.url,
            None,
            cfg.headers.as_ref(),
        )
        .with_context(|| format!("failed to create SSE transport for '{}'", server_name))?;
        Ok(Box::new(transport))
    } else {
        let transport = HttpTransport::new(&cfg.url, None, cfg.headers.as_ref())
            .with_context(|| format!("failed to create HTTP transport for '{}'", server_name))?;
        Ok(Box::new(transport))
    }
}

// ── Tool list watcher ─────────────────────────────────────────────────────────

fn spawn_tool_list_watcher(
    server_name: &str,
    client: Arc<Mutex<McpClient>>,
    tools: Arc<RwLock<Vec<McpToolDef>>>,
) {
    let name = server_name.to_owned();
    tokio::spawn(async move {
        let mut receiver = {
            let guard = client.lock().await;
            guard.tool_list_changed_receiver()
        };

        loop {
            match receiver.recv().await {
                Ok(()) => {
                    debug!(server = %name, "mcp tool list changed, refreshing");
                    let new_tools = {
                        let guard = client.lock().await;
                        guard.list_tools().await
                    };
                    match new_tools {
                        Ok(t) => *tools.write().await = t,
                        Err(e) => warn!(server = %name, "failed to refresh mcp tool list: {e}"),
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    warn!(server = %name, "mcp notification receiver lagged by {n}");
                }
            }
        }
    });
}

// ── Reconnect helper (used externally) ───────────────────────────────────────

/// Spawn a background task that attempts to reconnect `connection` with
/// exponential backoff.  Maximum 5 attempts.
pub fn spawn_reconnect_task(connection: Arc<RwLock<McpConnection>>) {
    tokio::spawn(async move {
        const MAX_ATTEMPTS: u32 = 5;
        const BASE_MS: u64 = 1_000;
        const MAX_DELAY_MS: u64 = 60_000;

        for attempt in 1..=MAX_ATTEMPTS {
            let delay_ms = std::cmp::min(BASE_MS * (1u64 << (attempt - 1)), MAX_DELAY_MS);
            tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;

            let server_name = connection.read().await.server_name.clone();
            debug!(server = %server_name, attempt, "mcp reconnect attempt");

            {
                let mut guard = connection.write().await;
                *guard.state.lock().await = ConnectionState::Reconnecting { attempt };
                match guard.connect().await {
                    Ok(()) => {
                        info!(server = %server_name, "mcp server reconnected");
                        return;
                    }
                    Err(e) => {
                        warn!(server = %server_name, attempt, "mcp reconnect failed: {e}");
                    }
                }
            }
        }

        let guard = connection.write().await;
        let msg = format!("failed to reconnect after {MAX_ATTEMPTS} attempts");
        warn!(server = %guard.server_name, "{msg}");
        *guard.state.lock().await = ConnectionState::Failed(msg);
    });
}

// ── McpConnectionManager ──────────────────────────────────────────────────────

/// Manages all MCP server connections for a session.
pub struct McpConnectionManager {
    connections: HashMap<String, Arc<RwLock<McpConnection>>>,
}

impl McpConnectionManager {
    pub fn new() -> Self {
        Self {
            connections: HashMap::new(),
        }
    }

    /// Add a server and connect to it.
    pub async fn connect_server(
        &mut self,
        name: String,
        config: McpServerConfig,
    ) -> anyhow::Result<()> {
        let mut conn = McpConnection::new(&name, config);
        conn.connect().await?;
        self.connections
            .insert(name, Arc::new(RwLock::new(conn)));
        Ok(())
    }

    /// Disconnect all servers.
    pub async fn disconnect_all(&mut self) {
        for (_, conn_lock) in self.connections.drain() {
            conn_lock.write().await.disconnect().await;
        }
    }

    /// Collect tool definitions from all connected servers.
    ///
    /// Returns `Vec<(server_name, Vec<McpToolDef>)>`.
    pub async fn all_tool_defs(&self) -> Vec<(String, Vec<McpToolDef>)> {
        let mut result = Vec::new();
        for (name, conn_lock) in &self.connections {
            let defs = conn_lock.read().await.tool_defs().await;
            if !defs.is_empty() {
                result.push((name.clone(), defs));
            }
        }
        result
    }

    /// Look up a connection by server name.
    pub fn get(&self, name: &str) -> Option<&Arc<RwLock<McpConnection>>> {
        self.connections.get(name)
    }

    /// Number of registered connections.
    pub fn len(&self) -> usize {
        self.connections.len()
    }

    pub fn is_empty(&self) -> bool {
        self.connections.is_empty()
    }
}

impl Default for McpConnectionManager {
    fn default() -> Self {
        Self::new()
    }
}
