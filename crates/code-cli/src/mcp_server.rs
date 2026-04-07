//! MCP server mode — expose tools over stdio via JSON-RPC 2.0.
//!
//! When invoked as `code mcp --serve`, the binary starts a server that
//! speaks the MCP (Model Context Protocol) over line-delimited JSON-RPC on
//! stdin/stdout.  Any MCP client (e.g. Claude Desktop, VS Code extension,
//! another Claude Code instance) can connect and use the full tool suite.
//!
//! Supported methods:
//! - `initialize`   — handshake, returns server capabilities
//! - `ping`         — keepalive
//! - `tools/list`   — enumerate available tools
//! - `tools/call`   — execute a tool and return its result
//!
//! Notifications handled:
//! - `initialized`             — post-handshake ack (no-op)
//! - `notifications/cancelled` — client cancels a pending request
//!
//! Ref: src/entrypoints/mcp.ts

use std::path::PathBuf;

use anyhow::Result;
use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tracing::{debug, info, warn};

use code_mcp::transport::{
    ERR_INVALID_PARAMS, ERR_METHOD_NOT_FOUND, MCP_PROTOCOL_VERSION,
};
use code_tools::registry::ToolRegistry;
use code_tools::ToolContext;
use code_types::permissions::{PermissionMode, ToolPermissionContext};
use code_types::tool::ToolResultPayload;

// ── Constants ────────────────────────────────────────────────────────────────

const SERVER_NAME: &str = "rust-code";
const SERVER_VERSION: &str = env!("CARGO_PKG_VERSION");

// ── Public entry point ───────────────────────────────────────────────────────

/// Serve as an MCP server via stdio.
///
/// This function blocks until stdin is closed (i.e. the client disconnects).
pub async fn serve() -> Result<()> {
    info!("MCP server starting on stdio (protocol {MCP_PROTOCOL_VERSION})");

    let cwd = std::env::current_dir()?;
    let registry = ToolRegistry::with_default_tools(&cwd);

    // Set up a ToolContext for tool execution.
    // In MCP server mode we use BypassPermissions because the MCP client is
    // responsible for gating access (the user already opted in by connecting).
    let session_id = uuid::Uuid::new_v4().to_string();
    let session_dir = dirs_next::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".claude")
        .join("sessions")
        .join(&session_id);
    tokio::fs::create_dir_all(&session_dir).await?;

    let mut permission_ctx = ToolPermissionContext::default();
    permission_ctx.mode = PermissionMode::BypassPermissions;

    let ctx = ToolContext {
        cwd,
        session_id,
        session_dir,
        permission_ctx,
        file_reading_limits: Default::default(),
        glob_limits: Default::default(),
    };

    // Stdio loop — read line-delimited JSON-RPC from stdin, write to stdout.
    let stdin = tokio::io::stdin();
    let mut stdout = tokio::io::stdout();
    let mut reader = BufReader::new(stdin);
    let mut line = String::new();

    loop {
        line.clear();
        match reader.read_line(&mut line).await {
            Ok(0) => {
                // EOF — client disconnected.
                debug!("MCP server: stdin closed (EOF)");
                break;
            }
            Ok(_) => {
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }

                // Parse the incoming JSON-RPC message.
                let msg: Value = match serde_json::from_str(trimmed) {
                    Ok(v) => v,
                    Err(e) => {
                        warn!("MCP server: failed to parse: {e}");
                        let err_resp = json!({
                            "jsonrpc": "2.0",
                            "id": null,
                            "error": {
                                "code": -32700,
                                "message": format!("Parse error: {e}")
                            }
                        });
                        write_json(&mut stdout, &err_resp).await?;
                        continue;
                    }
                };

                if msg.get("id").is_some() {
                    // Request — needs a response.
                    let id = msg["id"].clone();
                    let method = msg
                        .get("method")
                        .and_then(|m| m.as_str())
                        .unwrap_or("");
                    let params = msg.get("params").cloned();

                    let response =
                        dispatch_request(method, params, &registry, &ctx).await;

                    let resp_json = match response {
                        Ok(result) => json!({
                            "jsonrpc": "2.0",
                            "id": id,
                            "result": result
                        }),
                        Err(err) => json!({
                            "jsonrpc": "2.0",
                            "id": id,
                            "error": err
                        }),
                    };

                    write_json(&mut stdout, &resp_json).await?;
                } else {
                    // Notification — no response.
                    let method = msg
                        .get("method")
                        .and_then(|m| m.as_str())
                        .unwrap_or("");
                    handle_notification(method);
                }
            }
            Err(e) => {
                warn!("MCP server: read error: {e}");
                break;
            }
        }
    }

    info!("MCP server shutting down");
    Ok(())
}

// ── Request dispatch ─────────────────────────────────────────────────────────

async fn dispatch_request(
    method: &str,
    params: Option<Value>,
    registry: &ToolRegistry,
    ctx: &ToolContext,
) -> Result<Value, Value> {
    match method {
        "initialize" => handle_initialize(params),
        "ping" => Ok(json!({})),
        "tools/list" => handle_tools_list(registry),
        "tools/call" => handle_tools_call(params, registry, ctx).await,
        _ => Err(json!({
            "code": ERR_METHOD_NOT_FOUND,
            "message": format!("Method not found: {method}")
        })),
    }
}

// ── Method handlers ──────────────────────────────────────────────────────────

/// `initialize` — return protocol version, capabilities, and server info.
fn handle_initialize(_params: Option<Value>) -> Result<Value, Value> {
    Ok(json!({
        "protocolVersion": MCP_PROTOCOL_VERSION,
        "capabilities": {
            "tools": {
                "listChanged": false
            }
        },
        "serverInfo": {
            "name": SERVER_NAME,
            "version": SERVER_VERSION
        }
    }))
}

/// `tools/list` — enumerate all enabled tools with their JSON Schema.
fn handle_tools_list(registry: &ToolRegistry) -> Result<Value, Value> {
    let mut tools: Vec<Value> = registry
        .all()
        .filter(|t| t.is_enabled())
        .map(|t| {
            json!({
                "name": t.name(),
                "description": t.description(),
                "inputSchema": t.input_schema(),
            })
        })
        .collect();

    // Deterministic ordering by name.
    tools.sort_by(|a, b| {
        a["name"]
            .as_str()
            .unwrap_or("")
            .cmp(b["name"].as_str().unwrap_or(""))
    });

    Ok(json!({ "tools": tools }))
}

/// `tools/call` — look up a tool by name, execute it, return the result.
async fn handle_tools_call(
    params: Option<Value>,
    registry: &ToolRegistry,
    ctx: &ToolContext,
) -> Result<Value, Value> {
    let params = params.ok_or_else(|| {
        json!({
            "code": ERR_INVALID_PARAMS,
            "message": "Missing params for tools/call"
        })
    })?;

    let name = params
        .get("name")
        .and_then(|n| n.as_str())
        .ok_or_else(|| {
            json!({
                "code": ERR_INVALID_PARAMS,
                "message": "Missing 'name' in tools/call params"
            })
        })?;

    let arguments = params
        .get("arguments")
        .cloned()
        .unwrap_or_else(|| json!({}));

    let tool = registry.get(name).ok_or_else(|| {
        json!({
            "code": ERR_INVALID_PARAMS,
            "message": format!("Unknown tool: {name}")
        })
    })?;

    if !tool.is_enabled() {
        return Err(json!({
            "code": ERR_INVALID_PARAMS,
            "message": format!("Tool '{name}' is not available in the current environment")
        }));
    }

    // Execute the tool.  We generate a synthetic tool_use_id for the call.
    let tool_use_id = format!("mcp_server_{}", uuid::Uuid::new_v4());
    let result = tool.call(&tool_use_id, arguments, ctx, None).await;

    // Convert ToolResult → MCP content blocks.
    let text = match &result.content {
        ToolResultPayload::Text(s) => s.clone(),
        ToolResultPayload::Json(v) => serde_json::to_string_pretty(v).unwrap_or_default(),
    };

    Ok(json!({
        "content": [{
            "type": "text",
            "text": text
        }],
        "isError": result.is_error,
    }))
}

// ── Notification handling ────────────────────────────────────────────────────

fn handle_notification(method: &str) {
    match method {
        "initialized" => {
            debug!("MCP server: client completed initialization handshake");
        }
        "notifications/cancelled" => {
            debug!("MCP server: client cancelled a pending request");
        }
        other => {
            debug!("MCP server: unhandled notification '{other}'");
        }
    }
}

// ── I/O helper ───────────────────────────────────────────────────────────────

/// Serialize a JSON value as a single line and write to stdout, followed by a
/// newline and flush.
async fn write_json(
    stdout: &mut tokio::io::Stdout,
    value: &Value,
) -> Result<()> {
    let mut line = serde_json::to_string(value)?;
    line.push('\n');
    stdout.write_all(line.as_bytes()).await?;
    stdout.flush().await?;
    Ok(())
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initialize_returns_protocol_version() {
        let result = handle_initialize(Some(json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": { "name": "test", "version": "0.1" }
        })));
        let val = result.unwrap();
        assert_eq!(val["protocolVersion"], MCP_PROTOCOL_VERSION);
        assert_eq!(val["serverInfo"]["name"], SERVER_NAME);
        assert!(val["capabilities"]["tools"].is_object());
    }

    #[test]
    fn tools_list_contains_core_tools() {
        let registry = ToolRegistry::with_default_tools(std::path::Path::new("."));
        let result = handle_tools_list(&registry).unwrap();
        let tools = result["tools"].as_array().expect("tools should be array");
        assert!(!tools.is_empty(), "tools list should not be empty");

        // All tools should have name, description, and inputSchema.
        for tool in tools {
            assert!(tool["name"].is_string(), "tool missing 'name'");
            assert!(tool["description"].is_string(), "tool missing 'description'");
            assert!(tool["inputSchema"].is_object(), "tool missing 'inputSchema'");
        }

        // Core Tier 1 tools should be present.
        let names: Vec<&str> = tools
            .iter()
            .filter_map(|t| t["name"].as_str())
            .collect();
        for expected in &["Bash", "Read", "Write", "Edit", "Grep", "Glob"] {
            assert!(
                names.contains(expected),
                "missing core tool: {expected}"
            );
        }
    }

    #[tokio::test]
    async fn tools_call_missing_name_returns_error() {
        let registry = ToolRegistry::with_default_tools(std::path::Path::new("."));
        let ctx = ToolContext {
            cwd: std::env::current_dir().unwrap(),
            session_id: "test".to_owned(),
            session_dir: std::env::temp_dir(),
            permission_ctx: ToolPermissionContext::default(),
            file_reading_limits: Default::default(),
            glob_limits: Default::default(),
        };

        let result = handle_tools_call(Some(json!({})), &registry, &ctx).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err["code"], ERR_INVALID_PARAMS);
    }

    #[tokio::test]
    async fn tools_call_unknown_tool_returns_error() {
        let registry = ToolRegistry::with_default_tools(std::path::Path::new("."));
        let ctx = ToolContext {
            cwd: std::env::current_dir().unwrap(),
            session_id: "test".to_owned(),
            session_dir: std::env::temp_dir(),
            permission_ctx: ToolPermissionContext::default(),
            file_reading_limits: Default::default(),
            glob_limits: Default::default(),
        };

        let result = handle_tools_call(
            Some(json!({ "name": "NonExistentTool" })),
            &registry,
            &ctx,
        )
        .await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err["message"].as_str().unwrap().contains("Unknown tool"));
    }
}
