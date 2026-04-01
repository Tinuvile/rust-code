//! Agent SDK: public API types, session management, structured I/O.
//!
//! Ref: src/entrypoints/agentSdkTypes.ts, src/entrypoints/sdk/coreTypes.ts,
//!      src/entrypoints/sdk/controlSchemas.ts, src/bridge/

pub mod message;
pub mod session;
pub mod structured_io;

#[cfg(feature = "bridge_mode")]
pub mod bridge;
