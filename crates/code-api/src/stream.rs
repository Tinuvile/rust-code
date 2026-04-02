//! SSE stream parser — converts raw `text/event-stream` bytes into typed
//! `StreamEvent`s and assembles a complete `AssembledResponse`.
//!
//! The Anthropic Messages API sends Server-Sent Events (SSE).  Each event has:
//!   event: <type>
//!   data: <json>
//!
//! Ref: src/services/api/claude.ts (streamQuery, processStream)
//! Ref: @anthropic-ai/sdk streaming event handling

use std::collections::HashMap;

use bytes::Bytes;
use futures_util::{Stream, StreamExt};

use code_types::message::{ContentBlock, TextBlock, ThinkingBlock, ToolUseBlock, TokenUsage};
use code_types::stream::{
    AssembledResponse, ContentBlockDelta, ContentBlockStart, StreamEvent,
};

// ── Raw SSE parsing ───────────────────────────────────────────────────────────

/// Parse a raw SSE line buffer into `(event_type, data_json)`.
///
/// SSE format:
///   event: message_start\n
///   data: {"type": ...}\n\n
fn parse_sse_chunk(chunk: &str) -> Vec<(String, String)> {
    let mut results = Vec::new();
    let mut event_type = String::new();
    let mut data = String::new();

    for line in chunk.lines() {
        if let Some(rest) = line.strip_prefix("event:") {
            event_type = rest.trim().to_owned();
        } else if let Some(rest) = line.strip_prefix("data:") {
            data = rest.trim().to_owned();
        } else if line.is_empty() && !data.is_empty() {
            results.push((std::mem::take(&mut event_type), std::mem::take(&mut data)));
        }
    }
    // Flush last event if no trailing blank line.
    if !data.is_empty() {
        results.push((event_type, data));
    }
    results
}

// ── Stream conversion ─────────────────────────────────────────────────────────

/// Convert a raw byte stream (from `reqwest`) into a stream of `StreamEvent`s.
///
/// Filters out `ping` events and `[DONE]` markers.
pub fn parse_event_stream(
    byte_stream: impl Stream<Item = Result<Bytes, reqwest::Error>> + Send + 'static,
) -> impl Stream<Item = anyhow::Result<StreamEvent>> {
    async_stream::stream! {
        let mut buffer = String::new();
        tokio::pin!(byte_stream);

        while let Some(chunk_result) = byte_stream.next().await {
            let chunk = match chunk_result {
                Ok(b) => b,
                Err(e) => {
                    yield Err(anyhow::Error::from(e));
                    return;
                }
            };

            if let Ok(text) = std::str::from_utf8(&chunk) {
                buffer.push_str(text);
            }

            // Process complete events (terminated by double newline).
            while let Some(pos) = buffer.find("\n\n") {
                let block = buffer[..pos + 2].to_owned();
                buffer.drain(..pos + 2);

                for (event_type, data) in parse_sse_chunk(&block) {
                    if data == "[DONE]" || event_type == "ping" {
                        continue;
                    }
                    match parse_stream_event(&event_type, &data) {
                        Ok(Some(ev)) => yield Ok(ev),
                        Ok(None) => {} // skip unknown event types silently
                        Err(e) => {
                            tracing::debug!("stream parse error ({event_type}): {e}");
                        }
                    }
                }
            }
        }
    }
}

/// Parse a single SSE event into a `StreamEvent`.
///
/// Returns `Ok(None)` for unknown event types (forward-compat).
fn parse_stream_event(event_type: &str, data: &str) -> anyhow::Result<Option<StreamEvent>> {
    // The API embeds the type both in the SSE event field and in the JSON body.
    // We use the JSON body's `type` field as the source of truth.
    let v: serde_json::Value = serde_json::from_str(data)?;

    let kind = match event_type {
        "" => v.get("type").and_then(|t| t.as_str()).unwrap_or("").to_owned(),
        other => other.to_owned(),
    };

    let event = match kind.as_str() {
        "message_start" | "ping" | "message_stop" | "message_delta"
        | "content_block_start" | "content_block_delta" | "content_block_stop"
        | "error" => {
            serde_json::from_value(v).map(Some)?
        }
        _ => None,
    };
    Ok(event)
}

// ── Stream assembler ──────────────────────────────────────────────────────────

/// Assemble a complete `AssembledResponse` by consuming a stream of `StreamEvent`s.
///
/// Accumulates text, tool input JSON, and thinking blocks across deltas.
///
/// Ref: src/services/api/claude.ts (processStream, collectResponseStream)
pub async fn collect_stream(
    mut stream: impl Stream<Item = anyhow::Result<StreamEvent>> + Unpin,
) -> anyhow::Result<AssembledResponse> {
    let mut message_id = String::new();
    let mut model = String::new();
    let mut usage = TokenUsage {
        input_tokens: 0,
        output_tokens: 0,
        cache_creation_input_tokens: 0,
        cache_read_input_tokens: 0,
    };
    let mut stop_reason: Option<String> = None;

    // Per-index block builders.
    let mut text_builders: HashMap<usize, String> = HashMap::new();
    let mut thinking_builders: HashMap<usize, (String, Option<String>)> = HashMap::new(); // (thinking, signature)
    let mut tool_builders: HashMap<usize, (ToolUseBlock, String)> = HashMap::new(); // (block, partial_json)
    let mut block_order: Vec<(usize, BlockKind)> = Vec::new();

    #[derive(Clone)]
    enum BlockKind { Text, Thinking, Tool }

    while let Some(event) = stream.next().await {
        match event? {
            StreamEvent::MessageStart { message } => {
                message_id = message.id;
                model = message.model;
                usage.input_tokens = message.usage.input_tokens;
                usage.cache_creation_input_tokens = message.usage.cache_creation_input_tokens;
                usage.cache_read_input_tokens = message.usage.cache_read_input_tokens;
            }
            StreamEvent::ContentBlockStart { index, content_block } => {
                match content_block {
                    ContentBlockStart::Text { text } => {
                        text_builders.entry(index).or_default().push_str(&text);
                        block_order.push((index, BlockKind::Text));
                    }
                    ContentBlockStart::Thinking { thinking } => {
                        thinking_builders.entry(index).or_insert_with(|| (thinking, None));
                        block_order.push((index, BlockKind::Thinking));
                    }
                    ContentBlockStart::RedactedThinking { .. } => {
                        // Redacted thinking — nothing to assemble.
                    }
                    ContentBlockStart::ToolUse(tool) => {
                        tool_builders.insert(index, (tool, String::new()));
                        block_order.push((index, BlockKind::Tool));
                    }
                }
            }
            StreamEvent::ContentBlockDelta { index, delta } => {
                match delta {
                    ContentBlockDelta::TextDelta { text } => {
                        text_builders.entry(index).or_default().push_str(&text);
                    }
                    ContentBlockDelta::ThinkingDelta { thinking } => {
                        if let Some((buf, _)) = thinking_builders.get_mut(&index) {
                            buf.push_str(&thinking);
                        }
                    }
                    ContentBlockDelta::SignatureDelta { signature } => {
                        if let Some((_, sig)) = thinking_builders.get_mut(&index) {
                            *sig = Some(signature);
                        }
                    }
                    ContentBlockDelta::InputJsonDelta { partial_json } => {
                        if let Some((_, buf)) = tool_builders.get_mut(&index) {
                            buf.push_str(&partial_json);
                        }
                    }
                }
            }
            StreamEvent::ContentBlockStop { .. } => {}
            StreamEvent::MessageDelta { delta, usage: delta_usage } => {
                stop_reason = delta.stop_reason;
                usage.output_tokens = delta_usage.output_tokens;
            }
            StreamEvent::MessageStop => break,
            StreamEvent::Ping => {}
            StreamEvent::Error { error } => {
                return Err(anyhow::anyhow!("stream error {}: {}", error.kind, error.message));
            }
        }
    }

    // Build content blocks in original order.
    let mut content: Vec<ContentBlock> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for (index, kind) in &block_order {
        if !seen.insert(index) {
            continue;
        }
        match kind {
            BlockKind::Text => {
                if let Some(text) = text_builders.remove(index) {
                    content.push(ContentBlock::Text(TextBlock { text, cache_control: None }));
                }
            }
            BlockKind::Thinking => {
                if let Some((thinking, signature)) = thinking_builders.remove(index) {
                    content.push(ContentBlock::Thinking(ThinkingBlock { thinking, signature }));
                }
            }
            BlockKind::Tool => {
                if let Some((mut tool, json_buf)) = tool_builders.remove(index) {
                    tool.input = serde_json::from_str(&json_buf)
                        .unwrap_or(serde_json::Value::Object(serde_json::Map::new()));
                    content.push(ContentBlock::ToolUse(tool));
                }
            }
        }
    }

    Ok(AssembledResponse { message_id, model, content, stop_reason, usage })
}
