//! Per-session metadata stored at `{session_dir}/meta.json`.
//!
//! Mirrors `SessionMetadata` from `code-types` with async I/O helpers.
//!
//! Ref: src/utils/sessionStorage.ts (metadata persistence)

use std::path::Path;

use anyhow::Context;
use code_types::message::SessionMetadata;

/// Path to the metadata file inside a session directory.
pub fn meta_path(session_dir: &Path) -> std::path::PathBuf {
    session_dir.join("meta.json")
}

/// Load session metadata from `{session_dir}/meta.json`.
/// Returns `None` if the file does not exist.
pub async fn load_metadata(session_dir: &Path) -> anyhow::Result<Option<SessionMetadata>> {
    let path = meta_path(session_dir);
    match tokio::fs::read_to_string(&path).await {
        Ok(s) => {
            let meta = serde_json::from_str(&s)
                .with_context(|| format!("invalid metadata JSON at {}", path.display()))?;
            Ok(Some(meta))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.into()),
    }
}

/// Persist session metadata to `{session_dir}/meta.json` (atomic write).
pub async fn save_metadata(session_dir: &Path, meta: &SessionMetadata) -> anyhow::Result<()> {
    tokio::fs::create_dir_all(session_dir).await?;
    let path = meta_path(session_dir);
    let json = serde_json::to_string_pretty(meta)?;
    tokio::fs::write(&path, json).await?;
    Ok(())
}

/// Update the `last_active_at` and `total_cost_usd` fields in the stored metadata.
pub async fn touch_metadata(
    session_dir: &Path,
    additional_cost_usd: f64,
    message_count_delta: u32,
) -> anyhow::Result<()> {
    if let Some(mut meta) = load_metadata(session_dir).await? {
        meta.last_active_at = unix_now();
        meta.total_cost_usd += additional_cost_usd;
        meta.message_count += message_count_delta;
        save_metadata(session_dir, &meta).await?;
    }
    Ok(())
}

fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
