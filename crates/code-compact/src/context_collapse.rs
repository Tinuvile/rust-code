//! Context collapse: advanced multi-pass context optimisation.
//!
//! Goes beyond standard compaction by selectively removing or collapsing
//! message groups that are no longer load-bearing for the current task,
//! using heuristics derived from message metadata and tool-use patterns.
//!
//! Enabled by the `context_collapse` Cargo feature.
//!
//! Ref: src/services/contextCollapse/

use code_types::message::{ContentBlock, Message};

// ── CollapseOptions ───────────────────────────────────────────────────────────

/// Tuning parameters for context collapse.
#[derive(Debug, Clone)]
pub struct CollapseOptions {
    /// Target fraction of the context window to occupy after collapsing (0..1).
    pub target_fraction: f32,
    /// Never remove the most recent N message pairs (user+assistant).
    pub min_recent_turns: usize,
    /// Whether to collapse tool-result messages individually.
    pub collapse_tool_results: bool,
    /// Whether to collapse thinking blocks.
    pub collapse_thinking: bool,
}

impl Default for CollapseOptions {
    fn default() -> Self {
        Self {
            target_fraction: 0.5,
            min_recent_turns: 3,
            collapse_tool_results: true,
            collapse_thinking: true,
        }
    }
}

// ── CollapseResult ────────────────────────────────────────────────────────────

#[derive(Debug)]
pub struct CollapseResult {
    pub messages: Vec<Message>,
    pub removed_count: usize,
    pub collapsed_count: usize,
    /// Estimated tokens saved (rough character-based estimate).
    pub estimated_tokens_saved: u32,
}

// ── collapse_context ──────────────────────────────────────────────────────────

/// Collapse `messages` to fit within `target_fraction * context_window` tokens.
///
/// Strategy (in order of application):
/// 1. Strip `ThinkingBlock` content from assistant messages (if enabled).
/// 2. Truncate long `ToolResult` texts (if enabled).
/// 3. Remove complete old turns (user+assistant pairs) from the beginning,
///    keeping at least `min_recent_turns` pairs intact.
pub fn collapse_context(
    messages: Vec<Message>,
    context_window: u32,
    opts: CollapseOptions,
) -> CollapseResult {
    let target_chars = (context_window as f32 * opts.target_fraction * 4.0) as usize; // ~4 chars/token

    let mut result = messages;
    let original_len = result.len();
    let mut collapsed_count = 0usize;
    let mut chars_before: usize = estimate_chars(&result);

    // Pass 1: strip thinking blocks from assistant messages.
    if opts.collapse_thinking {
        for msg in &mut result {
            if let Message::Assistant(a) = msg {
                let before = a.content.len();
                a.content.retain(|b| !matches!(b, ContentBlock::Thinking(_)));
                collapsed_count += before - a.content.len();
            }
        }
    }

    // Pass 2: truncate long tool results.
    if opts.collapse_tool_results {
        const MAX_TOOL_RESULT_CHARS: usize = 500;
        for msg in &mut result {
            if let Message::User(u) = msg {
                for block in &mut u.content {
                    if let ContentBlock::ToolResult(tr) = block {
                        use code_types::message::ToolResultContent;
                        if let ToolResultContent::Text(ref t) = tr.content {
                            if t.len() > MAX_TOOL_RESULT_CHARS {
                                let truncated = format!(
                                    "{}… [truncated {} chars]",
                                    &t[..MAX_TOOL_RESULT_CHARS],
                                    t.len() - MAX_TOOL_RESULT_CHARS
                                );
                                tr.content = ToolResultContent::Text(truncated);
                                collapsed_count += 1;
                            }
                        }
                    }
                }
            }
        }
    }

    // Pass 3: drop old turns from the beginning to reach the target.
    let protected = opts.min_recent_turns * 2; // user+assistant pairs
    let mut removed_count = 0usize;

    while estimate_chars(&result) > target_chars && result.len() > protected {
        result.remove(0);
        removed_count += 1;
    }

    let chars_after = estimate_chars(&result);
    let estimated_tokens_saved = ((chars_before.saturating_sub(chars_after)) / 4) as u32;

    CollapseResult {
        messages: result,
        removed_count,
        collapsed_count,
        estimated_tokens_saved,
    }
}

// ── helpers ───────────────────────────────────────────────────────────────────

fn estimate_chars(messages: &[Message]) -> usize {
    messages.iter().map(message_chars).sum()
}

fn message_chars(msg: &Message) -> usize {
    match msg {
        Message::User(u) => u
            .content
            .iter()
            .map(|b| match b {
                ContentBlock::Text(t) => t.text.len(),
                ContentBlock::ToolResult(tr) => {
                    use code_types::message::ToolResultContent;
                    match &tr.content {
                        ToolResultContent::Text(s) => s.len(),
                        ToolResultContent::Blocks(bs) => bs.iter().map(|b| {
                            if let ContentBlock::Text(t) = b { t.text.len() } else { 0 }
                        }).sum(),
                    }
                }
                _ => 0,
            })
            .sum(),
        Message::Assistant(a) => a
            .content
            .iter()
            .map(|b| match b {
                ContentBlock::Text(t) => t.text.len(),
                ContentBlock::Thinking(th) => th.thinking.len(),
                _ => 0,
            })
            .sum(),
        _ => 0,
    }
}
