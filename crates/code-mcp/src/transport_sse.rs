//! SSE (Server-Sent Events) MCP transport.
//!
//! Requests are sent as HTTP POST (same as `HttpTransport`).
//! A persistent background GET connection receives server-initiated
//! notifications and optionally JSON-RPC responses via the SSE stream.
//!
//! Ref: src/services/mcp/client.ts (SSE transport section)

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use bytes::Bytes;
use futures_util::StreamExt;
use reqwest::{header, Client};
use serde_json::Value;
use tokio::sync::{broadcast, oneshot, Mutex};
use tracing::{debug, trace, warn};

use crate::transport::{
    JsonRpcNotification, JsonRpcRequest, JsonRpcResponse, McpTransport, RequestId, TransportError,
    TransportResult,
};

// ── SseTransport ──────────────────────────────────────────────────────────────

/// MCP transport using HTTP POST for requests and SSE for server notifications.
///
/// Some servers also route JSON-RPC *responses* over SSE; this implementation
/// handles both: it tries the HTTP response body first, then falls back to
/// waiting for the response on the SSE stream.
pub struct SseTransport {
    client: Client,
    endpoint: String,
    /// Pending requests awaiting a response on the SSE stream.
    pending: Arc<Mutex<HashMap<u64, oneshot::Sender<JsonRpcResponse>>>>,
    notification_tx: broadcast::Sender<JsonRpcNotification>,
    connected: Arc<AtomicBool>,
    next_id: Arc<AtomicU64>,
}

impl SseTransport {
    /// Create an `SseTransport` and start the SSE background listener.
    ///
    /// - `endpoint` — URL for JSON-RPC POST requests.
    /// - `sse_url`  — URL for the SSE notification stream.
    /// - `token`    — optional Bearer token.
    /// - `extra_headers` — additional headers from server config.
    pub fn new(
        endpoint: impl Into<String>,
        sse_url: impl Into<String>,
        token: Option<String>,
        extra_headers: Option<&HashMap<String, String>>,
    ) -> TransportResult<Self> {
        let mut default_headers = header::HeaderMap::new();
        default_headers.insert(
            header::CONTENT_TYPE,
            header::HeaderValue::from_static("application/json"),
        );

        if let Some(t) = &token {
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

        let pending: Arc<Mutex<HashMap<u64, oneshot::Sender<JsonRpcResponse>>>> =
            Arc::new(Mutex::new(HashMap::new()));
        let (notification_tx, _) = broadcast::channel(64);
        let connected = Arc::new(AtomicBool::new(true));

        tokio::spawn(sse_listener(
            client.clone(),
            sse_url.into(),
            token,
            Arc::clone(&pending),
            notification_tx.clone(),
            Arc::clone(&connected),
        ));

        Ok(Self {
            client,
            endpoint: endpoint.into(),
            pending,
            notification_tx,
            connected,
            next_id: Arc::new(AtomicU64::new(1)),
        })
    }

    pub fn next_id(&self) -> u64 {
        self.next_id.fetch_add(1, Ordering::Relaxed)
    }
}

#[async_trait]
impl McpTransport for SseTransport {
    async fn send_request(&self, req: &JsonRpcRequest) -> TransportResult<JsonRpcResponse> {
        if !self.connected.load(Ordering::Relaxed) {
            return Err(TransportError::ConnectionClosed);
        }

        let id = match &req.id {
            RequestId::Number(n) => *n,
            RequestId::Str(_) => {
                return Err(TransportError::Protocol(
                    "string request ids not supported for SSE transport".into(),
                ))
            }
        };

        // Pre-register a oneshot in case the server routes the response via SSE.
        let (tx, rx) = oneshot::channel::<JsonRpcResponse>();
        self.pending.lock().await.insert(id, tx);

        trace!(method = %req.method, "→ mcp sse request");

        let http_resp = self
            .client
            .post(&self.endpoint)
            .json(req)
            .send()
            .await
            .map_err(TransportError::Http)?;

        if !http_resp.status().is_success() {
            self.pending.lock().await.remove(&id);
            let status = http_resp.status();
            let body = http_resp.text().await.unwrap_or_default();
            return Err(TransportError::Protocol(format!("http {status}: {body}")));
        }

        // Try to decode a real JSON-RPC response from the HTTP body.
        let body_bytes = http_resp.bytes().await.map_err(TransportError::Http)?;
        if !body_bytes.is_empty() {
            if let Ok(rpc_resp) = serde_json::from_slice::<JsonRpcResponse>(&body_bytes) {
                if rpc_resp.result.is_some() || rpc_resp.error.is_some() {
                    self.pending.lock().await.remove(&id);
                    trace!("← mcp sse response (body)");
                    return Ok(rpc_resp);
                }
            }
        }

        // Response not in body — await it on the SSE stream (30 s timeout).
        match tokio::time::timeout(std::time::Duration::from_secs(30), rx).await {
            Ok(Ok(resp)) => {
                trace!("← mcp sse response (stream)");
                Ok(resp)
            }
            Ok(Err(_)) => Err(TransportError::ConnectionClosed),
            Err(_) => {
                self.pending.lock().await.remove(&id);
                Err(TransportError::Timeout)
            }
        }
    }

    fn notifications(&self) -> broadcast::Receiver<JsonRpcNotification> {
        self.notification_tx.subscribe()
    }

    async fn close(&self) -> TransportResult<()> {
        self.connected.store(false, Ordering::Relaxed);
        Ok(())
    }

    fn is_connected(&self) -> bool {
        self.connected.load(Ordering::Relaxed)
    }
}

// ── SSE background listener ───────────────────────────────────────────────────

async fn sse_listener(
    client: Client,
    sse_url: String,
    token: Option<String>,
    pending: Arc<Mutex<HashMap<u64, oneshot::Sender<JsonRpcResponse>>>>,
    notification_tx: broadcast::Sender<JsonRpcNotification>,
    connected: Arc<AtomicBool>,
) {
    let mut req = client
        .get(&sse_url)
        .header(header::ACCEPT, "text/event-stream");
    if let Some(t) = token {
        req = req.header(header::AUTHORIZATION, format!("Bearer {t}"));
    }

    let response = match req.send().await {
        Ok(r) => r,
        Err(e) => {
            warn!("mcp sse: failed to connect to SSE stream at {sse_url}: {e}");
            connected.store(false, Ordering::Relaxed);
            return;
        }
    };

    if !response.status().is_success() {
        warn!("mcp sse: SSE stream returned status {}", response.status());
        connected.store(false, Ordering::Relaxed);
        return;
    }

    debug!("mcp sse: SSE stream connected to {sse_url}");

    let mut stream = response.bytes_stream();
    let mut line_buf = String::new();
    let mut data_buf = String::new();

    while let Some(chunk_result) = stream.next().await {
        let chunk: Bytes = match chunk_result {
            Ok(c) => c,
            Err(e) => {
                warn!("mcp sse: stream error: {e}");
                break;
            }
        };

        let text = match std::str::from_utf8(&chunk) {
            Ok(s) => s,
            Err(e) => {
                warn!("mcp sse: non-utf8 chunk: {e}");
                continue;
            }
        };

        // Process the chunk character by character, assembling lines.
        for ch in text.chars() {
            if ch == '\n' {
                let line = line_buf.trim_end_matches('\r').to_owned();
                line_buf.clear();

                if line.starts_with("data:") {
                    let data = line["data:".len()..].trim_start();
                    data_buf.push_str(data);
                } else if line.is_empty() && !data_buf.is_empty() {
                    // Blank line = end of SSE event.
                    let event_data = std::mem::take(&mut data_buf);
                    process_sse_event(&event_data, &pending, &notification_tx).await;
                }
                // Ignore event:, id:, retry: lines.
            } else {
                line_buf.push(ch);
            }
        }
    }

    debug!("mcp sse: SSE stream ended");
    connected.store(false, Ordering::Relaxed);
    pending.lock().await.clear();
}

async fn process_sse_event(
    data: &str,
    pending: &Mutex<HashMap<u64, oneshot::Sender<JsonRpcResponse>>>,
    notification_tx: &broadcast::Sender<JsonRpcNotification>,
) {
    let v: Value = match serde_json::from_str(data) {
        Ok(v) => v,
        Err(e) => {
            warn!("mcp sse: failed to parse SSE data: {e}");
            return;
        }
    };

    if v.get("id").is_some() {
        if let Ok(resp) = serde_json::from_value::<JsonRpcResponse>(v) {
            let id = match &resp.id {
                Some(RequestId::Number(n)) => Some(*n),
                _ => None,
            };
            if let Some(id) = id {
                let sender = pending.lock().await.remove(&id);
                if let Some(tx) = sender {
                    let _ = tx.send(resp);
                }
            }
        }
    } else if let Ok(notif) = serde_json::from_value::<JsonRpcNotification>(v) {
        trace!(method = %notif.method, "← mcp sse notification");
        let _ = notification_tx.send(notif);
    }
}
