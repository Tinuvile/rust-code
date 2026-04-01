//! Session persistence: JSONL transcripts, input history, resume, paste store.
//!
//! Ref: src/utils/sessionStorage.ts, src/history.ts,
//!      src/utils/sessionRestore.ts, src/utils/conversationRecovery.ts

pub mod session;
pub mod transcript;
pub mod input_history;
pub mod resume;
pub mod paste_store;
pub mod metadata;

#[cfg(feature = "history_snip")]
pub mod snip;
