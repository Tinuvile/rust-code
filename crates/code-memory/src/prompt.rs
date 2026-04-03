//! Format memory entries for inclusion in the system prompt.
//!
//! Enforces a hard cap of 200 lines / 25 KB so memory never overwhelms the
//! context window.  Entries should be pre-sorted by relevance (highest first).
//!
//! Ref: src/memdir/memdir.ts loadMemoryPrompt

use crate::relevance::ScoredEntry;
use crate::scanner::ScannedMemoryEntry;

// ── Limits ────────────────────────────────────────────────────────────────────

/// Maximum number of lines in the combined memory prompt.
pub const MAX_LINES: usize = 200;
/// Maximum byte size of the combined memory prompt.
pub const MAX_BYTES: usize = 25 * 1024; // 25 KB

// ── Formatting ────────────────────────────────────────────────────────────────

/// Format a slice of scored entries into a single memory prompt string.
///
/// Iterates in order (caller should pre-sort by relevance), appending each
/// entry's section until the line or byte budget would be exceeded.
///
/// Returns an empty string when `entries` is empty.
pub fn format_memory_for_prompt(entries: &[ScoredEntry]) -> String {
    let mut output = String::new();
    let mut line_count: usize = 0;

    for scored in entries {
        let section = format_entry(&scored.entry);
        let section_lines = section.lines().count();

        if output.len() + section.len() > MAX_BYTES {
            break;
        }
        if line_count + section_lines > MAX_LINES {
            break;
        }

        output.push_str(&section);
        output.push('\n');
        line_count += section_lines + 1; // +1 for the separator newline
    }

    // Remove trailing newline added after last section.
    if output.ends_with('\n') && !output.is_empty() {
        output.pop();
    }

    output
}

/// Format a single `ScannedMemoryEntry` as a labeled XML-ish section.
///
/// ```text
/// <memory label="global memory">
/// {body}
/// </memory>
/// ```
fn format_entry(entry: &ScannedMemoryEntry) -> String {
    let label = entry.entry.label();
    let body = entry.body.trim_end();
    format!("<memory label=\"{label}\">\n{body}\n</memory>")
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory_type::{MemoryEntry, MemorySource};
    use crate::scanner::{MemoryFrontmatter, ScannedMemoryEntry};
    use std::path::PathBuf;

    fn make_scored(source: MemorySource, body: &str, score: f32) -> ScoredEntry {
        ScoredEntry {
            entry: ScannedMemoryEntry {
                entry: MemoryEntry {
                    content: body.to_owned(),
                    source,
                    path: PathBuf::from("test.md"),
                    is_claude_md: true,
                },
                frontmatter: MemoryFrontmatter::default(),
                body: body.to_owned(),
            },
            score,
        }
    }

    #[test]
    fn empty_input_returns_empty_string() {
        assert_eq!(format_memory_for_prompt(&[]), "");
    }

    #[test]
    fn single_entry_formatted() {
        let entries = vec![make_scored(MemorySource::Global, "Use snake_case.", 1.0)];
        let out = format_memory_for_prompt(&entries);
        assert!(out.contains("<memory label=\"global memory\">"));
        assert!(out.contains("Use snake_case."));
        assert!(out.contains("</memory>"));
    }

    #[test]
    fn byte_limit_respected() {
        // Create an entry whose section is just under the limit, then another.
        let big_body = "x".repeat(MAX_BYTES - 50);
        let entries = vec![
            make_scored(MemorySource::Global, &big_body, 2.0),
            make_scored(MemorySource::Project { depth: 0 }, "small content", 1.0),
        ];
        let out = format_memory_for_prompt(&entries);
        assert!(out.len() <= MAX_BYTES, "output must not exceed MAX_BYTES");
        assert!(!out.contains("small content"), "second entry should be excluded");
    }
}
