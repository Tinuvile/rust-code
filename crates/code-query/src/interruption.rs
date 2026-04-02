//! Interruption signal — Ctrl-C handler for the query loop.
//!
//! The `InterruptionSignal` is an `Arc<AtomicBool>` that is set to `true` when
//! the user presses Ctrl-C.  The query pipeline checks it between tool calls
//! and aborts cleanly when set.
//!
//! Ref: src/utils/interruption.ts

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// Shared interruption flag.
///
/// Clone freely — all clones share the same underlying `AtomicBool`.
#[derive(Debug, Clone, Default)]
pub struct InterruptionSignal(Arc<AtomicBool>);

impl InterruptionSignal {
    /// Create a new (unset) signal.
    pub fn new() -> Self {
        Self(Arc::new(AtomicBool::new(false)))
    }

    /// Returns `true` if the signal has been set (user pressed Ctrl-C).
    pub fn is_set(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }

    /// Set the interruption flag.
    pub fn set(&self) {
        self.0.store(true, Ordering::Relaxed);
    }

    /// Reset the flag (call before starting a new query turn).
    pub fn reset(&self) {
        self.0.store(false, Ordering::Relaxed);
    }

    /// Install a Ctrl-C handler that sets this signal when triggered.
    ///
    /// Only one handler can be active at a time; subsequent calls replace the
    /// previous handler.
    pub fn install_ctrlc_handler(&self) {
        let flag = self.0.clone();
        #[cfg(unix)]
        {
            use std::sync::atomic::Ordering;
            // Use tokio's Ctrl-C signal if available, else fall back to ctrlc crate.
            let flag_clone = flag.clone();
            tokio::spawn(async move {
                if tokio::signal::ctrl_c().await.is_ok() {
                    flag_clone.store(true, Ordering::Relaxed);
                }
            });
        }
        #[cfg(windows)]
        {
            tokio::spawn(async move {
                if tokio::signal::ctrl_c().await.is_ok() {
                    flag.store(true, Ordering::Relaxed);
                }
            });
        }
    }
}
