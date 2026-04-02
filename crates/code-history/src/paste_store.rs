//! Large-content paste store.
//!
//! When the user pastes a large block of text (images, big code snippets),
//! the content is stored on disk keyed by a SHA-256 hash, and only the hash
//! is held in memory.  This keeps the message list lean.
//!
//! Files are stored under `{session_dir}/pastes/{hash_prefix}/{hash}.txt`.
//!
//! Ref: src/utils/pasteStore.ts

use std::path::Path;

use sha2::{Digest, Sha256};

/// Minimum size (bytes) at which content is stored rather than inlined.
pub const PASTE_THRESHOLD_BYTES: usize = 4_096;

/// Store `content` in the paste store and return its hex-encoded SHA-256 hash.
pub async fn store_paste(content: &str, session_dir: &Path) -> anyhow::Result<String> {
    let hash = sha256_hex(content);
    let path = paste_path(session_dir, &hash);
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    // Only write if not already present (idempotent).
    if !path.exists() {
        tokio::fs::write(&path, content).await?;
    }
    Ok(hash)
}

/// Retrieve content from the paste store by hash.
/// Returns `None` if the hash is not found.
pub async fn load_paste(hash: &str, session_dir: &Path) -> anyhow::Result<Option<String>> {
    let path = paste_path(session_dir, hash);
    match tokio::fs::read_to_string(&path).await {
        Ok(s) => Ok(Some(s)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.into()),
    }
}

/// Delete a specific paste entry.
pub async fn delete_paste(hash: &str, session_dir: &Path) -> anyhow::Result<()> {
    let path = paste_path(session_dir, hash);
    match tokio::fs::remove_file(&path).await {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e.into()),
    }
}

/// Returns `true` if the content is large enough to warrant paste storage.
pub fn should_store(content: &str) -> bool {
    content.len() > PASTE_THRESHOLD_BYTES
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn sha256_hex(content: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    hex::encode(hasher.finalize())
}

fn paste_path(session_dir: &Path, hash: &str) -> std::path::PathBuf {
    // Use first 2 chars as subdirectory to avoid too many files in one dir.
    let prefix = if hash.len() >= 2 { &hash[..2] } else { "xx" };
    session_dir
        .join("pastes")
        .join(prefix)
        .join(format!("{hash}.txt"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn store_and_load_roundtrip() {
        let dir = tempdir();
        let content = "x".repeat(PASTE_THRESHOLD_BYTES + 1);
        let hash = store_paste(&content, &dir).await.unwrap();
        assert_eq!(hash.len(), 64); // SHA-256 hex = 64 chars
        let loaded = load_paste(&hash, &dir).await.unwrap();
        assert_eq!(loaded.as_deref(), Some(content.as_str()));
    }

    #[tokio::test]
    async fn load_missing_returns_none() {
        let dir = tempdir();
        let result = load_paste("aabbcc", &dir).await.unwrap();
        assert!(result.is_none());
    }

    fn tempdir() -> std::path::PathBuf {
        let p = std::env::temp_dir()
            .join(format!("paste-test-{}", uuid::Uuid::new_v4().simple()));
        std::fs::create_dir_all(&p).unwrap();
        p
    }
}
