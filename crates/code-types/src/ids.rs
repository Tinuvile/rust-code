//! Branded ID types for sessions and agents.
//!
//! Ref: src/types/ids.ts
//!
//! TypeScript uses string-brand types (`string & { __brand: 'AgentId' }`).
//! Rust uses newtypes — same zero-cost guarantee, enforced by the type system.

use uuid::Uuid;

// ── SessionId ────────────────────────────────────────────────────────────────

/// Uniquely identifies a top-level session.
///
/// Created once at startup (or resume) and never changes within a session.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
pub struct SessionId(Uuid);

impl SessionId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    pub fn from_uuid(id: Uuid) -> Self {
        Self(id)
    }

    pub fn as_uuid(&self) -> Uuid {
        self.0
    }
}

impl Default for SessionId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for SessionId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

// ── AgentId ──────────────────────────────────────────────────────────────────

/// Uniquely identifies a subagent within a session.
///
/// Format mirrors the TypeScript original: `a` + optional `<label>-` + 32 hex chars.
/// Example: `"aexplore-a1b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6"`
///
/// Only set for subagents; the main session thread has no AgentId.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
pub struct AgentId(String);

impl AgentId {
    /// Create a new AgentId with an optional human-readable label.
    ///
    /// Ref: src/types/ids.ts createAgentId()
    pub fn new(label: Option<&str>) -> Self {
        let hex = Uuid::new_v4().simple().to_string();
        let inner = match label {
            Some(l) if !l.is_empty() => format!("a{}-{}", l, hex),
            _ => format!("a{}", hex),
        };
        Self(inner)
    }

    /// Validate and wrap a raw string as an AgentId.
    ///
    /// Returns `None` if the string doesn't match the expected pattern.
    /// Ref: src/types/ids.ts toAgentId()
    pub fn from_str_validated(s: &str) -> Option<Self> {
        if !s.starts_with('a') || s.len() < 33 {
            return None;
        }
        // The hex part is always the last 32 chars after the final '-' (or after 'a').
        let hex_part = s.rsplitn(2, '-').next().unwrap_or(&s[1..]);
        if hex_part.len() == 32 && hex_part.chars().all(|c| c.is_ascii_hexdigit()) {
            Some(Self(s.to_owned()))
        } else {
            None
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for AgentId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}
