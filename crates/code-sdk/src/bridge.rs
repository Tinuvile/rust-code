//! IDE bridge: WebSocket server for Direct Connect (IDE extension integration).
//!
//! Only compiled when the `bridge_mode` feature is enabled.
//!
//! Ref: src/bridge/

#[cfg(feature = "bridge_mode")]
pub use inner::*;

#[cfg(feature = "bridge_mode")]
mod inner {
    use anyhow::Result;
    use serde::{Deserialize, Serialize};

    // ── Bridge config ─────────────────────────────────────────────────────────

    /// Configuration for the IDE bridge server.
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct BridgeConfig {
        /// WebSocket listen address (e.g. `"127.0.0.1:12345"`).
        pub address: String,
        /// Optional JWT secret for authenticating IDE connections.
        pub jwt_secret: Option<String>,
    }

    impl Default for BridgeConfig {
        fn default() -> Self {
            Self {
                address: "127.0.0.1:0".into(),
                jwt_secret: None,
            }
        }
    }

    // ── Bridge messages ───────────────────────────────────────────────────────

    /// A message sent from the IDE to the bridge.
    #[derive(Debug, Clone, Serialize, Deserialize)]
    #[serde(tag = "type", rename_all = "snake_case")]
    pub enum BridgeRequest {
        /// Send a user message to the active session.
        UserMessage { content: String },
        /// Interrupt the current query.
        Interrupt,
        /// Request a context snapshot.
        GetContext,
    }

    /// A message sent from the bridge to the IDE.
    #[derive(Debug, Clone, Serialize, Deserialize)]
    #[serde(tag = "type", rename_all = "snake_case")]
    pub enum BridgeResponse {
        /// An SDK message forwarded from the engine.
        SdkMessage(crate::message::SdkMessage),
        /// Acknowledgement of a `GetContext` request.
        Context { session_id: String, cwd: String },
        /// Error notification.
        Error { message: String },
    }

    // ── BridgeServer ─────────────────────────────────────────────────────────

    /// A stub IDE bridge server.
    ///
    /// A full implementation would use `tokio-tungstenite` for WebSocket I/O and
    /// JWT authentication.  This stub tracks configuration and exposes the
    /// address-binding logic.
    pub struct BridgeServer {
        config: BridgeConfig,
    }

    impl BridgeServer {
        pub fn new(config: BridgeConfig) -> Self {
            Self { config }
        }

        /// Start listening.  Returns the bound address.
        ///
        /// In a full implementation this binds a TCP listener and starts accepting
        /// WebSocket upgrades with tokio-tungstenite.
        pub async fn start(&self) -> Result<String> {
            // Stub: return the configured address.
            tracing::info!(address = %self.config.address, "IDE bridge server starting (stub)");
            Ok(self.config.address.clone())
        }

        pub fn config(&self) -> &BridgeConfig {
            &self.config
        }
    }
}
