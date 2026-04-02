//! Unified authentication provider.
//!
//! `AuthProvider` unifies all authentication backends (API key, OAuth, Bedrock,
//! Vertex, Azure) behind a single interface. The `detect()` function probes
//! the environment to pick the most appropriate provider automatically.
//!
//! Ref: src/utils/auth.ts (getAuthTokenSource, getAnthropicApiKey)

use reqwest::header::{HeaderName, HeaderValue};

use code_config::{global::GlobalConfig, settings::SettingsJson};

use crate::api_key::{get_api_key, ApiKeyWithSource};
use crate::oauth::{refresh_oauth_token, OAuthTokens, CLAUDE_AI_OAUTH};
use crate::secure_storage::SecureStorage;

// ── AuthProvider ──────────────────────────────────────────────────────────────

/// All supported authentication backends.
///
/// Ref: src/utils/auth.ts (auth source types)
#[derive(Debug, Clone)]
pub enum AuthProvider {
    /// Direct Anthropic API key (env var, helper command, or keychain).
    ApiKey(ApiKeyWithSource),

    /// Claude.ai OAuth (Bearer access token).
    ClaudeAiOAuth(OAuthTokens),

    /// AWS Bedrock — credentials are provided via the standard AWS credential
    /// chain; `credential_refresh_cmd` is the user-configured refresh script.
    AwsBedrock {
        region: String,
        credential_refresh_cmd: Option<String>,
    },

    /// GCP Vertex AI — credentials via Application Default Credentials;
    /// `auth_refresh_cmd` is the user-configured refresh script.
    GcpVertex {
        project_id: String,
        region: String,
        auth_refresh_cmd: Option<String>,
    },

    /// Azure AI Foundry.
    AzureFoundry {
        endpoint: String,
        api_key: Option<String>,
    },
}

impl AuthProvider {
    // ── Header construction ───────────────────────────────────────────────────

    /// Return the HTTP header name and value used to authenticate API requests.
    ///
    /// - API key  → `x-api-key: sk-…`
    /// - OAuth    → `Authorization: Bearer <token>`
    /// - Bedrock  → credentials are embedded in AWS SigV4 signing (not a header)
    /// - Vertex   → `Authorization: Bearer <token>` (handled by provider caller)
    pub async fn get_auth_header(&self) -> anyhow::Result<(HeaderName, HeaderValue)> {
        match self {
            AuthProvider::ApiKey(k) => {
                let name = HeaderName::from_static("x-api-key");
                let value = HeaderValue::from_str(&k.key)
                    .map_err(|_| anyhow::anyhow!("invalid API key header value"))?;
                Ok((name, value))
            }
            AuthProvider::ClaudeAiOAuth(tokens) => {
                let name = reqwest::header::AUTHORIZATION;
                let value = HeaderValue::from_str(&format!("Bearer {}", tokens.access_token))
                    .map_err(|_| anyhow::anyhow!("invalid OAuth token header value"))?;
                Ok((name, value))
            }
            AuthProvider::AwsBedrock { .. } | AuthProvider::GcpVertex { .. } | AuthProvider::AzureFoundry { .. } => {
                Err(anyhow::anyhow!(
                    "provider {:?} does not use a simple auth header",
                    std::mem::discriminant(self)
                ))
            }
        }
    }

    // ── Refresh ───────────────────────────────────────────────────────────────

    /// Refresh credentials if they are expired or near expiry.
    ///
    /// Returns `true` if a refresh was performed.
    ///
    /// Ref: src/utils/auth.ts checkAndRefreshOAuthTokenIfNeeded
    pub async fn refresh_if_needed(
        &mut self,
        storage: &dyn SecureStorage,
        client: &reqwest::Client,
    ) -> anyhow::Result<bool> {
        match self {
            AuthProvider::ClaudeAiOAuth(tokens) => {
                if !tokens.is_expired() {
                    return Ok(false);
                }
                let refresh_token = tokens
                    .refresh_token
                    .clone()
                    .ok_or_else(|| anyhow::anyhow!("OAuth token expired and no refresh token available"))?;

                let new_tokens = refresh_oauth_token(&refresh_token, &CLAUDE_AI_OAUTH, client).await?;

                // Merge refresh token (server may omit it if unchanged).
                let mut merged = new_tokens;
                if merged.refresh_token.is_none() {
                    merged.refresh_token = Some(refresh_token);
                }

                // Persist.
                let mut creds = storage.read().await?.unwrap_or_default();
                creds.oauth_tokens = Some(merged.clone());
                storage.write(&creds).await?;

                *tokens = merged;
                Ok(true)
            }
            AuthProvider::AwsBedrock { credential_refresh_cmd: Some(cmd), .. } => {
                let cmd = cmd.clone();
                tokio::process::Command::new("sh")
                    .arg("-c")
                    .arg(&cmd)
                    .status()
                    .await?;
                Ok(true)
            }
            AuthProvider::GcpVertex { auth_refresh_cmd: Some(cmd), .. } => {
                let cmd = cmd.clone();
                tokio::process::Command::new("sh")
                    .arg("-c")
                    .arg(&cmd)
                    .status()
                    .await?;
                Ok(true)
            }
            _ => Ok(false),
        }
    }

    // ── Auto-detection ────────────────────────────────────────────────────────

    /// Detect the most appropriate `AuthProvider` from the environment.
    ///
    /// Priority:
    /// 1. `ANTHROPIC_API_KEY` env var → `ApiKey(EnvVar)`
    /// 2. `CLAUDE_CODE_USE_BEDROCK` env var → `AwsBedrock`
    /// 3. `CLAUDE_CODE_USE_VERTEX` env var → `GcpVertex`
    /// 4. `AZURE_OPENAI_ENDPOINT` env var → `AzureFoundry`
    /// 5. Non-expired OAuth tokens in `SecureStorage` → `ClaudeAiOAuth`
    /// 6. `apiKeyHelper` command or stored key → `ApiKey`
    ///
    /// Ref: src/utils/auth.ts getAuthTokenSource
    pub async fn detect(
        config: &GlobalConfig,
        settings: &SettingsJson,
        storage: &dyn SecureStorage,
    ) -> anyhow::Result<Self> {
        // ── 1. Direct API key env var ─────────────────────────────────────────
        if let Ok(key) = std::env::var("ANTHROPIC_API_KEY") {
            if !key.is_empty() {
                return Ok(AuthProvider::ApiKey(crate::api_key::ApiKeyWithSource {
                    key,
                    source: crate::api_key::ApiKeySource::EnvVar,
                }));
            }
        }

        // ── 2. AWS Bedrock ────────────────────────────────────────────────────
        if std::env::var("CLAUDE_CODE_USE_BEDROCK").map_or(false, |v| is_truthy(&v)) {
            let region = std::env::var("AWS_DEFAULT_REGION")
                .or_else(|_| std::env::var("AWS_REGION"))
                .unwrap_or_else(|_| "us-east-1".to_owned());
            let refresh_cmd = settings.aws_auth_refresh.clone();
            return Ok(AuthProvider::AwsBedrock { region, credential_refresh_cmd: refresh_cmd });
        }

        // ── 3. GCP Vertex ─────────────────────────────────────────────────────
        if std::env::var("CLAUDE_CODE_USE_VERTEX").map_or(false, |v| is_truthy(&v)) {
            let project_id = std::env::var("ANTHROPIC_VERTEX_PROJECT_ID")
                .unwrap_or_else(|_| std::env::var("GCLOUD_PROJECT").unwrap_or_default());
            let region = std::env::var("CLOUD_ML_REGION")
                .unwrap_or_else(|_| "us-east5".to_owned());
            let refresh_cmd = settings.gcp_auth_refresh.clone();
            return Ok(AuthProvider::GcpVertex { project_id, region, auth_refresh_cmd: refresh_cmd });
        }

        // ── 4. Azure AI Foundry ───────────────────────────────────────────────
        if let Ok(endpoint) = std::env::var("AZURE_OPENAI_ENDPOINT") {
            if !endpoint.is_empty() {
                let api_key = std::env::var("AZURE_OPENAI_API_KEY").ok();
                return Ok(AuthProvider::AzureFoundry { endpoint, api_key });
            }
        }

        // ── 5. OAuth tokens in secure storage ─────────────────────────────────
        if let Ok(Some(creds)) = storage.read().await {
            if let Some(tokens) = creds.oauth_tokens {
                if !tokens.is_expired() || tokens.refresh_token.is_some() {
                    return Ok(AuthProvider::ClaudeAiOAuth(tokens));
                }
            }
        }

        // ── 6. OAuth tokens from file descriptor (CCR) ────────────────────────
        if let Some(tokens) = crate::api_key::get_oauth_token_from_fd().await {
            return Ok(AuthProvider::ClaudeAiOAuth(tokens));
        }

        // ── 7. API key via helper / stored / FD ──────────────────────────────
        if let Some(key_with_source) = get_api_key(config, settings, storage).await {
            return Ok(AuthProvider::ApiKey(key_with_source));
        }

        Err(anyhow::anyhow!(
            "No authentication configured. \
             Set ANTHROPIC_API_KEY or run `login` to authenticate."
        ))
    }
}

// ── 401 error handler ─────────────────────────────────────────────────────────

/// Handle a 401 response from the API by attempting token refresh.
///
/// On OAuth: tries to refresh the access token and returns `true` if
/// the caller should retry the request.
/// On API key: no refresh possible, returns `false`.
///
/// Ref: src/utils/auth.ts handleOAuth401Error
pub async fn handle_401(
    provider: &mut AuthProvider,
    storage: &dyn SecureStorage,
    client: &reqwest::Client,
) -> bool {
    match provider {
        AuthProvider::ClaudeAiOAuth(tokens) => {
            // Force expiry so refresh_if_needed always triggers.
            tokens.expires_at = 0;
            provider.refresh_if_needed(storage, client).await.is_ok()
        }
        _ => false,
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn is_truthy(s: &str) -> bool {
    matches!(s.to_lowercase().as_str(), "1" | "true" | "yes" | "on")
}
