//! Google Gemini API client.

use std::time::Duration;

use code_types::message::{
    ApiRole, ContentBlock, ImageSource, ToolResultContent,
};
use code_types::provider::{
    LlmProvider, LlmRequest, ModelPricing, ProviderCapabilities, ProviderKind,
};
use code_types::stream::AssembledResponse;

use super::stream::assemble_gemini_stream;
use super::types::*;

// ── GeminiClient ─────────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct GeminiClient {
    http: reqwest::Client,
    api_key: String,
    base_url: String,
}

impl GeminiClient {
    pub fn new(api_key: String, base_url: Option<String>) -> Self {
        let base_url = base_url
            .unwrap_or_else(|| "https://generativelanguage.googleapis.com".to_owned());
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(600))
            .build()
            .expect("valid reqwest client");
        Self { http, api_key, base_url }
    }

    fn stream_url(&self, model: &str) -> String {
        format!(
            "{}/v1beta/models/{}:streamGenerateContent?alt=sse&key={}",
            self.base_url.trim_end_matches('/'),
            model,
            self.api_key
        )
    }

    async fn send_streaming(
        &self,
        model: &str,
        request: &GenerateContentRequest,
    ) -> Result<AssembledResponse, Box<dyn std::error::Error + Send + Sync>> {
        let url = self.stream_url(model);

        let resp = self
            .http
            .post(&url)
            .header("Content-Type", "application/json")
            .json(request)
            .send()
            .await?;

        let status = resp.status().as_u16();
        if !resp.status().is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(format!("Gemini API error {status}: {body}").into());
        }

        let byte_stream = resp.bytes_stream();
        let mut assembled = assemble_gemini_stream(byte_stream).await?;
        assembled.model = model.to_owned();
        Ok(assembled)
    }
}

// ── Format conversion ────────────────────────────────────────────────────────

fn convert_request(req: &LlmRequest) -> GenerateContentRequest {
    let mut contents = Vec::new();

    for msg in &req.messages {
        let role = match msg.role {
            ApiRole::User => "user",
            ApiRole::Assistant => "model",
        };

        let parts = convert_content_blocks(&msg.content, role == "user");
        if !parts.is_empty() {
            contents.push(GeminiContent {
                role: role.to_owned(),
                parts,
            });
        }
    }

    // System instruction.
    let system_instruction = req.system.as_ref().map(|sys| {
        let text = extract_system_text(sys);
        GeminiContent {
            role: "user".to_owned(),
            parts: vec![GeminiPart::text(text)],
        }
    });

    // Tools.
    let tools = if req.tools.is_empty() {
        vec![]
    } else {
        vec![GeminiToolConfig {
            function_declarations: req
                .tools
                .iter()
                .map(|td| GeminiFunctionDeclaration {
                    name: td.name.clone(),
                    description: td.description.clone(),
                    parameters: td.parameters.clone(),
                })
                .collect(),
        }]
    };

    let generation_config = Some(GenerationConfig {
        max_output_tokens: Some(req.max_tokens),
        temperature: req.temperature,
        top_p: req.top_p,
    });

    GenerateContentRequest {
        contents,
        system_instruction,
        tools,
        generation_config,
    }
}

fn convert_content_blocks(blocks: &[ContentBlock], is_user: bool) -> Vec<GeminiPart> {
    let mut parts = Vec::new();
    for block in blocks {
        match block {
            ContentBlock::Text(t) => {
                parts.push(GeminiPart::text(&t.text));
            }
            ContentBlock::Image(img) => {
                match &img.source {
                    ImageSource::Base64 { media_type, data } => {
                        parts.push(GeminiPart::InlineData(GeminiInlineData {
                            mime_type: media_type.clone(),
                            data: data.clone(),
                        }));
                    }
                    ImageSource::Url { .. } => {
                        // Gemini requires inlineData; skip URL images.
                    }
                }
            }
            ContentBlock::ToolUse(tu) if !is_user => {
                parts.push(GeminiPart::function_call(&tu.name, tu.input.clone()));
            }
            ContentBlock::ToolResult(tr) if is_user => {
                let response_text = match &tr.content {
                    ToolResultContent::Text(s) => s.clone(),
                    ToolResultContent::Blocks(bs) => bs
                        .iter()
                        .filter_map(|b| {
                            if let ContentBlock::Text(t) = b { Some(t.text.as_str()) } else { None }
                        })
                        .collect::<Vec<_>>()
                        .join("\n"),
                };
                // Find the tool name — we don't have it in ToolResultBlock, use tool_use_id as fallback.
                parts.push(GeminiPart::function_response(
                    &tr.tool_use_id,
                    serde_json::json!({ "result": response_text }),
                ));
            }
            _ => {}
        }
    }
    parts
}

fn extract_system_text(system: &serde_json::Value) -> String {
    if let Some(s) = system.as_str() {
        return s.to_owned();
    }
    if let Some(arr) = system.as_array() {
        let parts: Vec<String> = arr
            .iter()
            .filter_map(|v| v.get("text").and_then(|t| t.as_str()).map(str::to_owned))
            .collect();
        return parts.join("\n");
    }
    system.to_string()
}

// ── LlmProvider implementation ───────────────────────────────────────────────

#[async_trait::async_trait]
impl LlmProvider for GeminiClient {
    fn kind(&self) -> ProviderKind {
        ProviderKind::Gemini
    }

    fn capabilities(&self, model: &str) -> ProviderCapabilities {
        let lower = model.to_lowercase();
        let (ctx, max_out) = if lower.contains("2.5-pro") || lower.contains("2.5-flash") {
            (1_000_000, 65_536)
        } else if lower.contains("2.0") {
            (1_000_000, 8_192)
        } else {
            (1_000_000, 8_192)
        };

        ProviderCapabilities {
            supports_streaming: true,
            supports_tool_calling: true,
            supports_thinking: lower.contains("2.5"),
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
        let model = request.model.clone();
        let gemini_req = convert_request(&request);
        self.send_streaming(&model, &gemini_req).await
    }

    fn pricing(&self, model: &str) -> Option<ModelPricing> {
        crate::cost::get_gemini_pricing(model)
    }
}
