//! Gemini stream parser.
//!
//! Gemini's streaming uses newline-delimited JSON chunks (each a complete
//! `GenerateContentResponse`), not SSE.

use bytes::Bytes;
use futures_util::{Stream, StreamExt};

use code_types::message::{ContentBlock, TextBlock, ToolUseBlock, TokenUsage};
use code_types::stream::AssembledResponse;

// uuid is in the workspace deps, re-use via the crate's Cargo.toml
// We generate synthetic message IDs for Gemini responses.

use super::types::{GenerateContentResponse, GeminiPart};

/// Parse a Gemini streaming response and assemble the final result.
pub async fn assemble_gemini_stream(
    byte_stream: impl Stream<Item = Result<Bytes, reqwest::Error>> + Send + 'static,
) -> anyhow::Result<AssembledResponse> {
    let model = String::new();
    let mut text_buf = String::new();
    let mut tool_calls: Vec<(String, serde_json::Value)> = Vec::new();
    let mut stop_reason: Option<String> = None;
    let mut usage = TokenUsage::default();

    let mut buffer = String::new();
    tokio::pin!(byte_stream);

    while let Some(chunk_result) = byte_stream.next().await {
        let chunk = chunk_result?;
        if let Ok(text) = std::str::from_utf8(&chunk) {
            buffer.push_str(text);
        }

        // Gemini streams chunks as newline-delimited JSON, or as a JSON array
        // prefixed with `data: `. Handle both.
        while let Some(pos) = buffer.find('\n') {
            let line = buffer[..pos].trim().to_owned();
            buffer.drain(..pos + 1);

            if line.is_empty() || line == "[" || line == "]" || line == "," {
                continue;
            }

            let json_str = line
                .strip_prefix("data:")
                .map(|s| s.trim())
                .unwrap_or(&line);

            // Strip trailing comma (array element).
            let json_str = json_str.trim_end_matches(',');

            if json_str == "[DONE]" {
                break;
            }

            let resp: GenerateContentResponse = match serde_json::from_str(json_str) {
                Ok(r) => r,
                Err(e) => {
                    tracing::debug!("gemini stream parse error: {e}");
                    continue;
                }
            };

            if let Some(u) = &resp.usage_metadata {
                usage.input_tokens = u.prompt_token_count;
                usage.output_tokens = u.candidates_token_count;
            }

            for candidate in &resp.candidates {
                if let Some(ref content) = candidate.content {
                    for part in &content.parts {
                        match part {
                            GeminiPart::Text(t) => {
                                text_buf.push_str(&t.text);
                            }
                            GeminiPart::FunctionCall(fc) => {
                                tool_calls.push((fc.name.clone(), fc.args.clone()));
                            }
                            _ => {}
                        }
                    }
                }
                if let Some(ref reason) = candidate.finish_reason {
                    stop_reason = Some(reason.clone());
                }
            }
        }
    }

    // Build content blocks.
    let mut content: Vec<ContentBlock> = Vec::new();

    if !text_buf.is_empty() {
        content.push(ContentBlock::Text(TextBlock {
            text: text_buf,
            cache_control: None,
        }));
    }

    for (i, (name, args)) in tool_calls.into_iter().enumerate() {
        content.push(ContentBlock::ToolUse(ToolUseBlock {
            id: format!("gemini_call_{i}"),
            name,
            input: args,
        }));
    }

    // Normalize stop reason.
    let has_tool_calls = content.iter().any(|b| matches!(b, ContentBlock::ToolUse(_)));
    let normalized_stop = if has_tool_calls {
        Some("tool_calls".to_owned())
    } else {
        stop_reason.map(|r| match r.as_str() {
            "STOP" => "end_turn".to_owned(),
            "MAX_TOKENS" => "max_tokens".to_owned(),
            other => other.to_owned(),
        })
    };

    Ok(AssembledResponse {
        message_id: uuid::Uuid::new_v4().to_string(),
        model,
        content,
        stop_reason: normalized_stop,
        usage,
    })
}
