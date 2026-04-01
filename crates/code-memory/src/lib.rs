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
