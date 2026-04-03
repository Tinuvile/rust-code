//! Plain HTTP MCP transport — each JSON-RPC request is an HTTP POST.
//!
//! This transport has no persistent connection and no push channel for
//! notifications.  It is the simplest transport and works with any HTTP/HTTPS
//! MCP server that responds synchronously.
//!
//! Ref: src/services/mcp/client.ts (HTTP transport section)

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use reqwest::{header, Client};
use tokio::sync::broadcast;
use tracing::trace;

use crate::transport::{
    JsonRpcNotification, JsonRpcRequest, JsonRpcResponse, McpTransport, TransportError,
    TransportResult,
};

// ── HttpTransport ─────────────────────────────────────────────────────────────

/// Stateless HTTP MCP transport.
///
/// Each `send_request` call is a single HTTP POST; responses come in the body.
/// No background task or persistent connection is maintained.
pub struct HttpTransport {
    client: Client,
    endpoint: String,
    /// Empty broadcast channel — HTTP has no server-push channel.
    notification_tx: broadcast::Sender<JsonRpcNotification>,
    next_id: Arc<AtomicU64>,
}

impl HttpTransport {
    /// Construct a new `HttpTransport`.
    ///
    /// `endpoint` should be the full URL of the JSON-RPC endpoint.
    /// `token` is an optional Bearer token added to every request.
    /// `extra_headers` are additional HTTP headers from the server config.
    pub fn new(
        endpoint: impl Into<String>,
        token: Option<String>,
        extra_headers: Option<&HashMap<String, String>>,
    ) -> TransportResult<Self> {
        let mut default_headers = header::HeaderMap::new();
        default_headers.insert(
            header::CONTENT_TYPE,
            header::HeaderValue::from_static("application/json"),
        );
        default_headers.insert(
            header::ACCEPT,
            header::HeaderValue::from_static("application/json"),
        );

        if let Some(t) = token {
            let value = header::HeaderValue::from_str(&format!("Bearer {t}"))
                .map_err(|e| TransportError::Protocol(format!("invalid token: {e}")))?;
            default_headers.insert(header::AUTHORIZATION, value);
        }

        if let Some(headers) = extra_headers {
            for (name, value) in headers {
                let hname = header::HeaderName::from_bytes(name.as_bytes())
                    .map_err(|e| TransportError::Protocol(format!("invalid header name: {e}")))?;
                let hval = header::HeaderValue::from_str(value)
                    .map_err(|e| TransportError::Protocol(format!("invalid header value: {e}")))?;
                default_headers.insert(hname, hval);
            }
        }

        let client = Client::builder()
            .default_headers(default_headers)
            .build()
            .map_err(TransportError::Http)?;

        let (notification_tx, _) = broadcast::channel(1);
        Ok(Self {
            client,
            endpoint: endpoint.into(),
            notification_tx,
            next_id: Arc::new(AtomicU64::new(1)),
        })
    }

    /// Allocate the next monotonically-increasing request ID.
    pub fn next_id(&self) -> u64 {
        self.next_id.fetch_add(1, Ordering::Relaxed)
    }
}

#[async_trait]
impl McpTransport for HttpTransport {
    async fn send_request(&self, req: &JsonRpcRequest) -> TransportResult<JsonRpcResponse> {
        trace!(method = %req.method, "→ mcp http request");

        let resp = self
            .client
            .post(&self.endpoint)
            .json(req)
            .send()
            .await
            .map_err(TransportError::Http)?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(TransportError::Protocol(format!(
                "http {status}: {body}"
            )));
        }

        let rpc_resp: JsonRpcResponse = resp.json().await.map_err(TransportError::Http)?;
        trace!("← mcp http response received");
        Ok(rpc_resp)
    }

    fn notifications(&self) -> broadcast::Receiver<JsonRpcNotification> {
        self.notification_tx.subscribe()
    }

    async fn close(&self) -> TransportResult<()> {
        // Stateless — nothing to tear down.
        Ok(())
    }

    fn is_connected(&self) -> bool {
        // Optimistic: we have no persistent connection to check.
        true
    }
}
