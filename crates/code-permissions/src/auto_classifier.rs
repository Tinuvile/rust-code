//! Auto-mode permission classifier — LLM-driven tool safety evaluation.
//!
//! When the permission mode is `Auto`, the evaluator returns `Ask` for tool
//! calls that aren't explicitly allowed/denied.  This module provides an
//! async classifier that uses a side-query to the LLM to decide whether the
//! tool call is safe to auto-approve.
//!
//! The classifier is a two-stage process:
//!
//! 1. **Fast stage**: A short max_tokens query with stop sequences — returns
//!    quickly if the tool is clearly safe.
//! 2. **Reasoning stage**: If stage 1 blocks, a longer query with
//!    chain-of-thought reasoning is issued.
//!
//! The response format uses simple XML:
//! ```text
//! <assessment>Brief reasoning</assessment>
//! <block>false</block>
//! ```
//!
//! Ref: src/utils/permissions/yoloClassifier.ts

use std::path::Path;

use tracing::{debug, warn};

use code_types::message::{
    ApiMessage, ApiRole, ContentBlock, Message, TextBlock,
};
use code_types::provider::LlmProvider;
use code_types::provider::LlmRequest;

// ── Public types ─────────────────────────────────────────────────────────────

/// Result of the auto-mode classifier.
#[derive(Debug, Clone)]
pub struct ClassifierDecision {
    /// Whether the tool call should be allowed.
    pub allow: bool,
    /// Human-readable reasoning from the classifier.
    pub reasoning: String,
    /// Tokens used by the classifier query.
    pub input_tokens: u32,
    pub output_tokens: u32,
}

/// Configuration for building classifier prompts.
#[derive(Debug, Clone, Default)]
pub struct ClassifierConfig {
    /// Additional allow rules from settings (e.g. `["Bash(git *)"]`).
    pub allow_rules: Vec<String>,
    /// Additional deny rules from settings.
    pub deny_rules: Vec<String>,
    /// Free-form environment description for the classifier.
    pub environment_desc: Option<String>,
}

// ── Classifier entry point ───────────────────────────────────────────────────

/// Classify whether a tool call should be auto-approved.
///
/// This is the main entry point for the auto-mode classifier.  It sends a
/// compact side-query to the LLM and parses the decision from the response.
///
/// Returns `None` if the classifier fails (network error, parse error, etc.)
/// — callers should fall back to asking the user.
pub async fn classify_tool_call(
    provider: &dyn LlmProvider,
    model: &str,
    tool_name: &str,
    tool_content: Option<&str>,
    tool_input: Option<&serde_json::Value>,
    conversation: &[Message],
    cwd: &Path,
    config: &ClassifierConfig,
) -> Option<ClassifierDecision> {
    let system_prompt = build_system_prompt(config, cwd);
    let user_prompt = build_user_prompt(
        tool_name,
        tool_content,
        tool_input,
        conversation,
    );

    // ── Stage 1: Fast classification ─────────────────────────────────────────
    let fast_decision = run_classifier_query(
        provider,
        model,
        &system_prompt,
        &user_prompt,
        64, // max_tokens — just enough for the XML tags
    )
    .await;

    match fast_decision {
        Some(d) if d.allow => {
            debug!(
                "auto-classifier: stage 1 ALLOW — {}",
                d.reasoning
            );
            return Some(d);
        }
        Some(d) => {
            debug!(
                "auto-classifier: stage 1 blocked, escalating to stage 2 — {}",
                d.reasoning
            );
            // Fall through to stage 2.
        }
        None => {
            debug!("auto-classifier: stage 1 failed, escalating to stage 2");
            // Fall through to stage 2.
        }
    }

    // ── Stage 2: Reasoning classification ────────────────────────────────────
    let reasoning_prompt = format!(
        "{user_prompt}\n\nPlease think step-by-step about the safety of this tool call before deciding."
    );

    let decision = run_classifier_query(
        provider,
        model,
        &system_prompt,
        &reasoning_prompt,
        512,
    )
    .await;

    match &decision {
        Some(d) if d.allow => {
            debug!("auto-classifier: stage 2 ALLOW — {}", d.reasoning);
        }
        Some(d) => {
            debug!("auto-classifier: stage 2 BLOCK — {}", d.reasoning);
        }
        None => {
            warn!("auto-classifier: stage 2 failed, falling back to user prompt");
        }
    }

    decision
}

// ── Internal helpers ─────────────────────────────────────────────────────────

async fn run_classifier_query(
    provider: &dyn LlmProvider,
    model: &str,
    system_prompt: &str,
    user_prompt: &str,
    max_tokens: u32,
) -> Option<ClassifierDecision> {
    let messages = vec![ApiMessage {
        role: ApiRole::User,
        content: vec![ContentBlock::Text(TextBlock {
            text: user_prompt.to_owned(),
            cache_control: None,
        })],
    }];

    let request = LlmRequest {
        model: model.to_owned(),
        messages,
        max_tokens,
        system: Some(serde_json::Value::String(system_prompt.to_owned())),
        tools: vec![],
        temperature: Some(0.0), // deterministic
        thinking: None,
        top_p: None,
    };

    let result = provider.send(request).await;

    match result {
        Ok(response) => {
            let text: String = response
                .content
                .iter()
                .filter_map(|b| {
                    if let ContentBlock::Text(t) = b {
                        Some(t.text.as_str())
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>()
                .join("");

            parse_classifier_response(
                &text,
                response.usage.input_tokens,
                response.usage.output_tokens,
            )
        }
        Err(e) => {
            warn!("auto-classifier query failed: {e}");
            None
        }
    }
}

// ── Prompt building ──────────────────────────────────────────────────────────

fn build_system_prompt(config: &ClassifierConfig, cwd: &Path) -> String {
    let mut prompt = String::from(
        "You are a permission safety classifier for an AI coding assistant. \
         Your task is to evaluate whether a tool call should be automatically \
         allowed or blocked.\n\n\
         IMPORTANT: You must be conservative. When in doubt, block the action.\n\n\
         ## Decision criteria\n\n\
         ALLOW a tool call if:\n\
         - It is a read-only operation (listing files, reading content, searching)\n\
         - It modifies only project files within the working directory\n\
         - It runs standard development commands (build, test, lint, format)\n\
         - It matches an explicit allow rule\n\n\
         BLOCK a tool call if:\n\
         - It could delete important files or data\n\
         - It runs commands that affect system state (install packages, modify configs)\n\
         - It accesses files outside the project directory\n\
         - It could expose secrets or sensitive data\n\
         - It runs network operations that could have side effects\n\
         - It matches an explicit deny rule\n\
         - The intent is unclear or potentially destructive\n\n",
    );

    // Environment context.
    prompt.push_str(&format!(
        "## Environment\n\nWorking directory: {}\n",
        cwd.display()
    ));

    if let Some(ref env_desc) = config.environment_desc {
        prompt.push_str(&format!("Environment notes: {env_desc}\n"));
    }

    // Permission rules.
    if !config.allow_rules.is_empty() {
        prompt.push_str("\n## Explicit allow rules\n\n");
        for rule in &config.allow_rules {
            prompt.push_str(&format!("- {rule}\n"));
        }
    }
    if !config.deny_rules.is_empty() {
        prompt.push_str("\n## Explicit deny rules\n\n");
        for rule in &config.deny_rules {
            prompt.push_str(&format!("- {rule}\n"));
        }
    }

    prompt.push_str(
        "\n## Response format\n\n\
         Respond with XML only — no other text:\n\n\
         ```\n\
         <assessment>Brief reasoning about safety</assessment>\n\
         <block>true</block>\n\
         ```\n\n\
         Use `<block>true</block>` to BLOCK the action, \
         `<block>false</block>` to ALLOW it.\n",
    );

    prompt
}

fn build_user_prompt(
    tool_name: &str,
    tool_content: Option<&str>,
    tool_input: Option<&serde_json::Value>,
    conversation: &[Message],
) -> String {
    let mut prompt = String::from("## Tool call to classify\n\n");
    prompt.push_str(&format!("Tool: {tool_name}\n"));

    if let Some(content) = tool_content {
        prompt.push_str(&format!("Content: {content}\n"));
    }
    if let Some(input) = tool_input {
        let input_str = serde_json::to_string_pretty(input).unwrap_or_default();
        // Truncate very large inputs.
        if input_str.len() > 2000 {
            prompt.push_str(&format!(
                "Input (truncated): {}...\n",
                &input_str[..2000]
            ));
        } else {
            prompt.push_str(&format!("Input: {input_str}\n"));
        }
    }

    // Build a compact transcript from recent conversation.
    let transcript = build_transcript(conversation);
    if !transcript.is_empty() {
        prompt.push_str(&format!(
            "\n## Recent conversation context\n\n<transcript>\n{transcript}\n</transcript>\n"
        ));
    }

    prompt
}

/// Build a compact transcript from conversation messages.
///
/// We include only the last few exchanges to keep the classifier prompt small.
/// Text is truncated per-message to avoid overwhelming the classifier.
fn build_transcript(conversation: &[Message]) -> String {
    // Take last N messages (enough for context without blowing up the budget).
    const MAX_MESSAGES: usize = 10;
    const MAX_TEXT_PER_MSG: usize = 500;

    let start = conversation.len().saturating_sub(MAX_MESSAGES);
    let recent = &conversation[start..];

    let mut lines = Vec::new();
    for msg in recent {
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
                let text = text.trim().to_owned();
                if !text.is_empty() {
                    let truncated = truncate_str(&text, MAX_TEXT_PER_MSG);
                    lines.push(format!("User: {truncated}"));
                }
            }
            Message::Assistant(a) => {
                let text: String = a
                    .content
                    .iter()
                    .filter_map(|b| match b {
                        ContentBlock::Text(t) => Some(t.text.clone()),
                        ContentBlock::ToolUse(tu) => {
                            Some(format!("[tool_use: {}]", tu.name))
                        }
                        _ => None,
                    })
                    .collect::<Vec<_>>()
                    .join(" ");
                let text = text.trim().to_owned();
                if !text.is_empty() {
                    let truncated = truncate_str(&text, MAX_TEXT_PER_MSG);
                    lines.push(format!("Assistant: {truncated}"));
                }
            }
            _ => {}
        }
    }
    lines.join("\n")
}

fn truncate_str(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_owned()
    } else {
        // Find a char boundary.
        let end = s
            .char_indices()
            .take_while(|&(i, _)| i < max_len)
            .last()
            .map(|(i, c)| i + c.len_utf8())
            .unwrap_or(max_len);
        format!("{}...", &s[..end])
    }
}

// ── Response parsing ─────────────────────────────────────────────────────────

/// Parse the classifier's XML response.
///
/// Expected format:
/// ```text
/// <assessment>Some reasoning</assessment>
/// <block>true</block>
/// ```
fn parse_classifier_response(
    text: &str,
    input_tokens: u32,
    output_tokens: u32,
) -> Option<ClassifierDecision> {
    // Extract <block>...</block>.
    let block_value = extract_xml_tag(text, "block")?;
    let should_block = match block_value.trim().to_lowercase().as_str() {
        "true" | "yes" | "1" => true,
        "false" | "no" | "0" => false,
        _ => {
            warn!(
                "auto-classifier: unrecognised <block> value: '{block_value}'"
            );
            // Fail safe — block.
            true
        }
    };

    let reasoning = extract_xml_tag(text, "assessment")
        .unwrap_or_else(|| "No reasoning provided".to_owned());

    Some(ClassifierDecision {
        allow: !should_block,
        reasoning,
        input_tokens,
        output_tokens,
    })
}

/// Extract the inner text of a simple XML tag from a string.
fn extract_xml_tag(text: &str, tag: &str) -> Option<String> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let start = text.find(&open)?;
    let end = text.find(&close)?;
    if end <= start {
        return None;
    }
    let inner = &text[start + open.len()..end];
    Some(inner.trim().to_owned())
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_allow_response() {
        let text = "<assessment>Reading a file is safe</assessment>\n<block>false</block>";
        let decision = parse_classifier_response(text, 100, 20).unwrap();
        assert!(decision.allow);
        assert_eq!(decision.reasoning, "Reading a file is safe");
        assert_eq!(decision.input_tokens, 100);
    }

    #[test]
    fn parse_block_response() {
        let text = "<assessment>This command deletes system files</assessment>\n<block>true</block>";
        let decision = parse_classifier_response(text, 200, 30).unwrap();
        assert!(!decision.allow);
        assert!(decision.reasoning.contains("deletes system files"));
    }

    #[test]
    fn parse_missing_block_returns_none() {
        let text = "<assessment>Some reasoning</assessment>";
        assert!(parse_classifier_response(text, 0, 0).is_none());
    }

    #[test]
    fn parse_unknown_block_value_is_safe_block() {
        let text = "<assessment>Unclear</assessment>\n<block>maybe</block>";
        let decision = parse_classifier_response(text, 0, 0).unwrap();
        // Fail safe — treat as blocked.
        assert!(!decision.allow);
    }

    #[test]
    fn extract_xml_tag_works() {
        assert_eq!(
            extract_xml_tag("<foo>bar</foo>", "foo"),
            Some("bar".to_owned())
        );
        assert_eq!(
            extract_xml_tag("prefix<a>  hello world  </a>suffix", "a"),
            Some("hello world".to_owned())
        );
        assert_eq!(extract_xml_tag("no tags here", "foo"), None);
    }

    #[test]
    fn truncate_str_short() {
        assert_eq!(truncate_str("hello", 10), "hello");
    }

    #[test]
    fn truncate_str_long() {
        let s = "a".repeat(200);
        let t = truncate_str(&s, 100);
        assert!(t.len() <= 104); // 100 + "..."
        assert!(t.ends_with("..."));
    }

    #[test]
    fn system_prompt_includes_rules() {
        let config = ClassifierConfig {
            allow_rules: vec!["Bash(git *)".to_owned()],
            deny_rules: vec!["Bash(rm -rf *)".to_owned()],
            environment_desc: Some("Rust project".to_owned()),
        };
        let prompt = build_system_prompt(&config, Path::new("/home/user/project"));
        assert!(prompt.contains("Bash(git *)"));
        assert!(prompt.contains("Bash(rm -rf *)"));
        assert!(prompt.contains("Rust project"));
        assert!(prompt.contains("/home/user/project"));
    }

    #[test]
    fn user_prompt_includes_tool_info() {
        let prompt = build_user_prompt(
            "Bash",
            Some("npm install express"),
            None,
            &[],
        );
        assert!(prompt.contains("Tool: Bash"));
        assert!(prompt.contains("npm install express"));
    }

    #[test]
    fn transcript_truncates_old_messages() {
        use code_types::message::UserMessage;
        let mut msgs = Vec::new();
        for i in 0..20 {
            msgs.push(Message::User(UserMessage {
                uuid: uuid::Uuid::new_v4(),
                content: vec![ContentBlock::Text(TextBlock {
                    text: format!("Message {i}"),
                    cache_control: None,
                })],
                is_api_error_message: false,
                agent_id: None,
            }));
        }
        let transcript = build_transcript(&msgs);
        // Only last 10 should appear.
        assert!(!transcript.contains("Message 0"));
        assert!(transcript.contains("Message 19"));
    }
}
