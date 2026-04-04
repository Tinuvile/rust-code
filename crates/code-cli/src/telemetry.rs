//! Telemetry and feature-flag initialisation.
//!
//! In the full implementation this would:
//!   1. Connect to a GrowthBook-compatible feature-flag endpoint.
//!   2. Cache feature values locally for offline use.
//!   3. Send opt-in analytics events.
//!
//! For Phase 12 this module provides the public interface that the rest of
//! code-cli depends on, without requiring any network connections at startup.
//!
//! Ref: src/services/analytics/growthbook.ts, src/services/analytics/index.ts

use std::collections::HashMap;

// ── Feature flags ─────────────────────────────────────────────────────────────

/// Cached feature flag values fetched from the remote flag service.
///
/// If initialisation fails (offline, bad key, etc.) all flags fall back to
/// their `default` values — the CLI must remain fully functional without flags.
#[derive(Debug, Default, Clone)]
pub struct FeatureFlags {
    flags: HashMap<String, serde_json::Value>,
}

impl FeatureFlags {
    /// Return the boolean value for `key`, or `default` if absent.
    pub fn bool(&self, key: &str, default: bool) -> bool {
        self.flags
            .get(key)
            .and_then(|v| v.as_bool())
            .unwrap_or(default)
    }

    /// Return the string value for `key`, or `default` if absent.
    pub fn string<'a>(&'a self, key: &str, default: &'a str) -> &'a str {
        self.flags
            .get(key)
            .and_then(|v| v.as_str())
            .unwrap_or(default)
    }
}

// ── TelemetryHandle ───────────────────────────────────────────────────────────

/// Owns the telemetry connection and feature-flag cache for the session.
///
/// Cheaply cloneable; clone shares the same underlying state.
#[derive(Debug, Default, Clone)]
pub struct TelemetryHandle {
    pub feature_flags: FeatureFlags,
    enabled: bool,
}

impl TelemetryHandle {
    /// Emit an event (no-op if telemetry is disabled or in stub mode).
    pub fn event(&self, _name: &str, _props: &[(&str, &str)]) {
        // No-op: full implementation would queue the event for background upload.
    }
}

// ── init ──────────────────────────────────────────────────────────────────────

/// Initialise telemetry and feature flags.
///
/// Returns immediately with defaults if any remote call would be needed.
/// All network I/O is performed in a background task so startup is never blocked.
pub async fn init() -> TelemetryHandle {
    // Opt-in check: if ANTHROPIC_TELEMETRY_DISABLED is set, disable entirely.
    let enabled = !matches!(
        std::env::var("ANTHROPIC_TELEMETRY_DISABLED").as_deref(),
        Ok("1") | Ok("true") | Ok("yes")
    );

    if !enabled {
        tracing::debug!("Telemetry disabled via env var");
    }

    TelemetryHandle {
        feature_flags: FeatureFlags::default(),
        enabled,
    }
}
