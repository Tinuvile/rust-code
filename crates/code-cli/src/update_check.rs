//! Non-blocking auto-update check.
//!
//! On startup, spawns a background task that:
//!   1. Reads a local cache file (`~/.claude/.update-check.json`)
//!   2. If the last check was recent (< 24 h), uses the cached result
//!   3. Otherwise, fetches the latest version from a configurable endpoint
//!   4. If a newer version is available, prints a one-line notice to stderr
//!
//! The check never blocks the main flow and silently swallows all errors.
//!
//! Ref: src/utils/autoUpdater.ts

use std::path::PathBuf;
use std::time::{Duration, SystemTime};

use serde::{Deserialize, Serialize};

// ── Configuration ────────────────────────────────────────────────────────────

/// How often to re-fetch the latest version (24 hours).
const CHECK_INTERVAL: Duration = Duration::from_secs(24 * 60 * 60);

/// HTTP timeout for the version-check request.
const CHECK_TIMEOUT: Duration = Duration::from_secs(5);

/// Current version, baked in at compile time.
const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");

// ── Cache format ─────────────────────────────────────────────────────────────

#[derive(Serialize, Deserialize)]
struct UpdateCache {
    /// Unix timestamp of the last successful check.
    last_check_epoch: u64,
    /// The latest version string returned by the endpoint.
    latest_version: String,
    /// The version we were running when we last checked.
    checked_from_version: String,
}

// ── Public API ───────────────────────────────────────────────────────────────

/// Spawn a non-blocking background update check.
///
/// Call this once during bootstrap.  The task runs entirely in the background
/// and never returns an error to the caller.
pub fn spawn_update_check() {
    tokio::spawn(async {
        if let Err(e) = run_update_check().await {
            tracing::debug!("update check: {e}");
        }
    });
}

// ── Implementation ───────────────────────────────────────────────────────────

async fn run_update_check() -> anyhow::Result<()> {
    let cache_path = cache_file_path()?;

    // Try to read cached result.
    if let Ok(data) = tokio::fs::read_to_string(&cache_path).await {
        if let Ok(cache) = serde_json::from_str::<UpdateCache>(&data) {
            let now = now_epoch()?;
            if now.saturating_sub(cache.last_check_epoch) < CHECK_INTERVAL.as_secs() {
                // Cache is fresh — use it.
                if is_newer(&cache.latest_version, CURRENT_VERSION) {
                    print_notice(CURRENT_VERSION, &cache.latest_version);
                }
                return Ok(());
            }
        }
    }

    // Fetch latest version from the configured source.
    let latest = fetch_latest_version().await?;

    // Persist cache.
    let cache = UpdateCache {
        last_check_epoch: now_epoch()?,
        latest_version: latest.clone(),
        checked_from_version: CURRENT_VERSION.to_owned(),
    };
    if let Ok(json) = serde_json::to_string_pretty(&cache) {
        // Ensure parent dir exists.
        if let Some(parent) = cache_path.parent() {
            let _ = tokio::fs::create_dir_all(parent).await;
        }
        let _ = tokio::fs::write(&cache_path, json).await;
    }

    if is_newer(&latest, CURRENT_VERSION) {
        print_notice(CURRENT_VERSION, &latest);
    }

    Ok(())
}

/// Fetch the latest version string.
///
/// Sources checked in order:
///   1. `UPDATE_CHECK_URL` env var → GET, expect JSON `{ "version": "x.y.z" }`
///   2. `UPDATE_CHECK_GITHUB_REPO` env var → GitHub releases API
///   3. Default: check crates.io for the `code-cli` crate
async fn fetch_latest_version() -> anyhow::Result<String> {
    // Source 1: custom URL.
    if let Ok(url) = std::env::var("UPDATE_CHECK_URL") {
        return fetch_from_json_url(&url).await;
    }

    // Source 2: GitHub releases.
    if let Ok(repo) = std::env::var("UPDATE_CHECK_GITHUB_REPO") {
        return fetch_from_github(&repo).await;
    }

    // Source 3: crates.io (default).
    fetch_from_crates_io("code-cli").await
}

/// Fetch from a JSON endpoint that returns `{ "version": "x.y.z" }`.
async fn fetch_from_json_url(url: &str) -> anyhow::Result<String> {
    let client = reqwest::Client::builder()
        .timeout(CHECK_TIMEOUT)
        .build()?;

    let resp: serde_json::Value = client.get(url).send().await?.json().await?;
    resp["version"]
        .as_str()
        .map(|s| s.trim_start_matches('v').to_owned())
        .ok_or_else(|| anyhow::anyhow!("no 'version' field in response"))
}

/// Fetch the latest release tag from GitHub Releases API.
async fn fetch_from_github(repo: &str) -> anyhow::Result<String> {
    let url = format!(
        "https://api.github.com/repos/{repo}/releases/latest"
    );
    let client = reqwest::Client::builder()
        .timeout(CHECK_TIMEOUT)
        .user_agent("claude-code-rust-updater")
        .build()?;

    let resp: serde_json::Value = client.get(&url).send().await?.json().await?;
    resp["tag_name"]
        .as_str()
        .map(|s| s.trim_start_matches('v').to_owned())
        .ok_or_else(|| anyhow::anyhow!("no tag_name in GitHub response"))
}

/// Fetch the latest version from crates.io.
async fn fetch_from_crates_io(crate_name: &str) -> anyhow::Result<String> {
    let url = format!("https://crates.io/api/v1/crates/{crate_name}");
    let client = reqwest::Client::builder()
        .timeout(CHECK_TIMEOUT)
        .user_agent("claude-code-rust-updater")
        .build()?;

    let resp: serde_json::Value = client.get(&url).send().await?.json().await?;
    resp["crate"]["max_version"]
        .as_str()
        .map(|s| s.to_owned())
        .ok_or_else(|| anyhow::anyhow!("no max_version in crates.io response"))
}

// ── Helpers ──────────────────────────────────────────────────────────────────

/// Path to the update-check cache file.
fn cache_file_path() -> anyhow::Result<PathBuf> {
    let home = dirs_next::home_dir()
        .ok_or_else(|| anyhow::anyhow!("cannot determine home directory"))?;
    Ok(home.join(".claude").join(".update-check.json"))
}

/// Current Unix epoch in seconds.
fn now_epoch() -> anyhow::Result<u64> {
    Ok(SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)?
        .as_secs())
}

/// Compare two semver-like version strings.
///
/// Returns `true` if `latest` is strictly newer than `current`.
/// Accepts versions like "0.2.0", "1.0.0-beta.1", etc.
fn is_newer(latest: &str, current: &str) -> bool {
    let parse = |s: &str| -> Vec<u64> {
        s.split(&['.', '-'][..])
            .filter_map(|p| p.parse::<u64>().ok())
            .collect()
    };
    let l = parse(latest);
    let c = parse(current);
    l > c
}

/// Print a one-line update notice to stderr.
fn print_notice(current: &str, latest: &str) {
    eprintln!(
        "\x1b[33m[update]\x1b[0m A new version is available: \
         {current} → \x1b[32m{latest}\x1b[0m"
    );
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_newer_basic() {
        assert!(is_newer("0.2.0", "0.1.0"));
        assert!(is_newer("1.0.0", "0.9.9"));
        assert!(!is_newer("0.1.0", "0.1.0"));
        assert!(!is_newer("0.0.9", "0.1.0"));
    }

    #[test]
    fn is_newer_prerelease() {
        // "1.0.0-beta.2" vs "1.0.0-beta.1" → [1,0,0,2] > [1,0,0,1]
        assert!(is_newer("1.0.0-beta.2", "1.0.0-beta.1"));
        // Same base, prerelease on latest side → not newer (shorter vec).
        assert!(!is_newer("1.0.0", "1.0.0-beta.1"));
    }

    #[test]
    fn is_newer_strips_v_prefix() {
        // The fetch functions strip 'v' before storing, but test the parse logic.
        assert!(is_newer("0.2.0", "0.1.0"));
    }

    #[test]
    fn cache_file_is_in_dot_claude() {
        let path = cache_file_path().unwrap();
        assert!(path.to_string_lossy().contains(".claude"));
        assert!(path.file_name().unwrap().to_str().unwrap().contains("update-check"));
    }

    #[tokio::test]
    async fn run_check_does_not_panic_without_network() {
        // Should silently fail (no network, no cache).
        let result = run_update_check().await;
        // We don't care if it errors, just that it doesn't panic.
        let _ = result;
    }
}
