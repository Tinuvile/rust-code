//! Startup credential prefetch — reads from secure storage in parallel with
//! CLI argument parsing so credentials are ready the first time they're needed.
//!
//! Ref: src/utils/secureStorage/keychainPrefetch.ts

use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::secure_storage::{SecureStorage, StoredCredentials};

// ── Result types ──────────────────────────────────────────────────────────────

/// Outcome of a prefetch run.
#[derive(Debug)]
pub struct PrefetchResult {
    /// Credentials loaded from storage, or `None` on timeout / not found.
    pub credentials: Option<StoredCredentials>,
    /// Wall-clock time the prefetch took.
    pub elapsed: Duration,
}

// ── CredentialPrefetch ────────────────────────────────────────────────────────

/// A handle to a background credential prefetch task.
///
/// Call `start()` as early as possible in `main`, then `complete()` before
/// the credentials are first needed.
///
/// Ref: src/utils/secureStorage/keychainPrefetch.ts startMdmRawRead / ensureKeychainPrefetchCompleted
pub struct CredentialPrefetch {
    handle: tokio::task::JoinHandle<PrefetchResult>,
}

/// Maximum time to wait for the prefetch to complete.
const PREFETCH_TIMEOUT: Duration = Duration::from_secs(10);

impl CredentialPrefetch {
    /// Spawn the prefetch task immediately.
    ///
    /// The `storage` reference is cloned into the background task so this
    /// function returns without blocking.
    pub fn start(storage: Arc<dyn SecureStorage>) -> Self {
        let handle = tokio::spawn(async move {
            let t0 = Instant::now();
            let credentials = match tokio::time::timeout(PREFETCH_TIMEOUT, storage.read()).await {
                Ok(Ok(creds)) => creds,
                Ok(Err(e)) => {
                    tracing::debug!("credential prefetch read error: {e}");
                    None
                }
                Err(_) => {
                    tracing::debug!("credential prefetch timed out");
                    None
                }
            };
            PrefetchResult { credentials, elapsed: t0.elapsed() }
        });
        Self { handle }
    }

    /// Wait for the prefetch to complete (up to `PREFETCH_TIMEOUT`).
    ///
    /// Returns the result; the storage cache will already be warm from
    /// the background read, so subsequent `storage.read()` calls are fast.
    pub async fn complete(self) -> PrefetchResult {
        match tokio::time::timeout(PREFETCH_TIMEOUT, self.handle).await {
            Ok(Ok(result)) => result,
            Ok(Err(join_err)) => {
                tracing::warn!("credential prefetch task panicked: {join_err}");
                PrefetchResult { credentials: None, elapsed: Duration::ZERO }
            }
            Err(_) => {
                tracing::warn!("credential prefetch timed out on completion");
                PrefetchResult { credentials: None, elapsed: PREFETCH_TIMEOUT }
            }
        }
    }
}
