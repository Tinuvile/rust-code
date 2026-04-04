# Claude Code — Rust Port

A complete Rust rewrite of [Anthropic's Claude Code CLI](https://claude.ai/code), implemented as a 19-crate Cargo workspace. The port preserves the full feature surface of the original TypeScript implementation while leveraging Rust's type system, async runtime (Tokio), and zero-cost abstractions.

---

## Architecture

```
code-cli          ← binary entry point (args, bootstrap, output)
├── code-tui      ← interactive terminal UI (ratatui + crossterm)
├── code-query    ← QueryEngine: API streaming, tool dispatch
├── code-tools    ← 30+ tool implementations
├── code-commands ← slash-command registry (/help, /compact, …)
├── code-agents   ← sub-agent system (definition, loader, runner)
├── code-skills   ← skill/prompt library system
├── code-tasks    ← background task & todo management
├── code-sdk      ← NDJSON SDK I/O + session management
├── code-compact  ← context compaction & context-collapse
├── code-memory   ← CLAUDE.md + memory-dir + team memory
├── code-history  ← transcript persistence & message snipping
├── code-config   ← settings.json, globalConfig, mcpConfig
├── code-api      ← AnthropicClient: streaming HTTP + retries
├── code-lsp      ← LSP hover/definition tool stubs
├── code-mcp      ← MCP server protocol (tool exposure)
├── code-types    ← shared message types, IDs, content blocks
├── code-permission ← permission rules, ToolPermissionContext
└── code-format   ← output formatting helpers
```

---

## Crates

| Crate | Description |
|---|---|
| `code-api` | `AnthropicClient` — streaming HTTP requests to the Messages API, retry logic, token counting |
| `code-agents` | Agent definition, YAML/Markdown loader, sub-agent runner, fork, resume, coordinator |
| `code-cli` | Binary: argument parsing, bootstrap sequence, TUI/non-interactive dispatch, telemetry |
| `code-commands` | Slash-command registry; built-in commands: `/help`, `/compact`, `/version`, `/status`, `/clear`, `/logout`, `/login`, `/config`, `/mcp`, `/resume` |
| `code-compact` | `Compactor` (summarise via API), `ReactiveCompactor` (watch-channel auto-trigger), `collapse_context` |
| `code-config` | `GlobalConfig`, `SettingsJson`, `McpServerConfig`, project-local config loading |
| `code-format` | Terminal colour helpers, token-count formatting, cost formatting |
| `code-history` | Transcript `append`/`load`/`fork`, `snip_messages` (tombstone-based pruning) |
| `code-lsp` | LSP client stubs: `lsp_hover`, `lsp_definition` tools |
| `code-mcp` | MCP server: expose tools over stdio/SSE, proxy external MCP servers |
| `code-memory` | CLAUDE.md discovery (tree-walk), `memdir` entries, `TeamMemoryStore` |
| `code-permission` | `ToolPermissionContext`, rule evaluation, `PermissionMode` variants |
| `code-query` | `QueryEngine`: manages conversation state, calls API, dispatches tool calls, broadcasts `Message` events |
| `code-sdk` | `SdkMessage` NDJSON protocol, `SdkWriter`/`SdkReader`, `SessionManager`, bridge server stub |
| `code-skills` | `Skill` type, 6 bundled skills, Markdown loader, MCP-tool→Skill adapter |
| `code-tasks` | `TaskStore`, shell task spawning, `TaskOutput` log capture, `TodoList`, scheduled tasks |
| `code-tools` | All 30+ tool implementations (see Tools section below) |
| `code-tui` | Full interactive terminal UI: ratatui widgets, event loop, vim mode, markdown renderer |
| `code-types` | `Message` enum, `ContentBlock`, `UserMessage`, `AssistantMessage`, `ToolUseBlock`, `ToolResultBlock`, IDs |

---

## Getting Started

### Prerequisites

- Rust 1.78+ (stable)
- An Anthropic API key

### Build

```bash
git clone <repo>
cd rust-code
cargo build --release
```

The compiled binary is at `target/release/code-cli`.

### Environment

```bash
export ANTHROPIC_API_KEY=sk-ant-...
```

Optional variables:

| Variable | Description |
|---|---|
| `ANTHROPIC_BASE_URL` | Override API endpoint (default: `https://api.anthropic.com`) |
| `ANTHROPIC_MODEL` | Default model (default: `claude-sonnet-4-6`) |
| `ANTHROPIC_TELEMETRY_DISABLED` | Set to `1` / `true` / `yes` to opt out of analytics |
| `CLAUDE_CONFIG_DIR` | Override `~/.claude` config directory |

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

### Common Flags

| Flag | Description |
|---|---|
| `-p <prompt>` | Run a single prompt non-interactively |
| `-c <prompt>` | Continue the most recent session |
| `--model <id>` | Override the model for this session |
| `--output-format <fmt>` | `text` (default) / `json` / `stream-json` |
| `--allowed-tools <list>` | Comma-separated list of tools to allow |
| `--disallowed-tools <list>` | Comma-separated list of tools to block |
| `--permission-mode <mode>` | `default` / `accept-edits` / `bypass-permissions` / `plan` |
| `--mcp` | Start as MCP server |
| `--resume <id>` | Resume a specific session by ID |
| `--no-stream` | Disable streaming (collect full response) |
| `--verbose` | Enable debug logging |
| `--max-turns <n>` | Limit agentic loop turns |

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
| `LspDefinition` | Jump-to-definition via LSP |
| `LspHover` | Hover type information via LSP |
| `Monitor` | Observe a background task's output stream |
| `NotebookEdit` | Edit Jupyter notebook cells |
| `PowerShell` | Execute PowerShell commands (Windows) |
| `Read` | Read files (text, PDF, images, notebooks) |
| `RemoteTrigger` | Trigger a remote automation endpoint |
| `Sleep` | Pause execution for N milliseconds |
| `SpawnAgent` | Launch a sub-agent with a delegated task |
| `SyntheticOutput` | Inject synthetic tool output (testing) |
| `Task` | Spawn and manage background shell tasks |
| `TaskOutput` | Read captured output from a task |
| `TaskStop` | Stop a running background task |
| `TodoWrite` | Write a structured todo list |
| `ToolSearch` | Search available deferred tools by keyword |
| `WebFetch` | Fetch and render a web page |
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
| `coordinator_mode` | Multi-agent coordinator with shared scratchpad |
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
cargo test --workspace
```

### Checking a Single Crate

```bash
cargo check -p code-tui
cargo test -p code-tools
```

### Project Layout

```
crates/
  code-api/         # HTTP client
  code-agents/      # Agent system
  code-cli/         # Binary
  code-commands/    # Slash commands
  code-compact/     # Compaction
  code-config/      # Config loading
  code-format/      # Formatting helpers
  code-history/     # Transcript I/O
  code-lsp/         # LSP stubs
  code-mcp/         # MCP server
  code-memory/      # Memory system
  code-permission/  # Permission rules
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
| 2 | code-config, code-permission | Complete |
| 3 | code-api (AnthropicClient) | Complete |
| 4 | code-tools Tier 1 (Read, Write, Edit, Glob, Grep, Bash) | Complete |
| 5 | code-tools Tier 2–4 (all remaining tools) | Complete |
| 6 | code-history, code-query, dependency stubs | Complete |
| 7 | code-memory, code-commands, code-format | Complete |
| 8 | code-compact, code-mcp, code-lsp, code-config extensions | Complete |
| 9 | code-tui (ratatui interactive UI) | Complete |
| 10 | code-agents, code-skills | Complete |
| 11 | code-tasks, code-sdk | Complete |
| 12 | Feature-gated stubs (all 14 features) | Complete |
| 13 | Analytics, integration tests, CI | Planned |

---

## License

This project is a clean-room Rust reimplementation for educational and research purposes.
The original Claude Code product is © Anthropic, PBC.
