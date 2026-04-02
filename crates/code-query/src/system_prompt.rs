//! System prompt construction.
//!
//! Builds the `system` field of an API request from:
//!   - The base Claude Code system prompt
//!   - User CLAUDE.md / memory entries
//!   - Model-specific instructions
//!
//! Ref: src/utils/systemPrompt.ts (buildSystemPrompt)

use std::path::Path;

use code_types::message::{CacheControl, ContentBlock, TextBlock};
use code_memory::memory_type::MemoryEntry;

/// Context required to build the system prompt.
#[derive(Debug, Clone)]
pub struct SystemPromptConfig {
    /// Active model (used for model-specific instructions).
    pub model: String,
    /// Working directory of the session.
    pub cwd: std::path::PathBuf,
    /// Optional custom appendix added after the base prompt.
    pub appendix: Option<String>,
    /// Whether to include extended thinking instructions.
    pub extended_thinking: bool,
}

impl SystemPromptConfig {
    pub fn new(model: impl Into<String>, cwd: impl Into<std::path::PathBuf>) -> Self {
        Self {
            model: model.into(),
            cwd: cwd.into(),
            appendix: None,
            extended_thinking: false,
        }
    }
}

/// Build the system prompt content blocks for an API request.
///
/// Returns a `Vec<ContentBlock>` that is passed as `request.system`.
/// The first block uses `cache_control: ephemeral` so that prompt caching
/// applies to the static base prompt.
pub async fn build_system_prompt(
    config: &SystemPromptConfig,
    memory_entries: &[MemoryEntry],
) -> Vec<ContentBlock> {
    let mut blocks: Vec<ContentBlock> = Vec::new();

    // ── Base system prompt (cache-breakered) ──────────────────────────────────
    let base = base_system_prompt(&config.cwd, &config.model);
    blocks.push(ContentBlock::Text(TextBlock {
        text: base,
        cache_control: Some(CacheControl { kind: "ephemeral".into() }),
    }));

    // ── CLAUDE.md / memory entries ────────────────────────────────────────────
    if !memory_entries.is_empty() {
        let mut mem_text = String::from("<memory_context>\n");
        for entry in memory_entries {
            mem_text.push_str(&format!(
                "<{label}>\n{content}\n</{label}>\n",
                label = sanitize_xml_tag(&entry.label()),
                content = entry.content.trim(),
            ));
        }
        mem_text.push_str("</memory_context>");
        blocks.push(ContentBlock::Text(TextBlock {
            text: mem_text,
            cache_control: Some(CacheControl { kind: "ephemeral".into() }),
        }));
    }

    // ── Optional appendix ─────────────────────────────────────────────────────
    if let Some(appendix) = &config.appendix {
        if !appendix.is_empty() {
            blocks.push(ContentBlock::Text(TextBlock {
                text: appendix.clone(),
                cache_control: None,
            }));
        }
    }

    blocks
}

// ── Base prompt ───────────────────────────────────────────────────────────────

fn base_system_prompt(cwd: &Path, _model: &str) -> String {
    let cwd_str = cwd.to_string_lossy();
    format!(
        "You are Claude Code, Anthropic's official CLI for Claude. \
You are an interactive agent that helps users with software engineering tasks: \
solving bugs, adding features, refactoring code, explaining code, and more.\n\n\
The current working directory is: {cwd_str}\n\n\
When you run tools, they execute in this directory unless you specify an absolute path. \
Use the Read, Edit, Write, Bash, Glob, and Grep tools to explore and modify code. \
Always prefer the dedicated tools over Bash equivalents (e.g., use Read instead of `cat`).\n\n\
Be concise and precise. Lead with the answer or action. Avoid filler words.",
    )
}

fn sanitize_xml_tag(s: &str) -> String {
    s.replace([' ', '/'], "_")
     .replace(|c: char| !c.is_alphanumeric() && c != '_' && c != '-', "")
}
