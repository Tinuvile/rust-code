//! Hook system: event emission and handler execution.
//!
//! Ref: src/utils/hooks/hookEvents.ts, src/utils/hooks/AsyncHookRegistry.ts,
//!      src/utils/hooks/execAgentHook.ts, src/utils/hooks/execHttpHook.ts

pub mod event;
pub mod registry;
pub mod config;
pub mod executor_shell;
pub mod executor_http;
pub mod executor_prompt;
pub mod session_hooks;
pub mod post_sampling;
pub mod tool_hooks;
