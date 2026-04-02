//! Cache of files read during the current session.
//!
//! Tracks which files the agent has read so that:
//!   - `FileEdit` can reject edits to files not yet read.
//!   - The system prompt can include a "files you've read" hint.
//!
//! Ref: src/utils/fileStateCache.ts

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Instant;

/// State recorded for a single file read.
#[derive(Debug, Clone)]
pub struct FileReadState {
    /// When the file was first read in this session.
    pub first_read_at: Instant,
    /// File size in bytes at the time of reading.
    pub size_bytes: u64,
    /// Total number of times this file was read.
    pub read_count: u32,
}

/// Tracks files read during the current session.
#[derive(Debug, Default)]
pub struct FileStateCache {
    reads: HashMap<PathBuf, FileReadState>,
}

impl FileStateCache {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record that `path` was read.
    pub fn record_read(&mut self, path: impl Into<PathBuf>, size_bytes: u64) {
        let path = path.into();
        let entry = self.reads.entry(path).or_insert(FileReadState {
            first_read_at: Instant::now(),
            size_bytes,
            read_count: 0,
        });
        entry.read_count += 1;
        entry.size_bytes = size_bytes; // update in case file changed
    }

    /// Returns `true` if the file has been read at least once this session.
    pub fn was_read(&self, path: &Path) -> bool {
        self.reads.contains_key(path)
    }

    /// Return the state for a file, if it was read.
    pub fn get(&self, path: &Path) -> Option<&FileReadState> {
        self.reads.get(path)
    }

    /// All paths that were read this session.
    pub fn all_read_paths(&self) -> impl Iterator<Item = &Path> {
        self.reads.keys().map(|p| p.as_path())
    }

    /// Clear all recorded reads (called on session clear).
    pub fn clear(&mut self) {
        self.reads.clear();
    }

    /// Number of distinct files read.
    pub fn len(&self) -> usize {
        self.reads.len()
    }

    pub fn is_empty(&self) -> bool {
        self.reads.is_empty()
    }
}
