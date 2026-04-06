//! OpenAI SSE stream parser.
//!
//! Converts `data: {...}` SSE lines from the OpenAI Chat Completions API
//! into the unified `AssembledResponse`.

use std::collections::HashMap;

use bytes::Bytes;
use futures_util::{Stream, StreamExt};

use code_types::message::{ContentBlock, TextBlock, ThinkingBlock, ToolUseBlock, TokenUsage};
use code_types::stream::AssembledResponse;

use super::types::ChatCompletionChunk;

// ── Stream assembler ─────────────────────────────────────────────────────────

/// Accumulator for tool call fragments across SSE deltas.
#[derive(Debug, Default)]
struct ToolCallAccum {
    id: String,
    name: String,
    arguments: String,
}

/// Parse an OpenAI SSE byte stream and assemble a complete response.
pub async fn assemble_openai_stream(
    byte_stream: impl Stream<Item = Result<Bytes, reqwest::Error>> + Send + 'static,
) -> anyhow::Result<AssembledResponse> {
    let mut message_id = String::new();
    let mut model = String::new();
    let mut text_buf = String::new();
    let mut reasoning_buf = String::new();
    let mut tool_accums: HashMap<u32, ToolCallAccum> = HashMap::new();
    let mut stop_reason: Option<String> = None;
    let mut usage = TokenUsage::default();

    let mut buffer = String::new();
    tokio::pin!(byte_stream);

    while let Some(chunk_result) = byte_stream.next().await {
        let chunk = chunk_result?;
        if let Ok(text) = std::str::from_utf8(&chunk) {
            buffer.push_str(text);
        }

        // Process complete lines.
        while let Some(pos) = buffer.find('\n') {
            let line = buffer[..pos].trim().to_owned();
            buffer.drain(..pos + 1);

            if line.is_empty() || line.starts_with(':') {
                continue;
            }

            let data = if let Some(rest) = line.strip_prefix("data:") {
                rest.trim()
            } else {
                continue;
            };

            if data == "[DONE]" {
                break;
            }

            let chunk: ChatCompletionChunk = match serde_json::from_str(data) {
                Ok(c) => c,
                Err(e) => {
                    tracing::debug!("openai stream parse error: {e}");
                    continue;
                }
            };

            if message_id.is_empty() {
                message_id = chunk.id.clone();
            }
            if model.is_empty() && !chunk.model.is_empty() {
                model = chunk.model.clone();
            }

            // Process usage if present (stream_options.include_usage).
            if let Some(u) = &chunk.usage {
                usage.input_tokens = u.prompt_tokens;
                usage.output_tokens = u.completion_tokens;
            }

            for choice in &chunk.choices {
                // Text content delta.
                if let Some(ref content) = choice.delta.content {
                    text_buf.push_str(content);
                }

                // Reasoning content delta (DeepSeek).
                if let Some(ref reasoning) = choice.delta.reasoning_content {
                    reasoning_buf.push_str(reasoning);
                }

                // Tool call deltas.
                if let Some(ref tool_calls) = choice.delta.tool_calls {
                    for tc in tool_calls {
                        let accum = tool_accums.entry(tc.index).or_default();
                        if let Some(ref id) = tc.id {
                            accum.id = id.clone();
                        }
                        if let Some(ref func) = tc.function {
                            if let Some(ref name) = func.name {
                                accum.name = name.clone();
                            }
                            if let Some(ref args) = func.arguments {
                                accum.arguments.push_str(args);
                            }
                        }
                    }
                }

                // Finish reason.
                if let Some(ref reason) = choice.finish_reason {
                    stop_reason = Some(reason.clone());
                }
            }
        }
    }

    // Build content blocks in order: reasoning → text → tool_calls.
    let mut content: Vec<ContentBlock> = Vec::new();

    if !reasoning_buf.is_empty() {
        content.push(ContentBlock::Thinking(ThinkingBlock {
            thinking: reasoning_buf,
            signature: None,
        }));
    }

    if !text_buf.is_empty() {
        content.push(ContentBlock::Text(TextBlock {
            text: text_buf,
            cache_control: None,
        }));
    }

    // Sort tool calls by index.
    let mut tool_indices: Vec<u32> = tool_accums.keys().copied().collect();
    tool_indices.sort();
    for idx in tool_indices {
        if let Some(accum) = tool_accums.remove(&idx) {
            let input = serde_json::from_str(&accum.arguments)
                .unwrap_or(serde_json::Value::Object(serde_json::Map::new()));
            content.push(ContentBlock::ToolUse(ToolUseBlock {
                id: accum.id,
                name: accum.name,
                input,
            }));
        }
    }

    // Normalize stop reason: OpenAI "stop" → "end_turn", "tool_calls" stays.
    let normalized_stop = stop_reason.map(|r| match r.as_str() {
        "stop" => "end_turn".to_owned(),
        "length" => "max_tokens".to_owned(),
        other => other.to_owned(),
    });

    Ok(AssembledResponse {
        message_id,
        model,
        content,
        stop_reason: normalized_stop,
        usage,
    })
}
