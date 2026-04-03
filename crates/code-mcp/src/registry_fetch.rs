//! Official MCP server registry fetcher.
//!
//! Fetches the list of known MCP servers from the Smithery registry.
//! Best-effort: returns an empty list on any error.  Results are cached
//! in-process for 1 hour to avoid redundant network calls.
//!
//! Ref: src/services/mcp/officialRegistry.ts

use std::sync::Mutex;
use std::time::{Duration, Instant};

use reqwest::Client;
use serde::{Deserialize, Serialize};
use tracing::{debug, trace};

// ── Types ─────────────────────────────────────────────────────────────────────

/// A single entry in the official MCP server registry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistryEntry {
    pub name: String,
    pub description: Option<String>,
    pub homepage: Option<String>,
    /// npm package name or similar install identifier.
    pub package: Option<String>,
}

// ── In-process cache ──────────────────────────────────────────────────────────

struct Cache {
    entries: Vec<RegistryEntry>,
    fetched_at: Instant,
}

static REGISTRY_CACHE: Mutex<Option<Cache>> = Mutex::new(None);

const CACHE_TTL: Duration = Duration::from_secs(3600); // 1 hour
const FETCH_TIMEOUT: Duration = Duration::from_secs(5);

// ── Public API ────────────────────────────────────────────────────────────────

/// Fetch the official MCP server registry.
///
/// Returns the cached result if it is less than 1 hour old.
/// On any network or parse error, returns an empty `Vec`.
pub async fn fetch_registry(client: &Client) -> Vec<RegistryEntry> {
    // Check in-process cache.
    if let Some(cached) = get_cached() {
        return cached;
    }

    match fetch_from_network(client).await {
        Ok(entries) => {
            store_cached(entries.clone());
            entries
        }
        Err(e) => {
            trace!("mcp registry fetch failed (best-effort): {e}");
            vec![]
        }
    }
}

// ── Private helpers ───────────────────────────────────────────────────────────

fn get_cached() -> Option<Vec<RegistryEntry>> {
    if let Ok(guard) = REGISTRY_CACHE.lock() {
        if let Some(cache) = guard.as_ref() {
            if cache.fetched_at.elapsed() < CACHE_TTL {
                debug!("mcp registry: returning cached result ({} entries)", cache.entries.len());
                return Some(cache.entries.clone());
            }
        }
    }
    None
}

fn store_cached(entries: Vec<RegistryEntry>) {
    if let Ok(mut guard) = REGISTRY_CACHE.lock() {
        *guard = Some(Cache {
            entries,
            fetched_at: Instant::now(),
        });
    }
}

async fn fetch_from_network(client: &Client) -> anyhow::Result<Vec<RegistryEntry>> {
    const REGISTRY_URL: &str = "https://registry.smithery.ai/servers";

    let resp = tokio::time::timeout(FETCH_TIMEOUT, client.get(REGISTRY_URL).send())
        .await
        .map_err(|_| anyhow::anyhow!("registry fetch timed out"))?
        .map_err(|e| anyhow::anyhow!("registry fetch error: {e}"))?;

    if !resp.status().is_success() {
        anyhow::bail!("registry fetch returned status {}", resp.status());
    }

    let entries: Vec<RegistryEntry> = resp
        .json()
        .await
        .map_err(|e| anyhow::anyhow!("registry parse error: {e}"))?;

    debug!("mcp registry: fetched {} entries", entries.len());
    Ok(entries)
}
