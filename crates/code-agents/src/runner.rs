//! Agent runner: execute an AgentDefinition in an isolated QueryEngine instance.
//!
//! Each agent gets its own `QueryEngine` with:
//!   - The same `AnthropicClient` (shared HTTP connection pool)
//!   - A fresh `SessionId` scoped to the sub-task
//!   - A tool set filtered to `AgentDefinition::tools`
//!   - The agent's `system_prompt` injected as the `system_appendix`
//!
//! Ref: src/tools/AgentTool/runAgent.ts

use std::sync::Arc;

use anyhow::Result;
use uuid::Uuid;

use code_query::engine::{QueryEngine, QueryEngineConfig};
use code_types::ids::SessionId;
use code_types::message::{ContentBlock, Message, TextBlock, UserMessage};
use code_types::permissions::{PermissionMode, ToolPermissionContext};

use crate::definition::AgentDefinition;

// ── RunOptions ────────────────────────────────────────────────────────────────

/// Options for a single agent run.
pub struct RunOptions {
    /// The prompt text to send to the agent.
    pub prompt: String,
    /// Parent session's permission context (will be cloned and potentially tightened).
    pub parent_permission_ctx: ToolPermissionContext,
    /// Parent session's working directory.
    pub cwd: std::path::PathBuf,
    /// Parent session's model (used when the agent has no model override).
    pub model: String,
    /// Parent session directory (for tool result storage).
    pub session_dir: std::path::PathBuf,
}

// ── AgentRun result ───────────────────────────────────────────────────────────

/// The outcome of a completed agent run.
pub struct AgentRunResult {
    /// Full text assembled from all assistant messages.
    pub output: String,
    /// All messages produced by the agent engine (for display/logging).
    pub messages: Vec<Message>,
    /// Approximate total cost of the agent run.
    pub cost_usd: f64,
}

// ── run_agent ─────────────────────────────────────────────────────────────────

/// Execute `agent` on `opts.prompt` and return the assembled result.
///
/// Creates an isolated `QueryEngine` whose tool set is constrained to
/// `agent.tools`. The agent's `system_prompt` is injected as the
/// `system_appendix` (appended after the standard system prompt).
pub async fn run_agent(
    agent: &AgentDefinition,
    opts: RunOptions,
    client: code_api::client::AnthropicClient,
) -> Result<AgentRunResult> {
    let session_id = SessionId::new();

    // Build a permission context for the sub-agent.
    let mut permission_ctx = opts.parent_permission_ctx.clone();
    // Sub-agents cannot bypass permissions regardless of parent mode.
    if permission_ctx.mode == PermissionMode::BypassPermissions {
        permission_ctx.mode = PermissionMode::Default;
    }

    let model = agent
        .model
        .clone()
        .unwrap_or_else(|| opts.model.clone());

    let engine_config = QueryEngineConfig {
        model,
        cwd: opts.cwd.clone(),
        session_id: session_id.to_string(),
        session_dir: opts.session_dir.clone(),
        permission_ctx,
        system_appendix: Some(agent.system_prompt.clone()),
    };

    let engine = Arc::new(QueryEngine::new(client, engine_config));
    let mut rx = engine.subscribe();

    let user_msg = UserMessage {
        uuid: Uuid::new_v4(),
        content: vec![ContentBlock::Text(TextBlock {
            text: opts.prompt.clone(),
            cache_control: None,
        })],
        is_api_error_message: false,
        agent_id: None,
    };

    // Run inside a LocalSet because QueryEngine::query is !Send.
    let engine2 = Arc::clone(&engine);
    let local = tokio::task::LocalSet::new();
    let mut conversation = Vec::new();
    local
        .run_until(async move {
            tokio::task::spawn_local(async move {
                let _ = engine2.query(user_msg, &mut conversation).await;
            })
            .await
            .ok()
        })
        .await;

    // Drain messages.
    let mut messages = Vec::new();
    while let Some(msg) = rx.try_recv() {
        messages.push(msg);
    }
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    while let Some(msg) = rx.try_recv() {
        messages.push(msg);
    }

    let cost_usd = engine.total_cost_usd();
    let output = assemble_text_output(&messages);

    Ok(AgentRunResult { output, messages, cost_usd })
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn assemble_text_output(messages: &[Message]) -> String {
    let mut parts = Vec::new();
    for msg in messages {
        if let Message::Assistant(a) = msg {
            for block in &a.content {
                if let ContentBlock::Text(t) = block {
                    parts.push(t.text.clone());
                }
            }
        }
    }
    parts.join("\n")
}
