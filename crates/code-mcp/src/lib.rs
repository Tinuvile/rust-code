//! MCP (Model Context Protocol) client implementation.
//!
//! Provides three transports (stdio, HTTP, SSE), a JSON-RPC 2.0 client,
//! connection lifecycle management, dynamic tool registration, config loading,
//! OAuth token storage, and an optional official server registry fetch.
//!
//! ## Quick start
//!
//! ```rust,no_run
//! use code_mcp::{McpConnectionManager, McpSessionConfig};
//! use code_config::settings::SettingsJson;
//!
//! async fn setup(settings: &SettingsJson) -> McpConnectionManager {
//!     let session_cfg = McpSessionConfig::from_settings(settings);
//!     let mut manager = McpConnectionManager::new();
//!     for (name, config) in session_cfg.servers {
//!         if let Err(e) = manager.connect_server(name.clone(), config).await {
//!             tracing::warn!("mcp server '{name}' failed to connect: {e}");
//!         }
//!     }
//!     manager
//! }
//! ```
//!
//! Ref: src/services/mcp/client.ts, src/services/mcp/MCPConnectionManager.tsx

pub mod transport;
pub mod transport_stdio;
pub mod transport_sse;
pub mod transport_http;
pub mod client;
pub mod connection;
pub mod config;
pub mod auth;
pub mod dynamic_tool;
pub mod registry_fetch;

// ── Re-exports ────────────────────────────────────────────────────────────────

// Wire / protocol types.
pub use transport::{
    InitializeResult,
    JsonRpcError,
    JsonRpcNotification,
    JsonRpcRequest,
    JsonRpcResponse,
    McpContent,
    McpResourceDef,
    McpToolDef,
    McpToolAnnotations,
    McpTransport,
    MCP_PROTOCOL_VERSION,
    RequestId,
    ServerCapabilities,
    ToolCallResult,
    TransportError,
    TransportResult,
};

// Transport implementations.
pub use transport_stdio::StdioTransport;
pub use transport_http::HttpTransport;
pub use transport_sse::SseTransport;

// Client.
pub use client::McpClient;

// Connection management.
pub use connection::{
    ConnectionState,
    McpConnection,
    McpConnectionManager,
    spawn_reconnect_task,
};

// Configuration.
pub use config::McpSessionConfig;

// Auth.
pub use auth::{McpToken, McpTokenStore};

// Dynamic tool integration.
pub use dynamic_tool::{build_mcp_tool, deregister_server_tools, register_server_tools};

// Registry.
pub use registry_fetch::{fetch_registry, RegistryEntry};
