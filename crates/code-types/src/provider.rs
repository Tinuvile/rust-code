//! LLM provider abstraction — trait and unified request/response types.
//!
//! The `LlmProvider` trait decouples the query engine from any specific API.
//! Each provider (Anthropic, OpenAI, Gemini, DeepSeek, Kimi, Minimax, …)
//! implements this trait and converts between its wire format and the unified
//! internal types (`ContentBlock`, `AssembledResponse`, etc.).

use serde::{Deserialize, Serialize};

use crate::message::ApiMessage;
use crate::stream::AssembledResponse;

// ── Provider identification ──────────────────────────────────────────────────

/// Identifies which LLM provider family a client belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderKind {
    /// Direct Anthropic API.
    Anthropic,
    /// AWS Bedrock (Anthropic models).
    Bedrock,
    /// GCP Vertex AI (Anthropic models).
    Vertex,
    /// Azure AI Foundry (Anthropic models).
    Azure,
    /// OpenAI (GPT-4o, o3, etc.).
    OpenAi,
    /// Google Gemini.
    Gemini,
    /// DeepSeek (OpenAI-compatible).
    DeepSeek,
    /// Moonshot / Kimi (OpenAI-compatible).
    Kimi,
    /// Minimax (OpenAI-compatible).
    Minimax,
    /// Generic OpenAI-compatible endpoint.
    OpenAiCompatible,
}

impl ProviderKind {
    /// Parse from a string (case-insensitive, accepts common aliases).
    pub fn from_str_loose(s: &str) -> Option<Self> {
        match s.to_lowercase().replace('-', "_").as_str() {
            "anthropic" => Some(Self::Anthropic),
            "bedrock" | "aws_bedrock" => Some(Self::Bedrock),
            "vertex" | "gcp_vertex" => Some(Self::Vertex),
            "azure" | "azure_foundry" => Some(Self::Azure),
            "openai" | "open_ai" => Some(Self::OpenAi),
            "gemini" | "google" | "google_gemini" => Some(Self::Gemini),
            "deepseek" | "deep_seek" => Some(Self::DeepSeek),
            "kimi" | "moonshot" => Some(Self::Kimi),
            "minimax" => Some(Self::Minimax),
            "openai_compatible" | "openai_compat" | "custom" => Some(Self::OpenAiCompatible),
            _ => None,
        }
    }

    /// Whether this provider uses the OpenAI Chat Completions wire format.
    pub fn is_openai_compatible(&self) -> bool {
        matches!(
            self,
            Self::OpenAi | Self::DeepSeek | Self::Kimi | Self::Minimax | Self::OpenAiCompatible
        )
    }

    /// Whether this provider uses the Anthropic Messages API wire format.
    pub fn is_anthropic_family(&self) -> bool {
        matches!(self, Self::Anthropic | Self::Bedrock | Self::Vertex | Self::Azure)
    }

    /// The environment variable name(s) for the API key, in priority order.
    pub fn api_key_env_vars(&self) -> &'static [&'static str] {
        match self {
            Self::Anthropic | Self::Bedrock | Self::Vertex | Self::Azure => {
                &["ANTHROPIC_API_KEY"]
            }
            Self::OpenAi => &["OPENAI_API_KEY"],
            Self::Gemini => &["GEMINI_API_KEY", "GOOGLE_API_KEY"],
            Self::DeepSeek => &["DEEPSEEK_API_KEY"],
            Self::Kimi => &["KIMI_API_KEY", "MOONSHOT_API_KEY"],
            Self::Minimax => &["MINIMAX_API_KEY"],
            Self::OpenAiCompatible => &["LLM_API_KEY"],
        }
    }

    /// Default base URL for the provider (can be overridden by settings).
    pub fn default_base_url(&self) -> &'static str {
        match self {
            Self::Anthropic => "https://api.anthropic.com",
            Self::Bedrock => "", // constructed dynamically from region
            Self::Vertex => "",  // constructed dynamically from project/region
            Self::Azure => "",   // constructed dynamically from resource
            Self::OpenAi => "https://api.openai.com",
            Self::Gemini => "https://generativelanguage.googleapis.com",
            Self::DeepSeek => "https://api.deepseek.com",
            Self::Kimi => "https://api.moonshot.cn",
            Self::Minimax => "https://api.minimax.chat",
            Self::OpenAiCompatible => "",
        }
    }
}

impl std::fmt::Display for ProviderKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Self::Anthropic => "anthropic",
            Self::Bedrock => "bedrock",
            Self::Vertex => "vertex",
            Self::Azure => "azure",
            Self::OpenAi => "openai",
            Self::Gemini => "gemini",
            Self::DeepSeek => "deepseek",
            Self::Kimi => "kimi",
            Self::Minimax => "minimax",
            Self::OpenAiCompatible => "openai-compatible",
        };
        f.write_str(s)
    }
}

// ── Unified request types ────────────────────────────────────────────────────

/// Provider-agnostic tool definition.
///
/// Each provider adapter converts this to its wire format:
///   - Anthropic: `{name, description, input_schema}`
///   - OpenAI:    `{type:"function", function:{name, description, parameters}}`
///   - Gemini:    `{functionDeclarations:[{name, description, parameters}]}`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    /// JSON Schema describing the tool's input parameters.
    pub parameters: serde_json::Value,
}

/// Extended thinking / reasoning configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThinkingConfig {
    pub enabled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub budget_tokens: Option<u32>,
}

/// Unified LLM request, provider-agnostic.
///
/// The provider's `send()` implementation converts this to its wire format.
#[derive(Debug, Clone)]
pub struct LlmRequest {
    pub model: String,
    pub messages: Vec<ApiMessage>,
    pub max_tokens: u32,
    pub system: Option<serde_json::Value>,
    pub tools: Vec<ToolDefinition>,
    pub temperature: Option<f32>,
    pub thinking: Option<ThinkingConfig>,
    pub top_p: Option<f32>,
}

// ── Provider capabilities ────────────────────────────────────────────────────

/// Runtime capabilities advertised by a provider for a given model.
#[derive(Debug, Clone)]
pub struct ProviderCapabilities {
    pub supports_streaming: bool,
    pub supports_tool_calling: bool,
    pub supports_thinking: bool,
    pub supports_images: bool,
    pub supports_cache_control: bool,
    pub max_context_window: u32,
    pub max_output_tokens: u32,
}

// ── Pricing ──────────────────────────────────────────────────────────────────

/// Pricing for a single model, in USD per million tokens.
#[derive(Debug, Clone, Copy)]
pub struct ModelPricing {
    pub input_per_mtok: f64,
    pub output_per_mtok: f64,
    pub cache_write_per_mtok: f64,
    pub cache_read_per_mtok: f64,
}

// ── The core trait ───────────────────────────────────────────────────────────

/// Provider-agnostic LLM client.
///
/// Implemented by `AnthropicClient`, `OpenAiClient`, `GeminiClient`, etc.
/// The `QueryEngine` holds an `Arc<dyn LlmProvider>` and never knows which
/// concrete provider it talks to.
#[async_trait::async_trait]
pub trait LlmProvider: Send + Sync {
    /// Which provider family this client belongs to.
    fn kind(&self) -> ProviderKind;

    /// Runtime capabilities for a specific model.
    fn capabilities(&self, model: &str) -> ProviderCapabilities;

    /// Send a request and return an assembled response.
    ///
    /// The implementation converts `LlmRequest` to the provider's wire format,
    /// sends it (streaming or non-streaming), parses the response stream, and
    /// returns the result as an `AssembledResponse`.
    async fn send(
        &self,
        request: LlmRequest,
    ) -> Result<AssembledResponse, Box<dyn std::error::Error + Send + Sync>>;

    /// Pricing info for a model (used by `CostTracker`).
    fn pricing(&self, model: &str) -> Option<ModelPricing>;

    /// Estimate tokens for a string (provider-specific heuristic).
    ///
    /// Default: ~4 characters per token.
    fn estimate_tokens(&self, text: &str) -> u32 {
        ((text.len() as f64 / 4.0).ceil() as u32).max(1)
    }
}

// ── Stop-reason normalization ────────────────────────────────────────────────

/// Check whether the model's stop reason indicates pending tool calls.
///
/// Different providers use different strings:
///   - Anthropic: `"tool_use"`
///   - OpenAI:    `"tool_calls"`
///   - Gemini:    presence of `functionCall` parts (stop_reason may be `"STOP"`)
pub fn is_tool_use_stop_reason(reason: Option<&str>, provider: ProviderKind) -> bool {
    match provider {
        ProviderKind::Anthropic
        | ProviderKind::Bedrock
        | ProviderKind::Vertex
        | ProviderKind::Azure => reason == Some("tool_use"),

        ProviderKind::OpenAi
        | ProviderKind::DeepSeek
        | ProviderKind::Kimi
        | ProviderKind::Minimax
        | ProviderKind::OpenAiCompatible => reason == Some("tool_calls"),

        ProviderKind::Gemini => {
            reason == Some("tool_calls") || reason == Some("TOOL_CALLS")
        }
    }
}

/// Resolve the API key for a provider by checking its known env vars.
///
/// Also checks `LLM_API_KEY` as a universal fallback, and an optional custom
/// env var name from settings.
pub fn resolve_api_key(
    provider: ProviderKind,
    custom_env_var: Option<&str>,
) -> Option<String> {
    // 1. Custom env var from settings.
    if let Some(var) = custom_env_var {
        if let Ok(val) = std::env::var(var) {
            if !val.is_empty() {
                return Some(val);
            }
        }
    }
    // 2. Provider-specific env vars.
    for var in provider.api_key_env_vars() {
        if let Ok(val) = std::env::var(var) {
            if !val.is_empty() {
                return Some(val);
            }
        }
    }
    // 3. Universal fallback.
    if let Ok(val) = std::env::var("LLM_API_KEY") {
        if !val.is_empty() {
            return Some(val);
        }
    }
    None
}

/// Auto-detect the provider from available environment variables.
///
/// Returns the first provider whose API key env var is set, or `None`.
pub fn detect_provider_from_env() -> Option<ProviderKind> {
    // Check Anthropic-specific flags first (Bedrock/Vertex/Azure).
    if std::env::var("CLAUDE_CODE_USE_BEDROCK").map_or(false, |v| is_truthy(&v)) {
        return Some(ProviderKind::Bedrock);
    }
    if std::env::var("CLAUDE_CODE_USE_VERTEX").map_or(false, |v| is_truthy(&v)) {
        return Some(ProviderKind::Vertex);
    }
    if std::env::var("ANTHROPIC_FOUNDRY_RESOURCE").is_ok()
        || std::env::var("ANTHROPIC_FOUNDRY_BASE_URL").is_ok()
    {
        return Some(ProviderKind::Azure);
    }

    // Check by API key presence.
    let checks: &[(ProviderKind, &[&str])] = &[
        (ProviderKind::Anthropic, &["ANTHROPIC_API_KEY"]),
        (ProviderKind::OpenAi, &["OPENAI_API_KEY"]),
        (ProviderKind::Gemini, &["GEMINI_API_KEY", "GOOGLE_API_KEY"]),
        (ProviderKind::DeepSeek, &["DEEPSEEK_API_KEY"]),
        (ProviderKind::Kimi, &["KIMI_API_KEY", "MOONSHOT_API_KEY"]),
        (ProviderKind::Minimax, &["MINIMAX_API_KEY"]),
    ];
    for (kind, vars) in checks {
        for var in *vars {
            if std::env::var(var).is_ok() {
                return Some(*kind);
            }
        }
    }
    None
}

fn is_truthy(s: &str) -> bool {
    matches!(s.to_lowercase().as_str(), "1" | "true" | "yes" | "on")
}
