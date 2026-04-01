//! Context compaction: summarize-and-replace, auto-compact, micro-compaction.
//!
//! Ref: src/services/compact/compact.ts, src/services/compact/autoCompact.ts,
//!      src/services/compact/microCompact.ts

pub mod compact;
pub mod auto_compact;
pub mod micro_compact;
pub mod grouping;
pub mod prompt;

#[cfg(feature = "reactive_compact")]
pub mod reactive;

#[cfg(feature = "context_collapse")]
pub mod context_collapse;
