//! Memory entry types — a single CLAUDE.md / memory file's contents.
//!
//! Ref: src/memdir/memoryTypes.ts

use std::path::PathBuf;

/// Where a memory entry originated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MemorySource {
    /// `~/.claude/CLAUDE.md` — user-level global memory.
    Global,
    /// A `CLAUDE.md` found in the project directory or a parent directory.
    /// `depth` = 0 means the current working directory; 1 = parent, etc.
    Project { depth: usize },
    /// `~/.claude/memory/*.md` — named memory entries (memdir).
    Memdir { name: String },
}

/// A single loaded memory entry.
#[derive(Debug, Clone)]
pub struct MemoryEntry {
    /// The raw content of the file.
    pub content: String,
    /// Where this entry came from.
    pub source: MemorySource,
    /// Absolute path on disk.
    pub path: PathBuf,
    /// True when the file was named `CLAUDE.md` (vs. a generic .md file).
    pub is_claude_md: bool,
}

impl MemoryEntry {
    /// A label suitable for display in the system prompt (e.g. `"global memory"`).
    pub fn label(&self) -> String {
        match &self.source {
            MemorySource::Global => "global memory".to_owned(),
            MemorySource::Project { depth: 0 } => "project memory".to_owned(),
            MemorySource::Project { depth } => format!("parent memory (depth {depth})"),
            MemorySource::Memdir { name } => format!("memory: {name}"),
        }
    }
}
