//! Stdio MCP transport — spawn a child process and communicate via stdin/stdout.
//!
//! The child process is expected to speak line-delimited JSON-RPC 2.0.
//! Each JSON message is terminated by a newline (`\n`).
//!
//! Ref: src/services/mcp/client.ts (stdio transport section)

use std::collections::HashMap;
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tokio::sync::{broadcast, oneshot, Mutex};
use tracing::{debug, trace, warn};

use crate::transport::{
    JsonRpcNotification, JsonRpcRequest, JsonRpcResponse, McpTransport, RequestId, TransportError,
    TransportResult,
};

// ── StdioTransport ────────────────────────────────────────────────────────────

/// MCP transport that communicates over a child process's stdin/stdout.
pub struct StdioTransport {
    stdin: Arc<Mutex<ChildStdin>>,
    pending: Arc<Mutex<HashMap<u64, oneshot::Sender<JsonRpcResponse>>>>,
    notification_tx: broadcast::Sender<JsonRpcNotification>,
    connected: Arc<AtomicBool>,
    /// Held so the child process stays alive for the transport's lifetime.
    _child: Arc<Mutex<Child>>,
}

impl StdioTransport {
    /// Spawn `command` with `args` and optional `env` / `cwd`, and set up the
    /// JSON-RPC reader loop on the child's stdout.
    pub async fn spawn(
        command: &str,
        args: &[String],
        env: Option<&HashMap<String, String>>,
        cwd: Option<&str>,
    ) -> TransportResult<Self> {
        let mut cmd = Command::new(command);
        cmd.args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .kill_on_drop(true);

        if let Some(env_map) = env {
            for (k, v) in env_map {
                cmd.env(k, v);
            }
        }
        if let Some(dir) = cwd {
            cmd.current_dir(dir);
        }

        let mut child = cmd.spawn()?;
        let stdin = child.stdin.take().ok_or_else(|| {
            TransportError::Io(std::io::Error::other("child stdin not available"))
        })?;
        let stdout = child.stdout.take().ok_or_else(|| {
            TransportError::Io(std::io::Error::other("child stdout not available"))
        })?;

        let pending: Arc<Mutex<HashMap<u64, oneshot::Sender<JsonRpcResponse>>>> =
            Arc::new(Mutex::new(HashMap::new()));
        let (notification_tx, _) = broadcast::channel(64);
        let connected = Arc::new(AtomicBool::new(true));

        // Spawn the background reader task.
        tokio::spawn(reader_loop(
            stdout,
            Arc::clone(&pending),
            notification_tx.clone(),
            Arc::clone(&connected),
        ));

        Ok(Self {
            stdin: Arc::new(Mutex::new(stdin)),
            pending,
            notification_tx,
            connected,
            _child: Arc::new(Mutex::new(child)),
        })
    }
}

#[async_trait]
impl McpTransport for StdioTransport {
    async fn send_request(&self, req: &JsonRpcRequest) -> TransportResult<JsonRpcResponse> {
        if !self.connected.load(Ordering::Relaxed) {
            return Err(TransportError::ConnectionClosed);
        }

        // Extract the numeric ID (all requests we create use Number variant).
        let id = match &req.id {
            RequestId::Number(n) => *n,
            RequestId::Str(_) => {
                return Err(TransportError::Protocol(
                    "string request ids not supported for stdio transport".into(),
                ))
            }
        };

        let (tx, rx) = oneshot::channel::<JsonRpcResponse>();
        self.pending.lock().await.insert(id, tx);

        // Serialize and write as a single line.
        let mut line = serde_json::to_string(req)?;
        line.push('\n');
        {
            let mut stdin = self.stdin.lock().await;
            stdin.write_all(line.as_bytes()).await?;
            stdin.flush().await?;
        }
        trace!(id, method = %req.method, "→ mcp request sent");

        // Wait for the response (30 s timeout).
        match tokio::time::timeout(std::time::Duration::from_secs(30), rx).await {
            Ok(Ok(resp)) => {
                trace!(id, "← mcp response received");
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

// ── Background reader loop ────────────────────────────────────────────────────

async fn reader_loop(
    stdout: ChildStdout,
    pending: Arc<Mutex<HashMap<u64, oneshot::Sender<JsonRpcResponse>>>>,
    notification_tx: broadcast::Sender<JsonRpcNotification>,
    connected: Arc<AtomicBool>,
) {
    let mut reader = BufReader::new(stdout);
    let mut line = String::new();

    loop {
        line.clear();
        match reader.read_line(&mut line).await {
            Ok(0) => {
                // EOF — child process exited.
                debug!("mcp stdio: child stdout closed (EOF)");
                break;
            }
            Ok(_) => {
                let trimmed = line.trim_end();
                if trimmed.is_empty() {
                    continue;
                }
                route_message(trimmed, &pending, &notification_tx).await;
            }
            Err(e) => {
                warn!("mcp stdio: read error: {e}");
                break;
            }
        }
    }

    connected.store(false, Ordering::Relaxed);

    // Drop all pending senders so their receivers get an error.
    let mut map = pending.lock().await;
    map.clear();
}

/// Deserialize a raw JSON line and route it to the appropriate channel.
async fn route_message(
    raw: &str,
    pending: &Mutex<HashMap<u64, oneshot::Sender<JsonRpcResponse>>>,
    notification_tx: &broadcast::Sender<JsonRpcNotification>,
) {
    let v: Value = match serde_json::from_str(raw) {
        Ok(v) => v,
        Err(e) => {
            warn!("mcp stdio: failed to parse message: {e}\nraw: {raw}");
            return;
        }
    };

    if v.get("id").is_some() {
        // It's a response (has an "id" field).
        match serde_json::from_value::<JsonRpcResponse>(v) {
            Ok(resp) => {
                if let Some(id) = numeric_id(&resp.id) {
                    // Extract sender before dropping the lock, then send.
                    let sender = pending.lock().await.remove(&id);
                    if let Some(tx) = sender {
                        let _ = tx.send(resp);
                    } else {
                        warn!("mcp stdio: received response for unknown id {id}");
                    }
                }
            }
            Err(e) => warn!("mcp stdio: failed to parse response: {e}"),
        }
    } else {
        // It's a notification (no "id" field).
        match serde_json::from_value::<JsonRpcNotification>(v) {
            Ok(notif) => {
                trace!(method = %notif.method, "← mcp notification");
                let _ = notification_tx.send(notif);
            }
            Err(e) => warn!("mcp stdio: failed to parse notification: {e}"),
        }
    }
}

fn numeric_id(id: &Option<RequestId>) -> Option<u64> {
    match id {
        Some(RequestId::Number(n)) => Some(*n),
        _ => None,
    }
}

// ── Unit tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    #[cfg(unix)]
    use serde_json::json;

    /// Spawn a simple shell echo server that reads one line from stdin and
    /// echoes back a matching response. Only runs on Unix.
    #[cfg(unix)]
    #[tokio::test]
    async fn test_roundtrip_unix() {
        let script = r#"read line; echo '{"jsonrpc":"2.0","id":1,"result":{"ok":true}}'"#;
        let transport = StdioTransport::spawn(
            "/bin/sh",
            &["-c".to_string(), script.to_string()],
            None,
            None,
        )
        .await
        .expect("spawn failed");

        let req = JsonRpcRequest::new(1, "test/ping", Some(json!({})));
        let resp = transport.send_request(&req).await.expect("send failed");
        assert!(resp.error.is_none());
        assert_eq!(resp.result.unwrap()["ok"], true);
    }
}
