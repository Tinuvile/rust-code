//! File edit history tracker.
//!
//! Records every file edit made during the session so that the TUI can show
//! a diff summary and the compact module can avoid compacting edited files.
//!
//! Ref: src/utils/fileHistory.ts

use std::path::{Path, PathBuf};

/// A single recorded file edit.
#[derive(Debug, Clone)]
pub struct FileEdit {
    /// Canonical absolute path of the file.
    pub path: PathBuf,
    /// Index of the message in the conversation that caused this edit.
    pub message_index: usize,
    /// Unix timestamp (seconds) when the edit was recorded.
    pub timestamp: u64,
}

/// Tracks all file edits made during a session.
#[derive(Debug, Default)]
pub struct FileHistoryTracker {
    edits: Vec<FileEdit>,
}

impl FileHistoryTracker {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record that `path` was edited at `message_index`.
    pub fn record_edit(&mut self, path: impl Into<PathBuf>, message_index: usize) {
        self.edits.push(FileEdit {
            path: path.into(),
            message_index,
            timestamp: unix_now(),
        });
    }

    /// All recorded edits, in chronological order.
    pub fn all_edits(&self) -> &[FileEdit] {
        &self.edits
    }

    /// Unique paths that were edited (deduped, insertion-order preserved).
    pub fn edited_paths(&self) -> Vec<&Path> {
        let mut seen = std::collections::HashSet::new();
        self.edits
            .iter()
            .filter(|e| seen.insert(e.path.as_path()))
            .map(|e| e.path.as_path())
            .collect()
    }

    /// Returns `true` if the given path was edited in this session.
    pub fn was_edited(&self, path: &Path) -> bool {
        self.edits.iter().any(|e| e.path == path)
    }

    /// Clear all recorded edits.
    pub fn clear(&mut self) {
        self.edits.clear();
    }
}

fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
