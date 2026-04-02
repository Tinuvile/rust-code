//! Memory entry loader — reads CLAUDE.md files and memdir entries from disk.
//!
//! Ref: src/memdir/memdir.ts (loadMemory, loadUserMemory)

use std::path::Path;

use crate::memory_type::{MemoryEntry, MemorySource};
use crate::paths::{global_claude_md_path, memdir_entries, project_claude_md_paths};

/// Load all memory entries visible from `cwd`.
///
/// Order:
///   1. Global `~/.claude/CLAUDE.md`
///   2. Memdir entries from `~/.claude/memory/*.md`
///   3. Project `CLAUDE.md` files (cwd first, then parents)
///
/// Entries that cannot be read are silently skipped.
pub async fn load_memory_entries(cwd: &Path) -> Vec<MemoryEntry> {
    let mut entries = Vec::new();

    // 1. Global CLAUDE.md
    if let Some(path) = global_claude_md_path() {
        if let Ok(content) = tokio::fs::read_to_string(&path).await {
            entries.push(MemoryEntry {
                content,
                source: MemorySource::Global,
                path,
                is_claude_md: true,
            });
        }
    }

    // 2. Memdir entries (~/.claude/memory/*.md)
    for path in memdir_entries() {
        if let Ok(content) = tokio::fs::read_to_string(&path).await {
            let name = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("unknown")
                .to_owned();
            entries.push(MemoryEntry {
                content,
                source: MemorySource::Memdir { name },
                path,
                is_claude_md: false,
            });
        }
    }

    // 3. Project CLAUDE.md files
    for (path, depth) in project_claude_md_paths(cwd) {
        if let Ok(content) = tokio::fs::read_to_string(&path).await {
            entries.push(MemoryEntry {
                content,
                source: MemorySource::Project { depth },
                path,
                is_claude_md: true,
            });
        }
    }

    entries
}
