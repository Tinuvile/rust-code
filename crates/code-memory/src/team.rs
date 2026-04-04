//! Team memory: shared memory entries across multiple users / workstations.
//!
//! Enabled by the `teammem` Cargo feature.
//!
//! Team memory entries live in a `.claude/team-memory/` directory that is
//! typically committed to version control or stored on a shared volume so all
//! team members see the same context.
//!
//! Ref: src/services/teamMemorySync/

use std::path::{Path, PathBuf};

use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::memory_type::MemoryEntry;

// ── TeamMemoryEntry ───────────────────────────────────────────────────────────

/// A memory entry shared across the team.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeamMemoryEntry {
    pub name: String,
    pub description: String,
    #[serde(rename = "type")]
    pub memory_type: String,
    /// Markdown body.
    pub content: String,
    /// Author / last-updated-by hint (informational only).
    pub author: Option<String>,
    /// Unix timestamp (s) of last write.
    pub updated_at: u64,
}

impl TeamMemoryEntry {
    /// Convert to the shared `MemoryEntry` format used by the prompt builder.
    pub fn to_memory_entry(&self, path: std::path::PathBuf) -> MemoryEntry {
        MemoryEntry {
            content: self.content.clone(),
            source: crate::memory_type::MemorySource::Memdir { name: self.name.clone() },
            path,
            is_claude_md: false,
        }
    }
}

// ── TeamMemoryStore ───────────────────────────────────────────────────────────

/// Reads/writes team memory entries from a shared directory.
pub struct TeamMemoryStore {
    dir: PathBuf,
}

impl TeamMemoryStore {
    /// Create a store rooted at `dir` (typically `<project_root>/.claude/team-memory/`).
    pub fn new(dir: PathBuf) -> Self {
        Self { dir }
    }

    /// Default team memory directory relative to `cwd`.
    pub fn default_for(cwd: &Path) -> Self {
        Self::new(cwd.join(".claude").join("team-memory"))
    }

    /// Load all team memory entries from disk.
    pub async fn load_all(&self) -> Result<Vec<TeamMemoryEntry>> {
        let mut entries = Vec::new();
        let mut dir = match tokio::fs::read_dir(&self.dir).await {
            Ok(d) => d,
            Err(_) => return Ok(entries),
        };

        while let Ok(Some(entry)) = dir.next_entry().await {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("md") {
                if let Ok(entry) = load_entry(&path).await {
                    entries.push(entry);
                }
            }
        }

        entries.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(entries)
    }

    /// Load all entries and convert to `MemoryEntry` for prompt injection.
    pub async fn load_for_prompt(&self) -> Result<Vec<MemoryEntry>> {
        let all = self.load_all().await?;
        Ok(all.into_iter().map(|e| {
            let filename = sanitize_filename(&e.name);
            let path = self.dir.join(format!("{filename}.md"));
            e.to_memory_entry(path)
        }).collect())
    }

    /// Write a team memory entry to disk.
    pub async fn write(&self, entry: &TeamMemoryEntry) -> Result<()> {
        tokio::fs::create_dir_all(&self.dir).await?;
        let filename = sanitize_filename(&entry.name);
        let path = self.dir.join(format!("{filename}.md"));

        let content = format!(
            "---\nname: {}\ndescription: {}\ntype: {}\n{}\nupdated_at: {}\n---\n\n{}",
            entry.name,
            entry.description,
            entry.memory_type,
            entry.author.as_ref().map(|a| format!("author: {a}")).unwrap_or_default(),
            entry.updated_at,
            entry.content,
        );

        tokio::fs::write(path, content).await?;
        Ok(())
    }

    /// Delete a team memory entry by name.
    pub async fn delete(&self, name: &str) -> Result<bool> {
        let filename = sanitize_filename(name);
        let path = self.dir.join(format!("{filename}.md"));
        match tokio::fs::remove_file(&path).await {
            Ok(()) => Ok(true),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(e) => Err(e.into()),
        }
    }
}

// ── helpers ───────────────────────────────────────────────────────────────────

async fn load_entry(path: &Path) -> Result<TeamMemoryEntry> {
    let raw = tokio::fs::read_to_string(path).await?;

    // Split YAML frontmatter from body.
    let (fm_str, body) = if let Some(rest) = raw.strip_prefix("---\n") {
        if let Some(end) = rest.find("\n---\n") {
            (&rest[..end], rest[end + 5..].trim_start())
        } else {
            ("", raw.as_str())
        }
    } else {
        ("", raw.as_str())
    };

    let fm: serde_yaml::Value = serde_yaml::from_str(fm_str).unwrap_or(serde_yaml::Value::Null);

    fn yaml_str(v: &serde_yaml::Value, key: &str) -> String {
        v.get(key).and_then(|v| v.as_str()).unwrap_or("").to_owned()
    }

    let updated_at = fm
        .get("updated_at")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);

    Ok(TeamMemoryEntry {
        name: yaml_str(&fm, "name"),
        description: yaml_str(&fm, "description"),
        memory_type: yaml_str(&fm, "type"),
        content: body.to_owned(),
        author: fm.get("author").and_then(|v| v.as_str()).map(str::to_owned),
        updated_at,
    })
}

fn sanitize_filename(name: &str) -> String {
    name.chars()
        .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
        .collect()
}
