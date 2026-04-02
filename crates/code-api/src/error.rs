//! API error types and classification.
//!
//! Ref: src/services/api/errors.ts
//! Ref: src/services/api/withRetry.ts (CannotRetryError, FallbackTriggeredError)

use thiserror::Error;

// ── ApiError ──────────────────────────────────────────────────────────────────

/// HTTP status + body from a failed API call.
#[derive(Debug, Clone)]
pub struct ApiError {
    pub status: u16,
    pub message: String,
    /// Raw body for debugging.
    pub body: Option<String>,
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "API error {}: {}", self.status, self.message)
    }
}

impl std::error::Error for ApiError {}

// ── Error classification ──────────────────────────────────────────────────────

/// How an error should be handled by the retry loop.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorClass {
    /// Can be retried with exponential backoff (network, 500, 529).
    Retryable,
    /// Rate limit — retry after delay (429).
    RateLimit,
    /// Overloaded — retry limited times (529).
    Overloaded,
    /// Cannot be retried (4xx except 429, auth errors, etc.).
    Fatal,
}

/// Classify an HTTP status code for the retry loop.
///
/// Ref: src/services/api/withRetry.ts (is529Error, shouldRetry logic)
pub fn classify_status(status: u16) -> ErrorClass {
    match status {
        429 => ErrorClass::RateLimit,
        529 => ErrorClass::Overloaded,
        500 | 502 | 503 | 504 => ErrorClass::Retryable,
        _ => ErrorClass::Fatal,
    }
}

/// `true` if this `reqwest::Error` represents a transient network failure.
pub fn is_network_error(e: &reqwest::Error) -> bool {
    e.is_connect() || e.is_timeout() || e.is_request()
}

// ── Prompt-too-long helpers ───────────────────────────────────────────────────

pub const PROMPT_TOO_LONG_ERROR_MESSAGE: &str = "Prompt is too long";
pub const REPEATED_529_ERROR_MESSAGE: &str =
    "Claude is overloaded. Please wait a moment and try again.";

/// Parse `actual_tokens` and `limit_tokens` from a PTL error message.
///
/// Matches: "prompt is too long: 137500 tokens > 135000 maximum"
///
/// Ref: src/services/api/errors.ts parsePromptTooLongTokenCounts
pub fn parse_prompt_too_long(raw: &str) -> (Option<u32>, Option<u32>) {
    let re = regex::Regex::new(
        r"(?i)prompt is too long[^0-9]*(\d+)\s*tokens?\s*>\s*(\d+)",
    )
    .expect("static regex");
    if let Some(caps) = re.captures(raw) {
        let actual = caps.get(1).and_then(|m| m.as_str().parse().ok());
        let limit = caps.get(2).and_then(|m| m.as_str().parse().ok());
        return (actual, limit);
    }
    (None, None)
}

/// `true` if `msg` starts with the API error prefix.
pub fn starts_with_api_error_prefix(msg: &str) -> bool {
    msg.starts_with("API Error") || msg.starts_with("Please run /login · API Error")
}

// ── Retry-level error types ───────────────────────────────────────────────────

/// Returned when all retry attempts have been exhausted.
///
/// Ref: src/services/api/withRetry.ts CannotRetryError
#[derive(Debug, Error)]
#[error("cannot retry after {attempts} attempts: {source}")]
pub struct CannotRetryError {
    pub attempts: u32,
    pub model: String,
    #[source]
    pub source: anyhow::Error,
}

/// Returned when the retry logic decides to switch to a fallback model.
///
/// Ref: src/services/api/withRetry.ts FallbackTriggeredError
#[derive(Debug, Error)]
#[error("model fallback triggered: {original_model} -> {fallback_model}")]
pub struct FallbackTriggeredError {
    pub original_model: String,
    pub fallback_model: String,
}
