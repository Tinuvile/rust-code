//! Agent color manager: assign unique TUI colors to concurrent agents.
//!
//! Ref: src/tools/AgentTool/agentColorManager.ts

use std::sync::{Arc, Mutex};

/// Named agent colors that map to ratatui/ANSI color sequences.
pub const AGENT_COLORS: &[&str] = &[
    "blue", "green", "yellow", "magenta", "cyan",
    "red", "orange", "purple", "teal", "pink",
];

/// Tracks which colors are currently in use by running agents.
#[derive(Debug, Default)]
pub struct AgentColorManager {
    assigned: Arc<Mutex<Vec<String>>>,
}

impl AgentColorManager {
    pub fn new() -> Self {
        Self::default()
    }

    /// Allocate the next available color for a new agent.
    ///
    /// If all colors are in use, wraps around (multiple agents share a color).
    pub fn assign(&self, agent_id: &str) -> String {
        let mut assigned = self.assigned.lock().unwrap();
        let color = AGENT_COLORS[assigned.len() % AGENT_COLORS.len()].to_owned();
        assigned.push(agent_id.to_owned());
        color
    }

    /// Release the color held by `agent_id`.
    pub fn release(&self, agent_id: &str) {
        let mut assigned = self.assigned.lock().unwrap();
        assigned.retain(|id| id != agent_id);
    }

    /// Return the number of currently active agents.
    pub fn active_count(&self) -> usize {
        self.assigned.lock().unwrap().len()
    }
}
