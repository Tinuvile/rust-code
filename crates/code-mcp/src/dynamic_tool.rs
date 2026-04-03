//! Convert MCP server tool definitions into `McpTool` instances.
//!
//! One `McpTool` is created per (server_name, tool_name) pair.  The tool's
//! `call_fn` captures an `Arc<Mutex<McpClient>>` and dispatches calls over
//! the live transport.
//!
//! Ref: src/tools/MCPTool/MCPTool.ts

use std::sync::Arc;

use code_tools::{mcp_tool::{McpCallFn, McpTool}, ToolRegistry};
use code_types::tool::{ToolResult, ToolResultPayload};
use serde_json::Value;
use tokio::sync::Mutex;

use crate::client::McpClient;
use crate::transport::McpToolDef;

// ── Build a single McpTool ────────────────────────────────────────────────────

/// Convert one [`McpToolDef`] from `server_name` into a [`McpTool`] with a
/// real [`McpCallFn`] wired to `client`.
pub fn build_mcp_tool(
    server_name: &str,
    def: &McpToolDef,
    client: Arc<Mutex<McpClient>>,
) -> McpTool {
    let tool_name = format!("mcp__{server_name}__{}", def.name);
    let description = def.description.clone().unwrap_or_default();
    let read_only = def
        .annotations
        .as_ref()
        .and_then(|a| a.read_only_hint)
        .unwrap_or(false);

    let mcp_name = def.name.clone();
    let client_ref = Arc::clone(&client);

    let call_fn: McpCallFn = Arc::new(move |tool_use_id: String, _tool_name: String, input: Value| {
        let client = Arc::clone(&client_ref);
        let mcp_name = mcp_name.clone();
        let tool_use_id = tool_use_id.clone();

        Box::pin(async move {
            let result = {
                let guard = client.lock().await;
                guard.call_tool(&mcp_name, input).await
            };

            match result {
                Ok(call_result) => {
                    let is_error = call_result.is_error.unwrap_or(false);
                    let text: String = call_result
                        .content
                        .iter()
                        .filter_map(|c| c.text.as_deref())
                        .collect::<Vec<_>>()
                        .join("\n");
                    ToolResult {
                        tool_use_id,
                        content: ToolResultPayload::Text(text),
                        is_error,
                        was_truncated: false,
                    }
                }
                Err(e) => ToolResult {
                    tool_use_id,
                    content: ToolResultPayload::Text(format!("MCP tool error: {e}")),
                    is_error: true,
                    was_truncated: false,
                },
            }
        })
    });

    McpTool {
        tool_name,
        tool_description: description,
        schema: def.input_schema.clone(),
        read_only,
        call_fn: Some(call_fn),
    }
}

// ── Bulk registration ─────────────────────────────────────────────────────────

/// Convert all tool definitions from a connected MCP server and register them
/// into `registry`.
///
/// Existing tools with names matching `mcp__{server_name}__*` are replaced.
pub fn register_server_tools(
    registry: &mut ToolRegistry,
    server_name: &str,
    tool_defs: &[McpToolDef],
    client: Arc<Mutex<McpClient>>,
) {
    for def in tool_defs {
        let tool = build_mcp_tool(server_name, def, Arc::clone(&client));
        registry.register(Box::new(tool));
    }
}

/// Remove all tools previously registered for `server_name` from `registry`.
///
/// Tool names follow the `mcp__{server_name}__*` convention.
pub fn deregister_server_tools(registry: &mut ToolRegistry, server_name: &str, tool_defs: &[McpToolDef]) {
    for def in tool_defs {
        let name = format!("mcp__{server_name}__{}", def.name);
        registry.remove(&name);
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::{
        JsonRpcNotification, JsonRpcRequest, JsonRpcResponse, McpTransport, RequestId,
        TransportError, TransportResult,
    };
    use async_trait::async_trait;
    use serde_json::json;
    use std::sync::Mutex as StdMutex;
    use tokio::sync::broadcast;

    struct EchoTransport {
        response: StdMutex<Option<JsonRpcResponse>>,
        notification_tx: broadcast::Sender<JsonRpcNotification>,
    }

    impl EchoTransport {
        fn returning(resp: JsonRpcResponse) -> Self {
            let (tx, _) = broadcast::channel(1);
            Self {
                response: StdMutex::new(Some(resp)),
                notification_tx: tx,
            }
        }
    }

    #[async_trait]
    impl McpTransport for EchoTransport {
        async fn send_request(&self, _req: &JsonRpcRequest) -> TransportResult<JsonRpcResponse> {
            self.response
                .lock()
                .unwrap()
                .take()
                .ok_or(TransportError::ConnectionClosed)
        }
        fn notifications(&self) -> broadcast::Receiver<JsonRpcNotification> {
            self.notification_tx.subscribe()
        }
        async fn close(&self) -> TransportResult<()> { Ok(()) }
        fn is_connected(&self) -> bool { true }
    }

    #[tokio::test]
    async fn test_build_and_call() {
        let call_resp = JsonRpcResponse {
            jsonrpc: "2.0".into(),
            id: Some(RequestId::Number(1)),
            result: Some(json!({
                "content": [{ "type": "text", "text": "hello from mcp" }],
                "isError": false
            })),
            error: None,
        };
        let transport = EchoTransport::returning(call_resp);
        let client = Arc::new(Mutex::new(McpClient::new(Box::new(transport), "test")));

        let def = McpToolDef {
            name: "greet".into(),
            description: Some("greets".into()),
            input_schema: json!({}),
            annotations: None,
        };

        let tool = build_mcp_tool("myserver", &def, client);
        assert_eq!(tool.tool_name, "mcp__myserver__greet");
        assert!(!tool.read_only);
        assert!(tool.call_fn.is_some());

        // Invoke the call_fn directly.
        let result = (tool.call_fn.as_ref().unwrap())(
            "tu-1".into(),
            "mcp__myserver__greet".into(),
            json!({}),
        )
        .await;

        assert!(!result.is_error);
        assert!(matches!(result.content, ToolResultPayload::Text(ref t) if t.contains("hello from mcp")));
    }
}
