use clap::Parser;

/// AI-powered coding assistant CLI.
#[derive(Parser, Debug)]
#[command(name = "code", about = "AI-powered coding assistant")]
pub struct Cli {
    /// Initial prompt (non-interactive when combined with --print)
    pub prompt: Option<String>,

    /// Run a single command non-interactively and exit
    #[arg(short = 'c', long)]
    pub command: Option<String>,

    /// Print response and exit (non-interactive)
    #[arg(short = 'p', long)]
    pub print: bool,

    /// Model to use
    #[arg(long)]
    pub model: Option<String>,

    /// LLM provider (anthropic, openai, gemini, deepseek, kimi, minimax, openai-compatible)
    #[arg(long)]
    pub provider: Option<String>,

    /// Base URL for custom OpenAI-compatible providers
    #[arg(long)]
    pub provider_base_url: Option<String>,

    /// Replace the default system prompt
    #[arg(long)]
    pub system_prompt: Option<String>,

    /// Append to the default system prompt
    #[arg(long)]
    pub append_system_prompt: Option<String>,

    /// Permission mode (default | auto | plan | bypass-permissions)
    #[arg(long, default_value = "default")]
    pub permission_mode: String,

    /// Comma-separated list of allowed tools
    #[arg(long)]
    pub allowed_tools: Option<String>,

    /// Comma-separated list of disallowed tools
    #[arg(long)]
    pub disallowed_tools: Option<String>,

    /// Resume a previous session
    #[arg(long)]
    pub resume: bool,

    /// Session ID to resume
    #[arg(long)]
    pub session_id: Option<String>,

    /// Agent type to use
    #[arg(long)]
    pub agent: Option<String>,

    /// MCP config file path
    #[arg(long)]
    pub mcp_config: Option<String>,

    /// Enable verbose output
    #[arg(long, short = 'v')]
    pub verbose: bool,

    /// Enable debug output
    #[arg(long)]
    pub debug: bool,

    /// Maximum number of agentic turns
    #[arg(long)]
    pub max_turns: Option<u32>,

    /// Maximum budget in USD
    #[arg(long)]
    pub max_budget: Option<f64>,

    /// Output format (text | json | stream-json)
    #[arg(long, default_value = "text")]
    pub output_format: String,

    #[command(subcommand)]
    pub subcommand: Option<Subcommand>,
}

#[derive(clap::Subcommand, Debug)]
pub enum Subcommand {
    /// Run as an MCP server via stdio
    Mcp(McpArgs),
}

#[derive(Parser, Debug)]
pub struct McpArgs {
    /// Serve as an MCP server
    #[arg(long)]
    pub serve: bool,
}
