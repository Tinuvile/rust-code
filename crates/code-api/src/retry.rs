//! Retry loop with exponential backoff.
//!
//! Retry classification:
//! - 429 (rate-limit): backoff proportional to `Retry-After` header
//! - 529 (overloaded): up to MAX_529_RETRIES, then fall through
//! - 5xx / network: exponential backoff up to MAX_RETRIES
//! - 4xx (other): fatal, do not retry
//!
//! Ref: src/services/api/withRetry.ts

use std::time::Duration;

use crate::error::{classify_status, is_network_error, CannotRetryError, ErrorClass};

// ── Constants ─────────────────────────────────────────────────────────────────

/// Base delay between retries.
///
/// Ref: src/services/api/withRetry.ts BASE_DELAY_MS
pub const BASE_DELAY_MS: u64 = 500;

/// Maximum number of retry attempts.
///
/// Ref: src/services/api/withRetry.ts DEFAULT_MAX_RETRIES
pub const MAX_RETRIES: u32 = 10;

/// Maximum consecutive 529 (overload) retries before giving up.
///
/// Ref: src/services/api/withRetry.ts MAX_529_RETRIES
pub const MAX_529_RETRIES: u32 = 3;

/// Absolute cap on retry delay.
const MAX_BACKOFF_MS: u64 = 60_000;

// ── Retry policy ──────────────────────────────────────────────────────────────

/// Controls retry behavior for a specific request.
#[derive(Debug, Clone)]
pub struct RetryPolicy {
    pub max_retries: u32,
    pub max_529_retries: u32,
    pub model: String,
    pub fallback_model: Option<String>,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_retries: MAX_RETRIES,
            max_529_retries: MAX_529_RETRIES,
            model: crate::model::DEFAULT_MODEL.to_owned(),
            fallback_model: None,
        }
    }
}

// ── Backoff calculation ───────────────────────────────────────────────────────

/// Calculate the delay before the next retry attempt.
///
/// Uses exponential backoff with jitter: `BASE * 2^attempt * (0.5..1.5)`.
///
/// Ref: src/services/api/withRetry.ts (delay logic)
pub fn backoff_delay(attempt: u32) -> Duration {
    let base = BASE_DELAY_MS * (1u64 << attempt.min(10));
    let capped = base.min(MAX_BACKOFF_MS);
    // Add ±25% jitter.
    let jitter = (rand::random::<f64>() * 0.5 + 0.75) * capped as f64;
    Duration::from_millis(jitter as u64)
}

/// Parse the `Retry-After` header value (seconds or HTTP date) and return
/// the delay duration.  Falls back to `backoff_delay(attempt)` on failure.
pub fn parse_retry_after(header_value: &str, attempt: u32) -> Duration {
    if let Ok(secs) = header_value.trim().parse::<u64>() {
        return Duration::from_secs(secs.min(300));
    }
    backoff_delay(attempt)
}

// ── Retry loop ────────────────────────────────────────────────────────────────

/// Outcome of a single retry attempt.
pub enum AttemptOutcome<T> {
    Success(T),
    ShouldRetry { delay: Duration, reason: String },
    Fatal(anyhow::Error),
    FallbackModel(String),
}

/// Classify a `reqwest` error or an `ApiError` and decide what to do next.
pub fn classify_error(
    error: &anyhow::Error,
    attempt: u32,
    consecutive_529: u32,
    policy: &RetryPolicy,
) -> AttemptOutcome<()> {
    // Check if it's an HTTP API error.
    if let Some(api_err) = error.downcast_ref::<crate::error::ApiError>() {
        let class = classify_status(api_err.status);
        match class {
            ErrorClass::Fatal => return AttemptOutcome::Fatal(anyhow::anyhow!("{}", api_err)),
            ErrorClass::RateLimit => {
                if attempt >= policy.max_retries {
                    return AttemptOutcome::Fatal(anyhow::anyhow!("rate limit exceeded after {attempt} retries"));
                }
                return AttemptOutcome::ShouldRetry {
                    delay: backoff_delay(attempt),
                    reason: format!("429 rate limit (attempt {attempt})"),
                };
            }
            ErrorClass::Overloaded => {
                if consecutive_529 >= policy.max_529_retries {
                    // Switch to fallback model if configured.
                    if let Some(fb) = &policy.fallback_model {
                        return AttemptOutcome::FallbackModel(fb.clone());
                    }
                    return AttemptOutcome::Fatal(anyhow::anyhow!(
                        "API overloaded after {consecutive_529} attempts"
                    ));
                }
                return AttemptOutcome::ShouldRetry {
                    delay: backoff_delay(attempt),
                    reason: format!("529 overloaded (attempt {attempt})"),
                };
            }
            ErrorClass::Retryable => {
                if attempt >= policy.max_retries {
                    return AttemptOutcome::Fatal(anyhow::anyhow!("{}", api_err));
                }
                return AttemptOutcome::ShouldRetry {
                    delay: backoff_delay(attempt),
                    reason: format!("{} server error (attempt {attempt})", api_err.status),
                };
            }
        }
    }

    // Check if it's a network error.
    if let Some(req_err) = error.downcast_ref::<reqwest::Error>() {
        if is_network_error(req_err) && attempt < policy.max_retries {
            return AttemptOutcome::ShouldRetry {
                delay: backoff_delay(attempt),
                reason: format!("network error (attempt {attempt}): {req_err}"),
            };
        }
    }

    AttemptOutcome::Fatal(anyhow::anyhow!("{error}"))
}

/// Execute an async operation with retry logic.
///
/// The closure receives the current model name (may change on fallback).
/// Yields retry notifications via `on_retry` callback.
///
/// Ref: src/services/api/withRetry.ts withRetry
pub async fn with_retry<F, Fut, T>(
    policy: RetryPolicy,
    mut on_retry: impl FnMut(String),
    mut op: F,
) -> Result<T, CannotRetryError>
where
    F: FnMut(String, u32) -> Fut,
    Fut: std::future::Future<Output = anyhow::Result<T>>,
{
    let mut attempt = 0u32;
    let mut consecutive_529 = 0u32;
    let mut current_model = policy.model.clone();

    loop {
        match op(current_model.clone(), attempt).await {
            Ok(result) => return Ok(result),
            Err(e) => {
                // Track consecutive 529s.
                if let Some(api_err) = e.downcast_ref::<crate::error::ApiError>() {
                    if api_err.status == 529 {
                        consecutive_529 += 1;
                    } else {
                        consecutive_529 = 0;
                    }
                }

                match classify_error(&e, attempt, consecutive_529, &policy) {
                    AttemptOutcome::Success(_) => unreachable!(),
                    AttemptOutcome::ShouldRetry { delay, reason } => {
                        tracing::debug!("retrying: {reason}");
                        on_retry(reason);
                        tokio::time::sleep(delay).await;
                        attempt += 1;
                    }
                    AttemptOutcome::FallbackModel(fallback) => {
                        tracing::debug!("switching to fallback model: {fallback}");
                        on_retry(format!("switching to {fallback}"));
                        current_model = fallback;
                        consecutive_529 = 0;
                        attempt += 1;
                    }
                    AttemptOutcome::Fatal(err) => {
                        return Err(CannotRetryError {
                            attempts: attempt,
                            model: current_model,
                            source: err,
                        });
                    }
                }
            }
        }
    }
}
