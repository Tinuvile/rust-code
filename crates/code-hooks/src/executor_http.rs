//! HTTP hook executor: POST the event JSON to a URL, parse the response as a HookDecision.
//!
//! Ref: src/utils/hooks/execHttpHook.ts

use std::time::Duration;

use tokio::time::timeout;

use crate::event::{HookDecision, HookEvent};

const TIMEOUT: Duration = Duration::from_secs(30);

/// Execute an HTTP hook by POSTing the serialized `event` to `url`.
///
/// Expects the response body to be a JSON `HookDecision`.  Any error
/// (network, timeout, parse failure) returns `Continue`.
pub async fn run_http_hook(url: &str, event: &HookEvent) -> HookDecision {
    let result = timeout(TIMEOUT, run_http_inner(url, event)).await;
    match result {
        Ok(Ok(decision)) => decision,
        _ => HookDecision::Continue,
    }
}

async fn run_http_inner(url: &str, event: &HookEvent) -> anyhow::Result<HookDecision> {
    let client = reqwest::Client::new();
    let response = client
        .post(url)
        .json(event)
        .send()
        .await?;

    let body = response.text().await?;
    let trimmed = body.trim();
    if trimmed.is_empty() {
        return Ok(HookDecision::Continue);
    }

    let decision: HookDecision =
        serde_json::from_str(trimmed).unwrap_or(HookDecision::Continue);
    Ok(decision)
}
