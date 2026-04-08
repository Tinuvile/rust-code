//! Integration test for Gemini provider.
//!
//! Requires GEMINI_API_KEY environment variable.
//! Run with: cargo test --package code-api --test gemini_integration -- --nocapture

use std::sync::Arc;

use code_api::providers::registry::{create_provider, ProviderConfig};
use code_types::message::{ApiMessage, ApiRole, ContentBlock, TextBlock};
use code_types::provider::{LlmProvider, LlmRequest, ProviderKind};

fn get_api_key() -> String {
    std::env::var("GEMINI_API_KEY")
        .expect("GEMINI_API_KEY must be set to run Gemini integration tests")
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

fn text_msg(role: ApiRole, text: &str) -> ApiMessage {
    ApiMessage {
        role,
        content: vec![ContentBlock::Text(TextBlock {
            text: text.to_owned(),
            cache_control: None,
        })],
    }
}

// ── Test 1: Simple single-turn conversation ─────────────────────────────────

#[tokio::test]
async fn test_simple_conversation() {
    let provider = make_provider();

    let request = LlmRequest {
        model: "gemini-2.5-flash".to_owned(),
        messages: vec![text_msg(ApiRole::User, "What is 2 + 3? Answer with just the number.")],
        max_tokens: 32,
        system: None,
        tools: vec![],
        temperature: Some(0.0),
        thinking: None,
        top_p: None,
    };

    let response = provider.send(request).await;
    assert!(response.is_ok(), "API call failed: {:?}", response.err());

    let response = response.unwrap();
    println!("[Test 1] Response: {:?}", response);

    // Check we got text content.
    assert!(!response.content.is_empty(), "No content in response");
    let text = response
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
    println!("[Test 1] Text: {}", text);
    assert!(text.contains('5'), "Expected '5' in response, got: {text}");

    // Check usage.
    assert!(response.usage.input_tokens > 0, "No input tokens");
    assert!(response.usage.output_tokens > 0, "No output tokens");
    println!(
        "[Test 1] Tokens: in={}, out={}",
        response.usage.input_tokens, response.usage.output_tokens
    );

    // Check stop reason.
    assert_eq!(response.stop_reason.as_deref(), Some("end_turn"));
    println!("[Test 1] Stop reason: {:?}", response.stop_reason);

    // Check model.
    assert!(response.model.contains("gemini"), "Model not set: {}", response.model);
}

// ── Test 2: System prompt ───────────────────────────────────────────────────

#[tokio::test]
async fn test_system_prompt() {
    let provider = make_provider();

    let request = LlmRequest {
        model: "gemini-2.5-flash".to_owned(),
        messages: vec![text_msg(ApiRole::User, "What is your name?")],
        max_tokens: 64,
        system: Some(serde_json::json!("You are a helpful assistant named RustBot. Always mention your name.")),
        tools: vec![],
        temperature: Some(0.0),
        thinking: None,
        top_p: None,
    };

    let response = provider.send(request).await;
    assert!(response.is_ok(), "API call failed: {:?}", response.err());

    let response = response.unwrap();
    let text = response
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
    println!("[Test 2] Text: {}", text);
    assert!(
        text.to_lowercase().contains("rustbot"),
        "System prompt not followed, got: {text}"
    );
}

// ── Test 3: Multi-turn conversation ─────────────────────────────────────────

#[tokio::test]
async fn test_multi_turn() {
    let provider = make_provider();

    let request = LlmRequest {
        model: "gemini-2.5-flash".to_owned(),
        messages: vec![
            text_msg(ApiRole::User, "Remember this number: 42"),
            text_msg(ApiRole::Assistant, "Got it, I'll remember the number 42."),
            text_msg(ApiRole::User, "What number did I ask you to remember? Reply with just the number."),
        ],
        max_tokens: 256,
        system: None,
        tools: vec![],
        temperature: Some(0.0),
        thinking: None,
        top_p: None,
    };

    let response = provider.send(request).await;
    assert!(response.is_ok(), "API call failed: {:?}", response.err());

    let response = response.unwrap();
    let text = response
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
    println!("[Test 3] Text: {}", text);
    assert!(text.contains("42"), "Multi-turn context lost, got: {text}");
}

// ── Test 4: Tool calling ────────────────────────────────────────────────────

#[tokio::test]
async fn test_tool_calling() {
    let provider = make_provider();

    let tools = vec![code_types::provider::ToolDefinition {
        name: "get_weather".to_owned(),
        description: "Get current weather for a city".to_owned(),
        parameters: serde_json::json!({
            "type": "object",
            "properties": {
                "city": { "type": "string", "description": "City name" }
            },
            "required": ["city"]
        }),
    }];

    let request = LlmRequest {
        model: "gemini-2.5-flash".to_owned(),
        messages: vec![text_msg(
            ApiRole::User,
            "What's the weather in Tokyo? Use the get_weather tool.",
        )],
        max_tokens: 256,
        system: None,
        tools,
        temperature: Some(0.0),
        thinking: None,
        top_p: None,
    };

    let response = provider.send(request).await;
    assert!(response.is_ok(), "API call failed: {:?}", response.err());

    let response = response.unwrap();
    println!("[Test 4] Content blocks: {:?}", response.content);

    // Check for tool use.
    let tool_uses: Vec<_> = response
        .content
        .iter()
        .filter_map(|b| {
            if let ContentBlock::ToolUse(tu) = b {
                Some(tu)
            } else {
                None
            }
        })
        .collect();

    assert!(!tool_uses.is_empty(), "Expected tool call, got: {:?}", response.content);
    assert_eq!(tool_uses[0].name, "get_weather");
    println!(
        "[Test 4] Tool call: {} with input {}",
        tool_uses[0].name, tool_uses[0].input
    );

    // The input should mention Tokyo.
    let input_str = tool_uses[0].input.to_string().to_lowercase();
    assert!(input_str.contains("tokyo"), "Expected Tokyo in input, got: {input_str}");

    // Stop reason should be tool_calls.
    assert_eq!(response.stop_reason.as_deref(), Some("tool_calls"));
}

// ── Test 5: Provider capabilities ───────────────────────────────────────────

#[tokio::test]
async fn test_capabilities() {
    let provider = make_provider();

    let caps = provider.capabilities("gemini-2.5-flash");
    assert!(caps.supports_streaming);
    assert!(caps.supports_tool_calling);
    assert!(caps.supports_thinking);
    assert!(caps.supports_images);
    assert!(!caps.supports_cache_control);
    assert_eq!(caps.max_context_window, 1_000_000);
    assert_eq!(caps.max_output_tokens, 65_536);
    println!("[Test 5] Capabilities: {:?}", caps);

    let pricing = provider.pricing("gemini-2.5-flash");
    assert!(pricing.is_some());
    let p = pricing.unwrap();
    println!("[Test 5] Pricing: ${}/MTok in, ${}/MTok out", p.input_per_mtok, p.output_per_mtok);
}

// ── Test 6: Chinese language support ────────────────────────────────────────

#[tokio::test]
async fn test_chinese_conversation() {
    let provider = make_provider();

    let request = LlmRequest {
        model: "gemini-2.5-flash".to_owned(),
        messages: vec![text_msg(ApiRole::User, "用中文回答：1+1等于几？只回答数字。")],
        max_tokens: 256,
        system: None,
        tools: vec![],
        temperature: Some(0.0),
        thinking: None,
        top_p: None,
    };

    let response = provider.send(request).await;
    assert!(response.is_ok(), "API call failed: {:?}", response.err());

    let response = response.unwrap();
    let text = response
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
    println!("[Test 6] Chinese response: {}", text);
    assert!(text.contains('2'), "Expected '2' in response, got: {text}");
}
