//! Coordinator mode: multi-agent orchestration with a shared scratchpad.
//!
//! This module is only compiled when the `coordinator_mode` feature is enabled.
//!
//! Ref: src/coordinator/coordinatorMode.ts

#[cfg(feature = "coordinator_mode")]
pub use inner::*;

#[cfg(feature = "coordinator_mode")]
mod inner {
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    use anyhow::Result;
    use serde::{Deserialize, Serialize};

    use crate::definition::AgentDefinition;

    // ── Shared scratchpad ─────────────────────────────────────────────────────

    /// A shared key-value scratchpad visible to all coordinator workers.
    #[derive(Debug, Default, Clone)]
    pub struct SharedScratchpad {
        data: Arc<Mutex<HashMap<String, serde_json::Value>>>,
    }

    impl SharedScratchpad {
        pub fn new() -> Self {
            Self::default()
        }

        pub fn set(&self, key: impl Into<String>, value: serde_json::Value) {
            self.data.lock().unwrap().insert(key.into(), value);
        }

        pub fn get(&self, key: &str) -> Option<serde_json::Value> {
            self.data.lock().unwrap().get(key).cloned()
        }

        pub fn snapshot(&self) -> HashMap<String, serde_json::Value> {
            self.data.lock().unwrap().clone()
        }
    }

    // ── WorkerTask ────────────────────────────────────────────────────────────

    /// A unit of work dispatched to a worker agent by the coordinator.
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct WorkerTask {
        pub task_id: String,
        pub agent_type: String,
        pub prompt: String,
    }

    // ── CoordinatorResult ─────────────────────────────────────────────────────

    /// Aggregated result from all worker agents.
    #[derive(Debug)]
    pub struct CoordinatorResult {
        pub outputs: HashMap<String, String>,
        pub total_cost_usd: f64,
    }

    // ── run_coordinator ───────────────────────────────────────────────────────

    /// Dispatch multiple tasks to worker agents in parallel and aggregate results.
    pub async fn run_coordinator(
        tasks: Vec<WorkerTask>,
        agents: &[AgentDefinition],
        client: code_api::client::AnthropicClient,
        cwd: std::path::PathBuf,
        model: String,
        session_dir: std::path::PathBuf,
        permission_ctx: code_types::permissions::ToolPermissionContext,
        scratchpad: SharedScratchpad,
    ) -> Result<CoordinatorResult> {
        use crate::runner::{run_agent, RunOptions};
        use futures_util::future::join_all;

        let _ = scratchpad; // available to tasks via injection in a full implementation

        let futures: Vec<_> = tasks
            .into_iter()
            .map(|task| {
                let agent_def = agents
                    .iter()
                    .find(|a| a.agent_type == task.agent_type)
                    .cloned();
                let client = client.clone();
                let cwd = cwd.clone();
                let model = model.clone();
                let session_dir = session_dir.clone();
                let permission_ctx = permission_ctx.clone();

                async move {
                    let Some(agent) = agent_def else {
                        return (task.task_id, Err(anyhow::anyhow!("agent type '{}' not found", task.agent_type)));
                    };
                    let opts = RunOptions {
                        prompt: task.prompt,
                        parent_permission_ctx: permission_ctx,
                        cwd,
                        model,
                        session_dir,
                    };
                    let result = run_agent(&agent, opts, client).await;
                    (task.task_id, result)
                }
            })
            .collect();

        let results = join_all(futures).await;

        let mut outputs = HashMap::new();
        let mut total_cost_usd = 0.0;
        for (task_id, result) in results {
            match result {
                Ok(r) => {
                    total_cost_usd += r.cost_usd;
                    outputs.insert(task_id, r.output);
                }
                Err(e) => {
                    outputs.insert(task_id, format!("ERROR: {e}"));
                }
            }
        }

        Ok(CoordinatorResult { outputs, total_cost_usd })
    }
}
