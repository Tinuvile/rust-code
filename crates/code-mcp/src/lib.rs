//! MCP (Model Context Protocol) client implementation.
//!
//! Ref: src/services/mcp/client.ts, src/services/mcp/types.ts,
//!      src/services/mcp/MCPConnectionManager.tsx

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
