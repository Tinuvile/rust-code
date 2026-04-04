//! Reactive context compaction: monitor token usage and auto-compact in real time.
//!
//! When the `reactive_compact` feature is enabled, the query engine can hook into
//! this module to trigger compaction automatically as the context window fills up,
//! rather than waiting until a hard threshold is hit.
//!
//! Ref: src/services/compact/reactiveCompact.ts

use std::sync::Arc;
use std::time::Duration;

use tokio::sync::watch;
use tokio::time::interval;

// ── Thresholds ────────────────────────────────────────────────────────────────

/// Start watching at this fraction of the context window.
pub const WATCH_THRESHOLD: f32 = 0.60;

/// Trigger compaction at this fraction of the context window.
pub const COMPACT_THRESHOLD: f32 = 0.80;

/// How often to re-check token usage (milliseconds).
pub const POLL_INTERVAL_MS: u64 = 5_000;

// ── ReactiveCompactionState ───────────────────────────────────────────────────

/// Current state of the reactive compaction monitor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompactionState {
    /// Below the watch threshold — no action needed.
    Idle,
    /// Between watch and compact thresholds — watching closely.
    Watching,
    /// Above the compact threshold — compaction has been triggered.
    Compacting,
    /// Compaction completed and token usage is back within bounds.
    Recovered,
}

// ── ReactiveCompactor ─────────────────────────────────────────────────────────

/// Monitors token-usage and fires compaction callbacks when thresholds are crossed.
pub struct ReactiveCompactor {
    context_window: u32,
    state_tx: watch::Sender<CompactionState>,
    /// Receiver that external code can subscribe to for state changes.
    pub state_rx: watch::Receiver<CompactionState>,
}

impl ReactiveCompactor {
    /// Create a monitor for a given model context window size (in tokens).
    pub fn new(context_window: u32) -> Self {
        let (state_tx, state_rx) = watch::channel(CompactionState::Idle);
        Self { context_window, state_tx, state_rx }
    }

    /// Update the monitor with the latest token count.
    ///
    /// Returns the new `CompactionState`.
    pub fn update(&self, used_tokens: u32) -> CompactionState {
        let fraction = used_tokens as f32 / self.context_window.max(1) as f32;
        let new_state = if fraction >= COMPACT_THRESHOLD {
            CompactionState::Compacting
        } else if fraction >= WATCH_THRESHOLD {
            CompactionState::Watching
        } else {
            CompactionState::Idle
        };

        let _ = self.state_tx.send(new_state.clone());
        new_state
    }

    /// Mark the monitor as recovered after a successful compaction.
    pub fn mark_recovered(&self) {
        let _ = self.state_tx.send(CompactionState::Recovered);
    }

    /// Whether compaction should be triggered right now.
    pub fn should_compact(&self, used_tokens: u32) -> bool {
        let fraction = used_tokens as f32 / self.context_window.max(1) as f32;
        fraction >= COMPACT_THRESHOLD
    }

    /// Spawn a background polling task that calls `on_compact` each time the
    /// compact threshold is crossed (at most once per poll interval).
    ///
    /// The task stops when the returned `JoinHandle` is dropped.
    pub fn spawn_monitor<F, Fut>(
        self: Arc<Self>,
        mut token_provider: impl FnMut() -> u32 + Send + 'static,
        mut on_compact: F,
    ) -> tokio::task::JoinHandle<()>
    where
        F: FnMut() -> Fut + Send + 'static,
        Fut: std::future::Future<Output = ()> + Send,
    {
        tokio::spawn(async move {
            let mut ticker = interval(Duration::from_millis(POLL_INTERVAL_MS));
            let mut last_compacted = false;

            loop {
                ticker.tick().await;
                let used = token_provider();
                let state = self.update(used);

                if state == CompactionState::Compacting && !last_compacted {
                    tracing::info!(
                        used_tokens = used,
                        context_window = self.context_window,
                        "Reactive compaction triggered"
                    );
                    on_compact().await;
                    self.mark_recovered();
                    last_compacted = true;
                } else if state == CompactionState::Idle {
                    last_compacted = false;
                }
            }
        })
    }
}

// ── standalone helpers ────────────────────────────────────────────────────────

/// Returns true if a proactive compaction is advisable given current usage.
pub fn should_compact_reactively(used_tokens: u32, context_window: u32) -> bool {
    let fraction = used_tokens as f32 / context_window.max(1) as f32;
    fraction >= COMPACT_THRESHOLD
}
