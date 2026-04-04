//! Built-in agent definitions shipped with the binary.
//!
//! Ref: src/tools/AgentTool/built-in/generalPurposeAgent.ts,
//!      exploreAgent.ts, planAgent.ts, verificationAgent.ts,
//!      claudeCodeGuideAgent.ts, statuslineSetup.ts

use crate::definition::{AgentDefinition, AgentSource};

// ── Shared preamble (mirrors TypeScript SHARED_PREFIX / SHARED_GUIDELINES) ────

const SHARED_PREFIX: &str =
    "You are an agent for Claude Code, Anthropic's official CLI for Claude. \
Given the user's message, you should use the tools available to complete the task. \
Complete the task fully — don't gold-plate, but don't leave it half-done.";

const SHARED_GUIDELINES: &str =
    "Your strengths:
- Searching for code, configurations, and patterns across large codebases
- Analyzing multiple files to understand system architecture
- Investigating complex questions that require exploring many files
- Performing multi-step research tasks

Guidelines:
- For file searches: search broadly when you don't know where something lives. \
Use Read when you know the specific file path.
- For analysis: Start broad and narrow down. Use multiple search strategies if the first \
doesn't yield results.
- Be thorough: Check multiple locations, consider different naming conventions, look for \
related files.
- NEVER create files unless they're absolutely necessary for achieving your goal. \
ALWAYS prefer editing an existing file to creating a new one.
- NEVER proactively create documentation files (*.md) or README files. \
Only create documentation files if explicitly requested.";

// ── Individual agents ─────────────────────────────────────────────────────────

pub fn general_purpose_agent() -> AgentDefinition {
    AgentDefinition {
        agent_type: "general-purpose".into(),
        name: "General Purpose Agent".into(),
        system_prompt: format!(
            "{SHARED_PREFIX} When you complete the task, respond with a concise report covering \
what was done and any key findings — the caller will relay this to the user, so it only needs \
the essentials.\n\n{SHARED_GUIDELINES}"
        ),
        description: "General-purpose agent for researching and executing multi-step tasks.".into(),
        when_to_use:
            "General-purpose agent for researching complex questions, searching for code, and \
executing multi-step tasks. When you are searching for a keyword or file and are not confident \
that you will find the right match in the first few tries use this agent to perform the search for you."
                .into(),
        tools: vec!["*".into()],
        model: None,
        color: None,
        source: AgentSource::BuiltIn,
        max_turns: None,
    }
}

pub fn explore_agent() -> AgentDefinition {
    AgentDefinition {
        agent_type: "Explore".into(),
        name: "Explore".into(),
        system_prompt: format!(
            "{SHARED_PREFIX} You are specialized for fast codebase exploration. \
Use Glob for broad file pattern matching and Grep for content search. \
Specify the desired thoroughness level in your report: \"quick\", \"medium\", or \"very thorough\".\n\n\
{SHARED_GUIDELINES}"
        ),
        description: "Fast agent specialized for exploring codebases.".into(),
        when_to_use:
            "Fast agent specialized for exploring codebases. Use this when you need to quickly \
find files by patterns (eg. \"src/components/**/*.tsx\"), search code for keywords \
(eg. \"API endpoints\"), or answer questions about the codebase \
(eg. \"how do API endpoints work?\"). When calling this agent, specify the desired \
thoroughness level: \"quick\" for basic searches, \"medium\" for moderate exploration, \
or \"very thorough\" for comprehensive analysis across multiple locations and naming conventions."
                .into(),
        tools: vec![
            "Glob".into(),
            "Grep".into(),
            "Read".into(),
            "WebFetch".into(),
            "WebSearch".into(),
        ],
        model: None,
        color: Some("green".into()),
        source: AgentSource::BuiltIn,
        max_turns: None,
    }
}

pub fn plan_agent() -> AgentDefinition {
    AgentDefinition {
        agent_type: "Plan".into(),
        name: "Plan".into(),
        system_prompt: format!(
            "{SHARED_PREFIX} You are a software architect agent for designing implementation plans. \
Read the relevant files first, then return step-by-step plans, identify critical files, and \
consider architectural trade-offs. Do NOT write code.\n\n{SHARED_GUIDELINES}"
        ),
        description: "Software architect agent for designing implementation plans.".into(),
        when_to_use:
            "Software architect agent for designing implementation plans. Use this when you need \
to plan the implementation strategy for a task. Returns step-by-step plans, identifies critical \
files, and considers architectural trade-offs."
                .into(),
        tools: vec![
            "Glob".into(),
            "Grep".into(),
            "Read".into(),
            "WebFetch".into(),
            "WebSearch".into(),
        ],
        model: None,
        color: Some("blue".into()),
        source: AgentSource::BuiltIn,
        max_turns: None,
    }
}

pub fn verification_agent() -> AgentDefinition {
    AgentDefinition {
        agent_type: "verify".into(),
        name: "Verification Agent".into(),
        system_prompt: format!(
            "{SHARED_PREFIX} You verify that code changes are correct. \
Run the project's test suite, read the modified files, and check for regressions. \
Report pass/fail with specific test names and failure messages."
        ),
        description: "Verifies correctness of code changes by running tests.".into(),
        when_to_use:
            "Use this agent to verify that code changes are correct after an edit session."
                .into(),
        tools: vec!["*".into()],
        model: None,
        color: Some("yellow".into()),
        source: AgentSource::BuiltIn,
        max_turns: Some(10),
    }
}

pub fn claude_code_guide_agent() -> AgentDefinition {
    AgentDefinition {
        agent_type: "claude-code-guide".into(),
        name: "Claude Code Guide".into(),
        system_prompt:
            "You answer questions about Claude Code (the CLI tool), the Claude Agent SDK, \
and the Claude API. Use WebSearch and WebFetch to look up official documentation when needed. \
Be precise: include exact option names, flag names, and code examples."
                .into(),
        description: "Answers questions about Claude Code, the Agent SDK, and the API.".into(),
        when_to_use:
            "Use this agent when the user asks questions (\"Can Claude...\", \"Does Claude...\", \
\"How do I...\") about: (1) Claude Code (the CLI tool) - features, hooks, slash commands, MCP servers, \
settings, IDE integrations, keyboard shortcuts; (2) Claude Agent SDK - building custom agents; \
(3) Claude API (formerly Anthropic API) - API usage, tool use, Anthropic SDK usage."
                .into(),
        tools: vec![
            "Glob".into(),
            "Grep".into(),
            "Read".into(),
            "WebFetch".into(),
            "WebSearch".into(),
        ],
        model: None,
        color: Some("cyan".into()),
        source: AgentSource::BuiltIn,
        max_turns: None,
    }
}

pub fn statusline_setup_agent() -> AgentDefinition {
    AgentDefinition {
        agent_type: "statusline-setup".into(),
        name: "Status Line Setup".into(),
        system_prompt:
            "Configure the user's Claude Code status line setting. \
Read the current config file, make the requested change, and confirm."
                .into(),
        description: "Configures the Claude Code status line setting.".into(),
        when_to_use:
            "Use this agent to configure the user's Claude Code status line setting."
                .into(),
        tools: vec!["Read".into(), "Edit".into()],
        model: None,
        color: None,
        source: AgentSource::BuiltIn,
        max_turns: Some(5),
    }
}

// ── Public entry point ────────────────────────────────────────────────────────

/// Return all built-in agents in registration order.
pub fn all_builtin_agents() -> Vec<AgentDefinition> {
    vec![
        general_purpose_agent(),
        statusline_setup_agent(),
        explore_agent(),
        plan_agent(),
        verification_agent(),
        claude_code_guide_agent(),
    ]
}
