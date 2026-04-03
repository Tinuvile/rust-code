//! Memory system: MEMORY.md loading, memdir scanning, auto-extraction.
//!
//! Ref: src/memdir/memdir.ts, src/memdir/memoryTypes.ts,
//!      src/memdir/findRelevantMemories.ts, src/memdir/memoryScan.ts

pub mod memory_type;
pub mod loader;
pub mod scanner;
pub mod relevance;
pub mod paths;
pub mod prompt;
pub mod auto_extract;

#[cfg(feature = "teammem")]
pub mod team;

// ── Re-exports ────────────────────────────────────────────────────────────────

pub use memory_type::{MemoryEntry, MemorySource};
pub use loader::load_memory_entries;
pub use scanner::{MemoryFrontmatter, ScannedMemoryEntry, parse_frontmatter, scan_entries};
pub use relevance::{ScoredEntry, rank_entries, score_entry};
pub use prompt::{format_memory_for_prompt, MAX_BYTES, MAX_LINES};
pub use auto_extract::{AutoExtractHandle, trigger_auto_extract};
