//! Background memory auto-extraction.
//!
//! Analyzes a conversation using an LLM side-query to extract useful memory
//! entries (project conventions, preferences, patterns) and writes them to
//! the user's memdir (`~/.claude/memory/`).
//!
//! The extraction runs in a background tokio task and communicates completion
//! via a oneshot channel.
//!
//! Ref: src/services/extractMemories/

use std::path::{Path, PathBuf};
use std::sync::Arc;

use code_types::message::{ContentBlock, Message};
use code_types::provider::LlmProvider;

use crate::paths::global_memdir_path;

// ── Handle ────────────────────────────────────────────────────────────────────

/// A handle returned when background memory extraction is requested.
///
/// The `result_rx` receiver will resolve when extraction finishes.
/// - `Ok(summary)` → extraction succeeded, summary describes what was saved.
/// - `Err(...)` → extraction failed or was cancelled.
pub struct AutoExtractHandle {
    /// Completion channel.
    pub result_rx: tokio::sync::oneshot::Receiver<anyhow::Result<String>>,
}

// ── Configuration ─────────────────────────────────────────────────────────────

/// Configuration for memory auto-extraction.
pub struct AutoExtractConfig {
    /// The LLM model to use for the extraction side-query.
    pub model: String,
    /// Working directory (for project context).
    pub cwd: PathBuf,
    /// Maximum number of recent messages to include in the transcript.
    pub max_messages: usize,
    /// Maximum characters per message in the transcript.
    pub max_chars_per_message: usize,
}

impl Default for AutoExtractConfig {
    fn default() -> Self {
        Self {
            model: "claude-sonnet-4-6".to_owned(),
            cwd: PathBuf::from("."),
            max_messages: 30,
            max_chars_per_message: 1000,
        }
    }
}

// ── Trigger ───────────────────────────────────────────────────────────────────

/// Request background memory extraction from a conversation.
///
/// Spawns a background task that:
/// 1. Builds a compact transcript of the conversation
/// 2. Sends an extraction prompt to the LLM
/// 3. Parses the response for memory entries
/// 4. Writes new entries to `~/.claude/memory/`
///
/// Returns a handle whose receiver resolves when done.
pub fn trigger_auto_extract(
    conversation: &[Message],
    provider: Arc<dyn LlmProvider>,
    config: AutoExtractConfig,
) -> AutoExtractHandle {
    let transcript = build_transcript(conversation, config.max_messages, config.max_chars_per_message);
    let (tx, rx) = tokio::sync::oneshot::channel::<anyhow::Result<String>>();

    if transcript.is_empty() {
        // Nothing to extract — complete immediately.
        let _ = tx.send(Ok("No conversation content to extract from.".to_owned()));
        return AutoExtractHandle { result_rx: rx };
    }

    let model = config.model;
    let cwd = config.cwd;

    tokio::spawn(async move {
        let result = run_extraction(&transcript, &cwd, &*provider, &model).await;
        let _ = tx.send(result);
    });

    AutoExtractHandle { result_rx: rx }
}

/// Trigger with no-op behavior (for environments without an LLM provider).
///
/// Returns a handle whose receiver immediately closes.
pub fn trigger_auto_extract_noop(
    _conversation: &[Message],
    _cwd: &Path,
) -> AutoExtractHandle {
    let (tx, rx) = tokio::sync::oneshot::channel::<anyhow::Result<String>>();
    drop(tx);
    AutoExtractHandle { result_rx: rx }
}

// ── Core extraction logic ───────────────────────────────────────────────────

async fn run_extraction(
    transcript: &str,
    cwd: &Path,
    provider: &dyn LlmProvider,
    model: &str,
) -> anyhow::Result<String> {
    let system_prompt = build_system_prompt(cwd);
    let user_prompt = build_user_prompt(transcript);

    // Build the LLM request.
    let request = code_types::provider::LlmRequest {
        model: model.to_owned(),
        messages: vec![code_types::message::ApiMessage {
            role: code_types::message::ApiRole::User,
            content: vec![ContentBlock::Text(code_types::message::TextBlock {
                text: user_prompt,
                cache_control: None,
            })],
        }],
        max_tokens: 1024,
        system: Some(serde_json::json!([{
            "type": "text",
            "text": system_prompt,
        }])),
        tools: vec![],
        temperature: Some(0.0),
        thinking: None,
        top_p: None,
    };

    let response = provider.send(request).await
        .map_err(|e| anyhow::anyhow!("Memory extraction LLM call failed: {e}"))?;

    // Extract text from the response.
    let response_text = response
        .content
        .iter()
        .filter_map(|block| {
            if let ContentBlock::Text(t) = block {
                Some(t.text.as_str())
            } else {
                None
            }
        })
        .collect::<Vec<_>>()
        .join("\n");

    if response_text.trim().is_empty() {
        return Ok("No memories extracted (empty response).".to_owned());
    }

    // Parse entries from the response.
    let entries = parse_extraction_response(&response_text);

    if entries.is_empty() {
        return Ok("No actionable memories found in conversation.".to_owned());
    }

    // Write entries to memdir.
    let written = write_entries_to_memdir(&entries).await?;

    Ok(format!(
        "Extracted {} memory entries: {}",
        written,
        entries.iter().map(|e| e.title.as_str()).collect::<Vec<_>>().join(", ")
    ))
}

// ── Prompt construction ─────────────────────────────────────────────────────

fn build_system_prompt(cwd: &Path) -> String {
    let cwd_str = cwd.to_string_lossy();
    format!(
        r#"You are a memory extraction assistant. Your job is to analyze a conversation
between a user and an AI coding assistant and extract useful memories that should
be saved for future sessions.

The project directory is: {cwd_str}

Extract ONLY genuinely useful, reusable information such as:
- Project conventions (naming, structure, patterns)
- User preferences (code style, tools, workflows)
- Important project facts (tech stack, deployment targets, test frameworks)
- Recurring patterns or decisions

Do NOT extract:
- One-time fixes or specific bug details
- Conversation-specific context that won't be useful later
- Information that's already in standard config files (package.json, Cargo.toml, etc.)
- Obvious or trivial information

For each memory entry, respond with XML blocks:
<memory>
<title>short-kebab-case-name</title>
<content>
The actual memory content to save. Keep it concise and actionable.
</content>
</memory>

If there are no useful memories to extract, respond with:
<no_memories/>

Keep entries concise (1-3 sentences each). Maximum 5 entries per extraction."#
    )
}

fn build_user_prompt(transcript: &str) -> String {
    format!(
        "Analyze this conversation and extract useful memories:\n\n\
         <conversation>\n{transcript}\n</conversation>"
    )
}

// ── Transcript building ─────────────────────────────────────────────────────

fn build_transcript(
    conversation: &[Message],
    max_messages: usize,
    max_chars: usize,
) -> String {
    let mut parts = Vec::new();
    let recent: Vec<&Message> = conversation.iter().rev().take(max_messages).collect();

    for msg in recent.into_iter().rev() {
        match msg {
            Message::User(u) => {
                let text = extract_text_from_blocks(&u.content, max_chars);
                if !text.is_empty() {
                    parts.push(format!("User: {text}"));
                }
            }
            Message::Assistant(a) => {
                let text = extract_text_from_blocks(&a.content, max_chars);
                if !text.is_empty() {
                    parts.push(format!("Assistant: {text}"));
                }
            }
            _ => {}
        }
    }

    parts.join("\n\n")
}

fn extract_text_from_blocks(blocks: &[ContentBlock], max_chars: usize) -> String {
    let mut parts = Vec::new();
    for block in blocks {
        match block {
            ContentBlock::Text(t) => {
                let text: String = t.text.chars().take(max_chars).collect();
                if text.len() < t.text.len() {
                    parts.push(format!("{text}..."));
                } else {
                    parts.push(text);
                }
            }
            ContentBlock::ToolUse(tu) => {
                parts.push(format!("[Tool: {}]", tu.name));
            }
            ContentBlock::ToolResult(tr) => {
                parts.push(format!("[ToolResult: {}]", tr.tool_use_id));
            }
            _ => {}
        }
    }
    parts.join("\n")
}

// ── Response parsing ────────────────────────────────────────────────────────

/// A parsed memory entry from the LLM response.
#[derive(Debug, Clone)]
struct ExtractedEntry {
    title: String,
    content: String,
}

fn parse_extraction_response(response: &str) -> Vec<ExtractedEntry> {
    // Check for <no_memories/>.
    if response.contains("<no_memories") {
        return Vec::new();
    }

    let mut entries = Vec::new();
    let re_memory = regex::Regex::new(
        r"(?s)<memory>\s*<title>(.*?)</title>\s*<content>\s*(.*?)\s*</content>\s*</memory>"
    ).unwrap();

    for caps in re_memory.captures_iter(response) {
        let title = caps[1].trim().to_owned();
        let content = caps[2].trim().to_owned();

        if !title.is_empty() && !content.is_empty() {
            // Sanitize the title for use as a filename.
            let safe_title = sanitize_filename(&title);
            entries.push(ExtractedEntry {
                title: safe_title,
                content,
            });
        }
    }

    // Limit to 5 entries.
    entries.truncate(5);
    entries
}

/// Sanitize a string for use as a filename (kebab-case, no special chars).
fn sanitize_filename(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>()
        .replace("--", "-")
        .trim_matches('-')
        .to_owned()
}

// ── Writing to memdir ───────────────────────────────────────────────────────

async fn write_entries_to_memdir(entries: &[ExtractedEntry]) -> anyhow::Result<usize> {
    let memdir = global_memdir_path()
        .ok_or_else(|| anyhow::anyhow!("Cannot determine memdir path (no home directory)"))?;

    tokio::fs::create_dir_all(&memdir).await?;

    let mut written = 0;
    for entry in entries {
        let filename = format!("{}.md", entry.title);
        let path = memdir.join(&filename);

        // If the file already exists, append rather than overwrite.
        let content = if path.exists() {
            let existing = tokio::fs::read_to_string(&path).await.unwrap_or_default();
            if existing.contains(&entry.content) {
                // Duplicate — skip.
                tracing::debug!(file = %filename, "memory entry already exists, skipping");
                continue;
            }
            format!("{}\n\n{}", existing.trim(), entry.content)
        } else {
            format!("# {}\n\n{}\n", entry.title, entry.content)
        };

        tokio::fs::write(&path, &content).await?;
        tracing::info!(file = %filename, "wrote memory entry");
        written += 1;
    }

    Ok(written)
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use code_types::message::{AssistantMessage, TextBlock, TokenUsage, UserMessage};
    use uuid::Uuid;

    fn text_block(text: &str) -> ContentBlock {
        ContentBlock::Text(TextBlock {
            text: text.to_owned(),
            cache_control: None,
        })
    }

    fn user_msg(text: &str) -> Message {
        Message::User(UserMessage {
            uuid: Uuid::new_v4(),
            content: vec![text_block(text)],
            is_api_error_message: false,
            agent_id: None,
        })
    }

    fn assistant_msg(text: &str) -> Message {
        Message::Assistant(AssistantMessage {
            uuid: Uuid::new_v4(),
            content: vec![text_block(text)],
            model: "test".to_owned(),
            stop_reason: Some("end_turn".to_owned()),
            usage: TokenUsage {
                input_tokens: 10,
                output_tokens: 10,
                cache_creation_input_tokens: 0,
                cache_read_input_tokens: 0,
            },
            agent_id: None,
        })
    }

    #[test]
    fn transcript_builds_from_conversation() {
        let conversation = vec![
            user_msg("Hello, can you help me set up a Rust project?"),
            assistant_msg("Sure! What kind of project would you like?"),
            user_msg("A web server using Axum"),
        ];
        let transcript = build_transcript(&conversation, 10, 500);
        assert!(transcript.contains("User: Hello"));
        assert!(transcript.contains("Assistant: Sure!"));
        assert!(transcript.contains("User: A web server"));
    }

    #[test]
    fn transcript_respects_max_messages() {
        let conversation = vec![
            user_msg("msg1"),
            assistant_msg("msg2"),
            user_msg("msg3"),
            assistant_msg("msg4"),
        ];
        let transcript = build_transcript(&conversation, 2, 500);
        assert!(!transcript.contains("msg1"));
        assert!(!transcript.contains("msg2"));
        assert!(transcript.contains("msg3"));
        assert!(transcript.contains("msg4"));
    }

    #[test]
    fn transcript_truncates_long_messages() {
        let long_msg = "a".repeat(200);
        let conversation = vec![user_msg(&long_msg)];
        let transcript = build_transcript(&conversation, 10, 50);
        assert!(transcript.contains("..."));
        assert!(transcript.len() < 200);
    }

    #[test]
    fn parse_response_with_entries() {
        let response = r#"
Here are the memories I extracted:

<memory>
<title>project-uses-axum</title>
<content>
This project uses Axum as the web framework with Tower middleware.
</content>
</memory>

<memory>
<title>prefer-anyhow-errors</title>
<content>
The user prefers anyhow for error handling in application code.
</content>
</memory>
"#;
        let entries = parse_extraction_response(response);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].title, "project-uses-axum");
        assert!(entries[0].content.contains("Axum"));
        assert_eq!(entries[1].title, "prefer-anyhow-errors");
    }

    #[test]
    fn parse_response_no_memories() {
        let response = "After analyzing the conversation, there are no useful memories.\n<no_memories/>";
        let entries = parse_extraction_response(response);
        assert!(entries.is_empty());
    }

    #[test]
    fn parse_response_empty() {
        let entries = parse_extraction_response("");
        assert!(entries.is_empty());
    }

    #[test]
    fn parse_response_max_five_entries() {
        let mut response = String::new();
        for i in 0..8 {
            response.push_str(&format!(
                "<memory><title>entry-{i}</title><content>Content {i}</content></memory>\n"
            ));
        }
        let entries = parse_extraction_response(&response);
        assert_eq!(entries.len(), 5);
    }

    #[test]
    fn sanitize_filename_works() {
        assert_eq!(sanitize_filename("My Project Setup"), "my-project-setup");
        assert_eq!(sanitize_filename("use_axum_framework"), "use_axum_framework");
        assert_eq!(sanitize_filename("test!!!"), "test");
        assert_eq!(sanitize_filename("hello--world"), "hello-world");
    }

    #[test]
    fn noop_handle_closes_immediately() {
        let mut handle = trigger_auto_extract_noop(&[], Path::new("/tmp"));
        assert!(handle.result_rx.try_recv().is_err());
    }
}
