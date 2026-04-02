//! Large tool result persistence.
//!
//! When a tool result exceeds `PERSIST_THRESHOLD_CHARS`, the full content is
//! written to disk and a compact preview is returned in the message instead.
//! This prevents the model's context window from being overwhelmed by huge
//! command outputs or file contents.
//!
//! Ref: src/utils/toolResultStorage.ts

use std::path::{Path, PathBuf};

use tracing::debug;

// ── Constants ─────────────────────────────────────────────────────────────────

/// Characters above which a result is persisted to disk.
pub const PERSIST_THRESHOLD_CHARS: usize = 50_000;

/// Bytes kept in the in-message preview.
pub const PREVIEW_SIZE_BYTES: usize = 2_000;

// ── Path helpers ──────────────────────────────────────────────────────────────

/// Return the on-disk path for a persisted tool result.
///
/// `{session_dir}/tool-results/{tool_use_id}.txt`
pub fn tool_result_path(session_dir: &Path, tool_use_id: &str) -> PathBuf {
    session_dir
        .join("tool-results")
        .join(format!("{tool_use_id}.txt"))
}

// ── Core function ─────────────────────────────────────────────────────────────

/// If `content` exceeds `PERSIST_THRESHOLD_CHARS`, write the full content to
/// `{session_dir}/tool-results/{tool_use_id}.txt` and return a compact preview
/// message.
///
/// Returns `(final_content, was_truncated)`.
pub async fn maybe_persist_result(
    content: String,
    tool_use_id: &str,
    tool_name: &str,
    session_dir: &Path,
) -> (String, bool) {
    if content.chars().count() <= PERSIST_THRESHOLD_CHARS {
        return (content, false);
    }

    let path = tool_result_path(session_dir, tool_use_id);

    // Ensure the parent directory exists.
    if let Some(parent) = path.parent() {
        if let Err(e) = tokio::fs::create_dir_all(parent).await {
            // On failure, fall back to the truncated preview inline.
            debug!("result_storage: could not create dir {}: {}", parent.display(), e);
            let (preview, _) = generate_preview(&content, PREVIEW_SIZE_BYTES);
            return (
                format!("{preview}\n\n[Output truncated — could not write to disk: {e}]"),
                true,
            );
        }
    }

    let total_chars = content.chars().count();

    // Write full content to disk.
    if let Err(e) = tokio::fs::write(&path, &content).await {
        debug!("result_storage: write failed for {}: {}", path.display(), e);
        let (preview, _) = generate_preview(&content, PREVIEW_SIZE_BYTES);
        return (
            format!("{preview}\n\n[Output truncated — could not write to disk: {e}]"),
            true,
        );
    }

    let (preview, has_more) = generate_preview(&content, PREVIEW_SIZE_BYTES);

    let persisted_msg = build_persisted_message(
        tool_name,
        tool_use_id,
        &path,
        total_chars,
        &preview,
        has_more,
    );

    debug!(
        "result_storage: persisted {} chars for tool_use_id={} to {}",
        total_chars,
        tool_use_id,
        path.display()
    );

    (persisted_msg, true)
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Generate a UTF-8 safe preview truncated at the last newline within
/// `max_bytes`.
///
/// Returns `(preview, has_more)`.
pub fn generate_preview(content: &str, max_bytes: usize) -> (String, bool) {
    if content.len() <= max_bytes {
        return (content.to_owned(), false);
    }

    // Walk back from max_bytes to find a valid UTF-8 boundary.
    let mut end = max_bytes;
    while end > 0 && !content.is_char_boundary(end) {
        end -= 1;
    }

    let truncated = &content[..end];

    // Try to cut at the last newline for a cleaner preview.
    let cut = truncated.rfind('\n').unwrap_or(end);
    let preview = &content[..cut];

    (preview.to_owned(), true)
}

fn build_persisted_message(
    tool_name: &str,
    tool_use_id: &str,
    path: &Path,
    total_chars: usize,
    preview: &str,
    has_more: bool,
) -> String {
    let mut msg = String::new();
    msg.push_str(&format!(
        "<persisted-output tool=\"{tool_name}\" tool_use_id=\"{tool_use_id}\" \
         total_chars=\"{total_chars}\" path=\"{}\">\n",
        path.display()
    ));
    msg.push_str(preview);
    if has_more {
        msg.push_str("\n… [output truncated — full result saved to disk]");
    }
    msg.push_str("\n</persisted-output>");
    msg
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_content_is_unchanged() {
        let short = "hello world".to_owned();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let (out, truncated) = rt.block_on(maybe_persist_result(
            short.clone(),
            "tool-1",
            "Bash",
            Path::new("/tmp"),
        ));
        assert_eq!(out, short);
        assert!(!truncated);
    }

    #[test]
    fn preview_truncates_at_newline() {
        let content = "line1\nline2\nline3 with some more text";
        let (preview, has_more) = generate_preview(content, 15);
        assert!(preview.ends_with("line2") || preview.ends_with("line1"));
        assert!(has_more);
    }

    #[test]
    fn preview_short_content() {
        let content = "hello";
        let (preview, has_more) = generate_preview(content, 100);
        assert_eq!(preview, "hello");
        assert!(!has_more);
    }
}
