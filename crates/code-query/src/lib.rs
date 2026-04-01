//! Query engine and pipeline: the core API call + tool execution loop.
//!
//! Ref: src/QueryEngine.ts, src/query.ts, src/utils/messages.ts,
//!      src/utils/queryContext.ts, src/services/compact/autoCompact.ts

pub mod engine;
pub mod pipeline;
pub mod messages;
pub mod system_prompt;
pub mod attachments;
pub mod message_queue;
pub mod file_state_cache;
pub mod file_history;
pub mod attribution;
pub mod token_budget;
pub mod interruption;
