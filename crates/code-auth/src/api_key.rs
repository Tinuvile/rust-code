//! API key management: resolution, caching, and file-descriptor reads.
//!
//! Resolution priority (highest to lowest):
//!   1. `ANTHROPIC_API_KEY` environment variable
//!   2. `apiKeyHelper` external command (cached 5 min)
//!   3. File descriptor (`CLAUDE_CODE_API_KEY_FILE_DESCRIPTOR`)
//!   4. Well-known CCR file (`~/.claude/remote/.api_key`)
//!   5. Key stored in `SecureStorage`
//!
//! Ref: src/utils/auth.ts (getAnthropicApiKey, getApiKeyFromApiKeyHelper)
//! Ref: src/utils/authFileDescriptor.ts

use std::sync::OnceLock;
use std::time::{Duration, Instant};

use tokio::sync::Mutex;

use code_config::{global::GlobalConfig, settings::SettingsJson};

use crate::oauth::OAuthTokens;
use crate::secure_storage::SecureStorage;

// ── ApiKeySource ──────────────────────────────────────────────────────────────

/// Where a resolved API key came from.
///
/// Ref: src/utils/auth.ts ApiKeySource
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApiKeySource {
    /// `ANTHROPIC_API_KEY` environment variable.
    EnvVar,
    /// Output of the `apiKeyHelper` external command.
    ApiKeyHelper,
    /// Derived from an OAuth access token.
    OAuthDerived,
    /// Stored in `SecureStorage` (keychain / plaintext credentials file).
    StoredPrimary,
    /// Read from a file descriptor (CCR / remote environment).
    FileDescriptor,
}

/// A resolved API key together with its origin.
#[derive(Debug, Clone)]
pub struct ApiKeyWithSource {
    pub key: String,
    pub source: ApiKeySource,
}

// ── TTL cache for apiKeyHelper ────────────────────────────────────────────────

/// Default TTL for API keys obtained from an external helper command.
///
/// Ref: src/utils/auth.ts DEFAULT_API_KEY_HELPER_TTL = 5 minutes
const API_KEY_HELPER_TTL: Duration = Duration::from_secs(5 * 60);

struct CachedApiKey {
    value: String,
    cached_at: Instant,
}

static API_KEY_HELPER_CACHE: OnceLock<Mutex<Option<CachedApiKey>>> = OnceLock::new();

fn helper_cache() -> &'static Mutex<Option<CachedApiKey>> {
    API_KEY_HELPER_CACHE.get_or_init(|| Mutex::new(None))
}

// ── apiKeyHelper ──────────────────────────────────────────────────────────────

/// Execute the `apiKeyHelper` external command and return its trimmed stdout.
/// Results are cached for `API_KEY_HELPER_TTL`.
///
/// Ref: src/utils/auth.ts getApiKeyFromApiKeyHelper / getApiKeyFromApiKeyHelperCached
pub async fn get_api_key_from_helper(helper_cmd: &str) -> anyhow::Result<String> {
    // Check cache first.
    {
        let guard = helper_cache().lock().await;
        if let Some(ref cached) = *guard {
            if cached.cached_at.elapsed() < API_KEY_HELPER_TTL {
                return Ok(cached.value.clone());
            }
        }
    }

    // Spawn the helper command via the shell.
    let output = tokio::process::Command::new("sh")
        .arg("-c")
        .arg(helper_cmd)
        .output()
        .await?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow::anyhow!(
            "apiKeyHelper failed (exit {:?}): {stderr}",
            output.status.code()
        ));
    }

    let key = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    if key.is_empty() {
        return Err(anyhow::anyhow!("apiKeyHelper returned empty output"));
    }

    // Update cache.
    {
        let mut guard = helper_cache().lock().await;
        *guard = Some(CachedApiKey { value: key.clone(), cached_at: Instant::now() });
    }

    Ok(key)
}

// ── File descriptor reads (CCR / remote environments) ────────────────────────

/// Well-known paths for CCR (remote) credential files.
const CCR_API_KEY_PATH: &str = "/home/claude/.claude/remote/.api_key";
const CCR_OAUTH_TOKEN_PATH: &str = "/home/claude/.claude/remote/.oauth_token";

/// Resolve the path for a file descriptor number.
///
/// Ref: src/utils/authFileDescriptor.ts
fn fd_path(fd: u32) -> String {
    #[cfg(target_os = "linux")]
    return format!("/proc/self/fd/{fd}");
    #[cfg(not(target_os = "linux"))]
    return format!("/dev/fd/{fd}");
}

/// Try to read an API key from a file descriptor or the well-known CCR path.
///
/// Ref: src/utils/authFileDescriptor.ts getApiKeyFromFileDescriptor
pub async fn get_api_key_from_fd() -> Option<String> {
    // Try env-var-specified FD first.
    if let Ok(fd_str) = std::env::var("CLAUDE_CODE_API_KEY_FILE_DESCRIPTOR") {
        if let Ok(fd) = fd_str.parse::<u32>() {
            if let Ok(contents) = tokio::fs::read_to_string(fd_path(fd)).await {
                let key = contents.trim().to_owned();
                if !key.is_empty() {
                    return Some(key);
                }
            }
        }
    }
    // Fallback to well-known CCR path.
    tokio::fs::read_to_string(CCR_API_KEY_PATH)
        .await
        .ok()
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty())
}

/// Try to read an OAuth token from a file descriptor or the well-known CCR path.
///
/// Ref: src/utils/authFileDescriptor.ts getOAuthTokenFromFileDescriptor
pub async fn get_oauth_token_from_fd() -> Option<OAuthTokens> {
    let json = if let Ok(fd_str) = std::env::var("CLAUDE_CODE_OAUTH_TOKEN_FILE_DESCRIPTOR") {
        if let Ok(fd) = fd_str.parse::<u32>() {
            tokio::fs::read_to_string(fd_path(fd)).await.ok()
        } else {
            None
        }
    } else {
        None
    };

    let json = match json {
        Some(j) if !j.trim().is_empty() => j,
        _ => tokio::fs::read_to_string(CCR_OAUTH_TOKEN_PATH).await.ok()?,
    };

    serde_json::from_str(json.trim()).ok()
}

// ── Primary resolution function ───────────────────────────────────────────────

/// Resolve the best available API key using the full priority chain.
///
/// Returns `None` if no key is configured.
///
/// Ref: src/utils/auth.ts getAnthropicApiKeyWithSource
pub async fn get_api_key(
    config: &GlobalConfig,
    settings: &SettingsJson,
    storage: &dyn SecureStorage,
) -> Option<ApiKeyWithSource> {
    // 1. Environment variable (highest priority).
    if let Ok(key) = std::env::var("ANTHROPIC_API_KEY") {
        if !key.is_empty() {
            return Some(ApiKeyWithSource { key, source: ApiKeySource::EnvVar });
        }
    }

    // 2. apiKeyHelper command (from settings, with 5-min cache).
    let helper_cmd = settings
        .api_key_helper
        .as_deref()
        .or(config.env.get("apiKeyHelper").map(String::as_str));

    if let Some(cmd) = helper_cmd {
        if let Ok(key) = get_api_key_from_helper(cmd).await {
            return Some(ApiKeyWithSource { key, source: ApiKeySource::ApiKeyHelper });
        }
    }

    // 3. File descriptor / well-known CCR path.
    if let Some(key) = get_api_key_from_fd().await {
        return Some(ApiKeyWithSource { key, source: ApiKeySource::FileDescriptor });
    }

    // 4. Stored in secure storage.
    if let Ok(Some(creds)) = storage.read().await {
        if let Some(key) = creds.api_key {
            if !key.is_empty() {
                return Some(ApiKeyWithSource { key, source: ApiKeySource::StoredPrimary });
            }
        }
    }

    None
}
