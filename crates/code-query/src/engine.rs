//! Main query engine — the core `human→model→tools→model` loop.
//!
//! `QueryEngine::query()` drives a single user turn to completion:
//!   1. Append the user message to the conversation.
//!   2. Build system prompt from memory entries.
//!   3. Call `run_pipeline_turn()` to get an assistant message.
//!   4. If `stop_reason == "tool_use"`, run tools and loop.
//!   5. Emit a `SystemTurnDuration` message and return.
//!
//! Ref: src/QueryEngine.ts (query, *Turn, processTool*)

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use code_permissions::denial_tracking::DenialTrackingState;
use code_permissions::evaluator::PermissionEvaluator;
use code_tools::orchestration::run_tool_turn;
use code_tools::{NoopHookRunner, ToolContext, ToolRegistry};
use code_types::message::{Message, SystemTurnDurationMessage, UserMessage};
use code_types::permissions::ToolPermissionContext;
use code_types::tool::{FileReadingLimits, GlobLimits};
use uuid::Uuid;

use crate::attribution::SessionAttribution;
use crate::interruption::InterruptionSignal;
use crate::message_queue::MessageQueue;
use crate::messages::{extract_tool_use_blocks, is_tool_use_stop, tool_results_message};
use crate::pipeline::{run_pipeline_turn, PipelineConfig};
use crate::system_prompt::{build_system_prompt, SystemPromptConfig};
use crate::token_budget::TokenBudget;

// ── Configuration ─────────────────────────────────────────────────────────────

/// Static configuration for a `QueryEngine` instance.
#[derive(Clone)]
pub struct QueryEngineConfig {
    /// Model identifier (e.g. `"claude-sonnet-4-6"`).
    pub model: String,
    /// Working directory for the session.
    pub cwd: PathBuf,
    /// Session ID string (used for ToolContext).
    pub session_id: String,
    /// Session directory for tool results / todos.
    pub session_dir: PathBuf,
    /// Permission context snapshot.
    pub permission_ctx: ToolPermissionContext,
    /// Optional appendix appended after the base system prompt.
    pub system_appendix: Option<String>,
}

// ── QueryEngine ───────────────────────────────────────────────────────────────

/// The core query engine.
///
/// Holds shared state for the duration of a session.
pub struct QueryEngine {
    client: code_api::client::AnthropicClient,
    config: QueryEngineConfig,
    queue: MessageQueue,
    interruption: InterruptionSignal,
    attribution: Arc<Mutex<SessionAttribution>>,
    token_budget: Arc<Mutex<TokenBudget>>,
    denial_state: Arc<Mutex<DenialTrackingState>>,
}

impl QueryEngine {
    /// Create a new `QueryEngine`.
    pub fn new(
        client: code_api::client::AnthropicClient,
        config: QueryEngineConfig,
    ) -> Self {
        let token_budget = TokenBudget::for_model(&config.model);
        Self {
            client,
            config: config.clone(),
            queue: MessageQueue::new(),
            interruption: InterruptionSignal::new(),
            attribution: Arc::new(Mutex::new(SessionAttribution::new())),
            token_budget: Arc::new(Mutex::new(token_budget)),
            denial_state: Arc::new(Mutex::new(DenialTrackingState::new())),
        }
    }

    /// Subscribe to the message queue.
    ///
    /// Subscribers receive every `Message` published during `query()`.
    pub fn subscribe(&self) -> crate::message_queue::MessageReceiver {
        self.queue.subscribe()
    }

    /// Return the interruption signal.
    pub fn interruption_signal(&self) -> InterruptionSignal {
        self.interruption.clone()
    }

    /// Total cost accumulated across all turns.
    pub fn total_cost_usd(&self) -> f64 {
        self.attribution.lock().unwrap().total_cost_usd()
    }

    /// Execute a user turn to completion.
    ///
    /// Appends the user message and any assistant/tool messages to `conversation`.
    /// Returns `Ok(())` when the turn ends with `stop_reason = "end_turn"` or after
    /// an interruption.
    pub async fn query(
        &self,
        user_message: UserMessage,
        conversation: &mut Vec<Message>,
    ) -> anyhow::Result<()> {
        let turn_start = std::time::Instant::now();

        // Reset interruption for this turn.
        self.interruption.reset();

        // Append and broadcast the user message.
        conversation.push(Message::User(user_message));
        // Publish last appended message to queue.
        if let Some(last) = conversation.last() {
            self.queue.publish(last.clone());
        }

        // Build tool context.
        let tool_ctx = ToolContext {
            cwd: self.config.cwd.clone(),
            session_id: self.config.session_id.clone(),
            session_dir: self.config.session_dir.clone(),
            permission_ctx: self.config.permission_ctx.clone(),
            file_reading_limits: FileReadingLimits::default(),
            glob_limits: GlobLimits::default(),
        };

        // Build tool registry.
        let registry = ToolRegistry::with_default_tools(&self.config.cwd);

        // Build API tool list.
        let api_tools = registry
            .to_api_tools()
            .into_iter()
            .map(|t| code_api::client::ApiTool {
                name: t["name"].as_str().unwrap_or("").to_owned(),
                description: t["description"].as_str().unwrap_or("").to_owned(),
                input_schema: t["input_schema"].clone(),
            })
            .collect();

        let evaluator = PermissionEvaluator::new(self.config.cwd.clone());

        // Load memory entries.
        let memory_entries =
            code_memory::loader::load_memory_entries(&self.config.cwd).await;

        // Build system prompt.
        let prompt_config = SystemPromptConfig {
            model: self.config.model.clone(),
            cwd: self.config.cwd.clone(),
            appendix: self.config.system_appendix.clone(),
            extended_thinking: false,
        };
        let system = build_system_prompt(&prompt_config, &memory_entries).await;

        let pipeline_config = PipelineConfig {
            model: self.config.model.clone(),
            system,
            tools: api_tools,
            thinking: None,
            retry_policy: code_api::retry::RetryPolicy::default(),
        };

        // ── Main loop ────────────────────────────────────────────────────────
        loop {
            if self.interruption.is_set() {
                tracing::info!("query interrupted by user");
                break;
            }

            let (assistant_msg, turn_cost) =
                run_pipeline_turn(conversation, &pipeline_config, &self.client, &self.queue)
                    .await?;

            // Update token budget.
            {
                let mut budget = self.token_budget.lock().unwrap();
                budget.update(assistant_msg.usage.input_tokens);
            }

            // Record attribution.
            {
                let mut attr = self.attribution.lock().unwrap();
                attr.record_turn(turn_cost, &assistant_msg.model);
            }

            conversation.push(Message::Assistant(assistant_msg.clone()));

            if !is_tool_use_stop(&assistant_msg) {
                // end_turn or max_tokens — done.
                break;
            }

            if self.interruption.is_set() {
                break;
            }

            // Extract tool calls and run them.
            let tool_blocks = extract_tool_use_blocks(&assistant_msg);
            if tool_blocks.is_empty() {
                break;
            }

            let mut denial_state = self.denial_state.lock().unwrap();
            let hook_runner = NoopHookRunner;

            let tool_results = run_tool_turn(
                tool_blocks,
                &registry,
                &tool_ctx,
                &evaluator,
                &mut denial_state,
                &hook_runner,
                None,
            )
            .await;
            drop(denial_state);

            // Append tool results as a new user message.
            let results_msg = tool_results_message(&tool_results);
            conversation.push(Message::User(results_msg));
            if let Some(last) = conversation.last() {
                self.queue.publish(last.clone());
            }
        }

        // Publish turn duration.
        let duration_ms = turn_start.elapsed().as_millis() as u64;
        let (total_in, total_out) = {
            let attr = self.attribution.lock().unwrap();
            if let Some(last) = attr.last_turn() {
                (last.input_tokens, last.output_tokens)
            } else {
                (0, 0)
            }
        };
        let cost = self.total_cost_usd();
        self.queue.publish(Message::SystemTurnDuration(SystemTurnDurationMessage {
            uuid: Uuid::new_v4(),
            duration_ms,
            total_input_tokens: total_in,
            total_output_tokens: total_out,
            cost_usd: cost,
        }));

        Ok(())
    }
}

// ── Convenience builder ───────────────────────────────────────────────────────

/// Build a `QueryEngineConfig` with sensible defaults.
pub fn engine_config_from_api_key(
    api_key: &str,
    model: impl Into<String>,
    cwd: PathBuf,
    session_id: impl Into<String>,
    session_dir: PathBuf,
) -> (code_api::client::AnthropicClient, QueryEngineConfig) {
    let client = code_api::client::AnthropicClient::new(
        code_api::client::ClientConfig::from_api_key(api_key.to_owned()),
    );
    let config = QueryEngineConfig {
        model: model.into(),
        cwd,
        session_id: session_id.into(),
        session_dir,
        permission_ctx: ToolPermissionContext::default(),
        system_appendix: None,
    };
    (client, config)
}
