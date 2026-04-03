//! MCP protocol client — speaks MCP on top of any `McpTransport`.
//!
//! Handles the initialize handshake, tool/resource listing (with pagination),
//! tool calls, and notification subscriptions.
//!
//! Ref: src/services/mcp/client.ts

use anyhow::{bail, Context};
use serde_json::{json, Value};
use tokio::sync::broadcast;
use tracing::{debug, warn};

use crate::transport::{
    ClientCapabilities, ClientInfo, InitializeParams, InitializeResult, JsonRpcRequest,
    McpContent, McpResourceDef, McpToolDef, McpTransport, MCP_PROTOCOL_VERSION,
    ResourcesListResult, ToolCallParams, ToolCallResult, ToolsListResult,
    JsonRpcNotification,
};

// ── McpClient ─────────────────────────────────────────────────────────────────

/// Stateful MCP client built on a transport.
///
/// Call [`McpClient::initialize`] before any other method.
pub struct McpClient {
    transport: Box<dyn McpTransport>,
    server_name: String,
    next_id: std::sync::atomic::AtomicU64,
    initialized: bool,
}

impl McpClient {
    pub fn new(transport: Box<dyn McpTransport>, server_name: impl Into<String>) -> Self {
        Self {
            transport,
            server_name: server_name.into(),
            next_id: std::sync::atomic::AtomicU64::new(1),
            initialized: false,
        }
    }

    fn next_id(&self) -> u64 {
        self.next_id
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    }

    fn make_request(&self, method: &str, params: Option<Value>) -> JsonRpcRequest {
        JsonRpcRequest::new(self.next_id(), method, params)
    }

    // ── Initialize ────────────────────────────────────────────────────────────

    /// Perform the MCP `initialize` handshake.
    ///
    /// Must be called once before any other method.
    pub async fn initialize(&mut self) -> anyhow::Result<InitializeResult> {
        let params = InitializeParams {
            protocol_version: MCP_PROTOCOL_VERSION.to_owned(),
            capabilities: ClientCapabilities::default(),
            client_info: ClientInfo {
                name: "claude-code-rust".to_owned(),
                version: env!("CARGO_PKG_VERSION").to_owned(),
            },
        };

        let req = self.make_request("initialize", Some(serde_json::to_value(&params)?));
        let resp = self
            .transport
            .send_request(&req)
            .await
            .with_context(|| format!("failed to initialize mcp server '{}'", self.server_name))?;

        let result = extract_result::<InitializeResult>(resp, &self.server_name, "initialize")?;

        // Send the initialized notification (fire-and-forget).
        let notif_req = self.make_request("notifications/initialized", None);
        // We ignore errors here — servers may not require this.
        let _ = self.transport.send_request(&notif_req).await;

        debug!(
            server = %self.server_name,
            protocol_version = %result.protocol_version,
            "mcp server initialized"
        );
        self.initialized = true;
        Ok(result)
    }

    // ── Tools ─────────────────────────────────────────────────────────────────

    /// List all tools advertised by the server (handles pagination).
    pub async fn list_tools(&self) -> anyhow::Result<Vec<McpToolDef>> {
        let mut tools = Vec::new();
        let mut cursor: Option<String> = None;

        loop {
            let params = match &cursor {
                Some(c) => Some(json!({ "cursor": c })),
                None => None,
            };
            let req = self.make_request("tools/list", params);
            let resp = self
                .transport
                .send_request(&req)
                .await
                .with_context(|| {
                    format!("failed to list tools for mcp server '{}'", self.server_name)
                })?;

            let page = extract_result::<ToolsListResult>(resp, &self.server_name, "tools/list")?;
            tools.extend(page.tools);

            match page.next_cursor {
                Some(c) if !c.is_empty() => cursor = Some(c),
                _ => break,
            }
        }

        debug!(server = %self.server_name, count = tools.len(), "mcp tools listed");
        Ok(tools)
    }

    /// Call a tool by name with JSON `arguments`.
    pub async fn call_tool(
        &self,
        name: &str,
        arguments: Value,
    ) -> anyhow::Result<ToolCallResult> {
        let params = ToolCallParams {
            name: name.to_owned(),
            arguments: if arguments.is_null() { None } else { Some(arguments) },
        };
        let req = self.make_request("tools/call", Some(serde_json::to_value(&params)?));
        let resp = self
            .transport
            .send_request(&req)
            .await
            .with_context(|| {
                format!(
                    "failed to call tool '{}' on mcp server '{}'",
                    name, self.server_name
                )
            })?;

        extract_result::<ToolCallResult>(resp, &self.server_name, "tools/call")
    }

    // ── Resources ─────────────────────────────────────────────────────────────

    /// List resources (best-effort — returns empty vec if not supported).
    pub async fn list_resources(&self) -> anyhow::Result<Vec<McpResourceDef>> {
        let req = self.make_request("resources/list", None);
        let resp = match self.transport.send_request(&req).await {
            Ok(r) => r,
            Err(e) => {
                warn!(server = %self.server_name, "resources/list failed: {e}");
                return Ok(vec![]);
            }
        };

        match extract_result::<ResourcesListResult>(resp, &self.server_name, "resources/list") {
            Ok(r) => Ok(r.resources),
            Err(e) => {
                warn!(server = %self.server_name, "resources/list error: {e}");
                Ok(vec![])
            }
        }
    }

    /// Read a resource by URI.
    pub async fn read_resource(&self, uri: &str) -> anyhow::Result<McpContent> {
        let req = self.make_request("resources/read", Some(json!({ "uri": uri })));
        let resp = self
            .transport
            .send_request(&req)
            .await
            .with_context(|| {
                format!(
                    "failed to read resource '{}' from mcp server '{}'",
                    uri, self.server_name
                )
            })?;

        let result = extract_result::<crate::transport::ResourceReadResult>(
            resp,
            &self.server_name,
            "resources/read",
        )?;

        result.contents.into_iter().next().ok_or_else(|| {
            anyhow::anyhow!("mcp server '{}' returned empty contents for '{}'", self.server_name, uri)
        })
    }

    // ── Notifications ─────────────────────────────────────────────────────────

    /// Subscribe to raw transport notifications.
    pub fn raw_notifications(&self) -> broadcast::Receiver<JsonRpcNotification> {
        self.transport.notifications()
    }

    /// Subscribe to `notifications/tools/list_changed` events.
    ///
    /// Returns a `broadcast::Receiver<()>` that fires whenever the server
    /// notifies that its tool list has changed.
    pub fn tool_list_changed_receiver(&self) -> broadcast::Receiver<()> {
        let (tx, rx) = broadcast::channel::<()>(4);
        let mut notifs = self.transport.notifications();

        tokio::spawn(async move {
            loop {
                match notifs.recv().await {
                    Ok(n) if n.method == "notifications/tools/list_changed" => {
                        if tx.send(()).is_err() {
                            break; // All receivers dropped.
                        }
                    }
                    Ok(_) => {} // Ignore other notifications.
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        warn!("mcp: notification receiver lagged by {n} messages");
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
        });

        rx
    }

    // ── Status ────────────────────────────────────────────────────────────────

    pub fn is_connected(&self) -> bool {
        self.transport.is_connected()
    }

    pub fn server_name(&self) -> &str {
        &self.server_name
    }

    pub fn is_initialized(&self) -> bool {
        self.initialized
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn extract_result<T: serde::de::DeserializeOwned>(
    resp: crate::transport::JsonRpcResponse,
    server_name: &str,
    method: &str,
) -> anyhow::Result<T> {
    if let Some(err) = resp.error {
        bail!(
            "mcp rpc error {} from '{}' ({}): {}",
            err.code,
            server_name,
            method,
            err.message
        );
    }
    let result = resp.result.ok_or_else(|| {
        anyhow::anyhow!(
            "mcp server '{}' returned no result for '{}'",
            server_name,
            method
        )
    })?;
    serde_json::from_value::<T>(result)
        .with_context(|| format!("failed to decode '{}' response from '{}'", method, server_name))
}

// ── Unit tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::{
        JsonRpcResponse, RequestId, TransportError, TransportResult,
    };
    use async_trait::async_trait;
    use std::sync::Mutex as StdMutex;
    use tokio::sync::broadcast;

    struct MockTransport {
        responses: StdMutex<std::collections::VecDeque<JsonRpcResponse>>,
        notification_tx: broadcast::Sender<JsonRpcNotification>,
    }

    impl MockTransport {
        fn new(responses: Vec<JsonRpcResponse>) -> Self {
            let (tx, _) = broadcast::channel(4);
            Self {
                responses: StdMutex::new(responses.into()),
                notification_tx: tx,
            }
        }
    }

    #[async_trait]
    impl McpTransport for MockTransport {
        async fn send_request(&self, _req: &JsonRpcRequest) -> TransportResult<JsonRpcResponse> {
            self.responses
                .lock()
                .unwrap()
                .pop_front()
                .ok_or(TransportError::ConnectionClosed)
        }
        fn notifications(&self) -> broadcast::Receiver<JsonRpcNotification> {
            self.notification_tx.subscribe()
        }
        async fn close(&self) -> TransportResult<()> { Ok(()) }
        fn is_connected(&self) -> bool { true }
    }

    fn make_init_response() -> JsonRpcResponse {
        JsonRpcResponse {
            jsonrpc: "2.0".into(),
            id: Some(RequestId::Number(1)),
            result: Some(serde_json::json!({
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "serverInfo": { "name": "test-server", "version": "1.0" }
            })),
            error: None,
        }
    }

    #[tokio::test]
    async fn test_initialize() {
        // initialize sends "initialize" + "notifications/initialized"
        let responses = vec![
            make_init_response(),
            // notifications/initialized response (may be ignored)
            JsonRpcResponse {
                jsonrpc: "2.0".into(),
                id: Some(RequestId::Number(2)),
                result: Some(serde_json::json!({})),
                error: None,
            },
        ];
        let transport = MockTransport::new(responses);
        let mut client = McpClient::new(Box::new(transport), "test-server");
        let result = client.initialize().await.unwrap();
        assert_eq!(result.protocol_version, "2024-11-05");
        assert!(client.is_initialized());
    }

    #[tokio::test]
    async fn test_list_tools() {
        let tool_resp = JsonRpcResponse {
            jsonrpc: "2.0".into(),
            id: Some(RequestId::Number(1)),
            result: Some(serde_json::json!({
                "tools": [
                    { "name": "my_tool", "description": "a tool", "inputSchema": {} }
                ]
            })),
            error: None,
        };
        let transport = MockTransport::new(vec![tool_resp]);
        let client = McpClient::new(Box::new(transport), "test-server");
        let tools = client.list_tools().await.unwrap();
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].name, "my_tool");
    }
}
