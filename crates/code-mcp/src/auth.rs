//! MCP OAuth token management.
//!
//! Stores access/refresh tokens for HTTP-based MCP servers, persisting them
//! to `~/.claude/mcp-tokens.json`.  Tokens are automatically refreshed when
//! they are within 5 minutes of expiry.
//!
//! Ref: src/services/mcp/auth.ts

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Context;
use chrono::Utc;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tracing::{debug, warn};

// ── McpToken ──────────────────────────────────────────────────────────────────

/// An OAuth 2.0 token for a single MCP server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpToken {
    pub access_token: String,
    pub refresh_token: Option<String>,
    /// Unix milliseconds at which the token expires.
    pub expires_at_ms: i64,
    /// Token endpoint URL used for refresh.
    pub token_url: String,
}

/// Buffer before nominal expiry at which we consider the token expired.
const EXPIRY_BUFFER_MS: i64 = 5 * 60 * 1_000; // 5 minutes

impl McpToken {
    pub fn is_expired(&self) -> bool {
        Utc::now().timestamp_millis() >= self.expires_at_ms - EXPIRY_BUFFER_MS
    }
}

// ── McpTokenStore ─────────────────────────────────────────────────────────────

/// In-memory + persisted token store keyed by MCP server name.
#[derive(Clone)]
pub struct McpTokenStore {
    tokens: Arc<RwLock<HashMap<String, McpToken>>>,
    path: PathBuf,
}

impl McpTokenStore {
    /// Load an existing token file or create an empty store.
    pub async fn load() -> Self {
        let path = token_file_path();
        let tokens = match tokio::fs::read_to_string(&path).await {
            Ok(content) => serde_json::from_str::<HashMap<String, McpToken>>(&content)
                .unwrap_or_default(),
            Err(_) => HashMap::new(),
        };
        Self {
            tokens: Arc::new(RwLock::new(tokens)),
            path,
        }
    }

    /// Retrieve the stored token for `server_name`, if any.
    pub async fn get(&self, server_name: &str) -> Option<McpToken> {
        self.tokens.read().await.get(server_name).cloned()
    }

    /// Store or replace a token for `server_name`.
    pub async fn set(&self, server_name: &str, token: McpToken) {
        self.tokens
            .write()
            .await
            .insert(server_name.to_owned(), token);
        self.persist().await;
    }

    /// Remove the token for `server_name`.
    pub async fn remove(&self, server_name: &str) {
        self.tokens.write().await.remove(server_name);
        self.persist().await;
    }

    /// Return the current access token for `server_name`, refreshing if expired.
    ///
    /// Returns `None` if no token is stored.
    pub async fn get_or_refresh(
        &self,
        server_name: &str,
        http_client: &Client,
    ) -> anyhow::Result<Option<String>> {
        let token = match self.get(server_name).await {
            Some(t) => t,
            None => return Ok(None),
        };

        if !token.is_expired() {
            return Ok(Some(token.access_token));
        }

        // Attempt refresh.
        let refresh_token = match &token.refresh_token {
            Some(rt) => rt.clone(),
            None => {
                warn!(server = server_name, "mcp token expired and no refresh_token available");
                return Ok(None);
            }
        };

        debug!(server = server_name, "mcp token expired, refreshing");

        let resp = http_client
            .post(&token.token_url)
            .form(&[
                ("grant_type", "refresh_token"),
                ("refresh_token", &refresh_token),
            ])
            .send()
            .await
            .context("token refresh request failed")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("token refresh failed ({status}): {body}");
        }

        let refresh_resp: TokenResponse = resp.json().await.context("failed to parse token response")?;

        let expires_at_ms = Utc::now().timestamp_millis()
            + refresh_resp.expires_in.unwrap_or(3600) as i64 * 1_000;

        let new_token = McpToken {
            access_token: refresh_resp.access_token.clone(),
            refresh_token: refresh_resp.refresh_token.or(token.refresh_token),
            expires_at_ms,
            token_url: token.token_url,
        };

        self.set(server_name, new_token).await;
        Ok(Some(refresh_resp.access_token))
    }

    // ── Private ───────────────────────────────────────────────────────────────

    async fn persist(&self) {
        let data = {
            let tokens = self.tokens.read().await;
            match serde_json::to_string_pretty(&*tokens) {
                Ok(s) => s,
                Err(e) => {
                    warn!("mcp: failed to serialize tokens: {e}");
                    return;
                }
            }
        };

        // Atomic write: write to a temp file then rename.
        let tmp = self.path.with_extension("tmp");
        if let Err(e) = tokio::fs::write(&tmp, &data).await {
            warn!("mcp: failed to write token file: {e}");
            return;
        }
        if let Err(e) = tokio::fs::rename(&tmp, &self.path).await {
            warn!("mcp: failed to rename token file: {e}");
        }
    }
}

// ── Token endpoint response ───────────────────────────────────────────────────

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    refresh_token: Option<String>,
    expires_in: Option<u64>,
}

// ── Path helper ───────────────────────────────────────────────────────────────

fn token_file_path() -> PathBuf {
    dirs_next::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".claude")
        .join("mcp-tokens.json")
}
