//! Coordinator mode tools — spawn and manage sub-agents.
//!
//! Enabled by feature `coordinator_mode`.
//! These tools allow a "coordinator" agent to orchestrate multiple parallel
//! sub-agents.  `SpawnAgentTool` creates the record and launches background
//! execution via an injected `CoordinatorAgentRunner`.  `GetAgentOutputTool`
//! reads back the record to check status and retrieve output.
//!
//! Ref: src/tools/CoordinatorTool/CoordinatorTool.ts

use std::path::Path;
use std::sync::Arc;

use async_trait::async_trait;
use code_permissions::evaluator::ToolCallContext;
use code_types::tool::{ToolInputSchema, ToolResult, ValidationResult};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::{error_result, ok_result, ProgressSender, Tool, ToolContext};

// ── Agent record ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentStatus {
    Pending,
    Running,
    Completed,
    Failed,
}

impl std::fmt::Display for AgentStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Pending => write!(f, "pending"),
            Self::Running => write!(f, "running"),
            Self::Completed => write!(f, "completed"),
            Self::Failed => write!(f, "failed"),
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AgentRecord {
    pub id: String,
    pub role: String,
    pub prompt: String,
    pub status: AgentStatus,
    pub output: Option<String>,
    pub error: Option<String>,
    pub cost_usd: Option<f64>,
    pub created_at: u64,
    pub completed_at: Option<u64>,
}

fn agent_path(session_dir: &Path, id: &str) -> std::path::PathBuf {
    session_dir.join("agents").join(format!("{id}.json"))
}

fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

pub async fn write_agent(session_dir: &Path, rec: &AgentRecord) -> std::io::Result<()> {
    let dir = session_dir.join("agents");
    tokio::fs::create_dir_all(&dir).await?;
    let json = serde_json::to_string_pretty(rec)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
    tokio::fs::write(agent_path(session_dir, &rec.id), json).await
}

pub async fn read_agent(session_dir: &Path, id: &str) -> Option<AgentRecord> {
    let raw = tokio::fs::read_to_string(agent_path(session_dir, id)).await.ok()?;
    serde_json::from_str(&raw).ok()
}

/// List all agent records in the session directory.
pub async fn list_agents(session_dir: &Path) -> Vec<AgentRecord> {
    let dir = session_dir.join("agents");
    let mut entries = Vec::new();
    let Ok(mut read_dir) = tokio::fs::read_dir(&dir).await else {
        return entries;
    };
    while let Ok(Some(entry)) = read_dir.next_entry().await {
        let path = entry.path();
        if path.extension().map(|e| e == "json").unwrap_or(false) {
            if let Ok(raw) = tokio::fs::read_to_string(&path).await {
                if let Ok(rec) = serde_json::from_str::<AgentRecord>(&raw) {
                    entries.push(rec);
                }
            }
        }
    }
    entries.sort_by_key(|r| r.created_at);
    entries
}

// ── CoordinatorAgentRunner trait ─────────────────────────────────────────────

/// Trait for running agents from coordinator tools.
///
/// The concrete implementation is injected by the CLI/bootstrap layer where the
/// full dependency tree (`code-agents`, `code-query`, `LlmProvider`) is available.
/// This keeps `code-tools` free of those heavy dependencies.
#[async_trait]
pub trait CoordinatorAgentRunner: Send + Sync {
    /// Execute an agent and return its output text and cost.
    ///
    /// The runner should:
    /// 1. Look up or create an `AgentDefinition` for the given `role`
    /// 2. Call `code_agents::run_agent()` with the prompt
    /// 3. Return the output text and cost
    ///
    /// This method is called inside a spawned background task; it should
    /// block until the agent finishes.
    async fn run_agent(
        &self,
        role: &str,
        prompt: &str,
        cwd: &Path,
        session_dir: &Path,
    ) -> anyhow::Result<AgentRunOutput>;
}

/// Output from a completed coordinator agent run.
#[derive(Debug, Clone)]
pub struct AgentRunOutput {
    /// Full text assembled from all assistant messages.
    pub text: String,
    /// Approximate cost of the run in USD.
    pub cost_usd: f64,
}

// ── SpawnAgent ────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct SpawnAgentInput {
    agent_id: Option<String>,
    role: String,
    prompt: String,
}

/// Tool that spawns a sub-agent to handle part of the task.
///
/// If a `CoordinatorAgentRunner` is provided, the agent actually executes in the
/// background and the record is updated on completion.  Otherwise, the record is
/// created in `Pending` status (for environments without agent execution).
pub struct SpawnAgentTool {
    runner: Option<Arc<dyn CoordinatorAgentRunner>>,
}

impl SpawnAgentTool {
    /// Create with no runner — agents are recorded but not executed.
    pub fn new() -> Self {
        Self { runner: None }
    }

    /// Create with a runner — agents are executed in the background.
    pub fn with_runner(runner: Arc<dyn CoordinatorAgentRunner>) -> Self {
        Self { runner: Some(runner) }
    }
}

#[async_trait]
impl Tool for SpawnAgentTool {
    fn name(&self) -> &str { "SpawnAgent" }

    fn description(&self) -> &str {
        "Spawn a sub-agent to handle a specific part of the task. \
        The sub-agent runs in the background in its own context. \
        Use GetAgentOutput to check status and retrieve results."
    }

    fn input_schema(&self) -> ToolInputSchema {
        json!({
            "type": "object",
            "properties": {
                "agent_id": { "type": "string", "description": "Optional unique ID for this agent" },
                "role": { "type": "string", "description": "Short description of the agent's role" },
                "prompt": { "type": "string", "description": "Full instructions for the sub-agent" }
            },
            "required": ["role", "prompt"]
        })
    }

    fn is_read_only(&self, _input: &Value) -> bool { false }

    async fn validate_input(&self, input: &Value, _ctx: &ToolContext) -> ValidationResult {
        if input.get("prompt").and_then(|v| v.as_str()).map(|s| s.is_empty()).unwrap_or(true) {
            return ValidationResult::err("prompt is required", 1);
        }
        if input.get("role").and_then(|v| v.as_str()).map(|s| s.is_empty()).unwrap_or(true) {
            return ValidationResult::err("role is required", 1);
        }
        ValidationResult::ok()
    }

    fn permission_context<'a>(&'a self, input: &'a Value, cwd: &'a Path) -> ToolCallContext<'a> {
        ToolCallContext {
            tool_name: self.name(),
            content: input.get("role").and_then(|v| v.as_str()),
            input: Some(input),
            is_read_only: false,
            cwd,
        }
    }

    async fn call(
        &self,
        tool_use_id: &str,
        input: Value,
        ctx: &ToolContext,
        _progress: Option<&ProgressSender>,
    ) -> ToolResult {
        let parsed: SpawnAgentInput = match serde_json::from_value(input) {
            Ok(v) => v,
            Err(e) => return error_result(tool_use_id, format!("Invalid input: {e}")),
        };

        let id = parsed.agent_id.unwrap_or_else(|| format!("agent-{}", tool_use_id));
        let role = parsed.role.clone();
        let prompt = parsed.prompt.clone();

        let initial_status = if self.runner.is_some() {
            AgentStatus::Running
        } else {
            AgentStatus::Pending
        };

        let rec = AgentRecord {
            id: id.clone(),
            role: role.clone(),
            prompt: prompt.clone(),
            status: initial_status.clone(),
            output: None,
            error: None,
            cost_usd: None,
            created_at: unix_now(),
            completed_at: None,
        };

        if let Err(e) = write_agent(&ctx.session_dir, &rec).await {
            return error_result(tool_use_id, format!("Failed to persist agent record: {e}"));
        }

        // If we have a runner, spawn background execution.
        if let Some(runner) = &self.runner {
            let runner = Arc::clone(runner);
            let session_dir = ctx.session_dir.clone();
            let cwd = ctx.cwd.clone();
            let agent_id = id.clone();
            let spawn_role = role.clone();
            let spawn_prompt = prompt.clone();

            tokio::spawn(async move {
                tracing::info!(agent_id = %agent_id, role = %spawn_role, "coordinator: spawning sub-agent");

                match runner.run_agent(&spawn_role, &spawn_prompt, &cwd, &session_dir).await {
                    Ok(output) => {
                        let rec = AgentRecord {
                            id: agent_id.clone(),
                            role: spawn_role.clone(),
                            prompt: spawn_prompt.clone(),
                            status: AgentStatus::Completed,
                            output: Some(output.text),
                            error: None,
                            cost_usd: Some(output.cost_usd),
                            created_at: unix_now(),
                            completed_at: Some(unix_now()),
                        };
                        if let Err(e) = write_agent(&session_dir, &rec).await {
                            tracing::warn!(agent_id = %agent_id, error = %e, "failed to update agent record");
                        }
                        tracing::info!(
                            agent_id = %agent_id,
                            cost = output.cost_usd,
                            "coordinator: sub-agent completed"
                        );
                    }
                    Err(e) => {
                        let rec = AgentRecord {
                            id: agent_id.clone(),
                            role: spawn_role.clone(),
                            prompt: spawn_prompt.clone(),
                            status: AgentStatus::Failed,
                            output: None,
                            error: Some(e.to_string()),
                            cost_usd: None,
                            created_at: unix_now(),
                            completed_at: Some(unix_now()),
                        };
                        if let Err(write_err) = write_agent(&session_dir, &rec).await {
                            tracing::warn!(
                                agent_id = %agent_id,
                                error = %write_err,
                                "failed to update agent record after failure"
                            );
                        }
                        tracing::warn!(
                            agent_id = %agent_id,
                            error = %e,
                            "coordinator: sub-agent failed"
                        );
                    }
                }
            });

            ok_result(
                tool_use_id,
                format!(
                    "Sub-agent '{id}' ({}) launched and running in the background. \
                     Use GetAgentOutput with agent_id '{id}' to check status and retrieve results.",
                    role
                ),
            )
        } else {
            ok_result(
                tool_use_id,
                format!(
                    "Sub-agent '{id}' ({}) queued (no executor available — record saved to disk).",
                    role
                ),
            )
        }
    }
}

// ── GetAgentOutput ────────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct GetAgentOutputInput {
    agent_id: Option<String>,
}

/// Tool that retrieves the output and status of spawned sub-agents.
///
/// Can retrieve a specific agent by ID or list all agents and their statuses.
pub struct GetAgentOutputTool;

#[async_trait]
impl Tool for GetAgentOutputTool {
    fn name(&self) -> &str { "GetAgentOutput" }

    fn description(&self) -> &str {
        "Retrieve the output and status of a sub-agent spawned with SpawnAgent. \
         Pass agent_id to get a specific agent, or omit it to list all agents."
    }

    fn input_schema(&self) -> ToolInputSchema {
        json!({
            "type": "object",
            "properties": {
                "agent_id": {
                    "type": "string",
                    "description": "ID of the agent to query. Omit to list all agents."
                }
            },
            "required": []
        })
    }

    fn is_read_only(&self, _input: &Value) -> bool { true }

    fn permission_context<'a>(&'a self, input: &'a Value, cwd: &'a Path) -> ToolCallContext<'a> {
        ToolCallContext {
            tool_name: self.name(),
            content: input.get("agent_id").and_then(|v| v.as_str()),
            input: Some(input),
            is_read_only: true,
            cwd,
        }
    }

    async fn call(
        &self,
        tool_use_id: &str,
        input: Value,
        ctx: &ToolContext,
        _progress: Option<&ProgressSender>,
    ) -> ToolResult {
        let parsed: GetAgentOutputInput = match serde_json::from_value(input) {
            Ok(v) => v,
            Err(e) => return error_result(tool_use_id, format!("Invalid input: {e}")),
        };

        match parsed.agent_id {
            Some(id) => {
                // Get specific agent.
                match read_agent(&ctx.session_dir, &id).await {
                    Some(rec) => {
                        let mut output = format!(
                            "Agent: {}\nRole: {}\nStatus: {}\n",
                            rec.id, rec.role, rec.status
                        );
                        if let Some(cost) = rec.cost_usd {
                            output.push_str(&format!("Cost: ${cost:.4}\n"));
                        }
                        if let Some(ref text) = rec.output {
                            output.push_str(&format!("\n--- Output ---\n{text}"));
                        }
                        if let Some(ref err) = rec.error {
                            output.push_str(&format!("\n--- Error ---\n{err}"));
                        }
                        ok_result(tool_use_id, output)
                    }
                    None => error_result(
                        tool_use_id,
                        format!("Agent '{id}' not found."),
                    ),
                }
            }
            None => {
                // List all agents.
                let agents = list_agents(&ctx.session_dir).await;
                if agents.is_empty() {
                    return ok_result(tool_use_id, "No agents have been spawned.");
                }

                let mut output = format!("{} agent(s):\n\n", agents.len());
                for rec in &agents {
                    output.push_str(&format!(
                        "- {} [{}] {}{}\n",
                        rec.id,
                        rec.status,
                        rec.role,
                        if rec.status == AgentStatus::Completed { " (done)" } else { "" },
                    ));
                }

                let completed = agents.iter().filter(|r| r.status == AgentStatus::Completed).count();
                let running = agents.iter().filter(|r| r.status == AgentStatus::Running).count();
                let failed = agents.iter().filter(|r| r.status == AgentStatus::Failed).count();
                output.push_str(&format!(
                    "\nSummary: {completed} completed, {running} running, {failed} failed"
                ));

                ok_result(tool_use_id, output)
            }
        }
    }
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use code_types::permissions::ToolPermissionContext;
    use code_types::tool::FileReadingLimits;
    use code_types::tool::GlobLimits;
    use tempfile::TempDir;

    fn test_ctx(session_dir: &Path) -> ToolContext {
        ToolContext {
            cwd: session_dir.to_path_buf(),
            session_id: "test-session".to_owned(),
            session_dir: session_dir.to_path_buf(),
            permission_ctx: ToolPermissionContext::default(),
            file_reading_limits: FileReadingLimits::default(),
            glob_limits: GlobLimits::default(),
        }
    }

    #[tokio::test]
    async fn spawn_creates_pending_record() {
        let tmp = TempDir::new().unwrap();
        let ctx = test_ctx(tmp.path());

        let tool = SpawnAgentTool::new();
        let result = tool
            .call(
                "t1",
                json!({"role": "tester", "prompt": "Run tests"}),
                &ctx,
                None,
            )
            .await;

        assert!(!result.is_error);
        let rec = read_agent(tmp.path(), "agent-t1").await.unwrap();
        assert_eq!(rec.status, AgentStatus::Pending);
        assert_eq!(rec.role, "tester");
    }

    #[tokio::test]
    async fn spawn_with_runner_creates_running_record() {
        struct MockRunner;
        #[async_trait]
        impl CoordinatorAgentRunner for MockRunner {
            async fn run_agent(
                &self,
                _role: &str,
                _prompt: &str,
                _cwd: &Path,
                _session_dir: &Path,
            ) -> anyhow::Result<AgentRunOutput> {
                // Simulate some work.
                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                Ok(AgentRunOutput {
                    text: "Done!".to_owned(),
                    cost_usd: 0.01,
                })
            }
        }

        let tmp = TempDir::new().unwrap();
        let ctx = test_ctx(tmp.path());

        let runner = Arc::new(MockRunner);
        let tool = SpawnAgentTool::with_runner(runner);
        let result = tool
            .call(
                "t2",
                json!({"role": "builder", "prompt": "Build project"}),
                &ctx,
                None,
            )
            .await;

        assert!(!result.is_error);
        // Immediately after spawn, status should be Running.
        let rec = read_agent(tmp.path(), "agent-t2").await.unwrap();
        assert_eq!(rec.status, AgentStatus::Running);

        // Wait for background task to finish.
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;

        let rec = read_agent(tmp.path(), "agent-t2").await.unwrap();
        assert_eq!(rec.status, AgentStatus::Completed);
        assert_eq!(rec.output.as_deref(), Some("Done!"));
        assert_eq!(rec.cost_usd, Some(0.01));
    }

    #[tokio::test]
    async fn get_output_specific_agent() {
        let tmp = TempDir::new().unwrap();
        let ctx = test_ctx(tmp.path());

        // Write a completed record.
        let rec = AgentRecord {
            id: "a1".to_owned(),
            role: "tester".to_owned(),
            prompt: "Run tests".to_owned(),
            status: AgentStatus::Completed,
            output: Some("All tests passed".to_owned()),
            error: None,
            cost_usd: Some(0.005),
            created_at: unix_now(),
            completed_at: Some(unix_now()),
        };
        write_agent(tmp.path(), &rec).await.unwrap();

        let tool = GetAgentOutputTool;
        let result = tool
            .call("t3", json!({"agent_id": "a1"}), &ctx, None)
            .await;

        assert!(!result.is_error);
        let text = match &result.content {
            code_types::tool::ToolResultPayload::Text(t) => t.clone(),
            _ => String::new(),
        };
        assert!(text.contains("All tests passed"));
        assert!(text.contains("completed"));
    }

    #[tokio::test]
    async fn get_output_list_all() {
        let tmp = TempDir::new().unwrap();
        let ctx = test_ctx(tmp.path());

        // Write two records.
        for (id, status) in &[("a1", AgentStatus::Completed), ("a2", AgentStatus::Running)] {
            let rec = AgentRecord {
                id: id.to_string(),
                role: "worker".to_owned(),
                prompt: "Do stuff".to_owned(),
                status: status.clone(),
                output: None,
                error: None,
                cost_usd: None,
                created_at: unix_now(),
                completed_at: None,
            };
            write_agent(tmp.path(), &rec).await.unwrap();
        }

        let tool = GetAgentOutputTool;
        let result = tool.call("t4", json!({}), &ctx, None).await;

        assert!(!result.is_error);
        let text = match &result.content {
            code_types::tool::ToolResultPayload::Text(t) => t.clone(),
            _ => String::new(),
        };
        assert!(text.contains("2 agent(s)"));
        assert!(text.contains("1 completed"));
        assert!(text.contains("1 running"));
    }

    #[tokio::test]
    async fn get_output_not_found() {
        let tmp = TempDir::new().unwrap();
        let ctx = test_ctx(tmp.path());

        let tool = GetAgentOutputTool;
        let result = tool
            .call("t5", json!({"agent_id": "nonexistent"}), &ctx, None)
            .await;

        assert!(result.is_error);
    }

    #[tokio::test]
    async fn spawn_with_failing_runner() {
        struct FailRunner;
        #[async_trait]
        impl CoordinatorAgentRunner for FailRunner {
            async fn run_agent(
                &self,
                _role: &str,
                _prompt: &str,
                _cwd: &Path,
                _session_dir: &Path,
            ) -> anyhow::Result<AgentRunOutput> {
                anyhow::bail!("LLM API unavailable")
            }
        }

        let tmp = TempDir::new().unwrap();
        let ctx = test_ctx(tmp.path());

        let runner = Arc::new(FailRunner);
        let tool = SpawnAgentTool::with_runner(runner);
        let _result = tool
            .call(
                "t6",
                json!({"role": "explorer", "prompt": "Search code"}),
                &ctx,
                None,
            )
            .await;

        // Wait for background task.
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        let rec = read_agent(tmp.path(), "agent-t6").await.unwrap();
        assert_eq!(rec.status, AgentStatus::Failed);
        assert!(rec.error.as_deref().unwrap().contains("LLM API unavailable"));
    }
}
