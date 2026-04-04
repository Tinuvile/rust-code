//! Build Skill definitions from MCP server tool listings.
//!
//! Each MCP tool is exposed as a skill that can be invoked via slash command.
//!
//! Ref: src/skills/mcpSkillBuilders.ts

use crate::skill::{Skill, SkillContext, SkillSource};

// ── MCP tool descriptor ───────────────────────────────────────────────────────

/// Minimal description of a tool provided by an MCP server.
#[derive(Debug, Clone)]
pub struct McpToolDescriptor {
    /// Qualified name: `<server_name>/<tool_name>` or just `<tool_name>`.
    pub name: String,
    /// Human-readable description of what the tool does.
    pub description: String,
    /// Server that provides this tool.
    pub server_name: String,
}

// ── Builder ────────────────────────────────��──────────────────────────────��───

/// Build a `Skill` that invokes a single MCP tool.
///
/// The generated skill's `content` instructs the model to call the tool
/// with the user's arguments.
pub fn skill_from_mcp_tool(tool: &McpToolDescriptor) -> Skill {
    let content = format!(
        "You are invoking the `{}` tool from the `{}` MCP server.\n\
Translate the user's request into the appropriate tool call. \
Pass all relevant information as tool input parameters.\n\n\
Tool description: {}",
        tool.name, tool.server_name, tool.description
    );

    // Derive a slash-command name: replace `/` and spaces with `-`.
    let cmd_name = tool
        .name
        .replace('/', "-")
        .replace(' ', "-")
        .to_lowercase();

    Skill {
        name: cmd_name,
        description: tool.description.clone(),
        aliases: vec![],
        when_to_use: format!(
            "Use this skill to invoke `{}` from the `{}` MCP server.",
            tool.name, tool.server_name
        ),
        content,
        allowed_tools: vec![tool.name.clone()],
        model: None,
        user_invocable: true,
        context: SkillContext::Inline,
        source: SkillSource::Mcp,
        argument_hint: None,
    }
}

/// Build skills from a list of MCP tool descriptors.
pub fn skills_from_mcp_tools(tools: &[McpToolDescriptor]) -> Vec<Skill> {
    tools.iter().map(skill_from_mcp_tool).collect()
}
