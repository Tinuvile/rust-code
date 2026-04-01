//! Loader for `code.md` (formerly `claude.md`) files — the project context files
//! injected as system prompt attachments.
//!
//! Each file may start with a YAML frontmatter block (delimited by `---`).
//! The frontmatter can set `hooks`, `allowed_tools`, and other settings.
//!
//! Ref: src/utils/configConstants.ts (CLAUDE_MD_FILENAME → CODE_MD_FILENAME)
//! Ref: src/utils/config.ts (getLocalFiles, getGlobalFiles)
//! Ref: src/utils/attachments.ts (loadCodeMdAttachments)

use std::path::{Path, PathBuf};
use serde::{Deserialize, Serialize};

// ── Constants ─────────────────────────────────────────────────────────────────

/// The canonical filename (renamed from `claude.md`).
pub const CODE_MD_FILENAME: &str = "code.md";

/// Fallback filename for backward compatibility.
pub const LEGACY_CODE_MD_FILENAME: &str = "claude.md";

// ── YAML frontmatter ──────────────────────────────────────────────────────────

/// Settings extractable from a code.md frontmatter block.
///
/// The frontmatter is YAML between the opening `---` and closing `---`.
/// Unknown keys are preserved in `extra`.
///
/// Ref: src/utils/config.ts (CLAUDE.md frontmatter schema)
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct CodeMdFrontmatter {
    /// Tools the model may always use without confirmation (project-scoped).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub allowed_tools: Vec<String>,
    /// Tools disabled for this project.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub disabled_tools: Vec<String>,
    /// Hook definitions (same format as settings.json `hooks`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hooks: Option<crate::settings::HooksSettings>,
    /// Additional working directories granted read access.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub additional_directories: Vec<String>,
    /// External file includes (`@<path>` syntax resolved here).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub include: Vec<String>,
    /// Extra unknown frontmatter keys preserved for forward-compatibility.
    #[serde(flatten)]
    pub extra: std::collections::HashMap<String, serde_json::Value>,
}

// ── Parsed code.md ────────────────────────────────────────────────────────────

/// A loaded and parsed code.md file.
#[derive(Debug, Clone)]
pub struct CodeMd {
    /// Absolute path to the file.
    pub path: PathBuf,
    /// Parsed YAML frontmatter (empty if none present).
    pub frontmatter: CodeMdFrontmatter,
    /// Body text after the closing `---` (or the entire file if no frontmatter).
    pub body: String,
}

impl CodeMd {
    /// The full text to inject into the system prompt (frontmatter stripped).
    pub fn prompt_text(&self) -> &str {
        self.body.trim()
    }
}

// ── Parsing ───────────────────────────────────────────────────────────────────

/// Parse a code.md file from raw text.
///
/// Frontmatter is the YAML block between the first `---` and the next `---`.
/// If no frontmatter is present the entire content becomes the body.
pub fn parse_code_md(path: impl Into<PathBuf>, content: &str) -> CodeMd {
    // Strip UTF-8 BOM.
    let content = content.strip_prefix('\u{feff}').unwrap_or(content);

    let (frontmatter, body) = extract_frontmatter(content);

    CodeMd {
        path: path.into(),
        frontmatter,
        body: body.to_owned(),
    }
}

/// Split raw file content into (frontmatter, body).
fn extract_frontmatter(content: &str) -> (CodeMdFrontmatter, &str) {
    // Must start with `---` (optionally followed by a newline).
    let Some(rest) = content.strip_prefix("---") else {
        return (CodeMdFrontmatter::default(), content);
    };
    // Skip optional newline after opening `---`.
    let rest = rest.strip_prefix('\n').or_else(|| rest.strip_prefix("\r\n")).unwrap_or(rest);

    // Find closing `---`.
    let Some(end_idx) = find_frontmatter_end(rest) else {
        return (CodeMdFrontmatter::default(), content);
    };

    let yaml = &rest[..end_idx];
    let after_close = &rest[end_idx..];
    // Skip the closing `---` line.
    let body = after_close
        .strip_prefix("---")
        .and_then(|s| s.strip_prefix('\n').or_else(|| s.strip_prefix("\r\n")).or(Some(s)))
        .unwrap_or(after_close);

    let frontmatter: CodeMdFrontmatter = serde_yaml::from_str(yaml).unwrap_or_default();
    (frontmatter, body)
}

/// Find the byte index of the closing `---` line in the frontmatter region.
fn find_frontmatter_end(s: &str) -> Option<usize> {
    for (idx, line) in s.split_inclusive('\n').scan(0usize, |pos, line| {
        let start = *pos;
        *pos += line.len();
        Some((start, line))
    }) {
        let trimmed = line.trim_end_matches(['\n', '\r']);
        if trimmed == "---" {
            return Some(idx);
        }
    }
    None
}

// ── File discovery ────────────────────────────────────────────────────────────

/// Return the code.md path for a given directory (prefers `code.md` over `claude.md`).
pub fn code_md_path(dir: &Path) -> Option<PathBuf> {
    let primary = dir.join(CODE_MD_FILENAME);
    if primary.exists() {
        return Some(primary);
    }
    let legacy = dir.join(LEGACY_CODE_MD_FILENAME);
    if legacy.exists() {
        return Some(legacy);
    }
    None
}

/// Return all code.md paths to load for a session, in injection order:
/// 1. Global: `~/.claude/code.md`
/// 2. Git root (if different from cwd)
/// 3. Project cwd
///
/// Ref: src/utils/config.ts getLocalFiles / getGlobalFiles
pub fn discover_code_md_files(cwd: &Path) -> Vec<PathBuf> {
    let mut paths = Vec::new();

    // Global code.md
    if let Some(home) = dirs_next::home_dir() {
        if let Some(p) = code_md_path(&home.join(".claude")) {
            paths.push(p);
        }
    }

    // Attempt to find git root and include it if different from cwd.
    if let Some(git_root) = find_git_root(cwd) {
        if git_root != cwd {
            if let Some(p) = code_md_path(&git_root) {
                paths.push(p);
            }
        }
    }

    // Project (cwd) code.md
    if let Some(p) = code_md_path(cwd) {
        paths.push(p);
    }

    paths
}

/// Walk up from `start` to find the `.git` directory root.
fn find_git_root(start: &Path) -> Option<PathBuf> {
    let mut current = start.to_owned();
    loop {
        if current.join(".git").exists() {
            return Some(current);
        }
        if !current.pop() {
            return None;
        }
    }
}

// ── I/O ───────────────────────────────────────────────────────────────────────

/// Load and parse a code.md file from disk.
pub async fn load_code_md(path: &Path) -> anyhow::Result<Option<CodeMd>> {
    match tokio::fs::read_to_string(path).await {
        Ok(content) => Ok(Some(parse_code_md(path, &content))),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.into()),
    }
}

/// Load all code.md files for a session (see `discover_code_md_files`).
pub async fn load_all_code_md(cwd: &Path) -> Vec<CodeMd> {
    let paths = discover_code_md_files(cwd);
    let mut result = Vec::with_capacity(paths.len());
    for path in paths {
        match load_code_md(&path).await {
            Ok(Some(md)) => result.push(md),
            Ok(None) => {}
            Err(e) => tracing::warn!("failed to load {}: {e}", path.display()),
        }
    }
    result
}
