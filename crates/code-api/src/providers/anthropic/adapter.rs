//! Adapter: `impl LlmProvider for AnthropicClient`.
//!
//! Wraps the existing `AnthropicClient` so it satisfies the provider trait.
//! The actual HTTP logic, SSE parsing, and retry all remain in the original
//! `client.rs` / `stream.rs` modules — this file only does format conversion.

use code_types::provider::{
    LlmProvider, LlmRequest, ModelPricing, ProviderCapabilities, ProviderKind, ToolDefinition,
};
use code_types::stream::AssembledResponse;

use crate::client::{AnthropicClient, ApiTool, MessagesRequest, ThinkingConfig, ThinkingKind};
use crate::cost::get_pricing;
use crate::model::{get_context_window, get_max_output_tokens};
use crate::retry::RetryPolicy;

// ── Conversion helpers ───────────────────────────────────────────────────────

/// Convert a unified `ToolDefinition` to the Anthropic-specific `ApiTool`.
fn to_api_tool(td: &ToolDefinition) -> ApiTool {
    ApiTool {
        name: td.name.clone(),
        description: td.description.clone(),
        input_schema: td.parameters.clone(),
    }
}

/// Convert the unified `ThinkingConfig` to the Anthropic-specific type.
fn to_anthropic_thinking(
    cfg: &code_types::provider::ThinkingConfig,
) -> Option<ThinkingConfig> {
    if cfg.enabled {
        Some(ThinkingConfig {
            kind: ThinkingKind::Enabled,
            budget_tokens: cfg.budget_tokens,
        })
    } else {
        Some(ThinkingConfig {
            kind: ThinkingKind::Disabled,
            budget_tokens: None,
        })
    }
}

/// Build a `MessagesRequest` from an `LlmRequest`.
fn to_anthropic_request(req: &LlmRequest) -> MessagesRequest {
    use crate::client::{ApiMessage, ApiRole};

    let api_messages = req
        .messages
        .iter()
        .map(|m| ApiMessage {
            role: match m.role {
                code_types::message::ApiRole::User => ApiRole::User,
                code_types::message::ApiRole::Assistant => ApiRole::Assistant,
            },
            content: m.content.clone(),
        })
        .collect();

    MessagesRequest {
        model: req.model.clone(),
        messages: api_messages,
        max_tokens: req.max_tokens,
        system: req.system.clone(),
        tools: req.tools.iter().map(to_api_tool).collect(),
        stream: true,
        temperature: req.temperature,
        thinking: req.thinking.as_ref().and_then(to_anthropic_thinking),
        top_p: req.top_p,
    }
}

// ── LlmProvider implementation ───────────────────────────────────────────────

#[async_trait::async_trait]
impl LlmProvider for AnthropicClient {
    fn kind(&self) -> ProviderKind {
        // The AnthropicClient already knows its provider variant via config.provider.
        // Map the internal ApiProvider to the unified ProviderKind.
        match self.provider_variant() {
            crate::model::ApiProvider::Anthropic => ProviderKind::Anthropic,
            crate::model::ApiProvider::Bedrock => ProviderKind::Bedrock,
            crate::model::ApiProvider::Vertex => ProviderKind::Vertex,
            crate::model::ApiProvider::Azure => ProviderKind::Azure,
        }
    }

    fn capabilities(&self, model: &str) -> ProviderCapabilities {
        ProviderCapabilities {
            supports_streaming: true,
            supports_tool_calling: true,
            supports_thinking: true,
            supports_images: true,
            supports_cache_control: true,
            max_context_window: get_context_window(model),
            max_output_tokens: get_max_output_tokens(model),
        }
    }

    async fn send(
        &self,
        request: LlmRequest,
    ) -> Result<AssembledResponse, Box<dyn std::error::Error + Send + Sync>> {
        let anthropic_req = to_anthropic_request(&request);
        let assembled = self
            .stream(anthropic_req, RetryPolicy::default())
            .await
            .map_err(|e| -> Box<dyn std::error::Error + Send + Sync> { Box::new(e) })?;
        Ok(assembled)
    }

    fn pricing(&self, model: &str) -> Option<ModelPricing> {
        let p = get_pricing(model);
        Some(ModelPricing {
            input_per_mtok: p.input_per_mtok,
            output_per_mtok: p.output_per_mtok,
            cache_write_per_mtok: p.cache_write_per_mtok,
            cache_read_per_mtok: p.cache_read_per_mtok,
        })
    }
}
