//! CLAUDE.md YAML frontmatter parsing and entry scanning.
//!
//! Augments raw `MemoryEntry` values with structured metadata extracted from
//! optional YAML frontmatter blocks at the top of CLAUDE.md files.
//!
//! Ref: src/memdir/memoryScan.ts

use serde::{Deserialize, Serialize};

use crate::memory_type::MemoryEntry;

// ── Frontmatter types ─────────────────────────────────────────────────────────

/// Parsed YAML frontmatter from a CLAUDE.md file.
///
/// All fields are optional; a file with no frontmatter yields `Default::default()`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MemoryFrontmatter {
    /// Keywords that indicate when this entry is highly relevant.
    ///
    /// Example:
    /// ```yaml
    /// keywords: [rust, testing, cargo]
    /// ```
    #[serde(default)]
    pub keywords: Vec<String>,
    /// Human-readable description of what this file covers.
    pub description: Option<String>,
    /// Explicit priority weight.  Higher values are included first when
    /// the prompt formatter must truncate.
    pub priority: Option<u32>,
}

// ── ScannedMemoryEntry ────────────────────────────────────────────────────────

/// A `MemoryEntry` enriched with parsed frontmatter metadata.
#[derive(Debug, Clone)]
pub struct ScannedMemoryEntry {
    /// The original loaded entry.
    pub entry: MemoryEntry,
    /// Parsed YAML frontmatter (defaults if absent or malformed).
    pub frontmatter: MemoryFrontmatter,
    /// The body content after stripping the frontmatter block.
    pub body: String,
}

// ── Parsing ───────────────────────────────────────────────────────────────────

/// Parse a file's content into `(frontmatter, body)`.
///
/// Frontmatter is delimited by `---` on its own line at the very start of the
/// file, closed by another `---` line.  If either delimiter is absent or if
/// the YAML fails to parse, returns `(Default::default(), full_content)`.
pub fn parse_frontmatter(content: &str) -> (MemoryFrontmatter, String) {
    // Must start with `---` (optionally followed by `\r`).
    if !content.starts_with("---") {
        return (MemoryFrontmatter::default(), content.to_owned());
    }

    // Find the end of the opening delimiter line.
    let after_open = match content.find('\n') {
        Some(i) => i + 1,
        None => return (MemoryFrontmatter::default(), content.to_owned()),
    };

    // Find the closing `---` delimiter.
    let rest = &content[after_open..];
    let close_pos = rest
        .lines()
        .enumerate()
        .find(|(_, line)| line.trim() == "---")
        .map(|(i, _)| i);

    let close_line_idx = match close_pos {
        Some(idx) => idx,
        None => return (MemoryFrontmatter::default(), content.to_owned()),
    };

    // Reconstruct byte offset for the closing delimiter.
    let yaml_block: String = rest.lines().take(close_line_idx).collect::<Vec<_>>().join("\n");

    // The body is everything after the closing `---` line.
    let body_start_line = close_line_idx + 1;
    let body: String = rest
        .lines()
        .skip(body_start_line)
        .collect::<Vec<_>>()
        .join("\n");
    // Preserve a trailing newline if the original had one.
    let body = if content.ends_with('\n') && !body.ends_with('\n') {
        format!("{body}\n")
    } else {
        body
    };

    let frontmatter: MemoryFrontmatter = match serde_yaml::from_str(&yaml_block) {
        Ok(fm) => fm,
        Err(_) => MemoryFrontmatter::default(),
    };

    (frontmatter, body)
}

// ── Scanning ──────────────────────────────────────────────────────────────────

/// Enrich a slice of `MemoryEntry` values with parsed frontmatter.
///
/// Memdir entries (`is_claude_md == false`) do not have frontmatter — their
/// full content becomes the body with default frontmatter.
pub fn scan_entries(entries: Vec<MemoryEntry>) -> Vec<ScannedMemoryEntry> {
    entries
        .into_iter()
        .map(|entry| {
            let (frontmatter, body) = if entry.is_claude_md {
                parse_frontmatter(&entry.content)
            } else {
                (MemoryFrontmatter::default(), entry.content.clone())
            };
            ScannedMemoryEntry {
                entry,
                frontmatter,
                body,
            }
        })
        .collect()
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_frontmatter_returns_full_content() {
        let content = "# My notes\n\nSome content here.\n";
        let (fm, body) = parse_frontmatter(content);
        assert!(fm.keywords.is_empty());
        assert_eq!(body, content);
    }

    #[test]
    fn frontmatter_parsed_correctly() {
        let content = "---\nkeywords: [rust, testing]\npriority: 3\n---\n# Body\n\nHello.\n";
        let (fm, body) = parse_frontmatter(content);
        assert_eq!(fm.keywords, vec!["rust", "testing"]);
        assert_eq!(fm.priority, Some(3));
        assert_eq!(body, "# Body\n\nHello.\n");
    }

    #[test]
    fn malformed_yaml_defaults_to_full_content() {
        let content = "---\n: bad: yaml: [[\n---\n# Body\n";
        let (fm, _body) = parse_frontmatter(content);
        assert!(fm.keywords.is_empty());
    }

    #[test]
    fn missing_close_delimiter_returns_full_content() {
        let content = "---\nkeywords: [a]\n# No closing\n";
        let (fm, body) = parse_frontmatter(content);
        assert!(fm.keywords.is_empty());
        assert_eq!(body, content);
    }
}
