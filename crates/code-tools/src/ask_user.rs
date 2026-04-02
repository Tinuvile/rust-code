//! AskUserQuestionTool — request input from the user during a tool turn.
//!
//! In interactive mode (Phase 9 TUI), this tool sends a request through a
//! channel and blocks until the user responds.  In Phase 5 (no TUI) the tool
//! stubs its channel to return a "non-interactive session" error, which is the
//! correct behaviour for background agents.
//!
//! Ref: src/tools/AskUserQuestionTool/AskUserQuestionTool.ts

use std::path::Path;
use std::sync::Arc;

use async_trait::async_trait;
use code_permissions::evaluator::ToolCallContext;
use code_types::tool::{ToolInputSchema, ToolResult, ToolResultPayload, ValidationResult};
use serde::Deserialize;
use serde_json::{json, Value};
use tokio::sync::{oneshot, Mutex};

use crate::{error_result, ProgressSender, Tool, ToolContext};

// ── Channel types ─────────────────────────────────────────────────────────────

/// A pending user-prompt request sent from this tool to the TUI layer.
pub struct UserPromptRequest {
    /// The question to display.
    pub question: String,
    /// Options offered to the user (empty = free-form text input).
    pub options: Vec<String>,
    /// Channel to send the user's answer back through.
    pub reply: oneshot::Sender<String>,
}

/// Sender half of the user-prompt channel.
///
/// The TUI (Phase 9) holds the receiver and drives the dialog.
/// `None` means no interactive session is attached.
pub type UserPromptSender = Arc<Mutex<Option<tokio::sync::mpsc::UnboundedSender<UserPromptRequest>>>>;

// ── Input ─────────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct AskUserInput {
    question: String,
    options: Option<Vec<String>>,
}

// ── Tool ─────────────────────────────────────────────────────────────────────

pub struct AskUserQuestionTool {
    /// Channel to the interactive TUI layer.  `None` in non-interactive mode.
    prompt_tx: UserPromptSender,
}

impl AskUserQuestionTool {
    /// Create the tool with no interactive session attached.
    pub fn new() -> Self {
        Self {
            prompt_tx: Arc::new(Mutex::new(None)),
        }
    }

    /// Attach an interactive TUI sender.
    ///
    /// Called by the TUI layer (Phase 9) during startup.
    pub fn with_sender(tx: tokio::sync::mpsc::UnboundedSender<UserPromptRequest>) -> Self {
        Self {
            prompt_tx: Arc::new(Mutex::new(Some(tx))),
        }
    }
}

impl Default for AskUserQuestionTool {
    fn default() -> Self { Self::new() }
}

#[async_trait]
impl Tool for AskUserQuestionTool {
    fn name(&self) -> &str { "AskUserQuestion" }

    fn description(&self) -> &str {
        "Asks the user a question and waits for their response. \
        Use this when you need clarification or additional information from the user \
        that cannot be inferred from the context. \
        Only use this tool when absolutely necessary — prefer making reasonable \
        assumptions when possible."
    }

    fn input_schema(&self) -> ToolInputSchema {
        json!({
            "type": "object",
            "properties": {
                "question": {
                    "type": "string",
                    "description": "The question to ask the user"
                },
                "options": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Optional list of choices to present to the user"
                }
            },
            "required": ["question"]
        })
    }

    fn is_read_only(&self, _input: &Value) -> bool { true }

    fn is_concurrency_safe(&self, _input: &Value) -> bool { false }

    async fn validate_input(&self, input: &Value, _ctx: &ToolContext) -> ValidationResult {
        match input.get("question").and_then(|v| v.as_str()) {
            Some(q) if !q.is_empty() => ValidationResult::ok(),
            _ => ValidationResult::err("question is required and must not be empty", 1),
        }
    }

    fn permission_context<'a>(&'a self, input: &'a Value, cwd: &'a Path) -> ToolCallContext<'a> {
        ToolCallContext {
            tool_name: self.name(),
            content: input.get("question").and_then(|v| v.as_str()),
            input: Some(input),
            is_read_only: true,
            cwd,
        }
    }

    async fn call(
        &self,
        tool_use_id: &str,
        input: Value,
        _ctx: &ToolContext,
        _progress: Option<&ProgressSender>,
    ) -> ToolResult {
        let parsed: AskUserInput = match serde_json::from_value(input) {
            Ok(v) => v,
            Err(e) => return error_result(tool_use_id, format!("Invalid input: {e}")),
        };

        let guard = self.prompt_tx.lock().await;
        let tx = match guard.as_ref() {
            Some(t) => t,
            None => {
                // Non-interactive session — return a clear error.
                return error_result(
                    tool_use_id,
                    "AskUserQuestion requires an interactive session. \
                     This tool cannot be used in non-interactive or background modes.",
                );
            }
        };

        // Send the prompt request and await the reply.
        let (reply_tx, reply_rx) = oneshot::channel();
        let request = UserPromptRequest {
            question: parsed.question,
            options: parsed.options.unwrap_or_default(),
            reply: reply_tx,
        };

        if tx.send(request).is_err() {
            return error_result(tool_use_id, "Failed to send prompt request to UI.");
        }
        drop(guard);

        match tokio::time::timeout(std::time::Duration::from_secs(300), reply_rx).await {
            Ok(Ok(answer)) => ToolResult {
                tool_use_id: tool_use_id.to_owned(),
                content: ToolResultPayload::Text(answer),
                is_error: false,
                was_truncated: false,
            },
            Ok(Err(_)) => error_result(tool_use_id, "UI closed before answering the prompt."),
            Err(_) => error_result(tool_use_id, "Timed out waiting for user response (5 minutes)."),
        }
    }
}
