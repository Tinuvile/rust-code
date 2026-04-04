//! Agent SDK: public API types, session management, structured I/O.
//!
//! Ref: src/entrypoints/agentSdkTypes.ts, src/entrypoints/sdk/coreTypes.ts,
//!      src/entrypoints/sdk/controlSchemas.ts, src/bridge/

pub mod message;
pub mod session;
pub mod structured_io;

#[cfg(feature = "bridge_mode")]
pub mod bridge;

// ── Public re-exports ─────────────────────────────────────────────────────────

pub use message::{
    SdkAssistantMessage, SdkMessage, SdkResultMessage, SdkSystemMessage,
    SdkToolResultMessage, SdkToolUseMessage, SdkUserMessage,
};
pub use session::{
    CreateSessionOptions, ForkSessionOptions, ForkSessionResult, ListSessionsOptions,
    SdkSessionInfo, SessionManager,
};
pub use structured_io::{emit, SdkInput, SdkReader, SdkWriter};
