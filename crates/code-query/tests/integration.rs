//! Integration test suite — end-to-end tests using a mock LLM provider.
//!
//! These tests exercise the full query loop: user message → LLM → tool calls →
//! tool execution → LLM → final answer, without hitting any real API.

use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use serde_json::json;
use uuid::Uuid;

use code_types::message::{
    ContentBlock, Message, TextBlock, TokenUsage, ToolUseBlock, UserMessage,
};
use code_types::permissions::{PermissionMode, ToolPermissionContext};
use code_types::provider::{
    LlmProvider, LlmRequest, ModelPricing, ProviderCapabilities, ProviderKind,
};
use code_types::stream::AssembledResponse;

use code_query::engine::{QueryEngine, QueryEngineConfig};

// ═══════════════════════════════════════════════════════════════════════════════
// Mock LLM Provider
// ═══════════════════════════════════════════════════════════════════════════════

/// A mock provider that returns pre-programmed responses in sequence.
///
/// Each call to `send()` returns the next response from the list.
/// Panics if more calls are made than responses available.
struct MockProvider {
    responses: Vec<AssembledResponse>,
    call_index: AtomicUsize,
}

impl MockProvider {
    fn new(responses: Vec<AssembledResponse>) -> Self {
        Self {
            responses,
            call_index: AtomicUsize::new(0),
        }
    }

    /// How many times `send()` was called.
    fn call_count(&self) -> usize {
        self.call_index.load(Ordering::SeqCst)
    }
}

#[async_trait::async_trait]
impl LlmProvider for MockProvider {
    fn kind(&self) -> ProviderKind {
        ProviderKind::Anthropic
    }

    fn capabilities(&self, _model: &str) -> ProviderCapabilities {
        ProviderCapabilities {
            supports_streaming: true,
            supports_tool_calling: true,
            supports_thinking: true,
            supports_images: true,
            supports_cache_control: true,
            max_context_window: 200_000,
            max_output_tokens: 8192,
        }
    }

    async fn send(
        &self,
        _request: LlmRequest,
    ) -> Result<AssembledResponse, Box<dyn std::error::Error + Send + Sync>> {
        let idx = self.call_index.fetch_add(1, Ordering::SeqCst);
        if idx < self.responses.len() {
            Ok(self.responses[idx].clone())
        } else {
            Err(format!(
                "MockProvider: no response for call index {idx} (only {} responses configured)",
                self.responses.len()
            )
            .into())
        }
    }

    fn pricing(&self, _model: &str) -> Option<ModelPricing> {
        Some(ModelPricing {
            input_per_mtok: 3.0,
            output_per_mtok: 15.0,
            cache_write_per_mtok: 3.75,
            cache_read_per_mtok: 0.3,
        })
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// Helpers
// ═══════════════════════════════════════════════════════════════════════════════

fn test_config(session_dir: &std::path::Path) -> QueryEngineConfig {
    QueryEngineConfig {
        model: "mock-model".to_owned(),
        cwd: PathBuf::from("."),
        session_id: Uuid::new_v4().to_string(),
        session_dir: session_dir.to_path_buf(),
        permission_ctx: ToolPermissionContext {
            mode: PermissionMode::BypassPermissions,
            ..Default::default()
        },
        system_appendix: None,
        provider_kind: ProviderKind::Anthropic,
    }
}

fn text_response(text: &str) -> AssembledResponse {
    AssembledResponse {
        message_id: Uuid::new_v4().to_string(),
        model: "mock-model".to_owned(),
        content: vec![ContentBlock::Text(TextBlock {
            text: text.to_owned(),
            cache_control: None,
        })],
        stop_reason: Some("end_turn".to_owned()),
        usage: TokenUsage {
            input_tokens: 100,
            output_tokens: 50,
            ..Default::default()
        },
    }
}

fn tool_call_response(tool_id: &str, tool_name: &str, input: serde_json::Value) -> AssembledResponse {
    AssembledResponse {
        message_id: Uuid::new_v4().to_string(),
        model: "mock-model".to_owned(),
        content: vec![ContentBlock::ToolUse(ToolUseBlock {
            id: tool_id.to_owned(),
            name: tool_name.to_owned(),
            input,
        })],
        stop_reason: Some("tool_use".to_owned()),
        usage: TokenUsage {
            input_tokens: 100,
            output_tokens: 80,
            ..Default::default()
        },
    }
}

fn mixed_response(
    text: &str,
    tool_id: &str,
    tool_name: &str,
    tool_input: serde_json::Value,
) -> AssembledResponse {
    AssembledResponse {
        message_id: Uuid::new_v4().to_string(),
        model: "mock-model".to_owned(),
        content: vec![
            ContentBlock::Text(TextBlock {
                text: text.to_owned(),
                cache_control: None,
            }),
            ContentBlock::ToolUse(ToolUseBlock {
                id: tool_id.to_owned(),
                name: tool_name.to_owned(),
                input: tool_input,
            }),
        ],
        stop_reason: Some("tool_use".to_owned()),
        usage: TokenUsage {
            input_tokens: 100,
            output_tokens: 100,
            ..Default::default()
        },
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════════════════════

/// Simple conversation: user says hello, model responds, no tool calls.
#[tokio::test]
async fn simple_conversation_no_tools() {
    let tmp = tempfile::tempdir().unwrap();
    let provider = Arc::new(MockProvider::new(vec![
        text_response("Hello! How can I help you today?"),
    ]));
    let config = test_config(tmp.path());
    let engine = QueryEngine::new(Arc::clone(&provider) as Arc<dyn LlmProvider>, config);

    let mut conversation = Vec::new();
    let user_msg = UserMessage::text("Hello");

    engine.query(user_msg, &mut conversation).await.unwrap();

    // Should have: UserMessage, AssistantMessage, SystemTurnDuration
    assert_eq!(conversation.len(), 2); // user + assistant
    assert!(matches!(&conversation[0], Message::User(_)));
    assert!(matches!(&conversation[1], Message::Assistant(_)));

    if let Message::Assistant(a) = &conversation[1] {
        assert_eq!(a.stop_reason.as_deref(), Some("end_turn"));
        let text = a.content.iter().find_map(|b| {
            if let ContentBlock::Text(t) = b { Some(t.text.as_str()) } else { None }
        });
        assert_eq!(text, Some("Hello! How can I help you today?"));
    }

    assert_eq!(provider.call_count(), 1);
}

/// Tool use loop: model calls Glob tool, gets result, then produces final answer.
#[tokio::test]
async fn tool_use_loop_glob() {
    let tmp = tempfile::tempdir().unwrap();

    let provider = Arc::new(MockProvider::new(vec![
        // Turn 1: model asks to list files
        tool_call_response("tool_1", "Glob", json!({ "pattern": "*.toml" })),
        // Turn 2: model produces final answer after seeing tool result
        text_response("I found the Cargo.toml file."),
    ]));
    let config = test_config(tmp.path());
    let engine = QueryEngine::new(Arc::clone(&provider) as Arc<dyn LlmProvider>, config);

    let mut conversation = Vec::new();
    let user_msg = UserMessage::text("List all .toml files");

    engine.query(user_msg, &mut conversation).await.unwrap();

    // Conversation: User → Assistant(tool_use) → User(tool_result) → Assistant(text)
    assert_eq!(conversation.len(), 4);
    assert!(matches!(&conversation[0], Message::User(_)));
    assert!(matches!(&conversation[1], Message::Assistant(_)));
    assert!(matches!(&conversation[2], Message::User(_))); // tool result
    assert!(matches!(&conversation[3], Message::Assistant(_)));

    // The tool result message should contain a ToolResult content block.
    if let Message::User(u) = &conversation[2] {
        let has_tool_result = u.content.iter().any(|b| matches!(b, ContentBlock::ToolResult(_)));
        assert!(has_tool_result, "tool result message should contain ToolResult block");
    }

    // Final answer should be text.
    if let Message::Assistant(a) = &conversation[3] {
        assert_eq!(a.stop_reason.as_deref(), Some("end_turn"));
    }

    assert_eq!(provider.call_count(), 2);
}

/// Tool use with Read tool: model reads a real file (test creates a temp file).
#[tokio::test]
async fn tool_use_read_file() {
    let tmp = tempfile::tempdir().unwrap();

    // Create a temp file to read.
    let test_file = tmp.path().join("test.txt");
    std::fs::write(&test_file, "Hello from test file!").unwrap();

    let provider = Arc::new(MockProvider::new(vec![
        // Turn 1: model calls Read
        tool_call_response(
            "tool_read_1",
            "Read",
            json!({ "file_path": test_file.to_string_lossy() }),
        ),
        // Turn 2: final answer
        text_response("The file contains: Hello from test file!"),
    ]));

    let mut config = test_config(tmp.path());
    config.cwd = tmp.path().to_path_buf();
    let engine = QueryEngine::new(Arc::clone(&provider) as Arc<dyn LlmProvider>, config);

    let mut conversation = Vec::new();
    let user_msg = UserMessage::text("Read test.txt");

    engine.query(user_msg, &mut conversation).await.unwrap();

    assert_eq!(conversation.len(), 4);

    // Verify the tool result contains the file content.
    if let Message::User(u) = &conversation[2] {
        let tool_result_text = u.content.iter().find_map(|b| {
            if let ContentBlock::ToolResult(tr) = b {
                match &tr.content {
                    code_types::message::ToolResultContent::Text(s) => Some(s.clone()),
                    _ => None,
                }
            } else {
                None
            }
        });
        assert!(
            tool_result_text.is_some(),
            "tool result should contain text"
        );
        let text = tool_result_text.unwrap();
        assert!(
            text.contains("Hello from test file!"),
            "tool result should contain file content, got: {text}"
        );
    }

    assert_eq!(provider.call_count(), 2);
}

/// Multi-tool turn: model calls two tools in one response.
#[tokio::test]
async fn multi_tool_single_turn() {
    let tmp = tempfile::tempdir().unwrap();

    // Create test files.
    let file_a = tmp.path().join("a.txt");
    let file_b = tmp.path().join("b.txt");
    std::fs::write(&file_a, "content A").unwrap();
    std::fs::write(&file_b, "content B").unwrap();

    // Model calls two Glob + Read in one response.
    let multi_tool_response = AssembledResponse {
        message_id: Uuid::new_v4().to_string(),
        model: "mock-model".to_owned(),
        content: vec![
            ContentBlock::ToolUse(ToolUseBlock {
                id: "t1".to_owned(),
                name: "Read".to_owned(),
                input: json!({ "file_path": file_a.to_string_lossy() }),
            }),
            ContentBlock::ToolUse(ToolUseBlock {
                id: "t2".to_owned(),
                name: "Read".to_owned(),
                input: json!({ "file_path": file_b.to_string_lossy() }),
            }),
        ],
        stop_reason: Some("tool_use".to_owned()),
        usage: TokenUsage {
            input_tokens: 100,
            output_tokens: 80,
            ..Default::default()
        },
    };

    let provider = Arc::new(MockProvider::new(vec![
        multi_tool_response,
        text_response("Both files read successfully."),
    ]));

    let mut config = test_config(tmp.path());
    config.cwd = tmp.path().to_path_buf();
    let engine = QueryEngine::new(Arc::clone(&provider) as Arc<dyn LlmProvider>, config);

    let mut conversation = Vec::new();
    engine
        .query(UserMessage::text("Read both files"), &mut conversation)
        .await
        .unwrap();

    // User → Assistant(2 tools) → User(2 results) → Assistant(text)
    assert_eq!(conversation.len(), 4);

    // Check that both tool results are present.
    if let Message::User(u) = &conversation[2] {
        let result_count = u
            .content
            .iter()
            .filter(|b| matches!(b, ContentBlock::ToolResult(_)))
            .count();
        assert_eq!(result_count, 2, "should have 2 tool results");
    }

    assert_eq!(provider.call_count(), 2);
}

/// Mixed response: model produces text + tool call in one message.
#[tokio::test]
async fn text_and_tool_in_one_response() {
    let tmp = tempfile::tempdir().unwrap();

    let provider = Arc::new(MockProvider::new(vec![
        mixed_response(
            "Let me check that for you.",
            "t1",
            "Glob",
            json!({ "pattern": "*.rs" }),
        ),
        text_response("Found some Rust files."),
    ]));

    let config = test_config(tmp.path());
    let engine = QueryEngine::new(Arc::clone(&provider) as Arc<dyn LlmProvider>, config);

    let mut conversation = Vec::new();
    engine
        .query(UserMessage::text("Find Rust files"), &mut conversation)
        .await
        .unwrap();

    assert_eq!(conversation.len(), 4);

    // First assistant message should have both text and tool_use.
    if let Message::Assistant(a) = &conversation[1] {
        let has_text = a.content.iter().any(|b| matches!(b, ContentBlock::Text(_)));
        let has_tool = a.content.iter().any(|b| matches!(b, ContentBlock::ToolUse(_)));
        assert!(has_text, "should have text block");
        assert!(has_tool, "should have tool_use block");
    }

    assert_eq!(provider.call_count(), 2);
}

/// Unknown tool: model calls a tool that doesn't exist → error result returned.
#[tokio::test]
async fn unknown_tool_returns_error() {
    let tmp = tempfile::tempdir().unwrap();

    let provider = Arc::new(MockProvider::new(vec![
        tool_call_response("t1", "NonExistentTool", json!({})),
        text_response("I see the tool failed, let me try another approach."),
    ]));

    let config = test_config(tmp.path());
    let engine = QueryEngine::new(Arc::clone(&provider) as Arc<dyn LlmProvider>, config);

    let mut conversation = Vec::new();
    engine
        .query(UserMessage::text("Do something"), &mut conversation)
        .await
        .unwrap();

    assert_eq!(conversation.len(), 4);

    // The tool result should be an error.
    if let Message::User(u) = &conversation[2] {
        let is_error = u.content.iter().any(|b| {
            if let ContentBlock::ToolResult(tr) = b {
                tr.is_error == Some(true)
            } else {
                false
            }
        });
        assert!(is_error, "unknown tool should produce an error result");
    }

    assert_eq!(provider.call_count(), 2);
}

/// Multi-turn conversation: two user turns.
#[tokio::test]
async fn multi_turn_conversation() {
    let tmp = tempfile::tempdir().unwrap();

    let provider = Arc::new(MockProvider::new(vec![
        text_response("Hello! I'm Claude."),
        text_response("2 + 2 = 4"),
    ]));

    let config = test_config(tmp.path());
    let engine = QueryEngine::new(Arc::clone(&provider) as Arc<dyn LlmProvider>, config);

    let mut conversation = Vec::new();

    // Turn 1
    engine
        .query(UserMessage::text("Hi, who are you?"), &mut conversation)
        .await
        .unwrap();
    assert_eq!(conversation.len(), 2);

    // Turn 2
    engine
        .query(UserMessage::text("What is 2+2?"), &mut conversation)
        .await
        .unwrap();
    assert_eq!(conversation.len(), 4);

    // Both turns should have user + assistant pairs.
    assert!(matches!(&conversation[0], Message::User(_)));
    assert!(matches!(&conversation[1], Message::Assistant(_)));
    assert!(matches!(&conversation[2], Message::User(_)));
    assert!(matches!(&conversation[3], Message::Assistant(_)));

    assert_eq!(provider.call_count(), 2);
}

/// Chained tool calls: model calls tool A, sees result, calls tool B, sees result, answers.
#[tokio::test]
async fn chained_tool_calls() {
    let tmp = tempfile::tempdir().unwrap();

    // Create a file to read in the second step.
    let test_file = tmp.path().join("data.txt");
    std::fs::write(&test_file, "important data").unwrap();

    let provider = Arc::new(MockProvider::new(vec![
        // Step 1: model calls Glob
        tool_call_response("t1", "Glob", json!({ "pattern": "*.txt" })),
        // Step 2: after seeing Glob result, model calls Read
        tool_call_response(
            "t2",
            "Read",
            json!({ "file_path": test_file.to_string_lossy() }),
        ),
        // Step 3: final answer
        text_response("The data file contains: important data"),
    ]));

    let mut config = test_config(tmp.path());
    config.cwd = tmp.path().to_path_buf();
    let engine = QueryEngine::new(Arc::clone(&provider) as Arc<dyn LlmProvider>, config);

    let mut conversation = Vec::new();
    engine
        .query(UserMessage::text("Find and read txt files"), &mut conversation)
        .await
        .unwrap();

    // User → Assistant(Glob) → User(result) → Assistant(Read) → User(result) → Assistant(text)
    assert_eq!(conversation.len(), 6);
    assert_eq!(provider.call_count(), 3);

    // Verify chain: alternating assistant/user messages.
    assert!(matches!(&conversation[0], Message::User(_)));
    assert!(matches!(&conversation[1], Message::Assistant(_)));
    assert!(matches!(&conversation[2], Message::User(_)));
    assert!(matches!(&conversation[3], Message::Assistant(_)));
    assert!(matches!(&conversation[4], Message::User(_)));
    assert!(matches!(&conversation[5], Message::Assistant(_)));
}

/// Cost tracking: verify total cost accumulates across tool-use turns.
#[tokio::test]
async fn cost_tracking_across_turns() {
    let tmp = tempfile::tempdir().unwrap();

    let provider = Arc::new(MockProvider::new(vec![
        tool_call_response("t1", "Glob", json!({ "pattern": "*" })),
        text_response("Done."),
    ]));

    let config = test_config(tmp.path());
    let engine = QueryEngine::new(Arc::clone(&provider) as Arc<dyn LlmProvider>, config);

    let mut conversation = Vec::new();
    engine
        .query(UserMessage::text("test"), &mut conversation)
        .await
        .unwrap();

    // Cost should be > 0 (two API calls with non-zero token usage).
    let cost = engine.total_cost_usd();
    assert!(cost > 0.0, "cost should be non-zero after two API calls, got {cost}");
}

/// Message queue: subscribers receive all messages in real time.
#[tokio::test]
async fn message_queue_subscription() {
    let tmp = tempfile::tempdir().unwrap();

    let provider = Arc::new(MockProvider::new(vec![
        text_response("Hello!"),
    ]));

    let config = test_config(tmp.path());
    let engine = QueryEngine::new(Arc::clone(&provider) as Arc<dyn LlmProvider>, config);

    let mut rx = engine.subscribe();

    let mut conversation = Vec::new();
    engine
        .query(UserMessage::text("Hi"), &mut conversation)
        .await
        .unwrap();

    // Collect all messages from the queue.
    let mut received = Vec::new();
    while let Some(msg) = rx.try_recv() {
        received.push(msg);
    }

    // Should have received: UserMessage, AssistantMessage, SystemTurnDuration
    assert!(
        received.len() >= 2,
        "queue should have at least 2 messages (user + assistant), got {}",
        received.len()
    );
}

/// Write tool: model writes a file and we verify it exists.
#[tokio::test]
async fn tool_use_write_file() {
    let tmp = tempfile::tempdir().unwrap();

    let output_file = tmp.path().join("output.txt");
    let provider = Arc::new(MockProvider::new(vec![
        tool_call_response(
            "tw1",
            "Write",
            json!({
                "file_path": output_file.to_string_lossy(),
                "file_contents": "written by test"
            }),
        ),
        text_response("File written."),
    ]));

    let mut config = test_config(tmp.path());
    config.cwd = tmp.path().to_path_buf();
    let engine = QueryEngine::new(Arc::clone(&provider) as Arc<dyn LlmProvider>, config);

    let mut conversation = Vec::new();
    engine
        .query(UserMessage::text("Write a file"), &mut conversation)
        .await
        .unwrap();

    assert_eq!(conversation.len(), 4);

    // Verify the file was actually created.
    assert!(output_file.exists(), "output file should have been created");
    let content = std::fs::read_to_string(&output_file).unwrap();
    assert_eq!(content, "written by test");
}

/// Bash tool: model runs a simple echo command.
#[tokio::test]
async fn tool_use_bash() {
    let tmp = tempfile::tempdir().unwrap();

    let provider = Arc::new(MockProvider::new(vec![
        tool_call_response(
            "tb1",
            "Bash",
            json!({
                "command": "echo hello_from_test",
                "description": "Echo test"
            }),
        ),
        text_response("The command output 'hello_from_test'."),
    ]));

    let mut config = test_config(tmp.path());
    config.cwd = tmp.path().to_path_buf();
    let engine = QueryEngine::new(Arc::clone(&provider) as Arc<dyn LlmProvider>, config);

    let mut conversation = Vec::new();
    engine
        .query(UserMessage::text("Run echo"), &mut conversation)
        .await
        .unwrap();

    assert_eq!(conversation.len(), 4);

    // Check the tool result contains the echo output.
    if let Message::User(u) = &conversation[2] {
        let output = u.content.iter().find_map(|b| {
            if let ContentBlock::ToolResult(tr) = b {
                match &tr.content {
                    code_types::message::ToolResultContent::Text(s) => Some(s.clone()),
                    _ => None,
                }
            } else {
                None
            }
        });
        assert!(output.is_some(), "bash tool should return output");
        let text = output.unwrap();
        assert!(
            text.contains("hello_from_test"),
            "bash output should contain echo text, got: {text}"
        );
    }

    assert_eq!(provider.call_count(), 2);
}

/// Grep tool: model searches for a pattern in files.
/// Skipped if ripgrep (`rg`) is not installed.
#[tokio::test]
async fn tool_use_grep() {
    // Skip if rg is not available.
    if std::process::Command::new("rg").arg("--version").output().is_err() {
        eprintln!("skipping tool_use_grep: ripgrep (rg) not found in PATH");
        return;
    }

    let tmp = tempfile::tempdir().unwrap();

    // Create a file with searchable content.
    std::fs::write(tmp.path().join("code.rs"), "fn main() { println!(\"hello\"); }").unwrap();

    let provider = Arc::new(MockProvider::new(vec![
        tool_call_response(
            "tg1",
            "Grep",
            json!({
                "pattern": "main",
                "path": tmp.path().to_string_lossy()
            }),
        ),
        text_response("Found 'main' in code.rs"),
    ]));

    let mut config = test_config(tmp.path());
    config.cwd = tmp.path().to_path_buf();
    let engine = QueryEngine::new(Arc::clone(&provider) as Arc<dyn LlmProvider>, config);

    let mut conversation = Vec::new();
    engine
        .query(UserMessage::text("Search for main"), &mut conversation)
        .await
        .unwrap();

    assert_eq!(conversation.len(), 4);

    // Grep result should contain the file match.
    if let Message::User(u) = &conversation[2] {
        let output = u.content.iter().find_map(|b| {
            if let ContentBlock::ToolResult(tr) = b {
                match &tr.content {
                    code_types::message::ToolResultContent::Text(s) => Some(s.clone()),
                    _ => None,
                }
            } else {
                None
            }
        });
        assert!(output.is_some(), "grep should return results");
        let text = output.unwrap();
        assert!(
            text.contains("code.rs"),
            "grep result should reference code.rs, got: {text}"
        );
    }

    assert_eq!(provider.call_count(), 2);
}

/// Edit tool: model edits an existing file.
#[tokio::test]
async fn tool_use_edit_file() {
    let tmp = tempfile::tempdir().unwrap();

    let edit_file = tmp.path().join("edit_me.txt");
    std::fs::write(&edit_file, "old content here").unwrap();

    let provider = Arc::new(MockProvider::new(vec![
        tool_call_response(
            "te1",
            "Edit",
            json!({
                "file_path": edit_file.to_string_lossy(),
                "old_string": "old content",
                "new_string": "new content"
            }),
        ),
        text_response("File edited."),
    ]));

    let mut config = test_config(tmp.path());
    config.cwd = tmp.path().to_path_buf();
    let engine = QueryEngine::new(Arc::clone(&provider) as Arc<dyn LlmProvider>, config);

    let mut conversation = Vec::new();
    engine
        .query(UserMessage::text("Edit the file"), &mut conversation)
        .await
        .unwrap();

    assert_eq!(conversation.len(), 4);

    // Verify the file was edited.
    let content = std::fs::read_to_string(&edit_file).unwrap();
    assert!(
        content.contains("new content"),
        "file should contain 'new content', got: {content}"
    );
    assert!(
        !content.contains("old content"),
        "file should not contain 'old content'"
    );
}
