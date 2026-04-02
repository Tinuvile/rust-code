//! Model name normalization, context window sizes, and provider detection.
//!
//! Ref: src/utils/model/model.ts (getCanonicalName, getDefaultSonnetModel)
//! Ref: src/utils/context.ts (getContextWindowForModel, getModelMaxOutputTokens)
//! Ref: src/utils/model/providers.ts (getAPIProvider)

// ── Default models ────────────────────────────────────────────────────────────

/// Default "main" model (Sonnet 4.6).
///
/// Ref: src/utils/model/model.ts getDefaultSonnetModel
pub const DEFAULT_MODEL: &str = "claude-sonnet-4-6";

/// Default "large" model (Opus 4.6).
///
/// Ref: src/utils/model/model.ts getDefaultOpusModel
pub const DEFAULT_LARGE_MODEL: &str = "claude-opus-4-6";

/// Small/fast model used for cheap operations (Haiku 4.5).
///
/// Ref: src/utils/model/model.ts getSmallFastModel
pub const SMALL_FAST_MODEL: &str = "claude-haiku-4-5-20251001";

// ── Context windows ───────────────────────────────────────────────────────────

/// Default context window size (200k tokens for all current models).
pub const CONTEXT_WINDOW_DEFAULT: u32 = 200_000;

/// Maximum context window for 1M-context models.
pub const CONTEXT_WINDOW_1M: u32 = 1_000_000;

/// Default max output tokens.
pub const MAX_OUTPUT_TOKENS_DEFAULT: u32 = 32_000;

/// Capped default for slot-reservation optimization.
///
/// Ref: src/utils/context.ts CAPPED_DEFAULT_MAX_TOKENS
pub const MAX_OUTPUT_TOKENS_CAPPED: u32 = 8_000;

/// Upper limit for output tokens.
pub const MAX_OUTPUT_TOKENS_UPPER: u32 = 64_000;

/// Max output tokens for compact/summary operations.
pub const COMPACT_MAX_OUTPUT_TOKENS: u32 = 20_000;

// ── Context window lookup ─────────────────────────────────────────────────────

/// Return the context window size for a given model name.
///
/// The `[1m]` suffix can be appended to any model name to request 1M context.
///
/// Ref: src/utils/context.ts getContextWindowForModel
pub fn get_context_window(model: &str) -> u32 {
    // Allow disable via env (HIPAA compliance).
    if std::env::var("CLAUDE_CODE_DISABLE_1M_CONTEXT").map_or(false, |v| is_truthy(&v)) {
        return CONTEXT_WINDOW_DEFAULT;
    }
    // Explicit [1m] suffix opt-in.
    if model.contains("[1m]") || model.contains("[1M]") {
        return CONTEXT_WINDOW_1M;
    }
    // Env override (ant-only).
    if std::env::var("USER_TYPE").as_deref() == Ok("ant") {
        if let Ok(val) = std::env::var("CLAUDE_CODE_MAX_CONTEXT_TOKENS") {
            if let Ok(n) = val.parse::<u32>() {
                if n > 0 {
                    return n;
                }
            }
        }
    }
    // Models that support 1M natively (sonnet-4.x, opus-4-6).
    let canonical = canonical_model_name(model);
    if canonical.contains("claude-sonnet-4") || canonical.contains("opus-4-6") {
        return CONTEXT_WINDOW_1M;
    }
    CONTEXT_WINDOW_DEFAULT
}

/// Return the max output token limit for a model.
///
/// Ref: src/utils/context.ts getModelMaxOutputTokens
pub fn get_max_output_tokens(model: &str) -> u32 {
    let canonical = canonical_model_name(model);
    // claude-3-5-haiku and smaller models are capped at 8k.
    if canonical.contains("haiku") {
        return MAX_OUTPUT_TOKENS_DEFAULT;
    }
    MAX_OUTPUT_TOKENS_DEFAULT
}

// ── Model name normalization ──────────────────────────────────────────────────

/// Normalize a model name to its canonical form (strip date suffixes, aliases).
///
/// Ref: src/utils/model/model.ts getCanonicalName
pub fn canonical_model_name(model: &str) -> String {
    let m = model.trim().to_lowercase();
    // Strip [1m] suffix.
    let m = m.replace("[1m]", "").replace("[1M]", "");
    // Common alias expansions.
    match m.as_str() {
        "sonnet" | "claude-sonnet" => return DEFAULT_MODEL.to_owned(),
        "opus" | "claude-opus" => return DEFAULT_LARGE_MODEL.to_owned(),
        "haiku" | "claude-haiku" => return SMALL_FAST_MODEL.to_owned(),
        _ => {}
    }
    m
}

// ── Provider detection ────────────────────────────────────────────────────────

/// Which Anthropic-compatible API provider to use.
///
/// Ref: src/utils/model/providers.ts APIProvider
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApiProvider {
    /// Direct Anthropic API (`api.anthropic.com`).
    Anthropic,
    /// AWS Bedrock.
    Bedrock,
    /// GCP Vertex AI.
    Vertex,
    /// Azure AI Foundry.
    Azure,
}

/// Detect the API provider from environment variables.
///
/// Ref: src/utils/model/providers.ts getAPIProvider
pub fn detect_provider() -> ApiProvider {
    if std::env::var("CLAUDE_CODE_USE_BEDROCK").map_or(false, |v| is_truthy(&v)) {
        return ApiProvider::Bedrock;
    }
    if std::env::var("CLAUDE_CODE_USE_VERTEX").map_or(false, |v| is_truthy(&v)) {
        return ApiProvider::Vertex;
    }
    if std::env::var("ANTHROPIC_FOUNDRY_RESOURCE").is_ok()
        || std::env::var("ANTHROPIC_FOUNDRY_BASE_URL").is_ok()
    {
        return ApiProvider::Azure;
    }
    ApiProvider::Anthropic
}

/// Return the base URL for a given provider.
pub fn provider_base_url(provider: ApiProvider) -> String {
    match provider {
        ApiProvider::Anthropic => std::env::var("ANTHROPIC_BASE_URL")
            .unwrap_or_else(|_| "https://api.anthropic.com".to_owned()),
        ApiProvider::Bedrock => {
            let region = std::env::var("AWS_DEFAULT_REGION")
                .or_else(|_| std::env::var("AWS_REGION"))
                .unwrap_or_else(|_| "us-east-1".to_owned());
            format!(
                "https://bedrock-runtime.{region}.amazonaws.com/model/{{model}}/invoke-with-response-stream"
            )
        }
        ApiProvider::Vertex => {
            let project = std::env::var("ANTHROPIC_VERTEX_PROJECT_ID").unwrap_or_default();
            let region = std::env::var("CLOUD_ML_REGION")
                .unwrap_or_else(|_| "us-east5".to_owned());
            format!("https://{region}-aiplatform.googleapis.com/v1/projects/{project}/locations/{region}/publishers/anthropic/models/{{model}}:streamRawPredict")
        }
        ApiProvider::Azure => {
            let resource = std::env::var("ANTHROPIC_FOUNDRY_RESOURCE").ok();
            let base_url = std::env::var("ANTHROPIC_FOUNDRY_BASE_URL").ok();
            if let Some(url) = base_url {
                url
            } else if let Some(res) = resource {
                format!("https://{res}.services.ai.azure.com/anthropic/v1/messages")
            } else {
                "https://api.anthropic.com".to_owned()
            }
        }
    }
}

// ── Beta headers ──────────────────────────────────────────────────────────────

/// Beta feature headers sent with API requests.
///
/// Ref: src/constants/betas.ts, src/utils/betas.ts getMergedBetas
pub struct BetaHeaders;

impl BetaHeaders {
    /// All standard beta headers for a request to a given model.
    pub fn for_model(model: &str, is_oauth_user: bool) -> Vec<String> {
        let mut betas = vec![
            "claude-code-20250219".to_owned(),
        ];
        if is_oauth_user {
            betas.push("claude-code-tokens-2025-05-07".to_owned());
        }
        // 1M context beta.
        if get_context_window(model) >= CONTEXT_WINDOW_1M {
            betas.push("extended-cache-ttl-2025-04-11".to_owned());
        }
        betas
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn is_truthy(s: &str) -> bool {
    matches!(s.to_lowercase().as_str(), "1" | "true" | "yes" | "on")
}
