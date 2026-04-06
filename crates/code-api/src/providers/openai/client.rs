//! OpenAI Chat Completions API client.

use std::time::Duration;

use reqwest::header::{HeaderMap, HeaderName, HeaderValue, AUTHORIZATION};

use code_types::message::{
    ApiRole, ContentBlock, ImageSource, ToolResultContent,
};
use code_types::provider::{
    LlmProvider, LlmRequest, ModelPricing, ProviderCapabilities, ProviderKind, ToolDefinition,
};
use code_types::stream::AssembledResponse;

use super::stream::assemble_openai_stream;
use super::types::*;

// ── OpenAiClient ─────────────────────────────────────────────────────────────

/// HTTP client for the OpenAI Chat Completions API.
#[derive(Clone)]
pub struct OpenAiClient {
    http: reqwest::Client,
    api_key: String,
    base_url: String,
    provider_kind: ProviderKind,
}

impl OpenAiClient {
    pub fn new(api_key: String, base_url: Option<String>, provider_kind: ProviderKind) -> Self {
        let base_url = base_url.unwrap_or_else(|| provider_kind.default_base_url().to_owned());
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(600))
            .build()
            .expect("valid reqwest client");
        Self { http, api_key, base_url, provider_kind }
    }

    fn completions_url(&self) -> String {
        format!("{}/v1/chat/completions", self.base_url.trim_end_matches('/'))
    }

    fn build_headers(&self) -> HeaderMap {
        let mut headers = HeaderMap::new();
        if let Ok(val) = HeaderValue::from_str(&format!("Bearer {}", self.api_key)) {
            headers.insert(AUTHORIZATION, val);
        }
        headers.insert(
            HeaderName::from_static("content-type"),
            HeaderValue::from_static("application/json"),
        );
        headers
    }

    /// Send a streaming request.
    async fn send_streaming(
        &self,
        request: &ChatCompletionRequest,
    ) -> Result<AssembledResponse, Box<dyn std::error::Error + Send + Sync>> {
        let url = self.completions_url();
        let headers = self.build_headers();

        let resp = self.http.post(&url).headers(headers).json(request).send().await?;

        let status = resp.status().as_u16();
        if !resp.status().is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(format!("OpenAI API error {status}: {body}").into());
        }

        let byte_stream = resp.bytes_stream();
        let assembled = assemble_openai_stream(byte_stream).await?;
        Ok(assembled)
    }
}

// ── Format conversion: LlmRequest → ChatCompletionRequest ───────────────────

fn convert_request(req: &LlmRequest) -> ChatCompletionRequest {
    let mut messages = Vec::new();

    // System prompt.
    if let Some(ref system) = req.system {
        let system_text = extract_system_text(system);
        if !system_text.is_empty() {
            messages.push(ChatMessage {
                role: "system".to_owned(),
                content: Some(ChatContent::Text(system_text)),
                tool_calls: None,
                tool_call_id: None,
                name: None,
            });
        }
    }

    // Convert each API message.
    for msg in &req.messages {
        match msg.role {
            ApiRole::User => {
                messages.extend(convert_user_message(&msg.content));
            }
            ApiRole::Assistant => {
                messages.extend(convert_assistant_message(&msg.content));
            }
        }
    }

    let tools: Vec<ChatTool> = req.tools.iter().map(convert_tool).collect();

    // Use max_completion_tokens for o1/o3 models, max_tokens otherwise.
    let is_reasoning_model = req.model.starts_with("o1") || req.model.starts_with("o3");
    let (max_tokens, max_completion_tokens) = if is_reasoning_model {
        (None, Some(req.max_tokens))
    } else {
        (Some(req.max_tokens), None)
    };

    ChatCompletionRequest {
        model: req.model.clone(),
        messages,
        max_tokens,
        max_completion_tokens,
        tools,
        stream: true,
        temperature: if is_reasoning_model { None } else { req.temperature },
        top_p: if is_reasoning_model { None } else { req.top_p },
        stream_options: Some(StreamOptions { include_usage: true }),
    }
}

fn convert_tool(td: &ToolDefinition) -> ChatTool {
    ChatTool {
        kind: "function".to_owned(),
        function: FunctionDefinition {
            name: td.name.clone(),
            description: td.description.clone(),
            parameters: td.parameters.clone(),
        },
    }
}

fn extract_system_text(system: &serde_json::Value) -> String {
    if let Some(s) = system.as_str() {
        return s.to_owned();
    }
    if let Some(arr) = system.as_array() {
        let parts: Vec<String> = arr
            .iter()
            .filter_map(|v| {
                v.get("text").and_then(|t| t.as_str()).map(str::to_owned)
            })
            .collect();
        return parts.join("\n");
    }
    system.to_string()
}

/// Convert user-side content blocks to OpenAI chat messages.
///
/// ToolResult blocks become separate `role: "tool"` messages.
fn convert_user_message(content: &[ContentBlock]) -> Vec<ChatMessage> {
    let mut messages = Vec::new();
    let mut parts = Vec::new();

    for block in content {
        match block {
            ContentBlock::Text(t) => {
                parts.push(ContentPart::Text { text: t.text.clone() });
            }
            ContentBlock::Image(img) => {
                let url = match &img.source {
                    ImageSource::Base64 { media_type, data } => {
                        format!("data:{media_type};base64,{data}")
                    }
                    ImageSource::Url { url } => url.clone(),
                };
                parts.push(ContentPart::ImageUrl {
                    image_url: ImageUrlContent { url },
                });
            }
            ContentBlock::ToolResult(tr) => {
                // Flush any accumulated parts first.
                if !parts.is_empty() {
                    messages.push(ChatMessage {
                        role: "user".to_owned(),
                        content: Some(ChatContent::Parts(std::mem::take(&mut parts))),
                        tool_calls: None,
                        tool_call_id: None,
                        name: None,
                    });
                }
                // Emit a tool result message.
                let text = match &tr.content {
                    ToolResultContent::Text(s) => s.clone(),
                    ToolResultContent::Blocks(blocks) => blocks
                        .iter()
                        .filter_map(|b| {
                            if let ContentBlock::Text(t) = b { Some(t.text.as_str()) } else { None }
                        })
                        .collect::<Vec<_>>()
                        .join("\n"),
                };
                messages.push(ChatMessage {
                    role: "tool".to_owned(),
                    content: Some(ChatContent::Text(text)),
                    tool_calls: None,
                    tool_call_id: Some(tr.tool_use_id.clone()),
                    name: None,
                });
            }
            _ => {
                // Skip thinking, redacted thinking, tool_use in user messages.
            }
        }
    }

    if !parts.is_empty() {
        messages.push(ChatMessage {
            role: "user".to_owned(),
            content: Some(if parts.len() == 1 {
                if let ContentPart::Text { ref text } = parts[0] {
                    ChatContent::Text(text.clone())
                } else {
                    ChatContent::Parts(parts)
                }
            } else {
                ChatContent::Parts(parts)
            }),
            tool_calls: None,
            tool_call_id: None,
            name: None,
        });
    }

    messages
}

/// Convert assistant-side content blocks to an OpenAI assistant message.
///
/// ToolUse blocks become the `tool_calls` array.
fn convert_assistant_message(content: &[ContentBlock]) -> Vec<ChatMessage> {
    let mut text_parts = Vec::new();
    let mut tool_calls = Vec::new();

    for block in content {
        match block {
            ContentBlock::Text(t) => {
                text_parts.push(t.text.clone());
            }
            ContentBlock::ToolUse(tu) => {
                tool_calls.push(ToolCall {
                    id: tu.id.clone(),
                    kind: "function".to_owned(),
                    function: FunctionCall {
                        name: tu.name.clone(),
                        arguments: tu.input.to_string(),
                    },
                });
            }
            _ => {
                // Skip thinking, images, tool_result in assistant messages.
            }
        }
    }

    let content = if text_parts.is_empty() {
        None
    } else {
        Some(ChatContent::Text(text_parts.join("")))
    };

    vec![ChatMessage {
        role: "assistant".to_owned(),
        content,
        tool_calls: if tool_calls.is_empty() { None } else { Some(tool_calls) },
        tool_call_id: None,
        name: None,
    }]
}

// ── LlmProvider implementation ───────────────────────────────────────────────

#[async_trait::async_trait]
impl LlmProvider for OpenAiClient {
    fn kind(&self) -> ProviderKind {
        self.provider_kind
    }

    fn capabilities(&self, model: &str) -> ProviderCapabilities {
        let lower = model.to_lowercase();
        let (ctx, max_out) = if lower.contains("gpt-4o") {
            (128_000, 16_384)
        } else if lower.starts_with("o3") || lower.starts_with("o1") {
            (200_000, 100_000)
        } else if lower.contains("gpt-4-turbo") {
            (128_000, 4_096)
        } else {
            (128_000, 16_384)
        };

        ProviderCapabilities {
            supports_streaming: true,
            supports_tool_calling: true,
            supports_thinking: lower.starts_with("o1") || lower.starts_with("o3"),
            supports_images: true,
            supports_cache_control: false,
            max_context_window: ctx,
            max_output_tokens: max_out,
        }
    }

    async fn send(
        &self,
        request: LlmRequest,
    ) -> Result<AssembledResponse, Box<dyn std::error::Error + Send + Sync>> {
        let chat_req = convert_request(&request);
        self.send_streaming(&chat_req).await
    }

    fn pricing(&self, model: &str) -> Option<ModelPricing> {
        crate::cost::get_openai_pricing(model)
    }
}
