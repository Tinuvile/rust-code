//! Secure credential storage backends.
//!
//! Priority chain (highest to lowest):
//! - macOS: Keychain (via `security` CLI) → plaintext fallback
//! - Windows: `keyring` crate (Credential Manager)
//! - Linux: plaintext fallback (TODO: libsecret)
//!
//! Ref: src/utils/secureStorage/index.ts
//! Ref: src/utils/secureStorage/macOsKeychainStorage.ts
//! Ref: src/utils/secureStorage/plainTextStorage.ts
//! Ref: src/utils/secureStorage/fallbackStorage.ts

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::oauth::OAuthTokens;

// ── StoredCredentials ─────────────────────────────────────────────────────────

/// The credential payload persisted by all storage backends.
///
/// Serialized to JSON on disk / in keychain.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StoredCredentials {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub oauth_tokens: Option<OAuthTokens>,
    /// Legacy plaintext API key stored alongside OAuth tokens.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,
}

// ── WriteResult ───────────────────────────────────────────────────────────────

/// Outcome of a write operation.
#[derive(Debug, Clone)]
pub struct WriteResult {
    pub success: bool,
    /// Non-fatal warning (e.g., "Credentials stored in plaintext").
    pub warning: Option<String>,
}

impl WriteResult {
    pub fn ok() -> Self {
        Self { success: true, warning: None }
    }
    pub fn with_warning(msg: impl Into<String>) -> Self {
        Self { success: true, warning: Some(msg.into()) }
    }
}

// ── SecureStorage trait ───────────────────────────────────────────────────────

/// Abstraction over all credential storage backends.
#[async_trait::async_trait]
pub trait SecureStorage: Send + Sync {
    fn name(&self) -> &str;
    async fn read(&self) -> anyhow::Result<Option<StoredCredentials>>;
    async fn write(&self, data: &StoredCredentials) -> anyhow::Result<WriteResult>;
    async fn delete(&self) -> anyhow::Result<bool>;
}

// ── macOS Keychain ────────────────────────────────────────────────────────────

#[cfg(target_os = "macos")]
const KEYCHAIN_SERVICE_OAUTH: &str = "Code-credentials";
#[cfg(target_os = "macos")]
const KEYCHAIN_SERVICE_LEGACY: &str = "Code";
#[cfg(target_os = "macos")]
const KEYCHAIN_CACHE_TTL: Duration = Duration::from_secs(30);
/// macOS security CLI stdin limit (4096 - 64 bytes headroom).
#[cfg(target_os = "macos")]
const STDIN_PAYLOAD_LIMIT: usize = 4032;

#[cfg(target_os = "macos")]
struct KeychainCache {
    data: Option<StoredCredentials>,
    cached_at: std::time::Instant,
}

/// macOS Keychain storage using the `security` CLI subprocess.
///
/// Ref: src/utils/secureStorage/macOsKeychainStorage.ts
#[cfg(target_os = "macos")]
pub struct MacOsKeychainStorage {
    service: String,
    username: String,
    cache: Arc<tokio::sync::Mutex<Option<KeychainCache>>>,
    #[allow(dead_code)]
    in_flight: Arc<tokio::sync::Mutex<Option<tokio::task::JoinHandle<anyhow::Result<Option<StoredCredentials>>>>>>,
}

#[cfg(target_os = "macos")]
impl MacOsKeychainStorage {
    pub fn new() -> Self {
        let username = std::env::var("USER")
            .unwrap_or_else(|_| whoami_fallback());
        Self {
            service: KEYCHAIN_SERVICE_OAUTH.to_owned(),
            username,
            cache: Arc::new(tokio::sync::Mutex::new(None)),
            in_flight: Arc::new(tokio::sync::Mutex::new(None)),
        }
    }

    async fn read_from_keychain(service: &str, username: &str) -> anyhow::Result<Option<StoredCredentials>> {
        use tokio::process::Command;
        let output = Command::new("/usr/bin/security")
            .args(["find-generic-password", "-s", service, "-a", username, "-w"])
            .output()
            .await?;

        if !output.status.success() {
            return Ok(None);
        }
        let raw = String::from_utf8_lossy(&output.stdout);
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return Ok(None);
        }
        // Decode hex if the data was hex-encoded on write.
        let json = if trimmed.chars().all(|c| c.is_ascii_hexdigit()) {
            let bytes = hex::decode(trimmed).unwrap_or_else(|_| trimmed.as_bytes().to_vec());
            String::from_utf8(bytes)?
        } else {
            trimmed.to_owned()
        };
        Ok(serde_json::from_str(&json).ok())
    }

    async fn write_to_keychain(service: &str, username: &str, data: &StoredCredentials) -> anyhow::Result<()> {
        use tokio::process::Command;
        use tokio::io::AsyncWriteExt;

        let json = serde_json::to_string(data)?;
        // Hex-encode to avoid shell escaping issues and argv exposure.
        let hex_payload = hex::encode(json.as_bytes());

        if hex_payload.len() <= STDIN_PAYLOAD_LIMIT {
            // Write via stdin: `echo <hex> | security -i`
            let mut child = Command::new("/usr/bin/security")
                .arg("-i")
                .stdin(std::process::Stdio::piped())
                .spawn()?;
            let script = format!(
                "add-generic-password -U -s {service} -a {username} -w {hex_payload}\n"
            );
            if let Some(stdin) = child.stdin.as_mut() {
                stdin.write_all(script.as_bytes()).await?;
            }
            child.wait().await?;
        } else {
            // Fallback: argv (less secure but necessary for large payloads).
            Command::new("/usr/bin/security")
                .args(["add-generic-password", "-U", "-s", service, "-a", username, "-w", &hex_payload])
                .output()
                .await?;
        }
        Ok(())
    }
}

#[cfg(target_os = "macos")]
fn whoami_fallback() -> String {
    std::process::Command::new("whoami")
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
        .unwrap_or_else(|_| "user".to_owned())
}

#[cfg(target_os = "macos")]
#[async_trait::async_trait]
impl SecureStorage for MacOsKeychainStorage {
    fn name(&self) -> &str { "macos-keychain" }

    async fn read(&self) -> anyhow::Result<Option<StoredCredentials>> {
        // Serve from cache if still fresh.
        {
            let guard = self.cache.lock().await;
            if let Some(ref c) = *guard {
                if c.cached_at.elapsed() < KEYCHAIN_CACHE_TTL {
                    return Ok(c.data.clone());
                }
            }
        }

        let result = Self::read_from_keychain(&self.service, &self.username).await;

        // Update cache on success; on error, serve stale data if available.
        match result {
            Ok(data) => {
                let mut guard = self.cache.lock().await;
                *guard = Some(KeychainCache { data: data.clone(), cached_at: std::time::Instant::now() });
                Ok(data)
            }
            Err(e) => {
                let guard = self.cache.lock().await;
                if let Some(ref c) = *guard {
                    tracing::warn!("keychain read failed ({e}), serving stale cache");
                    return Ok(c.data.clone());
                }
                Err(e)
            }
        }
    }

    async fn write(&self, data: &StoredCredentials) -> anyhow::Result<WriteResult> {
        Self::write_to_keychain(&self.service, &self.username, data).await?;
        // Invalidate cache.
        let mut guard = self.cache.lock().await;
        *guard = Some(KeychainCache { data: Some(data.clone()), cached_at: std::time::Instant::now() });
        Ok(WriteResult::ok())
    }

    async fn delete(&self) -> anyhow::Result<bool> {
        let output = tokio::process::Command::new("/usr/bin/security")
            .args(["delete-generic-password", "-s", &self.service, "-a", &self.username])
            .output()
            .await?;
        let mut guard = self.cache.lock().await;
        *guard = Some(KeychainCache { data: None, cached_at: std::time::Instant::now() });
        Ok(output.status.success())
    }
}

// ── Plaintext storage ─────────────────────────────────────────────────────────

/// Credentials file path: `~/.claude/.credentials.json`.
fn credentials_path() -> Option<std::path::PathBuf> {
    dirs_next::home_dir().map(|h| h.join(".claude").join(".credentials.json"))
}

/// Plaintext JSON fallback storage.
///
/// Sets permissions to 0600 on Unix.
///
/// Ref: src/utils/secureStorage/plainTextStorage.ts
pub struct PlainTextStorage;

#[async_trait::async_trait]
impl SecureStorage for PlainTextStorage {
    fn name(&self) -> &str { "plaintext" }

    async fn read(&self) -> anyhow::Result<Option<StoredCredentials>> {
        let path = match credentials_path() {
            Some(p) => p,
            None => return Ok(None),
        };
        match tokio::fs::read(&path).await {
            Ok(bytes) => Ok(serde_json::from_slice(&bytes).ok()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    async fn write(&self, data: &StoredCredentials) -> anyhow::Result<WriteResult> {
        let path = credentials_path()
            .ok_or_else(|| anyhow::anyhow!("cannot determine home directory"))?;
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        let json = serde_json::to_string_pretty(data)?;
        tokio::fs::write(&path, &json).await?;

        // Restrict permissions on Unix.
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let perms = std::fs::Permissions::from_mode(0o600);
            tokio::fs::set_permissions(&path, perms).await.ok();
        }

        Ok(WriteResult::with_warning(
            "Warning: credentials stored in plaintext. \
             Consider using a keychain for better security.",
        ))
    }

    async fn delete(&self) -> anyhow::Result<bool> {
        let path = match credentials_path() {
            Some(p) => p,
            None => return Ok(false),
        };
        match tokio::fs::remove_file(&path).await {
            Ok(()) => Ok(true),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(e) => Err(e.into()),
        }
    }
}

// ── Windows keyring ───────────────────────────────────────────────────────────

/// Windows Credential Manager storage via the `keyring` crate.
///
/// Ref: src/utils/secureStorage/ (Windows fallback path)
#[cfg(target_os = "windows")]
pub struct WindowsKeychainStorage {
    entry: keyring::Entry,
}

#[cfg(target_os = "windows")]
impl WindowsKeychainStorage {
    pub fn new() -> anyhow::Result<Self> {
        let entry = keyring::Entry::new("Code-credentials", "claude-code")?;
        Ok(Self { entry })
    }
}

#[cfg(target_os = "windows")]
#[async_trait::async_trait]
impl SecureStorage for WindowsKeychainStorage {
    fn name(&self) -> &str { "windows-credential-manager" }

    async fn read(&self) -> anyhow::Result<Option<StoredCredentials>> {
        match self.entry.get_password() {
            Ok(s) => Ok(serde_json::from_str(&s).ok()),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(anyhow::anyhow!("keyring read: {e}")),
        }
    }

    async fn write(&self, data: &StoredCredentials) -> anyhow::Result<WriteResult> {
        let json = serde_json::to_string(data)?;
        self.entry.set_password(&json)
            .map_err(|e| anyhow::anyhow!("keyring write: {e}"))?;
        Ok(WriteResult::ok())
    }

    async fn delete(&self) -> anyhow::Result<bool> {
        match self.entry.delete_credential() {
            Ok(()) => Ok(true),
            Err(keyring::Error::NoEntry) => Ok(false),
            Err(e) => Err(anyhow::anyhow!("keyring delete: {e}")),
        }
    }
}

// ── FallbackStorage ───────────────────────────────────────────────────────────

/// Combinator: try `primary`, fall back to `secondary` on failure.
///
/// On successful primary write from null state, deletes secondary (migration).
/// On primary write failure with secondary success, removes stale primary entry.
///
/// Ref: src/utils/secureStorage/fallbackStorage.ts
pub struct FallbackStorage {
    primary: Arc<dyn SecureStorage>,
    secondary: Arc<dyn SecureStorage>,
}

impl FallbackStorage {
    pub fn new(primary: Arc<dyn SecureStorage>, secondary: Arc<dyn SecureStorage>) -> Self {
        Self { primary, secondary }
    }
}

#[async_trait::async_trait]
impl SecureStorage for FallbackStorage {
    fn name(&self) -> &str { "fallback" }

    async fn read(&self) -> anyhow::Result<Option<StoredCredentials>> {
        match self.primary.read().await {
            Ok(Some(data)) => return Ok(Some(data)),
            Ok(None) => {}
            Err(e) => {
                tracing::warn!("primary storage read failed: {e}, trying secondary");
            }
        }
        self.secondary.read().await
    }

    async fn write(&self, data: &StoredCredentials) -> anyhow::Result<WriteResult> {
        let was_empty = matches!(self.primary.read().await, Ok(None));

        match self.primary.write(data).await {
            Ok(result) => {
                if was_empty {
                    // Migration: primary now has data, clean up secondary.
                    self.secondary.delete().await.ok();
                }
                Ok(result)
            }
            Err(primary_err) => {
                tracing::warn!("primary write failed ({primary_err}), falling back to secondary");
                let result = self.secondary.write(data).await?;
                // Remove stale entry from primary to prevent shadowing.
                self.primary.delete().await.ok();
                Ok(result)
            }
        }
    }

    async fn delete(&self) -> anyhow::Result<bool> {
        let p = self.primary.delete().await.unwrap_or(false);
        let s = self.secondary.delete().await.unwrap_or(false);
        Ok(p || s)
    }
}

// ── Platform selector ─────────────────────────────────────────────────────────

/// Create the platform-appropriate `SecureStorage` implementation.
///
/// - macOS: `FallbackStorage(MacOsKeychainStorage, PlainTextStorage)`
/// - Windows: `FallbackStorage(WindowsKeychainStorage, PlainTextStorage)`
/// - Linux: `PlainTextStorage`
pub fn create_secure_storage() -> Arc<dyn SecureStorage> {
    #[cfg(target_os = "macos")]
    {
        let primary: Arc<dyn SecureStorage> = Arc::new(MacOsKeychainStorage::new());
        let secondary: Arc<dyn SecureStorage> = Arc::new(PlainTextStorage);
        Arc::new(FallbackStorage::new(primary, secondary))
    }

    #[cfg(target_os = "windows")]
    {
        match WindowsKeychainStorage::new() {
            Ok(win) => {
                let primary: Arc<dyn SecureStorage> = Arc::new(win);
                let secondary: Arc<dyn SecureStorage> = Arc::new(PlainTextStorage);
                Arc::new(FallbackStorage::new(primary, secondary))
            }
            Err(e) => {
                tracing::warn!("Windows keyring unavailable ({e}), using plaintext storage");
                Arc::new(PlainTextStorage)
            }
        }
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        Arc::new(PlainTextStorage)
    }
}
