//! OAuth 2.0 PKCE flow for Claude.ai and Anthropic Console authentication.
//!
//! Ref: src/services/oauth/crypto.ts
//! Ref: src/services/oauth/auth-code-listener.ts
//! Ref: src/services/oauth/client.ts
//! Ref: src/services/oauth/index.ts
//! Ref: src/constants/oauth.ts

use std::sync::Arc;
use std::time::Duration;

use axum::{
    extract::{Query, State},
    response::Html,
    routing::get,
    Router,
};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use chrono::Utc;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::sync::oneshot;

// ── OAuth configuration ───────────────────────────────────────────────────────

/// OAuth endpoint configuration for a specific environment.
///
/// Ref: src/constants/oauth.ts OauthConfig
#[derive(Debug, Clone)]
pub struct OAuthConfig {
    pub base_api_url: &'static str,
    pub authorize_url: &'static str,
    pub token_url: &'static str,
    pub client_id: &'static str,
    pub success_url: &'static str,
    pub manual_redirect_url: &'static str,
}

/// Production claude.ai OAuth endpoints.
pub const CLAUDE_AI_OAUTH: OAuthConfig = OAuthConfig {
    base_api_url: "https://api.anthropic.com",
    authorize_url: "https://claude.ai/oauth/authorize",
    token_url: "https://claude.ai/oauth/token",
    client_id: "9d1c250a-e61b-44d9-88ed-5944d1962f5e",
    success_url: "https://claude.ai/oauth/success",
    manual_redirect_url: "https://claude.ai/oauth/callback",
};

/// Production Anthropic Console OAuth endpoints.
pub const CONSOLE_OAUTH: OAuthConfig = OAuthConfig {
    base_api_url: "https://api.anthropic.com",
    authorize_url: "https://console.anthropic.com/oauth/authorize",
    token_url: "https://console.anthropic.com/oauth/token",
    client_id: "9d1c250a-e61b-44d9-88ed-5944d1962f5e",
    success_url: "https://console.anthropic.com/oauth/success",
    manual_redirect_url: "https://console.anthropic.com/oauth/callback",
};

// ── Scopes ────────────────────────────────────────────────────────────────────

pub const SCOPE_INFERENCE: &str = "user:inference";
pub const SCOPE_PROFILE: &str = "user:profile";

pub const CLAUDE_AI_SCOPES: &[&str] =
    &["user:profile", "user:inference", "user:sessions:claude_code"];

pub const CONSOLE_SCOPES: &[&str] = &["org:create_api_key", "user:profile"];

// ── PKCE utilities ────────────────────────────────────────────────────────────

/// Generate a PKCE code verifier (32 random bytes, base64url-encoded).
///
/// Ref: src/services/oauth/crypto.ts generateCodeVerifier
pub fn generate_code_verifier() -> String {
    let mut bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}

/// Generate a PKCE code challenge: SHA-256(verifier), base64url-encoded.
///
/// Ref: src/services/oauth/crypto.ts generateCodeChallenge
pub fn generate_code_challenge(verifier: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(verifier.as_bytes());
    URL_SAFE_NO_PAD.encode(hasher.finalize())
}

/// Generate a random OAuth state parameter for CSRF protection.
///
/// Ref: src/services/oauth/crypto.ts generateState
pub fn generate_state() -> String {
    let mut bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}

// ── OAuth token types ─────────────────────────────────────────────────────────

/// A set of OAuth tokens returned by the authorization server.
///
/// `expires_at` is a Unix timestamp in **milliseconds**.
///
/// Ref: src/services/oauth/client.ts (token response shape)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthTokens {
    pub access_token: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub refresh_token: Option<String>,
    /// Unix timestamp in milliseconds when the access token expires.
    pub expires_at: i64,
    #[serde(default)]
    pub scopes: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub account_uuid: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub organization_uuid: Option<String>,
}

/// 5-minute buffer before expiry that triggers a proactive refresh.
const EXPIRY_BUFFER_MS: i64 = 5 * 60 * 1_000;

impl OAuthTokens {
    /// `true` if the access token has expired (accounting for 5-minute buffer).
    ///
    /// Ref: src/services/oauth/client.ts isOAuthTokenExpired
    pub fn is_expired(&self) -> bool {
        let now_ms = Utc::now().timestamp_millis();
        now_ms >= self.expires_at - EXPIRY_BUFFER_MS
    }

    /// `true` if the token set includes the given scope.
    pub fn has_scope(&self, scope: &str) -> bool {
        self.scopes.iter().any(|s| s == scope)
    }
}

// ── Token exchange wire format ────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: String,
    refresh_token: Option<String>,
    expires_in: u64,
    scope: Option<String>,
    account: Option<TokenAccount>,
    organization: Option<TokenOrg>,
}

#[derive(Debug, Deserialize)]
struct TokenAccount {
    uuid: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TokenOrg {
    uuid: Option<String>,
}

fn parse_token_response(resp: TokenResponse) -> OAuthTokens {
    let now_ms = Utc::now().timestamp_millis();
    let expires_at = now_ms + (resp.expires_in as i64) * 1_000;
    let scopes = resp
        .scope
        .unwrap_or_default()
        .split_whitespace()
        .map(str::to_owned)
        .collect();
    OAuthTokens {
        access_token: resp.access_token,
        refresh_token: resp.refresh_token,
        expires_at,
        scopes,
        account_uuid: resp.account.and_then(|a| a.uuid),
        organization_uuid: resp.organization.and_then(|o| o.uuid),
    }
}

// ── Token exchange / refresh ──────────────────────────────────────────────────

/// Exchange an authorization code for tokens via the token endpoint.
///
/// Ref: src/services/oauth/client.ts exchangeCodeForTokens
pub async fn exchange_code_for_tokens(
    code: &str,
    verifier: &str,
    redirect_uri: &str,
    config: &OAuthConfig,
    client: &reqwest::Client,
) -> anyhow::Result<OAuthTokens> {
    let params = [
        ("grant_type", "authorization_code"),
        ("code", code),
        ("redirect_uri", redirect_uri),
        ("code_verifier", verifier),
        ("client_id", config.client_id),
    ];
    let resp = client
        .post(config.token_url)
        .form(&params)
        .send()
        .await?
        .error_for_status()?;
    let body: TokenResponse = resp.json().await?;
    Ok(parse_token_response(body))
}

/// Refresh an access token using the stored refresh token.
///
/// Ref: src/services/oauth/client.ts refreshOAuthToken
pub async fn refresh_oauth_token(
    refresh_token: &str,
    config: &OAuthConfig,
    client: &reqwest::Client,
) -> anyhow::Result<OAuthTokens> {
    let params = [
        ("grant_type", "refresh_token"),
        ("refresh_token", refresh_token),
        ("client_id", config.client_id),
    ];
    let resp = client
        .post(config.token_url)
        .form(&params)
        .send()
        .await?
        .error_for_status()?;
    let body: TokenResponse = resp.json().await?;
    Ok(parse_token_response(body))
}

// ── Local callback server (axum) ──────────────────────────────────────────────

struct CallbackState {
    tx: Arc<tokio::sync::Mutex<Option<oneshot::Sender<Result<String, String>>>>>,
    expected_state: String,
}

#[derive(Debug, Deserialize)]
struct CallbackQuery {
    code: Option<String>,
    state: Option<String>,
    error: Option<String>,
    error_description: Option<String>,
}

async fn handle_callback(
    State(ctx): State<Arc<CallbackState>>,
    Query(params): Query<CallbackQuery>,
) -> Html<&'static str> {
    // CSRF check.
    if params.state.as_deref() != Some(ctx.expected_state.as_str()) {
        if let Some(tx) = ctx.tx.lock().await.take() {
            let _ = tx.send(Err("OAuth state mismatch".to_owned()));
        }
        return Html("<h1>Authentication failed: invalid state.</h1>");
    }

    if let Some(error) = params.error {
        let msg = params.error_description.unwrap_or(error);
        if let Some(tx) = ctx.tx.lock().await.take() {
            let _ = tx.send(Err(msg));
        }
        return Html("<h1>Authentication failed. You may close this tab.</h1>");
    }

    if let Some(code) = params.code {
        if let Some(tx) = ctx.tx.lock().await.take() {
            let _ = tx.send(Ok(code));
        }
        Html("<h1>Authentication successful! You may close this tab.</h1>")
    } else {
        if let Some(tx) = ctx.tx.lock().await.take() {
            let _ = tx.send(Err("No authorization code received".to_owned()));
        }
        Html("<h1>Authentication failed.</h1>")
    }
}

/// Temporary localhost HTTP server that captures the OAuth redirect.
///
/// Ref: src/services/oauth/auth-code-listener.ts AuthCodeListener
pub struct AuthCodeListener {
    port: u16,
    rx: oneshot::Receiver<Result<String, String>>,
    shutdown_tx: Option<oneshot::Sender<()>>,
}

impl AuthCodeListener {
    /// Bind to a random localhost port and start the server.
    pub async fn start(expected_state: String) -> anyhow::Result<Self> {
        let (code_tx, code_rx) = oneshot::channel::<Result<String, String>>();
        let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();

        let shared = Arc::new(CallbackState {
            tx: Arc::new(tokio::sync::Mutex::new(Some(code_tx))),
            expected_state,
        });

        let app = Router::new()
            .route("/callback", get(handle_callback))
            .with_state(shared);

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let port = listener.local_addr()?.port();

        tokio::spawn(async move {
            axum::serve(listener, app)
                .with_graceful_shutdown(async {
                    let _ = shutdown_rx.await;
                })
                .await
                .ok();
        });

        Ok(Self {
            port,
            rx: code_rx,
            shutdown_tx: Some(shutdown_tx),
        })
    }

    /// The port this server is listening on.
    pub fn port(&self) -> u16 {
        self.port
    }

    /// The `redirect_uri` to pass to the OAuth authorization server.
    pub fn redirect_uri(&self) -> String {
        format!("http://127.0.0.1:{}/callback", self.port)
    }

    /// Wait up to 5 minutes for the authorization code.
    pub async fn wait_for_code(mut self) -> anyhow::Result<String> {
        let result = tokio::time::timeout(Duration::from_secs(300), &mut self.rx)
            .await
            .map_err(|_| anyhow::anyhow!("OAuth callback timed out after 5 minutes"))?
            .map_err(|_| anyhow::anyhow!("OAuth callback channel closed unexpectedly"))?;

        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(());
        }
        result.map_err(|e| anyhow::anyhow!(e))
    }
}

// ── OAuthService ──────────────────────────────────────────────────────────────

/// Options controlling the OAuth browser flow.
pub struct OAuthOptions {
    /// Use claude.ai endpoints (default). If `false`, use Anthropic Console.
    pub login_with_claude_ai: bool,
    /// Request only `user:inference` (skip profile scope).
    pub inference_only: bool,
    /// Print the URL instead of opening the browser.
    pub skip_browser_open: bool,
    /// Restrict login to a specific organization.
    pub org_uuid: Option<String>,
}

impl Default for OAuthOptions {
    fn default() -> Self {
        Self {
            login_with_claude_ai: true,
            inference_only: false,
            skip_browser_open: false,
            org_uuid: None,
        }
    }
}

/// Orchestrates the OAuth PKCE browser authorization flow.
///
/// Ref: src/services/oauth/index.ts OAuthService
pub struct OAuthService {
    client: reqwest::Client,
}

impl OAuthService {
    pub fn new(client: reqwest::Client) -> Self {
        Self { client }
    }

    /// Run the full PKCE flow and return tokens on success.
    pub async fn start_oauth_flow(&self, options: OAuthOptions) -> anyhow::Result<OAuthTokens> {
        let config = if options.login_with_claude_ai {
            &CLAUDE_AI_OAUTH
        } else {
            &CONSOLE_OAUTH
        };

        // Allow overriding the client_id via env var (FedStart, staging, etc.).
        let client_id_override = std::env::var("CLAUDE_CODE_OAUTH_CLIENT_ID").ok();
        let client_id = client_id_override
            .as_deref()
            .unwrap_or(config.client_id);

        // Generate PKCE parameters.
        let verifier = generate_code_verifier();
        let challenge = generate_code_challenge(&verifier);
        let state = generate_state();

        // Start the local callback server.
        let listener = AuthCodeListener::start(state.clone()).await?;
        let redirect_uri = listener.redirect_uri();

        // Select scopes.
        let scopes: &[&str] = if options.inference_only {
            &[SCOPE_INFERENCE]
        } else if options.login_with_claude_ai {
            CLAUDE_AI_SCOPES
        } else {
            CONSOLE_SCOPES
        };

        // Build authorization URL.
        let mut url = reqwest::Url::parse(config.authorize_url)?;
        {
            let mut q = url.query_pairs_mut();
            q.append_pair("response_type", "code");
            q.append_pair("client_id", client_id);
            q.append_pair("redirect_uri", &redirect_uri);
            q.append_pair("code_challenge", &challenge);
            q.append_pair("code_challenge_method", "S256");
            q.append_pair("state", &state);
            q.append_pair("scope", &scopes.join(" "));
            if let Some(org) = &options.org_uuid {
                q.append_pair("organization_uuid", org);
            }
        }

        let auth_url = url.to_string();

        if options.skip_browser_open {
            eprintln!("Open this URL to authenticate:\n{auth_url}");
        } else {
            open::that_detached(&auth_url).ok();
            eprintln!("Browser opened for authentication. Waiting for callback…");
        }

        // Wait for the authorization code from the callback.
        let code = listener.wait_for_code().await?;

        // Exchange the authorization code for tokens.
        exchange_code_for_tokens(&code, &verifier, &redirect_uri, config, &self.client).await
    }
}
