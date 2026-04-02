//! Tool orchestration — execute batches of tool calls from an assistant turn.
//!
//! Read-only (concurrency-safe) tools within a turn are executed in parallel
//! using `tokio::task::JoinSet`.  Write tools are serialized individually.
//!
//! Ref: src/services/tools/toolOrchestration.ts (partitionToolCalls)

use code_permissions::denial_tracking::DenialTrackingState;
use code_permissions::evaluator::PermissionEvaluator;
use code_types::message::ToolUseBlock;
use code_types::tool::ToolResult;
use tokio::task::JoinSet;

use crate::execution::execute_tool;
use crate::hooks_stub::ToolHookRunner;
use crate::progress::ProgressSender;
use crate::registry::ToolRegistry;
use crate::{error_result, ToolContext};

// ── Batch types ───────────────────────────────────────────────────────────────

/// A group of tool use blocks that can be executed together.
pub struct ToolBatch {
    pub blocks: Vec<ToolUseBlock>,
    /// If `true`, all tools in the batch are concurrency-safe and can run in
    /// parallel.  If `false`, they must run sequentially.
    pub is_concurrent: bool,
}

// ── Partition ─────────────────────────────────────────────────────────────────

/// Partition a slice of tool use blocks into sequential / concurrent batches.
///
/// Consecutive concurrency-safe tool calls form one batch.  Each
/// non-safe tool call gets its own batch.
///
/// Ref: src/services/tools/toolOrchestration.ts partitionToolCalls
pub fn partition_tool_calls(
    blocks: &[ToolUseBlock],
    registry: &ToolRegistry,
) -> Vec<ToolBatch> {
    let mut batches: Vec<ToolBatch> = Vec::new();

    for block in blocks {
        let safe = registry
            .get(&block.name)
            .map(|t| t.is_concurrency_safe(&block.input))
            .unwrap_or(false);

        if safe {
            // Append to the last batch if it is also concurrent.
            if let Some(last) = batches.last_mut() {
                if last.is_concurrent {
                    last.blocks.push(block.clone());
                    continue;
                }
            }
            batches.push(ToolBatch {
                blocks: vec![block.clone()],
                is_concurrent: true,
            });
        } else {
            batches.push(ToolBatch {
                blocks: vec![block.clone()],
                is_concurrent: false,
            });
        }
    }

    batches
}

// ── Run turn ──────────────────────────────────────────────────────────────────

/// Execute all tool use blocks from a single assistant turn.
///
/// Returns results in the same order as the input blocks, regardless of the
/// order in which concurrent batches complete.
pub async fn run_tool_turn(
    blocks: Vec<ToolUseBlock>,
    registry: &ToolRegistry,
    ctx: &ToolContext,
    evaluator: &PermissionEvaluator,
    denial_state: &mut DenialTrackingState,
    hook_runner: &dyn ToolHookRunner,
    progress: Option<&ProgressSender>,
) -> Vec<ToolResult> {
    if blocks.is_empty() {
        return Vec::new();
    }

    let batches = partition_tool_calls(&blocks, registry);

    // We need to collect results in the original order.
    // Build an index: tool_use_id → position in `blocks`.
    let order: std::collections::HashMap<String, usize> = blocks
        .iter()
        .enumerate()
        .map(|(i, b)| (b.id.clone(), i))
        .collect();

    let mut results: Vec<Option<ToolResult>> = vec![None; blocks.len()];

    for batch in batches {
        if batch.is_concurrent && batch.blocks.len() > 1 {
            // Run concurrent batch with JoinSet.
            // We wrap registry, ctx, evaluator in Arc so they can be shared
            // across tasks.  denial_state is cloned (read-only during concurrent
            // phase) and merged back by taking the max counters.
            let denial_snapshot = denial_state.clone();
            let mut join_set: JoinSet<ToolResult> = JoinSet::new();

            for block in &batch.blocks {
                let block = block.clone();
                let ctx = ctx.clone();
                let denial = denial_snapshot.clone();
                // We pass a NoopHookRunner inside the tasks because we cannot
                // easily share the dyn trait object across threads.  The real
                // hooks (Phase 8) will handle this differently.
                join_set.spawn(async move {
                    // Create a local evaluator from the cwd.
                    let local_evaluator = PermissionEvaluator::new(ctx.cwd.clone());
                    let mut local_denial = denial;
                    execute_tool(
                        &block,
                        // We can't pass registry/hook_runner into spawned tasks
                        // without wrapping them in Arc<dyn Trait + Send + Sync>.
                        // For Phase 5, we create a fresh default registry per task.
                        // Phase 6 will refactor this with Arc<ToolRegistry>.
                        &crate::registry::ToolRegistry::with_default_tools(&ctx.cwd),
                        &ctx,
                        &local_evaluator,
                        &mut local_denial,
                        &crate::hooks_stub::NoopHookRunner,
                        None,
                    )
                    .await
                });
            }

            // Collect results and update denial_state.
            while let Some(res) = join_set.join_next().await {
                match res {
                    Ok(tool_result) => {
                        let pos = order[&tool_result.tool_use_id];
                        if tool_result.is_error {
                            denial_state.record_denial();
                        } else {
                            denial_state.record_allow();
                        }
                        results[pos] = Some(tool_result);
                    }
                    Err(e) => {
                        // Task panicked — find which tool use ID this was by
                        // matching position.  We can't recover the ID from the
                        // JoinError, so emit a generic error for remaining slots.
                        tracing::error!("tool task panicked: {e}");
                    }
                }
            }
        } else {
            // Sequential execution (single write tool or single safe tool).
            for block in &batch.blocks {
                let result = execute_tool(
                    block,
                    registry,
                    ctx,
                    evaluator,
                    denial_state,
                    hook_runner,
                    progress,
                )
                .await;
                let pos = order[&result.tool_use_id];
                results[pos] = Some(result);
            }
        }
    }

    // Fill any gaps (shouldn't happen, but defensive).
    results
        .into_iter()
        .enumerate()
        .map(|(i, r)| {
            r.unwrap_or_else(|| error_result(&blocks[i].id, "Tool execution did not produce a result"))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn make_block(id: &str, name: &str, input: serde_json::Value) -> ToolUseBlock {
        ToolUseBlock {
            id: id.to_owned(),
            name: name.to_owned(),
            input,
        }
    }

    #[test]
    fn partition_read_only_tools_are_concurrent() {
        let reg = ToolRegistry::with_default_tools(std::path::Path::new("."));
        let blocks = vec![
            make_block("1", "Glob", json!({ "pattern": "*.rs" })),
            make_block("2", "Glob", json!({ "pattern": "*.toml" })),
        ];
        let batches = partition_tool_calls(&blocks, &reg);
        assert_eq!(batches.len(), 1);
        assert!(batches[0].is_concurrent);
        assert_eq!(batches[0].blocks.len(), 2);
    }

    #[test]
    fn partition_write_tools_are_sequential() {
        let reg = ToolRegistry::with_default_tools(std::path::Path::new("."));
        let blocks = vec![
            make_block("1", "Write", json!({ "file_path": "/a", "file_contents": "" })),
            make_block("2", "Write", json!({ "file_path": "/b", "file_contents": "" })),
        ];
        let batches = partition_tool_calls(&blocks, &reg);
        assert_eq!(batches.len(), 2);
        assert!(!batches[0].is_concurrent);
        assert!(!batches[1].is_concurrent);
    }

    #[test]
    fn partition_mixed_breaks_at_write() {
        let reg = ToolRegistry::with_default_tools(std::path::Path::new("."));
        let blocks = vec![
            make_block("1", "Glob", json!({ "pattern": "*.rs" })),
            make_block("2", "Write", json!({ "file_path": "/a", "file_contents": "" })),
            make_block("3", "Glob", json!({ "pattern": "*.toml" })),
        ];
        let batches = partition_tool_calls(&blocks, &reg);
        assert_eq!(batches.len(), 3);
        assert!(batches[0].is_concurrent);
        assert!(!batches[1].is_concurrent);
        assert!(batches[2].is_concurrent);
    }
}
