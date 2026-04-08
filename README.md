# Claude Code — Rust Port

A complete Rust rewrite of [Anthropic's Claude Code CLI](https://claude.ai/code), implemented as a 19-crate Cargo workspace. The port preserves the full feature surface of the original TypeScript implementation while leveraging Rust's type system, async runtime (Tokio), and zero-cost abstractions.

**Multi-provider**: supports Anthropic (direct / Bedrock / Vertex / Azure), OpenAI, Google Gemini, DeepSeek, Kimi (Moonshot), Minimax, and any OpenAI-compatible endpoint.

---

## Architecture

```
code-cli            ← binary entry point (args, bootstrap, output)
├── code-tui        ← interactive terminal UI (ratatui + crossterm)
├── code-query      ← QueryEngine: API streaming, tool dispatch, system prompt
├── code-tools      ← 30+ tool implementations (incl. LSP, coordinator)
├── code-commands   ← slash-command registry (/help, /compact, …)
├── code-agents     ← sub-agent system (definition, loader, runner)
├── code-skills     ← skill/prompt library system
├── code-tasks      ← background task & todo management
├── code-sdk        ← NDJSON SDK I/O + session management
├── code-compact    ← context compaction & context-collapse
├── code-memory     ← CLAUDE.md + memory-dir + auto-extraction
├── code-history    ← transcript persistence & message snipping
├── code-config     ← settings.json, globalConfig, mcpConfig
├── code-api        ← multi-provider LLM client (Anthropic, OpenAI, Gemini, …)
├── code-auth       ← API key management, OAuth, secure storage
├── code-mcp        ← MCP client + server protocol (tool exposure)
├── code-hooks      ← hook system (pre/post tool, prompt submit)
├── code-types      ← shared message types, IDs, content blocks
└── code-permissions ← permission rules, LLM classifiers, auto-mode
```

---

## Crates

| Crate | Description |
|---|---|
| `code-api` | Multi-provider LLM client — Anthropic, OpenAI, Gemini, DeepSeek, Kimi, Minimax, OpenAI-compatible; streaming HTTP, retry logic, token counting, cost tracking |
| `code-agents` | Agent definition, YAML/Markdown loader, sub-agent runner, fork, resume, coordinator |
| `code-auth` | API key resolution (env vars, keychain, OAuth), secure storage (macOS Keychain, Windows Credential Manager, Linux libsecret/keyring) |
| `code-cli` | Binary: argument parsing, bootstrap sequence, TUI/non-interactive dispatch, MCP server mode, telemetry |
| `code-commands` | Slash-command registry; built-in commands: `/help`, `/compact`, `/version`, `/status`, `/clear`, `/logout`, `/login`, `/config`, `/mcp`, `/resume` |
| `code-compact` | `Compactor` (summarise via API), `ReactiveCompactor` (watch-channel auto-trigger), `collapse_context` |
| `code-config` | `GlobalConfig`, `SettingsJson`, `McpServerConfig`, project-local config loading |
| `code-history` | Transcript `append`/`load`/`fork`, `snip_messages` (tombstone-based pruning) |
| `code-hooks` | Hook system: pre/post tool hooks, prompt-submit hooks, shell and HTTP commands |
| `code-mcp` | MCP client (connect to external MCP servers) + MCP server mode (expose tools over JSON-RPC stdio) |
| `code-memory` | CLAUDE.md discovery (tree-walk), memdir entries, LLM-driven auto-extraction, team memory |
| `code-permissions` | Permission rules, evaluator, dangerous-pattern detection, LLM-driven auto-classifier, LLM bash safety classifier |
| `code-query` | `QueryEngine`: manages conversation state, calls provider API, dispatches tool calls, broadcasts `Message` events, builds system prompts (provider/agent/thinking-aware) |
| `code-sdk` | `SdkMessage` NDJSON protocol, `SdkWriter`/`SdkReader`, `SessionManager`, bridge server stub |
| `code-skills` | `Skill` type, 6 bundled skills, Markdown loader, MCP-tool-to-Skill adapter |
| `code-tasks` | `TaskStore`, shell task spawning, `TaskOutput` log capture, `TodoList`, `AgentExecutor` trait for background agents |
| `code-tools` | All 30+ tool implementations including LSP (hover/definition with real language servers), coordinator tools, web fetch with HTML-to-Markdown |
| `code-tui` | Full interactive terminal UI: ratatui widgets, event loop, vim mode, markdown renderer, diff view |
| `code-types` | `Message` enum, `ContentBlock`, `LlmProvider` trait, `ProviderKind`, image validation, IDs |

---

## Getting Started

### Prerequisites

- Rust 1.78+ (stable)
- An API key for at least one supported LLM provider

### Build

```bash
git clone <repo>
cd rust-code
cargo build --release
```

The compiled binary is at `target/release/code-cli`.

### Environment

Set an API key for your preferred provider:

```bash
# Anthropic (default)
export ANTHROPIC_API_KEY=sk-ant-...

# Or use another provider
export GEMINI_API_KEY=...
export OPENAI_API_KEY=sk-...
export DEEPSEEK_API_KEY=...
export KIMI_API_KEY=...
export MINIMAX_API_KEY=...

# Select which provider to use
export LLM_PROVIDER=gemini    # anthropic|openai|gemini|deepseek|kimi|minimax|openai-compatible
```

Provider is auto-detected from available API key environment variables when `LLM_PROVIDER` is not set.

#### Additional Variables

| Variable | Description |
|---|---|
| `LLM_PROVIDER` | Select provider: `anthropic`, `openai`, `gemini`, `deepseek`, `kimi`, `minimax`, `openai-compatible` |
| `LLM_API_KEY` | Universal fallback API key for any provider |
| `ANTHROPIC_BASE_URL` | Override Anthropic API endpoint |
| `ANTHROPIC_MODEL` | Default model (default: `claude-sonnet-4-6`) |
| `ANTHROPIC_TELEMETRY_DISABLED` | Set to `1` / `true` / `yes` to opt out of analytics |
| `CLAUDE_CONFIG_DIR` | Override `~/.claude` config directory |
| `CLAUDE_CODE_USE_BEDROCK` | Set to `1` to use AWS Bedrock |
| `CLAUDE_CODE_USE_VERTEX` | Set to `1` to use GCP Vertex AI |

---

## Multi-Provider Support

| Provider | Models | Wire Format |
|---|---|---|
| **Anthropic** | Claude Opus/Sonnet/Haiku | Anthropic Messages API |
| **AWS Bedrock** | Claude (via Bedrock) | Anthropic Messages API |
| **GCP Vertex AI** | Claude (via Vertex) | Anthropic Messages API |
| **Azure Foundry** | Claude (via Azure) | Anthropic Messages API |
| **OpenAI** | GPT-4o, o3, etc. | OpenAI Chat Completions |
| **Google Gemini** | Gemini 2.5 Pro/Flash, etc. | Gemini generateContent |
| **DeepSeek** | DeepSeek Chat/Coder | OpenAI-compatible |
| **Kimi (Moonshot)** | Moonshot v1 128k | OpenAI-compatible |
| **Minimax** | abab6.5s-chat | OpenAI-compatible |
| **Custom** | Any model | OpenAI-compatible (set base URL) |

Configure via CLI flags, settings.json, or environment variables:

```bash
# CLI
code-cli --provider gemini --model gemini-2.5-flash

# Settings (in .claude/settings.json)
{ "provider": "openai", "model": "gpt-4o" }

# Or just set the right API key — auto-detection handles the rest
export GEMINI_API_KEY=... && code-cli
```

---

## Usage

### Interactive TUI

```bash
code-cli
```

Launches the full-screen terminal UI with a scrollable message list, input box, and status bar.

```
┌─ Claude Code ─────────────────────────────────────────────┐
│  You: hello                                               │
│  Claude: Hi! How can I help you today?                    │
│   ✻ Running bash("ls -la")                               │
│   ✓ Done (45ms)                                           │
├────────────────────────────────────────────────────────────┤
│ > |_                                                       │
├────────────────────────────────────────────────────────────┤
│ claude-sonnet-4-6  ◆ auto  $0.0024  1.2k tok  ~/project  │
└────────────────────────────────────────────────────────────┘
```

**Key bindings:**

| Key | Action |
|---|---|
| `Enter` | Submit message |
| `Ctrl+C` | Interrupt current query |
| `Ctrl+D` | Exit |
| `↑` / `↓` | Scroll message list |
| `Page Up/Down` | Fast scroll |
| `Alt+←/→` | Word-left / word-right in input |
| `Ctrl+A` / `Ctrl+E` | Home / End |
| `Ctrl+K` | Clear input line |
| `↑` / `↓` (in input) | Input history |

### Non-Interactive (pipe / scripting)

```bash
# Single prompt
code-cli -p "Summarise the changes in the last 3 commits"

# Continue most-recent session
code-cli -c "Now add unit tests for those changes"

# Output formats
code-cli -p "List all .rs files" --output-format json
code-cli -p "Stream this response" --output-format stream-json
```

### MCP Server Mode

Expose Claude Code's tools to any MCP-compatible client:

```bash
code-cli --mcp
```

Implements the MCP JSON-RPC 2.0 protocol over stdio, supporting `initialize`, `tools/list`, and `tools/call` methods.

### Common Flags

| Flag | Description |
|---|---|
| `-p <prompt>` | Run a single prompt non-interactively |
| `-c <prompt>` | Continue the most recent session |
| `--model <id>` | Override the model for this session |
| `--provider <name>` | Override the LLM provider |
| `--provider-base-url <url>` | Custom API base URL |
| `--output-format <fmt>` | `text` (default) / `json` / `stream-json` |
| `--allowed-tools <list>` | Comma-separated list of tools to allow |
| `--disallowed-tools <list>` | Comma-separated list of tools to block |
| `--permission-mode <mode>` | `default` / `accept-edits` / `bypass-permissions` / `plan` / `auto` |
| `--mcp` | Start as MCP server |
| `--resume <id>` | Resume a specific session by ID |
| `--no-stream` | Disable streaming (collect full response) |
| `--verbose` | Enable debug logging |
| `--max-turns <n>` | Limit agentic loop turns |

---

## Permission System

The permission system controls which tool calls are auto-approved, which require user confirmation, and which are blocked.

### Modes

| Mode | Behavior |
|---|---|
| `default` | Read-only tools auto-allowed; write tools prompt user |
| `accept-edits` | File edit tools also auto-allowed |
| `auto` | LLM-driven classifier auto-approves safe operations |
| `bypass-permissions` | All tools auto-allowed (use with caution) |
| `plan` | Only read-only tools allowed (planning mode) |

### Auto Mode

When `--permission-mode auto` is set, a two-stage LLM classifier evaluates tool calls:

1. **Fast stage** (64 tokens): Quick yes/no decision
2. **Reasoning stage** (512 tokens): Chain-of-thought analysis if stage 1 blocks

For **Bash commands**, an additional specialized LLM classifier provides semantic analysis with 15 command categories (file_read, git_write, build_test, network, system_admin, etc.) and 18 allow / 19 deny rule descriptions.

---

## Tools

Claude Code exposes 30+ tools to the model:

| Tool | Description |
|---|---|
| `AskUserQuestion` | Prompt the user for clarification |
| `Bash` | Execute shell commands |
| `Brief` | Emit a short summary annotation |
| `Config` | Read/write Claude Code configuration |
| `CronCreate` | Schedule a recurring task |
| `CronDelete` | Delete a scheduled task |
| `CronList` | List all scheduled tasks |
| `Edit` | Exact-string file editor |
| `EnterPlanMode` | Switch to plan-only mode |
| `EnterWorktree` | Create an isolated git worktree |
| `ExitPlanMode` | Return from plan mode |
| `ExitWorktree` | Clean up and leave a worktree |
| `GetAgentOutput` | Retrieve output from a spawned sub-agent |
| `Glob` | Fast file-pattern search |
| `Grep` | Ripgrep-powered content search |
| `LspDefinition` | Jump-to-definition via LSP (requires language server in PATH) |
| `LspHover` | Hover type information via LSP (supports Rust, TS, Python, Go, C/C++, Java) |
| `Monitor` | Observe a background task's output stream |
| `NotebookEdit` | Edit Jupyter notebook cells |
| `PowerShell` | Execute PowerShell commands (Windows) |
| `Read` | Read files (text, PDF, images, notebooks) |
| `RemoteTrigger` | Trigger a remote automation endpoint |
| `Sleep` | Pause execution for N milliseconds |
| `SpawnAgent` | Launch a sub-agent with a delegated task (background execution with coordinator runner) |
| `SyntheticOutput` | Inject synthetic tool output (testing) |
| `Task` | Spawn and manage background shell tasks |
| `TaskOutput` | Read captured output from a task |
| `TaskStop` | Stop a running background task |
| `TodoWrite` | Write a structured todo list |
| `ToolSearch` | Search available deferred tools by keyword |
| `WebFetch` | Fetch a web page and convert HTML to Markdown |
| `WebSearch` | Search the web |
| `Write` | Write/overwrite files |

---

## Agents

Six built-in sub-agent types (selectable via `SpawnAgent`):

| Agent | Description |
|---|---|
| `general-purpose` | Full tool access; general research and coding tasks |
| `Explore` | Read-only search: Glob, Grep, Read, WebFetch, WebSearch |
| `Plan` | Architecture and planning; no code writing |
| `claude-code-guide` | Answers questions about Claude Code features and API |
| `statusline-setup` | Configures the status line (Read + Edit only) |
| `verification` | Runs tests and validates implementations; max 10 turns |

Custom agents can be defined in `~/.claude/agents/<name>.md` or `.claude/agents/<name>.md` using YAML frontmatter:

```markdown
---
name: my-agent
description: What this agent does
tools: [Read, Grep, Bash]
model: claude-sonnet-4-6
max_turns: 20
---

System prompt for the agent…
```

---

## Skills

Six bundled skills (invocable via `/<name>`):

| Skill | Description |
|---|---|
| `/batch` | Process multiple items in parallel |
| `/debug` | Systematic debugging workflow |
| `/loop` | Iterate until a condition is met |
| `/verify` | Verify correctness with tests |
| `/simplify` | Reduce complexity of selected code |
| `/remember` | Persist a fact to project memory |

Custom skills live in `~/.claude/skills/<name>.md` or `.claude/skills/<name>.md`.

---

## Feature Flags

Optional capabilities compiled in via Cargo features:

| Feature | Description |
|---|---|
| `reactive_compact` | Auto-compaction triggered by context-window usage thresholds |
| `context_collapse` | Multi-pass context optimisation (strip thinking, truncate results, drop old turns) |
| `agent_triggers` | Local scheduled-trigger engine for agents |
| `agent_triggers_remote` | Remote trigger endpoint for CI/webhook integration |
| `coordinator_mode` | Multi-agent coordinator with background execution and shared agent records |
| `monitor_tool` | `Monitor` tool for live task output streaming |
| `teammem` | Team-shared memory in `.claude/team-memory/` |
| `history_snip` | Tombstone-based selective message pruning |
| `bridge_mode` | SDK bridge server for multi-process integration |
| `skill_search` | Semantic search over the skill library |
| `kairos` | Scheduled-task cron engine |
| `proactive` | Proactive suggestion engine |
| `vim_mode` | Vim-style keybindings in the TUI input box |

Enable features at build time:

```bash
cargo build --release -p code-cli --features "reactive_compact,teammem,vim_mode"
```

---

## Development

### Running Tests

```bash
cargo test --workspace                  # all 200+ unit tests
cargo test --workspace --release        # optimized build

# Provider integration tests (require API keys)
GEMINI_API_KEY=... cargo test --package code-api --test gemini_integration
GEMINI_API_KEY=... cargo test --package code-api --test gemini_tool_loop
```

### Checking a Single Crate

```bash
cargo check -p code-tui
cargo test -p code-tools
cargo test -p code-permissions       # includes LLM classifier tests
```

### Project Layout

```
crates/
  code-api/         # Multi-provider LLM client
  code-agents/      # Agent system
  code-auth/        # API key & secure storage
  code-cli/         # Binary entry point
  code-commands/    # Slash commands
  code-compact/     # Compaction
  code-config/      # Config loading
  code-history/     # Transcript I/O
  code-hooks/       # Hook system
  code-mcp/         # MCP client + server
  code-memory/      # Memory system
  code-permissions/ # Permission rules & classifiers
  code-query/       # QueryEngine
  code-sdk/         # SDK protocol
  code-skills/      # Skills
  code-tasks/       # Task management
  code-tools/       # Tool implementations
  code-tui/         # Terminal UI
  code-types/       # Shared types
Cargo.toml          # Workspace manifest
```

### Implementation Status

| Phase | Scope | Status |
|---|---|---|
| 1 | Workspace scaffold, code-types | Complete |
| 2 | code-config, code-permissions | Complete |
| 3 | code-api (AnthropicClient) | Complete |
| 4 | code-tools Tier 1 (Read, Write, Edit, Glob, Grep, Bash) | Complete |
| 5 | code-tools Tier 2-4 (all remaining tools) | Complete |
| 6 | code-history, code-query, dependency stubs | Complete |
| 7 | code-memory, code-commands | Complete |
| 8 | code-compact, code-mcp, code-config extensions | Complete |
| 9 | code-tui (ratatui interactive UI) | Complete |
| 10 | code-agents, code-skills | Complete |
| 11 | code-tasks, code-sdk | Complete |
| 12 | Feature-gated stubs (all 14 features) | Complete |
| 13 | Multi-provider LLM support (OpenAI, Gemini, DeepSeek, Kimi, Minimax) | Complete |
| 14 | MCP server, Linux keyring, auto-mode classifier, LLM bash classifier | Complete |
| 15 | Coordinator agent execution, memory auto-extraction, LSP tools, image validation | Complete |

**Test coverage:** 200+ unit tests across 19 crates, 0 failures.

---

## License

This project is a clean-room Rust reimplementation for educational and research purposes.
The original Claude Code product is (c) Anthropic, PBC.
