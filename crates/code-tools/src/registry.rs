//! Tool registry — central lookup table for all available tools.
//!
//! Ref: src/tools.ts (getTools, findToolByName)

use std::collections::HashMap;
use std::path::Path;

use serde_json::json;

use crate::Tool;

// ── Registry ──────────────────────────────────────────────────────────────────

/// Central registry mapping tool names to their implementations.
pub struct ToolRegistry {
    tools: HashMap<String, Box<dyn Tool>>,
}

impl ToolRegistry {
    /// Create an empty registry.
    pub fn new() -> Self {
        Self {
            tools: HashMap::new(),
        }
    }

    /// Register a tool.  If a tool with the same name is already registered,
    /// it is replaced.
    pub fn register(&mut self, tool: Box<dyn Tool>) {
        self.tools.insert(tool.name().to_owned(), tool);
    }

    /// Look up a tool by name (case-sensitive).
    pub fn get(&self, name: &str) -> Option<&dyn Tool> {
        self.tools.get(name).map(|t| t.as_ref())
    }

    /// Iterate over all registered tools.
    pub fn all(&self) -> impl Iterator<Item = &dyn Tool> + '_ {
        self.tools.values().map(|t| t.as_ref())
    }

    /// Build the API-format tools list for inclusion in Anthropic API requests.
    ///
    /// Each element is `{ name, description, input_schema }`.
    pub fn to_api_tools(&self) -> Vec<serde_json::Value> {
        let mut tools: Vec<serde_json::Value> = self
            .tools
            .values()
            .map(|t| {
                json!({
                    "name": t.name(),
                    "description": t.description(),
                    "input_schema": t.input_schema(),
                })
            })
            .collect();

        // Sort by name for deterministic output.
        tools.sort_by(|a, b| {
            a["name"]
                .as_str()
                .unwrap_or("")
                .cmp(b["name"].as_str().unwrap_or(""))
        });
        tools
    }

    /// Construct a registry with all Tier 1 + Tier 2 tools registered.
    ///
    /// This is the default set used by `code-query` / `code-cli`.
    /// Tier 3 tools with `is_enabled() = false` (e.g. LSP stubs) are skipped.
    pub fn with_default_tools(_cwd: &Path) -> Self {
        let mut reg = Self::new();

        // Tier 1 — core file/shell tools
        reg.register(Box::new(crate::bash::BashTool));
        reg.register(Box::new(crate::file_read::FileReadTool));
        reg.register(Box::new(crate::file_write::FileWriteTool));
        reg.register(Box::new(crate::file_edit::FileEditTool));
        reg.register(Box::new(crate::grep::GrepTool));
        reg.register(Box::new(crate::glob::GlobTool));

        // Tier 2 — web / session / notebook
        reg.register(Box::new(crate::web_fetch::WebFetchTool));
        reg.register(Box::new(crate::web_search::WebSearchTool));
        reg.register(Box::new(crate::ask_user::AskUserQuestionTool::new()));
        reg.register(Box::new(crate::todo_write::TodoWriteTool));
        reg.register(Box::new(crate::notebook_edit::NotebookEditTool));

        // Tier 3 — specialized (enabled ones only)
        reg.register(Box::new(crate::powershell::PowerShellTool));
        let pm_state = crate::plan_mode::PlanModeState::new();
        reg.register(Box::new(crate::plan_mode::EnterPlanModeTool::new(pm_state.clone())));
        reg.register(Box::new(crate::plan_mode::ExitPlanModeTool::new(pm_state)));
        reg.register(Box::new(crate::worktree::EnterWorktreeTool));
        reg.register(Box::new(crate::worktree::ExitWorktreeTool));
        reg.register(Box::new(crate::task_tools::TaskCreateTool));
        reg.register(Box::new(crate::task_tools::TaskOutputTool));
        reg.register(Box::new(crate::task_tools::TaskStopTool));
        reg.register(Box::new(crate::config_tool::ConfigTool));
        reg.register(Box::new(crate::synthetic_output::SyntheticOutputTool));
        reg.register(Box::new(crate::brief::BriefTool));
        // LspHoverTool / LspDefinitionTool: is_enabled() = false, skip.

        reg
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn register_and_get() {
        let mut reg = ToolRegistry::new();
        reg.register(Box::new(crate::glob::GlobTool));
        assert!(reg.get("Glob").is_some());
        assert!(reg.get("Unknown").is_none());
    }

    #[test]
    fn to_api_tools_includes_schema() {
        let mut reg = ToolRegistry::new();
        reg.register(Box::new(crate::glob::GlobTool));
        let tools = reg.to_api_tools();
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0]["name"], "Glob");
        assert!(tools[0]["input_schema"].is_object());
    }

    #[test]
    fn default_tools_has_tier1() {
        let reg = ToolRegistry::with_default_tools(Path::new("."));
        for name in &["Bash", "Read", "Write", "Edit", "Grep", "Glob"] {
            assert!(reg.get(name).is_some(), "missing tool: {name}");
        }
    }
}
