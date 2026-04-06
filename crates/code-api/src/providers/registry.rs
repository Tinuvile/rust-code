//! Provider factory — construct the right `LlmProvider` from configuration.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use code_types::provider::{LlmProvider, ProviderKind};

use crate::client::{AnthropicClient, ClientConfig};
use crate::providers::gemini::client::GeminiClient;
use crate::providers::openai::client::OpenAiClient;
use crate::providers::openai_compat::client::OpenAiCompatClient;

// ── ProviderConfig ───────────────────────────────────────────────────────────

/// Everything needed to construct a provider.
pub struct ProviderConfig {
    pub kind: ProviderKind,
    pub api_key: String,
    pub base_url: Option<String>,
    pub extra_headers: HashMap<String, String>,
    pub timeout: Duration,
}

// ── Factory ──────────────────────────────────────────────────────────────────

/// Build the correct `LlmProvider` from `ProviderConfig`.
pub fn create_provider(config: ProviderConfig) -> Arc<dyn LlmProvider> {
    match config.kind {
        // Anthropic family: reuse the existing AnthropicClient.
        ProviderKind::Anthropic
        | ProviderKind::Bedrock
        | ProviderKind::Vertex
        | ProviderKind::Azure => {
            let mut client_config = ClientConfig::from_api_key(config.api_key);
            if !config.extra_headers.is_empty() {
                client_config.extra_headers = config.extra_headers;
            }
            if let Some(ref url) = config.base_url {
                if !url.is_empty() {
                    client_config.base_url = url.clone();
                }
            }
            client_config.timeout = config.timeout;
            Arc::new(AnthropicClient::new(client_config))
        }

        // OpenAI.
        ProviderKind::OpenAi => {
            Arc::new(OpenAiClient::new(config.api_key, config.base_url, ProviderKind::OpenAi))
        }

        // Gemini.
        ProviderKind::Gemini => {
            Arc::new(GeminiClient::new(config.api_key, config.base_url))
        }

        // OpenAI-compatible providers.
        ProviderKind::DeepSeek => Arc::new(OpenAiCompatClient::deepseek(config.api_key)),
        ProviderKind::Kimi => Arc::new(OpenAiCompatClient::kimi(config.api_key)),
        ProviderKind::Minimax => Arc::new(OpenAiCompatClient::minimax(config.api_key)),
        ProviderKind::OpenAiCompatible => {
            let base_url = config.base_url.unwrap_or_default();
            Arc::new(OpenAiCompatClient::custom(config.api_key, base_url))
        }
    }
}
