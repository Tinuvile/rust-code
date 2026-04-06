//! OpenAI-compatible provider wrapper.
//!
//! Wraps `OpenAiClient` with factory methods for DeepSeek, Kimi, Minimax,
//! and generic OpenAI-compatible endpoints.

use code_types::provider::{
    LlmProvider, LlmRequest, ModelPricing, ProviderCapabilities, ProviderKind,
};
use code_types::stream::AssembledResponse;

use crate::providers::openai::client::OpenAiClient;

// ── OpenAiCompatClient ───────────────────────────────────────────────────────

/// Thin wrapper around `OpenAiClient` for OpenAI-compatible APIs.
///
/// Each factory method sets the correct base URL and `ProviderKind`.
#[derive(Clone)]
pub struct OpenAiCompatClient {
    inner: OpenAiClient,
}

impl OpenAiCompatClient {
    /// DeepSeek API.
    pub fn deepseek(api_key: String) -> Self {
        Self {
            inner: OpenAiClient::new(
                api_key,
                Some("https://api.deepseek.com".to_owned()),
                ProviderKind::DeepSeek,
            ),
        }
    }

    /// Moonshot / Kimi API.
    pub fn kimi(api_key: String) -> Self {
        Self {
            inner: OpenAiClient::new(
                api_key,
                Some("https://api.moonshot.cn".to_owned()),
                ProviderKind::Kimi,
            ),
        }
    }

    /// Minimax API.
    pub fn minimax(api_key: String) -> Self {
        Self {
            inner: OpenAiClient::new(
                api_key,
                Some("https://api.minimax.chat".to_owned()),
                ProviderKind::Minimax,
            ),
        }
    }

    /// Generic OpenAI-compatible endpoint.
    pub fn custom(api_key: String, base_url: String) -> Self {
        Self {
            inner: OpenAiClient::new(
                api_key,
                Some(base_url),
                ProviderKind::OpenAiCompatible,
            ),
        }
    }
}

#[async_trait::async_trait]
impl LlmProvider for OpenAiCompatClient {
    fn kind(&self) -> ProviderKind {
        self.inner.kind()
    }

    fn capabilities(&self, model: &str) -> ProviderCapabilities {
        let mut caps = self.inner.capabilities(model);
        // Some compatible providers have different limits.
        let lower = model.to_lowercase();
        match self.inner.kind() {
            ProviderKind::DeepSeek => {
                if lower.contains("deepseek-chat") || lower.contains("deepseek-v3") {
                    caps.max_context_window = 64_000;
                    caps.max_output_tokens = 8_192;
                    caps.supports_thinking = false;
                } else if lower.contains("deepseek-reasoner") || lower.contains("r1") {
                    caps.max_context_window = 64_000;
                    caps.max_output_tokens = 8_192;
                    caps.supports_thinking = true;
                }
            }
            ProviderKind::Kimi => {
                if lower.contains("128k") {
                    caps.max_context_window = 128_000;
                } else if lower.contains("32k") {
                    caps.max_context_window = 32_000;
                } else {
                    caps.max_context_window = 8_000;
                }
                caps.max_output_tokens = 4_096;
                caps.supports_thinking = false;
            }
            ProviderKind::Minimax => {
                caps.max_context_window = 245_760;
                caps.max_output_tokens = 16_384;
                caps.supports_thinking = false;
            }
            _ => {}
        }
        caps
    }

    async fn send(
        &self,
        request: LlmRequest,
    ) -> Result<AssembledResponse, Box<dyn std::error::Error + Send + Sync>> {
        self.inner.send(request).await
    }

    fn pricing(&self, model: &str) -> Option<ModelPricing> {
        crate::cost::get_compat_pricing(model, self.inner.kind())
    }
}
