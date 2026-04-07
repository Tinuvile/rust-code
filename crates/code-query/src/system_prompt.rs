//! System prompt construction.
//!
//! Builds the `system` field of an API request from:
//!   - The base Claude Code system prompt
//!   - User CLAUDE.md / memory entries
//!   - Agent mode instructions
//!   - Model-specific / provider-specific instructions
//!   - Extended thinking configuration
//!
//! Ref: src/utils/systemPrompt.ts (buildSystemPrompt)

use std::path::Path;

use code_types::message::{CacheControl, ContentBlock, TextBlock};
use code_types::provider::ProviderKind;
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
    /// Whether this is an agent (sub-agent) rather than the main session.
    pub is_agent: bool,
    /// Optional agent name (for sub-agent context).
    pub agent_name: Option<String>,
    /// Optional agent system prompt (injected before the appendix).
    pub agent_system_prompt: Option<String>,
    /// Which LLM provider is in use (for provider-specific hints).
    pub provider_kind: ProviderKind,
}

impl SystemPromptConfig {
    pub fn new(model: impl Into<String>, cwd: impl Into<std::path::PathBuf>) -> Self {
        Self {
            model: model.into(),
            cwd: cwd.into(),
            appendix: None,
            extended_thinking: false,
            is_agent: false,
            agent_name: None,
            agent_system_prompt: None,
            provider_kind: ProviderKind::Anthropic,
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
    let base = base_system_prompt(
        &config.cwd,
        &config.model,
        config.provider_kind,
        config.is_agent,
    );
    blocks.push(ContentBlock::Text(TextBlock {
        text: base,
        cache_control: Some(CacheControl {
            kind: "ephemeral".into(),
        }),
    }));

    // ── Extended thinking instructions ────────────────────────────────────────
    if config.extended_thinking {
        blocks.push(ContentBlock::Text(TextBlock {
            text: thinking_instructions(&config.model),
            cache_control: None,
        }));
    }

    // ── Provider-specific instructions ────────────────────────────────────────
    let provider_hint = provider_specific_instructions(config.provider_kind, &config.model);
    if !provider_hint.is_empty() {
        blocks.push(ContentBlock::Text(TextBlock {
            text: provider_hint,
            cache_control: None,
        }));
    }

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
            cache_control: Some(CacheControl {
                kind: "ephemeral".into(),
            }),
        }));
    }

    // ── Agent system prompt ───────────────────────────────────────────────────
    if let Some(ref agent_prompt) = config.agent_system_prompt {
        if !agent_prompt.is_empty() {
            let text = if let Some(ref name) = config.agent_name {
                format!("You are operating as the \"{name}\" agent.\n\n{agent_prompt}")
            } else {
                agent_prompt.clone()
            };
            blocks.push(ContentBlock::Text(TextBlock {
                text,
                cache_control: None,
            }));
        }
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

// ── Base prompt ──────────────────────────────────────────────────────────────

fn base_system_prompt(
    cwd: &Path,
    model: &str,
    provider_kind: ProviderKind,
    is_agent: bool,
) -> String {
    let cwd_str = cwd.to_string_lossy();
    let identity = if is_agent {
        "You are a Claude Code sub-agent, an autonomous AI assistant handling a specific task \
         as part of a larger workflow."
    } else {
        "You are Claude Code, an interactive AI assistant for software engineering tasks: \
         solving bugs, adding features, refactoring code, explaining code, and more."
    };

    let tool_hint = if provider_kind.is_anthropic_family() {
        "Use the Read, Edit, Write, Bash, Glob, and Grep tools to explore and modify code. \
         Always prefer the dedicated tools over Bash equivalents (e.g., use Read instead of `cat`)."
    } else {
        "You have access to tools for reading files, writing files, editing files, \
         running shell commands, searching for files, and searching file contents. \
         Always prefer the dedicated tools over shell equivalents."
    };

    let agent_hint = if is_agent {
        "\n\nYou are running as a background sub-agent. Focus on completing the assigned task \
         and return a clear, concise summary of what you did."
    } else {
        ""
    };

    let model_hint = model_specific_instructions(model);

    format!(
        "{identity}\n\n\
         The current working directory is: {cwd_str}\n\n\
         When you run tools, they execute in this directory unless you specify an absolute path. \
         {tool_hint}\n\n\
         Be concise and precise. Lead with the answer or action. Avoid filler words.\
         {agent_hint}\
         {model_hint}",
    )
}

// ── Model-specific instructions ──────────────────────────────────────────────

fn model_specific_instructions(model: &str) -> String {
    let lower = model.to_lowercase();

    // OpenAI o-series reasoning models.
    if lower.starts_with("o1") || lower.starts_with("o3") {
        return "\n\nYou are a reasoning model. Think step by step before acting. \
                Use your reasoning capabilities to plan multi-step tasks carefully."
            .to_owned();
    }

    // Gemini models.
    if lower.starts_with("gemini") {
        return "\n\nWhen using tools, ensure all required parameters are provided. \
                Do not omit required fields in tool calls."
            .to_owned();
    }

    // DeepSeek models.
    if lower.contains("deepseek") {
        return "\n\nYou have access to a suite of development tools. \
                Use function calls to interact with the codebase."
            .to_owned();
    }

    String::new()
}

// ── Provider-specific instructions ───────────────────────────────────────────

fn provider_specific_instructions(kind: ProviderKind, _model: &str) -> String {
    match kind {
        ProviderKind::OpenAi | ProviderKind::OpenAiCompatible => {
            "When making tool calls, always include all required parameters. \
             If a parameter is optional and not needed, omit it."
                .to_owned()
        }
        ProviderKind::Gemini => {
            "Tool calls use the functionCall format. Ensure argument values match \
             the expected types in the schema."
                .to_owned()
        }
        _ => String::new(),
    }
}

// ── Extended thinking instructions ───────────────────────────────────────────

fn thinking_instructions(model: &str) -> String {
    let lower = model.to_lowercase();

    if lower.contains("claude") || lower.contains("sonnet") || lower.contains("opus") || lower.contains("haiku") {
        return "Extended thinking is enabled. Use your <thinking> blocks to reason \
                through complex problems before taking action. Break down multi-step \
                tasks into clear intermediate steps."
            .to_owned();
    }

    // For non-Claude models, provide generic chain-of-thought guidance.
    "Think step by step before taking action on complex tasks. Break down \
     multi-step problems into clear intermediate steps."
        .to_owned()
}

// ── Helpers ──────────────────────────────────────────────────────────────────

fn sanitize_xml_tag(s: &str) -> String {
    s.replace([' ', '/'], "_")
        .replace(
            |c: char| !c.is_alphanumeric() && c != '_' && c != '-',
            "",
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn base_prompt_contains_cwd() {
        let config = SystemPromptConfig::new("claude-sonnet-4-6", "/home/user/project");
        let blocks = build_system_prompt(&config, &[]).await;
        let text = blocks_to_text(&blocks);
        assert!(text.contains("/home/user/project"));
    }

    #[tokio::test]
    async fn memory_entries_included() {
        let config = SystemPromptConfig::new("claude-sonnet-4-6", "/tmp");
        let entries = vec![MemoryEntry {
            content: "Use Rust 2021 edition.".to_owned(),
            source: code_memory::memory_type::MemorySource::Global,
            path: std::path::PathBuf::from("/tmp/CLAUDE.md"),
            is_claude_md: true,
        }];
        let blocks = build_system_prompt(&config, &entries).await;
        let text = blocks_to_text(&blocks);
        assert!(text.contains("Use Rust 2021 edition."));
        assert!(text.contains("<memory_context>"));
    }

    #[tokio::test]
    async fn agent_mode_hint_included() {
        let mut config = SystemPromptConfig::new("claude-sonnet-4-6", "/tmp");
        config.is_agent = true;
        config.agent_name = Some("test-agent".to_owned());
        config.agent_system_prompt = Some("Focus on testing.".to_owned());
        let blocks = build_system_prompt(&config, &[]).await;
        let text = blocks_to_text(&blocks);
        assert!(text.contains("sub-agent"));
        assert!(text.contains("test-agent"));
        assert!(text.contains("Focus on testing."));
    }

    #[tokio::test]
    async fn appendix_included() {
        let mut config = SystemPromptConfig::new("claude-sonnet-4-6", "/tmp");
        config.appendix = Some("Always use TypeScript.".to_owned());
        let blocks = build_system_prompt(&config, &[]).await;
        let text = blocks_to_text(&blocks);
        assert!(text.contains("Always use TypeScript."));
    }

    #[tokio::test]
    async fn extended_thinking_included() {
        let mut config = SystemPromptConfig::new("claude-sonnet-4-6", "/tmp");
        config.extended_thinking = true;
        let blocks = build_system_prompt(&config, &[]).await;
        let text = blocks_to_text(&blocks);
        assert!(text.contains("thinking"));
    }

    #[tokio::test]
    async fn openai_provider_adds_hint() {
        let mut config = SystemPromptConfig::new("gpt-4o", "/tmp");
        config.provider_kind = ProviderKind::OpenAi;
        let blocks = build_system_prompt(&config, &[]).await;
        let text = blocks_to_text(&blocks);
        assert!(text.contains("required parameters"));
    }

    fn blocks_to_text(blocks: &[ContentBlock]) -> String {
        blocks
            .iter()
            .filter_map(|b| {
                if let ContentBlock::Text(t) = b {
                    Some(t.text.as_str())
                } else {
                    None
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}
