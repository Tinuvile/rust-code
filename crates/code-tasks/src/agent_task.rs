//! Agent task executor: run a sub-agent in the background.
//!
//! Defines the `AgentExecutor` trait so that `code-tasks` remains lightweight
//! and does not depend on `code-agents` / `code-query`.  The concrete executor
//! is wired up at the CLI level (see `code-cli` bootstrap).
//!
//! Ref: src/tasks/LocalAgentTask/

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;

use crate::output::TaskOutput;
use crate::store::TaskStore;
use crate::task::TaskId;

// ── AgentExecutor trait ──────────────────────────────────────────────────────

/// The output of a completed agent run.
#[derive(Debug, Clone)]
pub struct AgentRunOutput {
    /// Full text assembled from all assistant messages.
    pub text: String,
    /// Approximate total cost of the agent run.
    pub cost_usd: f64,
}

/// Trait for executing agents in background tasks.
///
/// Implementations live in the CLI layer where the full dependency tree
/// (`code-agents`, `code-query`, `LlmProvider`) is available.  This trait
/// keeps `code-tasks` from pulling in those heavy crates.
#[async_trait::async_trait]
pub trait AgentExecutor: Send + Sync {
    /// Run an agent with the given parameters and return its output.
    async fn run(
        &self,
        agent_type: &str,
        prompt: &str,
        cwd: &std::path::Path,
        model: &str,
        session_dir: &std::path::Path,
    ) -> Result<AgentRunOutput>;
}

// ── AgentTaskOptions ─────────────────────────────────────────────────────────

/// Options for spawning a background agent task.
pub struct AgentTaskOptions {
    /// Agent type to run (e.g. `"general-purpose"`).
    pub agent_type: String,
    /// The prompt / task description.
    pub prompt: String,
    /// Human-readable label for the task list.
    pub label: String,
    /// Directory to write the output log.
    pub tasks_dir: PathBuf,
    /// Session directory for tool results.
    pub session_dir: PathBuf,
    /// Working directory.
    pub cwd: PathBuf,
    /// Model identifier.
    pub model: String,
}

// ── spawn_agent_task ─────────────────────────────────────────────────────────

/// Spawn an agent task in the background and return its task id.
///
/// The agent runs inside a spawned tokio task.  Its status is updated in
/// `store` when it completes or fails.  Output is streamed to a log file
/// under `opts.tasks_dir/<task_id>.log`.
pub async fn spawn_agent_task(
    opts: AgentTaskOptions,
    store: Arc<TaskStore>,
    executor: Arc<dyn AgentExecutor>,
) -> Result<TaskId> {
    use crate::task::TaskRecord;

    let mut record = TaskRecord::new_agent(&opts.label, &opts.agent_type);
    let output = TaskOutput::new(&opts.tasks_dir, record.id.clone());
    record.log_path = Some(output.path().to_path_buf());

    let id = store.insert(record);
    store.update(&id, |r| r.mark_running());

    let store2 = Arc::clone(&store);
    let id2 = id.clone();
    let agent_type = opts.agent_type.clone();
    let prompt = opts.prompt.clone();
    let cwd = opts.cwd.clone();
    let model = opts.model.clone();
    let session_dir = opts.session_dir.clone();
    let log_path = output.path().to_path_buf();

    tokio::spawn(async move {
        // Write header to the log.
        let _ = output
            .append_line(&format!("# Agent: {agent_type}"))
            .await;
        let _ = output
            .append_line(&format!("# Prompt: {prompt}\n"))
            .await;

        // Execute the agent via the provided executor.
        match executor
            .run(&agent_type, &prompt, &cwd, &model, &session_dir)
            .await
        {
            Ok(result) => {
                // Write agent output to log.
                if !result.text.is_empty() {
                    let _ = output.append_line(&result.text).await;
                }
                let _ = output
                    .append_line(&format!(
                        "\n# Completed (cost: ${:.4})",
                        result.cost_usd
                    ))
                    .await;

                store2.update(&id2, |r| {
                    r.log_path = Some(log_path);
                    r.mark_completed(None);
                });

                tracing::info!(
                    task_id = %id2,
                    agent = %agent_type,
                    cost = result.cost_usd,
                    "agent task completed"
                );
            }
            Err(e) => {
                let err_msg = format!("Agent execution failed: {e}");
                let _ = output.append_line(&format!("\n# ERROR: {err_msg}")).await;

                store2.update(&id2, |r| {
                    r.log_path = Some(log_path);
                    r.mark_failed(&err_msg);
                });

                tracing::warn!(
                    task_id = %id2,
                    agent = %agent_type,
                    error = %e,
                    "agent task failed"
                );
            }
        }
    });

    Ok(id)
}
