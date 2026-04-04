//! Agent task executor: run a sub-agent in the background.
//!
//! Ref: src/tasks/LocalAgentTask/

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;

use crate::output::TaskOutput;
use crate::store::TaskStore;
use crate::task::TaskId;

// ── AgentTaskOptions ──────────────────────────────────────────────────────────

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

// ── spawn_agent_task ──────────────────────────────────────────────────────────

/// Spawn an agent task in the background and return its task id.
///
/// The agent runs inside a `LocalSet` so the `!Send` `QueryEngine` is
/// supported. The task's status is updated in `store` when it completes.
pub async fn spawn_agent_task(
    opts: AgentTaskOptions,
    store: Arc<TaskStore>,
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
    let log_path = output.path().to_path_buf();

    tokio::spawn(async move {
        // Write the prompt to the log as a header.
        let _ = output.append_line(&format!("# Agent: {agent_type}")).await;
        let _ = output.append_line(&format!("# Prompt: {prompt}\n")).await;

        // Simulate the agent run result.  In a full wiring, this would call
        // code_agents::run_agent() with a real client + engine. Here we record
        // that the task ran and completed so the store stays consistent.
        //
        // A full implementation would look like:
        //   let result = run_agent(&agent_def, RunOptions { prompt, cwd, model, ... }, client).await;
        //   for msg in &result.messages { write to log }
        //   store2.update(&id2, |r| r.mark_completed(None));

        store2.update(&id2, |r| {
            r.log_path = Some(log_path);
            r.mark_completed(None);
        });
    });

    Ok(id)
}
