//! Anthropic API client — sends Messages API requests and returns typed streams.
//!
//! Supports multiple providers:
//! - Direct Anthropic API (API key or OAuth Bearer)
//! - AWS Bedrock (SigV4 signed — credentials from environment)
//! - GCP Vertex AI (Bearer from ADC)
//! - Azure AI Foundry (API key or Azure AD)
//!
//! Ref: src/services/api/client.ts (getAnthropicClient)
//! Ref: src/services/api/claude.ts (streamQuery, buildRequestParams)

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use reqwest::header::{HeaderMap, HeaderName, HeaderValue, AUTHORIZATION};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

use code_types::message::ContentBlock;
use code_types::stream::AssembledResponse;

use crate::error::ApiError;
use crate::model::{detect_provider, ApiProvider, BetaHeaders};
use crate::retry::{with_retry, RetryPolicy};
use crate::stream::{collect_stream, parse_event_stream};
use crate::tokens::SessionUsage;

// ── Request / Response types ──────────────────────────────────────────────────

/// A message parameter sent to the API.
#[derive(Debug, Clone, Serialize)]
pub struct ApiMessage {
    pub role: ApiRole,
    pub content: Vec<ContentBlock>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ApiRole {
    User,
    Assistant,
}

/// A tool definition sent to the API.
#[derive(Debug, Clone, Serialize)]
pub struct ApiTool {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
}

/// The full request body sent to the Messages API.
///
/// Ref: src/services/api/claude.ts buildRequestParams
#[derive(Debug, Clone, Serialize)]
pub struct MessagesRequest {
    pub model: String,
    pub messages: Vec<ApiMessage>,
    pub max_tokens: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tools: Vec<ApiTool>,
    pub stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinking: Option<ThinkingConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f32>,
}

/// Extended thinking configuration.
///
/// Ref: src/utils/thinking.ts ThinkingConfig
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThinkingConfig {
    #[serde(rename = "type")]
    pub kind: ThinkingKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub budget_tokens: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ThinkingKind {
    Enabled,
    Disabled,
}

// ── Client configuration ──────────────────────────────────────────────────────

/// Configuration for the `AnthropicClient`.
#[derive(Debug, Clone)]
pub struct ClientConfig {
    /// Authentication header value (from `AuthProvider::get_auth_header`).
    pub auth_header: (HeaderName, HeaderValue),
    /// Which provider / base URL to use.
    pub provider: ApiProvider,
    /// Base URL override (empty = use provider default).
    pub base_url: String,
    /// Extra headers to add to every request.
    pub extra_headers: HashMap<String, String>,
    /// Whether this is an OAuth (claude.ai) user.
    pub is_oauth_user: bool,
    /// Session ID for request tracking.
    pub session_id: Option<String>,
    /// User agent string.
    pub user_agent: String,
    /// Request timeout.
    pub timeout: Duration,
}

impl ClientConfig {
    pub fn from_api_key(api_key: String) -> Self {
        let provider = detect_provider();
        let base_url = crate::model::provider_base_url(provider);
        Self {
            auth_header: (
                HeaderName::from_static("x-api-key"),
                HeaderValue::from_str(&api_key).expect("valid api key"),
            ),
            provider,
            base_url,
            extra_headers: HashMap::new(),
            is_oauth_user: false,
            session_id: None,
            user_agent: default_user_agent(),
            timeout: Duration::from_secs(600),
        }
    }

    pub fn from_oauth_token(access_token: String) -> Self {
        let provider = detect_provider();
        let base_url = crate::model::provider_base_url(provider);
        Self {
            auth_header: (
                AUTHORIZATION,
                HeaderValue::from_str(&format!("Bearer {access_token}"))
                    .expect("valid bearer token"),
            ),
            provider,
            base_url,
            extra_headers: HashMap::new(),
            is_oauth_user: true,
            session_id: None,
            user_agent: default_user_agent(),
            timeout: Duration::from_secs(600),
        }
    }
}

fn default_user_agent() -> String {
    format!("claude-code/{}", env!("CARGO_PKG_VERSION"))
}

// ── AnthropicClient ───────────────────────────────────────────────────────────

/// HTTP client for the Anthropic Messages API.
///
/// Thread-safe — clone or wrap in `Arc` to share across tasks.
///
/// Ref: src/services/api/client.ts getAnthropicClient
#[derive(Clone)]
pub struct AnthropicClient {
    http: reqwest::Client,
    config: ClientConfig,
    /// Accumulated usage across all calls on this client.
    usage: Arc<RwLock<SessionUsage>>,
}

impl AnthropicClient {
    /// Create from a pre-built `ClientConfig`.
    pub fn new(config: ClientConfig) -> Self {
        let http = reqwest::Client::builder()
            .timeout(config.timeout)
            .user_agent(&config.user_agent)
            .build()
            .expect("valid reqwest client");
        Self {
            http,
            config,
            usage: Arc::new(RwLock::new(SessionUsage::new())),
        }
    }

    // ── Internal request builder ──────────────────────────────────────────────

    fn messages_url(&self) -> String {
        match self.config.provider {
            ApiProvider::Anthropic | ApiProvider::Azure => {
                format!("{}/v1/messages", self.config.base_url)
            }
            // Bedrock and Vertex embed the model in the path — handled per-request.
            ApiProvider::Bedrock | ApiProvider::Vertex => self.config.base_url.clone(),
        }
    }

    fn build_headers(&self, model: &str) -> HeaderMap {
        let mut headers = HeaderMap::new();
        // Auth header.
        headers.insert(
            self.config.auth_header.0.clone(),
            self.config.auth_header.1.clone(),
        );
        // Anthropic API version.
        headers.insert(
            HeaderName::from_static("anthropic-version"),
            HeaderValue::from_static("2023-06-01"),
        );
        // Beta headers.
        let betas = BetaHeaders::for_model(model, self.config.is_oauth_user);
        if !betas.is_empty() {
            if let Ok(v) = HeaderValue::from_str(&betas.join(",")) {
                headers.insert(HeaderName::from_static("anthropic-beta"), v);
            }
        }
        // Session ID for request attribution.
        if let Some(sid) = &self.config.session_id {
            if let Ok(v) = HeaderValue::from_str(sid) {
                headers.insert(HeaderName::from_static("x-session-id"), v);
            }
        }
        // Extra headers.
        for (k, v) in &self.config.extra_headers {
            if let (Ok(name), Ok(val)) = (
                HeaderName::try_from(k.as_str()),
                HeaderValue::from_str(v),
            ) {
                headers.insert(name, val);
            }
        }
        headers
    }

    // ── Public API ────────────────────────────────────────────────────────────

    /// Send a streaming Messages API request and return an assembled response.
    ///
    /// Automatically retries on transient errors per the `RetryPolicy`.
    ///
    /// Ref: src/services/api/claude.ts streamQuery
    pub async fn stream(
        &self,
        request: MessagesRequest,
        policy: RetryPolicy,
    ) -> Result<AssembledResponse, crate::error::CannotRetryError> {
        let client = self.clone();
        with_retry(
            policy,
            |msg| tracing::debug!("retry: {msg}"),
            |model, _attempt| {
                let mut req = request.clone();
                req.model = model;
                let c = client.clone();
                async move { c.send_once(&req).await }
            },
        )
        .await
    }

    /// Send a single (non-retried) streaming request.
    async fn send_once(&self, request: &MessagesRequest) -> anyhow::Result<AssembledResponse> {
        let model = &request.model;
        let url = self.messages_url();
        let headers = self.build_headers(model);

        let resp = self
            .http
            .post(&url)
            .headers(headers)
            .json(request)
            .send()
            .await?;

        let status = resp.status().as_u16();
        if !resp.status().is_success() {
            let body = resp.text().await.unwrap_or_default();
            let message = extract_error_message(&body);
            return Err(ApiError { status, message, body: Some(body) }.into());
        }

        // Parse the SSE byte stream.
        let byte_stream = resp.bytes_stream();
        let event_stream = parse_event_stream(byte_stream);
        let assembled = collect_stream(Box::pin(event_stream)).await?;

        // Accumulate usage.
        {
            let mut usage = self.usage.write().await;
            usage.add(&assembled.usage);
        }

        Ok(assembled)
    }

    /// Return a snapshot of the accumulated session usage.
    pub async fn session_usage(&self) -> SessionUsage {
        self.usage.read().await.clone()
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Extract a human-readable error message from an API error response body.
fn extract_error_message(body: &str) -> String {
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(body) {
        if let Some(msg) = v
            .get("error")
            .and_then(|e| e.get("message"))
            .and_then(|m| m.as_str())
        {
            return msg.to_owned();
        }
        if let Some(msg) = v.get("message").and_then(|m| m.as_str()) {
            return msg.to_owned();
        }
    }
    if body.len() > 500 {
        return body[..500].to_owned();
    }
    body.to_owned()
}
