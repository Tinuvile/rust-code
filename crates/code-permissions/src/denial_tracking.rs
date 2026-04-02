//! Denial tracking — counts consecutive and total permission denials to decide
//! when to fall back to prompting the user directly.
//!
//! Ref: src/utils/permissions/denialTracking.ts

/// Maximum consecutive denials before the system falls back to always-ask.
pub const MAX_CONSECUTIVE_DENIALS: u32 = 3;

/// Maximum total denials in a session before falling back to always-ask.
pub const MAX_TOTAL_DENIALS: u32 = 20;

/// Tracks consecutive and cumulative permission denials for a single session.
///
/// Ref: src/utils/permissions/denialTracking.ts DenialTrackingState
#[derive(Debug, Clone, Default)]
pub struct DenialTrackingState {
    /// Denials in a row without any intervening allow.
    pub consecutive_denials: u32,
    /// Total denials in this session.
    pub total_denials: u32,
}

impl DenialTrackingState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a permission denial.
    pub fn record_denial(&mut self) {
        self.consecutive_denials += 1;
        self.total_denials += 1;
    }

    /// Reset the consecutive counter on any permission grant.
    pub fn record_allow(&mut self) {
        self.consecutive_denials = 0;
    }

    /// Returns `true` when denial thresholds have been exceeded and the
    /// permission system should fall back to always prompting the user.
    pub fn should_fallback_to_ask(&self) -> bool {
        self.consecutive_denials >= MAX_CONSECUTIVE_DENIALS
            || self.total_denials >= MAX_TOTAL_DENIALS
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tracks_denials_and_resets() {
        let mut state = DenialTrackingState::new();
        assert!(!state.should_fallback_to_ask());

        state.record_denial();
        state.record_denial();
        assert!(!state.should_fallback_to_ask());

        state.record_denial();
        assert!(state.should_fallback_to_ask());

        // An allow resets consecutive but not total.
        state.record_allow();
        assert!(!state.should_fallback_to_ask());
        assert_eq!(state.total_denials, 3);
    }

    #[test]
    fn total_limit() {
        let mut state = DenialTrackingState::new();
        for _ in 0..MAX_TOTAL_DENIALS {
            // Reset consecutive after each run of 2 to avoid hitting that limit.
            state.record_denial();
            state.record_denial();
            state.record_allow();
        }
        assert!(state.should_fallback_to_ask());
    }
}
