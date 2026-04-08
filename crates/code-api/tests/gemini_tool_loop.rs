//! Integration test: full tool-use loop with Gemini.
//!
//! Simulates a real file-reading conversation:
//!   1. User asks Gemini to read a file
//!   2. Gemini calls the Read tool
//!   3. We execute the tool (actually read the file)
//!   4. Send the tool result back to Gemini
//!   5. Gemini summarizes the file contents
//!
//! Run with: cargo test --package code-api --test gemini_tool_loop -- --nocapture

use std::sync::Arc;

use code_api::providers::registry::{create_provider, ProviderConfig};
use code_types::message::{
    ApiMessage, ApiRole, ContentBlock, TextBlock, ToolResultBlock, ToolResultContent, ToolUseBlock,
};
use code_types::provider::{LlmProvider, LlmRequest, ProviderKind, ToolDefinition};

fn get_api_key() -> String {
    std::env::var("GEMINI_API_KEY")
        .expect("GEMINI_API_KEY must be set")
}

fn make_provider() -> Arc<dyn LlmProvider> {
    create_provider(ProviderConfig {
        kind: ProviderKind::Gemini,
        api_key: get_api_key(),
        base_url: None,
        extra_headers: Default::default(),
        timeout: std::time::Duration::from_secs(30),
    })
}

fn read_tool_definition() -> ToolDefinition {
    ToolDefinition {
        name: "Read".to_owned(),
        description: "Read a file from the filesystem. Returns the file contents.".to_owned(),
        parameters: serde_json::json!({
            "type": "object",
            "properties": {
                "file_path": {
                    "type": "string",
                    "description": "Absolute path to the file to read"
                }
            },
            "required": ["file_path"]
        }),
    }
}

/// Full tool-use loop: user asks to read a real file, Gemini calls Read, we
/// execute it, then Gemini summarizes the contents.
#[tokio::test]
async fn test_read_file_tool_loop() {
    let provider = make_provider();
    let tools = vec![read_tool_definition()];

    // Use a real file from the project.
    let target_file = "F:/rust-code/Cargo.toml";

    // ── Turn 1: User asks to read the file ──────────────────────────────────
    println!("=== Turn 1: User asks to read file ===");

    let request1 = LlmRequest {
        model: "gemini-2.5-flash".to_owned(),
        messages: vec![ApiMessage {
            role: ApiRole::User,
            content: vec![ContentBlock::Text(TextBlock {
                text: format!(
                    "Please read the file at {target_file} and tell me how many crates are in the workspace."
                ),
                cache_control: None,
            })],
        }],
        max_tokens: 1024,
        system: Some(serde_json::json!(
            "You are a helpful coding assistant. Use the Read tool to read files when asked."
        )),
        tools: tools.clone(),
        temperature: Some(0.0),
        thinking: None,
        top_p: None,
    };

    let response1 = provider.send(request1).await;
    assert!(response1.is_ok(), "Turn 1 failed: {:?}", response1.err());
    let response1 = response1.unwrap();

    println!("Turn 1 content: {:?}", response1.content);
    println!("Turn 1 stop_reason: {:?}", response1.stop_reason);
    println!("Turn 1 tokens: in={}, out={}", response1.usage.input_tokens, response1.usage.output_tokens);

    // Expect a tool call to Read.
    let tool_use = response1
        .content
        .iter()
        .find_map(|b| {
            if let ContentBlock::ToolUse(tu) = b {
                Some(tu.clone())
            } else {
                None
            }
        })
        .expect("Expected Gemini to call the Read tool");

    assert_eq!(tool_use.name, "Read", "Expected Read tool call, got: {}", tool_use.name);
    println!("Tool call: Read({})", tool_use.input);

    // Extract file_path from tool input.
    let called_path = tool_use
        .input
        .get("file_path")
        .and_then(|v| v.as_str())
        .expect("Expected file_path in tool input");
    println!("Gemini wants to read: {called_path}");

    // Check stop reason.
    assert_eq!(
        response1.stop_reason.as_deref(),
        Some("tool_calls"),
        "Expected tool_calls stop reason"
    );

    // ── Execute the tool: actually read the file ────────────────────────────
    println!("\n=== Executing Read tool ===");

    let file_content = std::fs::read_to_string(called_path)
        .unwrap_or_else(|_| {
            // If Gemini used a different path format, try the original.
            std::fs::read_to_string(target_file)
                .expect("Could not read target file")
        });
    println!("File read OK, {} bytes", file_content.len());

    // ── Turn 2: Send tool result back, get summary ──────────────────────────
    println!("\n=== Turn 2: Send tool result, get summary ===");

    // Build the conversation history: user msg → assistant tool call → user tool result.
    let messages = vec![
        // Original user message.
        ApiMessage {
            role: ApiRole::User,
            content: vec![ContentBlock::Text(TextBlock {
                text: format!(
                    "Please read the file at {target_file} and tell me how many crates are in the workspace."
                ),
                cache_control: None,
            })],
        },
        // Assistant's tool call.
        ApiMessage {
            role: ApiRole::Assistant,
            content: response1.content.clone(),
        },
        // Tool result (as user message with ToolResult block).
        ApiMessage {
            role: ApiRole::User,
            content: vec![ContentBlock::ToolResult(ToolResultBlock {
                tool_use_id: tool_use.id.clone(),
                content: ToolResultContent::Text(file_content.clone()),
                is_error: None,
            })],
        },
    ];

    let request2 = LlmRequest {
        model: "gemini-2.5-flash".to_owned(),
        messages,
        max_tokens: 1024,
        system: Some(serde_json::json!(
            "You are a helpful coding assistant. Use the Read tool to read files when asked."
        )),
        tools,
        temperature: Some(0.0),
        thinking: None,
        top_p: None,
    };

    let response2 = provider.send(request2).await;
    assert!(response2.is_ok(), "Turn 2 failed: {:?}", response2.err());
    let response2 = response2.unwrap();

    println!("Turn 2 content: {:?}", response2.content);
    println!("Turn 2 stop_reason: {:?}", response2.stop_reason);
    println!("Turn 2 tokens: in={}, out={}", response2.usage.input_tokens, response2.usage.output_tokens);

    // Extract text response.
    let final_text = response2
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

    println!("\n=== Gemini's answer ===\n{final_text}");

    // The Cargo.toml has 19 workspace members. Gemini should mention a number.
    assert!(
        !final_text.is_empty(),
        "Expected text response from Gemini after tool result"
    );

    // Check that Gemini correctly counted or mentioned the crates.
    // The workspace has 19 members.
    assert!(
        final_text.contains("19") || final_text.contains("nineteen"),
        "Expected Gemini to count ~19 crates, got: {final_text}"
    );

    assert_eq!(
        response2.stop_reason.as_deref(),
        Some("end_turn"),
        "Expected end_turn after final answer"
    );

    println!("\n=== Full tool loop completed successfully! ===");
}
