//! Arrow-key input history for the TUI REPL.
//!
//! Up to `MAX_ENTRIES` lines are stored in `~/.claude/input_history`.
//! The cursor tracks the user's position as they press ↑/↓.
//!
//! Ref: src/history.ts

use std::path::PathBuf;

const MAX_ENTRIES: usize = 1_000;

/// Persisted line-input history with a navigation cursor.
#[derive(Debug, Clone)]
pub struct InputHistory {
    /// Ordered from oldest (index 0) to newest (last).
    entries: Vec<String>,
    /// `None` = no active navigation.  `Some(i)` = currently viewing entries[i].
    cursor: Option<usize>,
    /// The draft text saved when the user first presses ↑.
    draft: String,
}

impl InputHistory {
    /// Create an empty history.
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            cursor: None,
            draft: String::new(),
        }
    }

    /// Load history from the default file (`~/.claude/input_history`).
    ///
    /// Returns an empty history if the file does not exist or cannot be read.
    pub fn load() -> Self {
        let path = default_path();
        let mut h = Self::new();
        if let Ok(raw) = std::fs::read_to_string(&path) {
            for line in raw.lines() {
                if !line.is_empty() {
                    h.entries.push(line.to_owned());
                }
            }
            // Keep only the most recent MAX_ENTRIES.
            if h.entries.len() > MAX_ENTRIES {
                let start = h.entries.len() - MAX_ENTRIES;
                h.entries.drain(..start);
            }
        }
        h
    }

    /// Persist history to the default file.
    pub async fn save(&self) -> anyhow::Result<()> {
        let path = default_path();
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        let content: String = self
            .entries
            .iter()
            .map(|l| format!("{l}\n"))
            .collect();
        tokio::fs::write(&path, content).await?;
        Ok(())
    }

    /// Add a new entry (skip if identical to the most recent entry).
    ///
    /// Resets the navigation cursor.
    pub fn push(&mut self, line: String) {
        let line = line.trim().to_owned();
        if line.is_empty() {
            return;
        }
        // De-duplicate consecutive identical entries.
        if self.entries.last().map(|l| l == &line).unwrap_or(false) {
            self.cursor = None;
            return;
        }
        self.entries.push(line);
        if self.entries.len() > MAX_ENTRIES {
            self.entries.remove(0);
        }
        self.cursor = None;
    }

    /// Navigate backward (↑).  Returns the entry to display, or `None` if at the start.
    ///
    /// Saves the current input as a draft on first invocation.
    pub fn prev(&mut self, current_input: &str) -> Option<&str> {
        if self.entries.is_empty() {
            return None;
        }
        match self.cursor {
            None => {
                self.draft = current_input.to_owned();
                let idx = self.entries.len() - 1;
                self.cursor = Some(idx);
                Some(&self.entries[idx])
            }
            Some(0) => {
                // Already at the oldest entry.
                Some(&self.entries[0])
            }
            Some(i) => {
                let new_idx = i - 1;
                self.cursor = Some(new_idx);
                Some(&self.entries[new_idx])
            }
        }
    }

    /// Navigate forward (↓).  Returns the entry or the saved draft.
    pub fn next(&mut self) -> Option<&str> {
        match self.cursor {
            None => None,
            Some(i) if i + 1 >= self.entries.len() => {
                self.cursor = None;
                Some(&self.draft)
            }
            Some(i) => {
                let new_idx = i + 1;
                self.cursor = Some(new_idx);
                Some(&self.entries[new_idx])
            }
        }
    }

    /// Reset the navigation cursor (called on Enter or Escape).
    pub fn reset_cursor(&mut self) {
        self.cursor = None;
        self.draft.clear();
    }

    /// Total number of entries.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

impl Default for InputHistory {
    fn default() -> Self {
        Self::new()
    }
}

fn default_path() -> PathBuf {
    dirs_next::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".claude")
        .join("input_history")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn push_and_navigate() {
        let mut h = InputHistory::new();
        h.push("first".into());
        h.push("second".into());
        h.push("third".into());

        assert_eq!(h.prev(""), Some("third"));
        assert_eq!(h.prev(""), Some("second"));
        assert_eq!(h.prev(""), Some("first"));
        // Already at start — stays.
        assert_eq!(h.prev(""), Some("first"));
        // Forward again.
        assert_eq!(h.next(), Some("second"));
        assert_eq!(h.next(), Some("third"));
        // Past the end — returns draft.
        assert_eq!(h.next(), Some(""));
        // No more forward.
        assert_eq!(h.next(), None);
    }

    #[test]
    fn deduplicates_consecutive() {
        let mut h = InputHistory::new();
        h.push("same".into());
        h.push("same".into());
        assert_eq!(h.len(), 1);
    }
}
