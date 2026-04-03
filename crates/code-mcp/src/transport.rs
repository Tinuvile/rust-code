//! MCP transport trait and JSON-RPC 2.0 wire types.
//!
//! All transport implementations (stdio, HTTP, SSE) implement `McpTransport`.
//! The wire types in this module are the vocabulary shared by every layer.
//!
//! Ref: src/services/mcp/client.ts, MCP specification 2024-11-05

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;

// ── JSON-RPC 2.0 envelope types ───────────────────────────────────────────────

/// A JSON-RPC 2.0 request identifier (number or string).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(untagged)]
pub enum RequestId {
    Number(u64),
    Str(String),
}

/// An outbound JSON-RPC 2.0 request.
#[derive(Debug, Clone, Serialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: &'static str,
    pub id: RequestId,
    pub method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<Value>,
}

impl JsonRpcRequest {
    pub fn new(id: u64, method: impl Into<String>, params: Option<Value>) -> Self {
        Self {
            jsonrpc: "2.0",
            id: RequestId::Number(id),
            method: method.into(),
            params,
        }
    }
}

/// An inbound JSON-RPC 2.0 response.
#[derive(Debug, Clone, Deserialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    pub id: Option<RequestId>,
    pub result: Option<Value>,
    pub error: Option<JsonRpcError>,
}

/// An inbound JSON-RPC 2.0 notification (no `id`).
#[derive(Debug, Clone, Deserialize)]
pub struct JsonRpcNotification {
    pub jsonrpc: String,
    pub method: String,
    pub params: Option<Value>,
}

/// A JSON-RPC 2.0 error object.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct JsonRpcError {
    pub code: i32,
    pub message: String,
    pub data: Option<Value>,
}

// Standard JSON-RPC error codes.
pub const ERR_PARSE_ERROR: i32 = -32700;
pub const ERR_INVALID_REQUEST: i32 = -32600;
pub const ERR_METHOD_NOT_FOUND: i32 = -32601;
pub const ERR_INVALID_PARAMS: i32 = -32602;
pub const ERR_INTERNAL: i32 = -32603;

// ── MCP protocol types ────────────────────────────────────────────────────────

/// MCP protocol version used by this client.
pub const MCP_PROTOCOL_VERSION: &str = "2024-11-05";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InitializeParams {
    pub protocol_version: String,
    pub capabilities: ClientCapabilities,
    pub client_info: ClientInfo,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ClientCapabilities {
    // Future: roots, sampling
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientInfo {
    pub name: String,
    pub version: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InitializeResult {
    pub protocol_version: String,
    pub capabilities: ServerCapabilities,
    pub server_info: Option<ServerInfo>,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ServerCapabilities {
    pub tools: Option<ToolsCapability>,
    pub resources: Option<ResourcesCapability>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolsCapability {
    pub list_changed: Option<bool>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourcesCapability {
    pub subscribe: Option<bool>,
    pub list_changed: Option<bool>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ServerInfo {
    pub name: String,
    pub version: String,
}

// ── Tools ─────────────────────────────────────────────────────────────────────

/// A tool definition advertised by an MCP server.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpToolDef {
    pub name: String,
    pub description: Option<String>,
    /// JSON Schema for the tool's input parameters.
    pub input_schema: Value,
    pub annotations: Option<McpToolAnnotations>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpToolAnnotations {
    pub read_only_hint: Option<bool>,
    pub destructive_hint: Option<bool>,
    pub idempotent_hint: Option<bool>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolsListResult {
    pub tools: Vec<McpToolDef>,
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ToolCallParams {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub arguments: Option<Value>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolCallResult {
    pub content: Vec<McpContent>,
    pub is_error: Option<bool>,
}

/// A content block returned by an MCP tool call.
#[derive(Debug, Clone, Deserialize)]
pub struct McpContent {
    #[serde(rename = "type")]
    pub kind: String,
    pub text: Option<String>,
    // Image / resource fields omitted for Phase 7; can be added later.
}

// ── Resources ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpResourceDef {
    pub uri: String,
    pub name: String,
    pub description: Option<String>,
    pub mime_type: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourcesListResult {
    pub resources: Vec<McpResourceDef>,
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourceReadResult {
    pub contents: Vec<McpContent>,
}

// ── Transport error ───────────────────────────────────────────────────────────

#[derive(Debug, thiserror::Error)]
pub enum TransportError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("process exited with status {0:?}")]
    ProcessExited(Option<i32>),

    #[error("connection closed")]
    ConnectionClosed,

    #[error("request timed out")]
    Timeout,

    #[error("http error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("protocol error: {0}")]
    Protocol(String),
}

pub type TransportResult<T> = Result<T, TransportError>;

// ── McpTransport trait ────────────────────────────────────────────────────────

/// Abstract transport layer for MCP JSON-RPC communication.
///
/// Implementations: [`StdioTransport`], [`HttpTransport`], [`SseTransport`].
#[async_trait]
pub trait McpTransport: Send + Sync {
    /// Send a JSON-RPC request and await the matching response.
    async fn send_request(&self, req: &JsonRpcRequest) -> TransportResult<JsonRpcResponse>;

    /// Subscribe to server-initiated notifications.
    fn notifications(&self) -> tokio::sync::broadcast::Receiver<JsonRpcNotification>;

    /// Cleanly shut down the transport.
    async fn close(&self) -> TransportResult<()>;

    /// Whether the transport is currently considered connected.
    fn is_connected(&self) -> bool;
}
