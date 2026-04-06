//! Subagent forking: spawn a child agent that inherits conversation history.
//!
//! A forked subagent starts with the parent's conversation context already
//! loaded so it can continue where the parent left off without re-reading
//! files or re-running discovery steps.
//!
//! Ref: src/tools/AgentTool/forkSubagent.ts

use std::sync::Arc;

use anyhow::Result;

use code_types::message::Message;
use code_types::permissions::ToolPermissionContext;
use code_types::provider::{LlmProvider, ProviderKind};

use crate::definition::AgentDefinition;
use crate::runner::{run_agent, AgentRunResult, RunOptions};

// ── ForkOptions ───────────────────────────────────────────────────────────────

/// Options for forking a subagent with an inherited conversation context.
pub struct ForkOptions {
    /// The task prompt for the forked agent.
    pub prompt: String,
    /// Conversation messages to prepend (the parent's context).
    pub inherited_conversation: Vec<Message>,
    /// Parent permission context.
    pub parent_permission_ctx: ToolPermissionContext,
    /// Parent working directory.
    pub cwd: std::path::PathBuf,
    /// Model to use (parent's model unless overridden by agent definition).
    pub model: String,
    /// Session directory for tool results.
    pub session_dir: std::path::PathBuf,
    /// Which provider is in use.
    pub provider_kind: ProviderKind,
}

// ── fork_subagent ─────────────────────────────────────────────────────────────

/// Spawn a forked subagent.
///
/// The `inherited_conversation` is injected into the agent's context as a
/// synthetic system appendix summarising the parent's findings, keeping the
/// total token count manageable.
pub async fn fork_subagent(
    agent: &AgentDefinition,
    opts: ForkOptions,
    provider: Arc<dyn LlmProvider>,
) -> Result<AgentRunResult> {
    // Build a context summary from the inherited conversation.
    let context_summary = summarise_conversation(&opts.inherited_conversation);

    // Augment the agent's system prompt with the inherited context.
    let augmented_prompt = if context_summary.is_empty() {
        opts.prompt.clone()
    } else {
        format!(
            "## Context inherited from parent session\n\n{context_summary}\n\n---\n\n{}",
            opts.prompt
        )
    };

    let run_opts = RunOptions {
        prompt: augmented_prompt,
        parent_permission_ctx: opts.parent_permission_ctx,
        cwd: opts.cwd,
        model: opts.model,
        session_dir: opts.session_dir,
        provider_kind: opts.provider_kind,
    };

    run_agent(agent, run_opts, provider).await
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Build a compact text summary of conversation messages for context injection.
fn summarise_conversation(messages: &[Message]) -> String {
    use code_types::message::{ContentBlock, Message};

    let mut lines = Vec::new();
    for msg in messages {
        match msg {
            Message::User(u) => {
                let text: String = u
                    .content
                    .iter()
                    .filter_map(|b| {
                        if let ContentBlock::Text(t) = b {
                            Some(t.text.clone())
                        } else {
                            None
                        }
                    })
                    .collect::<Vec<_>>()
                    .join(" ");
                if !text.trim().is_empty() {
                    lines.push(format!("User: {text}"));
                }
            }
            Message::Assistant(a) => {
                let text: String = a
                    .content
                    .iter()
                    .filter_map(|b| {
                        if let ContentBlock::Text(t) = b {
                            Some(t.text.clone())
                        } else {
                            None
                        }
                    })
                    .collect::<Vec<_>>()
                    .join(" ");
                if !text.trim().is_empty() {
                    // Truncate long assistant responses to keep context compact.
                    let truncated = if text.len() > 1000 {
                        format!("{}…", &text[..1000])
                    } else {
                        text
                    };
                    lines.push(format!("Assistant: {truncated}"));
                }
            }
            _ => {}
        }
    }
    lines.join("\n")
}
