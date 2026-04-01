# Claude Code Rust Rewrite Plan

## Context

Claude Code 是 Anthropic 的官方 AI CLI 工具，当前以 TypeScript/Bun 实现，约 512K 行代码、1900+ 文件、35+ 子系统。目标是在 `f:\rust-code` 从零用 Rust 重写全部功能，并补完正在迭代中的 feature-gated 功能（基座模型更新除外）。

---

## Workspace 结构

```markdown
f:\rust-code\
  Cargo.toml                  # workspace root
  crates/
    code-types/               # 共享类型：消息、权限、配置、工具
    code-config/              # 配置加载：settings, MDM, CLAUDE.md
    code-auth/                # 认证：API key, OAuth, 安全存储
    code-api/                 # Anthropic API 客户端：流式、重试、token 计数
    code-permissions/         # 权限系统：模式、规则、分类器
    code-tools/               # Tool trait + 40+ 工具实现
    code-query/               # 查询引擎 + 查询管线
    code-commands/            # 斜杠命令系统 (60+ 命令)
    code-mcp/                 # MCP 客户端：stdio/SSE/HTTP 传输
    code-hooks/               # Hook 系统：pre/post tool use 事件
    code-memory/              # 记忆系统：MEMORY.md, memdir
    code-compact/             # 上下文压缩与自动压缩
    code-history/             # 会话持久化、transcript 存储
    code-tui/                 # 终端 UI (ratatui)
    code-skills/              # Skills 系统：内置 + 自定义
    code-agents/              # Agent/子 agent 系统、coordinator
    code-tasks/               # 后台任务、shell 任务
    code-sdk/                 # Agent SDK 类型与运行时
    code-cli/                 # CLI 入口 (二进制)
```

---

## Phase 1: 基础类型与配置 (Foundation)

### 1.1 核心类型 (`claude-types`)

| 做什么 | 参考源码 | Rust 方案 |
|--------|----------|-----------|
| `Message` 枚举 (User/Assistant/ToolUse/ToolResult/System/Progress/Tombstone/Attachment/CompactBoundary) | `src/types/message.ts` | `#[serde(tag = "type")]` enum，每个变体持有类型化 payload |
| `ContentBlock` 枚举 (Text/Image/ToolUse/ToolResult/Thinking) | `@anthropic-ai/sdk` 类型 | serde enum with `#[serde(untagged)]` |
| `PermissionMode` / `PermissionResult` / `ToolPermissionContext` | `src/types/permissions.ts`, `src/Tool.ts:123-148` | enum + struct, `DeepImmutable` → 直接用不可变引用 |
| `ToolInputJSONSchema` | `src/Tool.ts:15-21` | `serde_json::Value` 包装 |
| `ValidationResult` | `src/Tool.ts:95-101` | `Result<(), ValidationError>` |
| `ToolUseContext` | `src/Tool.ts:158-299` | struct, 去掉 React 相关字段，用 channel/callback 替代 |
| `AppState` | `src/state/AppState.ts`, `src/state/AppStateStore.ts` | struct + `Arc<RwLock<AppState>>` 替代 React context |
| `StreamEvent` | `src/types/message.ts` (RequestStartEvent 等) | enum: MessageStart/ContentBlockDelta/ToolUse/Usage/Stop |
| `AgentId` / `SessionId` | `src/types/ids.ts` | newtype over `uuid::Uuid` |
| 错误类型 | `src/utils/errors.ts` | `thiserror` 枚举 |

**关键 crate**: `serde`, `serde_json`, `uuid`, `thiserror`

### 1.2 配置系统 (`claude-config`)

| 做什么 | 参考源码 | Rust 方案 |
|--------|----------|-----------|
| `GlobalConfig` (序列化到 `~/.claude/config.json`) | `src/utils/config.ts` | serde struct |
| `ProjectConfig` (`.claude/config.json`) | `src/utils/config.ts` | serde struct |
| `SettingsJson` schema | `src/utils/settings/settings.ts`, `src/utils/settings/types.ts` | serde struct with `#[serde(default)]` |
| MDM 设置 (macOS plutil / Windows Registry) | `src/utils/settings/mdm/rawRead.ts` | `tokio::process::Command` 调 plutil; `winreg` crate 读注册表 |
| 分层设置合并 (MDM > enterprise > remote > user > project) | `src/utils/settings/settings.ts` | 自定义 merge 函数，优先级链 |
| CLAUDE.md 加载 + YAML frontmatter | `src/utils/configConstants.ts` | `serde_yaml` 解析 frontmatter |
| 文件变更监听 | 无直接对应，settings 有 changeDetector | `notify` crate watch config 文件 |

**关键 crate**: `serde_json`, `serde_yaml`, `dirs`, `winreg`, `notify`

### 1.3 平台工具

| 做什么 | 参考源码 | Rust 方案 |
|--------|----------|-----------|
| 平台检测 (Win/Mac/Linux) | `src/utils/platform.ts` | `std::env::consts::OS` |
| 路径展开、规范化 | `src/utils/path.ts` | `std::path`, `dunce` (Windows 长路径) |
| Git 操作 (root/branch/worktree) | `src/utils/git.ts` | `git2` crate |
| 文件编码检测 | `src/utils/file.ts` | `encoding_rs` |
| JSON safe-parse | `src/utils/json.ts` | `serde_json::from_str` 返回 Result |
| 文件锁 | 散布于多处 | `fs2` advisory lock |

---

## Phase 2: 认证与安全存储 (`claude-auth`)

| 做什么 | 参考源码 | Rust 方案 |
|--------|----------|-----------|
| `SecureStorage` trait | `src/utils/secureStorage/` | trait + 三个实现: macOS Keychain (`security` 子进程), Windows Credential Manager (`keyring`), plaintext 回退 |
| `AuthProvider` 枚举 | `src/utils/auth.ts` (2002 行) | enum: ApiKey/OAuth/AwsBedrock/GcpVertex/AzureFoundry |
| OAuth PKCE flow | `src/services/oauth/client.ts`, `auth-code-listener.ts`, `crypto.ts` | `reqwest` + `axum` 小型本地回调服务器 + `sha2`/`base64` |
| API key 管理 | `src/utils/auth.ts`, `src/utils/authFileDescriptor.ts` | 文件描述符传递 + env var 读取 |
| Keychain 预取 (启动时并行) | `src/utils/secureStorage/keychainPrefetch.ts` | `tokio::spawn` 并行读取 |
| AWS STS 认证链 | `src/utils/aws.ts` | `aws-config` + `aws-credential-types` |

**关键 crate**: `keyring`, `reqwest`, `axum`, `sha2`, `base64`, `aws-config`

---

## Phase 3: Anthropic API 客户端 (`claude-api`)

| 做什么 | 参考源码 | Rust 方案 |
|--------|----------|-----------|
| `AnthropicClient` (支持直连/Bedrock/Vertex/Foundry) | `src/services/api/client.ts`, `src/services/api/claude.ts` | struct 持有 `reqwest::Client`, 不同 provider 用 trait object |
| SSE 流式解析 (Messages API) | `src/services/api/claude.ts` | `reqwest-eventsource` 或手动解析 SSE event: message_start/content_block_delta 等 |
| 重试逻辑 (指数退避 + 错误分类) | `src/services/api/withRetry.ts`, `src/services/api/errors.ts` | `backoff` crate 或自定义，错误分类 enum (Retryable/Fatal/RateLimit) |
| Token 计数与估算 | `src/utils/tokens.ts` | 字符级启发式 + API 响应实际值 |
| 上下文窗口管理 | `src/utils/context.ts` | model→context_window 映射表 |
| Tool schema 转换 (Rust struct → JSON Schema) | `src/utils/api.ts` (toolToAPISchema) | `serde_json::Value` 构建 |
| Cost tracking | `src/cost-tracker.ts` (11K 行) | struct 累加 input/output token + 价格表 |
| Beta header 管理 | `src/utils/betas.ts` | 条件性添加 HTTP header |
| Model 名称归一化 | `src/utils/model/model.ts`, `providers.ts` | enum ModelProvider + model alias 映射 |

**关键 crate**: `reqwest`, `reqwest-eventsource`, `tokio`, `backoff`, `aws-sigv4`

**流式事件通道**: API 返回 `impl Stream<Item = StreamEvent>`, 通过 `tokio::sync::mpsc` 发送到 UI 和 Query Engine

---

## Phase 4: 权限系统 (`claude-permissions`)

| 做什么 | 参考源码 | Rust 方案 |
|--------|----------|-----------|
| `PermissionEvaluator` | `src/utils/permissions/permissions.ts` | struct with `evaluate(&self, tool, input, ctx) -> PermissionDecision` |
| 权限规则系统 | `src/utils/permissions/PermissionRule.ts`, `permissionsLoader.ts` | struct PermissionRule { source, tool_name, patterns, action } |
| 规则匹配 (工具名 + glob 通配符) | `src/utils/permissions/shellRuleMatching.ts` | `globset` crate |
| Bash 命令分类器 (危险命令检测) | `src/tools/BashTool/bashSecurity.ts`, `bashPermissions.ts` | `regex` 匹配危险模式 (rm -rf, curl\|bash 等), 后续可升级 `tree-sitter-bash` |
| 文件路径权限检查 | `src/utils/permissions/filesystem.ts`, `pathValidation.ts` | 工作目录边界检查 + symlink 解析 |
| 拒绝追踪 | `src/utils/permissions/denialTracking.ts` | 累计拒绝次数，超过阈值后 fallback 到提示 |
| 权限持久化 | `src/utils/permissions/PermissionUpdate.ts` | 保存 allow/deny 规则到 settings.json |
| 危险模式过滤 | `src/utils/permissions/dangerousPatterns.ts` | regex 集合 |

**模式行为**:
- `Default`: 非只读工具都要问用户
- `Auto`: 分类器预筛 + 危险命令拒绝
- `Plan`: 仅只读工具
- `BypassPermissions`: 全部允许

---

## Phase 5: 工具系统 (`claude-tools`)

### 5.1 Tool trait 与基础设施

| 做什么 | 参考源码 | Rust 方案 |
|--------|----------|-----------|
| `Tool` trait 定义 | `src/Tool.ts:158-299` (ToolUseContext) | 见下方 trait |
| `ToolRegistry` | `src/tools.ts` (getTools, findToolByName) | `HashMap<String, Box<dyn Tool>>` + alias 索引 |
| 工具编排 (并发/串行分区) | `src/services/tools/toolOrchestration.ts` | `tokio::JoinSet`, 只读工具并行，写工具串行 |
| 工具执行包装 (验证→权限→hooks→调用→post-hooks) | `src/services/tools/toolExecution.ts` | 管线函数 |
| 流式进度 | `src/services/tools/StreamingToolExecutor.ts` | `tokio::sync::mpsc::Sender<ToolProgress>` |
| 大结果存储 | `src/utils/toolResultStorage.ts` | 超过大小限制时写磁盘，返回摘要 + 文件路径 |

```rust
#[async_trait]
pub trait Tool: Send + Sync {
    fn name(&self) -> &str;
    fn aliases(&self) -> &[&str] { &[] }
    fn input_schema(&self) -> serde_json::Value;
    fn is_read_only(&self, input: &Value) -> bool;
    fn is_enabled(&self, ctx: &ToolContext) -> bool { true }
    async fn validate(&self, input: &Value, ctx: &ToolUseContext) -> Result<(), ValidationError>;
    async fn call(&self, input: Value, ctx: &mut ToolCallContext) -> ToolResult;
    fn description(&self, input: &Value) -> String;
    fn should_defer(&self) -> bool { false }
}
```

### 5.2 工具实现 (按优先级)

**Tier 1 — 核心 (必须首先实现)**:

| 工具 | 参考源码 | Rust 方案 |
|------|----------|-----------|
| **BashTool** | `src/tools/BashTool/BashTool.tsx` + `bashSecurity.ts` + `bashPermissions.ts` | `tokio::process::Command`, 超时/输出截断/后台任务, 沙箱支持 |
| **FileReadTool** | `src/tools/FileReadTool/FileReadTool.ts` | offset/limit 读取, 行号注释, 图片→base64, PDF 解析, Jupyter 解析 |
| **FileWriteTool** | `src/tools/FileWriteTool/FileWriteTool.ts` | 创建父目录, 写入, 编码/换行保持, 文件历史 |
| **FileEditTool** | `src/tools/FileEditTool/FileEditTool.ts` | old_string→new_string 精确替换, 唯一性验证, diff 生成 |
| **GrepTool** | `src/tools/GrepTool/GrepTool.ts` | 包装 `rg` 子进程, 支持 output_mode/context/-A/-B/glob/type |
| **GlobTool** | `src/tools/GlobTool/GlobTool.ts` | `globset` + `walkdir`, 排除模式, 按修改时间排序 |

**Tier 2 — 重要**:

| 工具 | 参考源码 | Rust 方案 |
|------|----------|-----------|
| **WebFetchTool** | `src/tools/WebFetchTool/` | `reqwest` + `scraper` (HTML→text), 内容截断 |
| **WebSearchTool** | `src/tools/WebSearchTool/` | 搜索 API 集成 |
| **AgentTool** | `src/tools/AgentTool/` (runAgent, forkSubagent, built-in/) | 隔离 QueryEngine 实例, 详见 Phase 10 |
| **SkillTool** | `src/tools/SkillTool/` | 技能执行, 详见 Phase 10 |
| **AskUserQuestionTool** | `src/tools/AskUserQuestionTool/` | 通过 channel 向 TUI 请求用户输入 |
| **TodoWriteTool** | `src/tools/TodoWriteTool/` | 文件 I/O |
| **NotebookEditTool** | `src/tools/NotebookEditTool/` | JSON cell 编辑 |

**Tier 3 — 专用工具**:

| 工具 | 参考源码 |
|------|----------|
| MCPTool / ListMcpResourcesTool / ReadMcpResourceTool | `src/tools/MCPTool/`, `src/tools/ListMcpResourcesTool/`, `src/tools/ReadMcpResourceTool/` |
| TaskCreate/Get/Update/List/Stop/OutputTool | `src/tools/Task*Tool/` |
| EnterPlanModeTool / ExitPlanModeTool | `src/tools/EnterPlanModeTool/`, `src/tools/ExitPlanModeTool/` |
| EnterWorktreeTool / ExitWorktreeTool | `src/tools/EnterWorktreeTool/`, `src/tools/ExitWorktreeTool/` |
| ConfigTool / ToolSearchTool / LSPTool | `src/tools/ConfigTool/`, `src/tools/ToolSearchTool/`, `src/tools/LSPTool/` |
| PowerShellTool / SyntheticOutputTool / BriefTool | `src/tools/PowerShellTool/`, `src/tools/SyntheticOutputTool/`, `src/tools/BriefTool/` |

**Tier 4 — Feature-gated (stubs → Phase 12 补完)**:

| 工具 | Feature Flag |
|------|-------------|
| SleepTool | `proactive` / `kairos` |
| CronCreate/Delete/ListTool | `agent_triggers` |
| RemoteTriggerTool | `agent_triggers_remote` |
| SendMessage/TeamCreate/TeamDeleteTool | `coordinator_mode` |
| MonitorTool | `monitor_tool` |
| PushNotificationTool / SubscribePRTool | `kairos` |

---

## Phase 6: 查询引擎与管线 (`claude-query`)

### 6.1 消息管理

| 做什么 | 参考源码 | Rust 方案 |
|--------|----------|-----------|
| MessageBuilder | `src/utils/messages.ts` (createUserMessage 等) | builder pattern |
| normalizeMessagesForAPI | `src/utils/messages.ts` | 剥离 UI 消息, 确保 tool_use/tool_result 配对 |
| 消息队列 | `src/utils/messageQueueManager.ts` | 优先级队列 |
| 附件系统 (CLAUDE.md/memory 注入) | `src/utils/attachments.ts` | 加载+去重+注入 system prompt |

### 6.2 系统提示词组装

| 做什么 | 参考源码 | Rust 方案 |
|--------|----------|-----------|
| System prompt builder | `src/utils/queryContext.ts` (fetchSystemPromptParts) | 拼接 identity + tools + permissions + context + memory + CLAUDE.md |
| 用户上下文注入 (git/platform/cwd/date) | `src/context.ts` | 模板字符串替换 |
| 自定义 system prompt | CLI `--system-prompt` / `--append-system-prompt` | 替换或追加 |

### 6.3 查询管线 (`query()`)

| 做什么 | 参考源码 | Rust 方案 |
|--------|----------|-----------|
| 主 API 调用循环 | `src/query.ts` (1729 行) | loop: build_messages → call_api → process_stream → execute_tools → append_results → 直到 end_turn |
| Token 预算执行 | `src/query/tokenBudget.ts`, `src/utils/tokens.ts` | 上下文窗口检查 → 触发 auto-compact |
| 自动压缩 | `src/services/compact/autoCompact.ts` | 接近上下文限制时触发 compact |
| 微压缩 | `src/services/compact/microCompact.ts` | 大工具结果的就地压缩 |
| Post-sampling hooks | `src/utils/hooks/postSamplingHooks.ts` | 每次 API 响应后执行 |
| Prompt-too-long 恢复 | `src/services/api/errors.ts` | 捕获错误 → 自动 compact → 重试 |
| 中断处理 | `src/query.ts` (Ctrl+C) | `tokio::select!` + AbortController 等价 (`CancellationToken`) |
| Fallback model | `src/services/api/withRetry.ts` (FallbackTriggeredError) | 特定错误时切换备用模型重试 |

### 6.4 QueryEngine

| 做什么 | 参考源码 | Rust 方案 |
|--------|----------|-----------|
| QueryEngine struct | `src/QueryEngine.ts` (1295 行) | 拥有 tools/commands/mcp_clients/state 的 struct |
| submit_query() | `QueryEngine.ts` processUserInput → query pipeline | 处理用户输入 → 运行查询管线 → 返回结果 |
| 会话管理 | createSession/resumeSession | 创建/恢复会话 |
| File state cache | `src/utils/fileStateCache.ts` | LRU cache 追踪已读文件 |
| File history | `src/utils/fileHistory.ts` | 追踪文件修改用于 undo |
| Attribution | `src/utils/commitAttribution.ts` | 追踪文件修改用于 commit |
| Thinking config | `src/utils/thinking.ts` | 扩展思维模式支持 |

**Rust 关键设计**: `tokio::select!` 用于取消; `mpsc` channel 从 API 流到 UI; `Arc<RwLock<Vec<Message>>>` 共享消息列表

---

## Phase 7: MCP 集成 (`claude-mcp`)

| 做什么 | 参考源码 | Rust 方案 |
|--------|----------|-----------|
| MCP transport trait | `src/services/mcp/client.ts` | trait McpTransport { async send/receive } |
| Stdio transport | 同上 | `tokio::process::Command` + stdin/stdout JSON-RPC |
| SSE transport | 同上 | `reqwest-eventsource` |
| Streamable HTTP transport | 同上 | `reqwest` request/response |
| MCP client (initialize/list_tools/call_tool/list_resources/read_resource) | `src/services/mcp/client.ts` | JSON-RPC 2.0 客户端 |
| 连接管理器 (connect/disconnect/reconnect) | `src/services/mcp/MCPConnectionManager.tsx`, `useManageMCPConnections.ts` | `HashMap<String, McpConnection>` + 生命周期管理 |
| 动态工具创建 (MCP tool def → `Box<dyn Tool>`) | `src/tools/MCPTool/MCPTool.ts` | 运行时注册 |
| MCP config 加载 | `src/services/mcp/config.ts` | 从 settings/project config/CLI 加载 |
| MCP OAuth | `src/services/mcp/auth.ts` | token 获取与刷新 |
| Official server registry | `src/services/mcp/officialRegistry.ts` | 预取官方服务器列表 |

**关键 crate**: 优先用 `mcp-sdk-rs`; 若不够成熟则自写 JSON-RPC 2.0

---

## Phase 8: 命令/Hook/Memory/历史/压缩

### 8.1 斜杠命令系统 (`claude-commands`)

| 做什么 | 参考源码 | Rust 方案 |
|--------|----------|-----------|
| `Command` trait | `src/commands.ts` (25K 行) | trait Command { fn name/aliases/description; async fn execute } |
| 命令注册表 | 同上 getCommands() | HashMap 查找 |

**实现优先级**:
1. **核心**: `/compact`, `/help`, `/clear`, `/exit`, `/config`, `/memory`, `/doctor`, `/login`, `/logout`, `/version`, `/status`
2. **重要**: `/commit`, `/diff`, `/review`, `/mcp`, `/resume`, `/session`, `/share`, `/export`, `/init`
3. **次要**: `/cost`, `/usage`, `/context`, `/theme`, `/vim`, `/keybindings`, `/color`, `/tasks`, `/skills`, `/rename`
4. **高级**: `/commit-push-pr`, `/autofix-pr`, `/issue`, `/pr_comments`, `/bughunter`
5. **Feature-gated**: `/proactive`, `/brief`, `/assistant`, `/bridge`, `/voice` (stubs)

参考: `src/commands/` 下 87 个子目录

### 8.2 会话历史 (`claude-history`)

| 做什么 | 参考源码 | Rust 方案 |
|--------|----------|-----------|
| 会话持久化 (JSONL) | `src/utils/sessionStorage.ts` | `~/.claude/sessions/{id}.jsonl` 追加写 |
| 输入历史 (最近 100 条) | `src/history.ts` (14K 行) | 环形缓冲区 |
| Transcript 录制 | `src/utils/sessionStorage.ts` recordTranscript | 追加写 JSONL |
| 会话恢复 | `src/utils/sessionRestore.ts`, `conversationRecovery.ts` | 从 JSONL 重建消息列表 |
| Paste 存储 (大粘贴内容外部存储) | `src/history.ts` | hash 引用外部文件 |

### 8.3 Hook 系统 (`claude-hooks`)

| 做什么 | 参考源码 | Rust 方案 |
|--------|----------|-----------|
| Hook 事件类型 | `src/utils/hooks/hookEvents.ts` | enum: PreToolUse/PostToolUse/PostToolUseFailure/PermissionDenied/Notification/SessionStart/Setup |
| Hook 配置加载 (CLAUDE.md frontmatter + settings.json) | `src/utils/hooks/hooksConfigManager.ts`, `hooksSettings.ts` | serde 解析 |
| Hook 执行器 | `execAgentHook.ts`, `execHttpHook.ts`, `execPromptHook.ts` | Shell 子进程 / HTTP webhook / prompt 注入 |
| Async hook registry | `src/utils/hooks/AsyncHookRegistry.ts` | `tokio::sync::broadcast` 广播事件 |
| Hook 结果处理 (修改工具输入/阻止执行/注入消息) | `src/services/tools/toolHooks.ts` | 返回 HookResult enum (Allow/Deny/Modify) |

### 8.4 记忆系统 (`claude-memory`)

| 做什么 | 参考源码 | Rust 方案 |
|--------|----------|-----------|
| 记忆类型 (user/feedback/project/reference) | `src/memdir/memoryTypes.ts` | enum MemoryType |
| MEMORY.md 入口加载 (200 行/25KB 上限) | `src/memdir/memdir.ts` loadMemoryPrompt | 读取 + 截断 |
| 记忆文件 YAML frontmatter 解析 | `src/memdir/memoryScan.ts` | `serde_yaml` |
| 相关记忆查找 (关键词相关度) | `src/memdir/findRelevantMemories.ts` | 关键词匹配 + 打分 |
| 自动记忆提取 (通过子 agent) | `src/services/extractMemories/` | fork 子 agent 提取 |
| Team memory | `src/memdir/teamMemPaths.ts`, `teamMemPrompts.ts` | 团队级记忆路径 + 同步 |

### 8.5 上下文压缩 (`claude-compact`)

| 做什么 | 参考源码 | Rust 方案 |
|--------|----------|-----------|
| Compact (摘要替换旧消息) | `src/services/compact/compact.ts` | fork API 调用做摘要 → 替换 |
| Auto-compact (接近上下文限制时触发) | `src/services/compact/autoCompact.ts` | token 估算 → 触发阈值 |
| Micro-compact (大工具结果就地压缩) | `src/services/compact/microCompact.ts` | 截断 + 摘要 |
| Compact boundary markers | 同上 | CompactBoundary message 变体 |

---

## Phase 9: 终端 UI (`claude-tui`)

**架构转变**: TypeScript 用自定义 React/Ink 渲染器 (`src/ink/`, 96 文件)。Rust 用 `ratatui` (基于 `crossterm` 的 immediate-mode TUI 框架) 完全替代。

### 9.1 核心 TUI 框架

| 做什么 | 参考源码 | Rust 方案 |
|--------|----------|-----------|
| Application 主循环 | `src/ink/ink.tsx`, `src/main.tsx` | `ratatui::Terminal<CrosstermBackend>` + event loop |
| 事件处理 (键盘/resize/tick) | `src/ink/parse-keypress.ts`, `src/ink/events/` | `crossterm::event::read()` |
| 屏幕管理 | `src/screens/` | enum Screen { Repl, Doctor, Resume, ... } |
| 焦点管理 | `src/ink/focus.ts` | 自定义 focus 栈 |

### 9.2 REPL 主界面

| 做什么 | 参考源码 | Rust 方案 |
|--------|----------|-----------|
| 输入框 (多行编辑/光标) | `src/hooks/useTextInput.ts`, `src/components/InputBox.tsx` | `tui-textarea` 或自定义 widget |
| 消息显示 (虚拟滚动) | `src/components/MessageList.tsx`, `src/hooks/useVirtualScroll.ts` | 自定义 ScrollableList widget |
| Markdown 渲染 | `src/utils/markdown.ts` | `pulldown-cmark` 解析 + `syntect` 代码高亮 + 自定义终端渲染 |
| Diff 渲染 | `src/components/` 中 diff 相关 | 内联彩色 diff widget |
| 状态栏 (model/cost/token/mode) | `src/components/` 底部组件 | ratatui Paragraph widget |
| 工具执行进度 (spinner/进度条) | `src/components/Spinner.tsx` | `throbber-widgets-tui` 或自定义 |

### 9.3 输入处理

| 做什么 | 参考源码 | Rust 方案 |
|--------|----------|-----------|
| 文本输入 (Home/End/词跳转) | `src/hooks/useTextInput.ts` | crossterm 键事件处理 |
| 历史导航 (上下箭头) | `src/hooks/useArrowKeyHistory.tsx` | 历史缓冲区索引 |
| 粘贴处理 (多行/图片) | `src/hooks/useTextInput.ts` | bracket paste 检测 |
| Vim 模式 | `src/vim/` | feature-gated, 状态机 |
| 快捷键系统 | `src/keybindings/` | 可配置 keymap |
| Ctrl+C/Ctrl+D | 散布于多处 | 中断/退出 |
| 斜杠命令补全 | `src/hooks/usePromptSuggestion.ts` | 前缀匹配 |

### 9.4 对话框组件

| 做什么 | 参考源码 |
|--------|----------|
| 权限提示对话框 | `src/hooks/toolPermission/handlers/interactiveHandler.ts` |
| OAuth 流程对话框 | `src/components/ConsoleOAuthFlow.tsx` |
| 会话恢复选择器 | `src/screens/ResumeConversation.tsx` |
| 成本阈值警告 | `src/components/CostThresholdDialog.tsx` |
| 帮助界面 | `src/commands/help/` |

**关键 crate**: `ratatui`, `crossterm`, `syntect`, `pulldown-cmark`

---

## Phase 10: CLI 入口与 Agent/Skills

### 10.1 CLI 入口 (`claude-cli`)

| 做什么 | 参考源码 | Rust 方案 |
|--------|----------|-----------|
| CLI 参数解析 | `src/main.tsx` (Commander.js) | `clap` derive macros |
| 并行预取 (MDM/Keychain/API 预连接/Feature flags) | `src/main.tsx` 开头 | `tokio::join!` |
| 引导序列 (config→settings→auth→tools→commands→MCP→hooks→skills→QueryEngine→TUI) | `src/main.tsx`, `src/entrypoints/init.ts` | 按序初始化 |
| 非交互模式 (`-p`/`-c`) | `src/main.tsx` | 单次查询→打印→退出 |
| MCP 服务器模式 | `src/entrypoints/mcp.ts` | 通过 stdio 提供 MCP 服务 |
| 输出格式 (json/stream-json/text) | `src/cli/` | 格式化输出 |

**CLI 参数**: `--model`, `--system-prompt`, `--append-system-prompt`, `--permission-mode`, `--allowedTools`, `--disallowedTools`, `--resume`, `--session-id`, `--agent`, `--mcp-config`, `--verbose`, `--debug`, `--max-turns`, `--max-budget`, `--output-format`

### 10.2 Agent 系统 (`claude-agents`)

| 做什么 | 参考源码 | Rust 方案 |
|--------|----------|-----------|
| AgentDefinition struct | `src/tools/AgentTool/loadAgentsDir.ts` | name/prompt/tools/model/color |
| 内置 agent | `src/tools/AgentTool/built-in/` (general, plan, explore, verify, guide) | 静态定义 |
| 自定义 agent 加载 | `src/tools/AgentTool/loadAgentsDir.ts` (.claude/agents/) | 从磁盘 YAML/JSON 加载 |
| Agent 执行 (隔离 QueryEngine) | `src/tools/AgentTool/runAgent.ts` | 创建新 QueryEngine 实例，限制工具集 |
| Subagent forking | `src/tools/AgentTool/forkSubagent.ts` | 共享 file state cache |
| Agent resume | `src/tools/AgentTool/resumeAgent.ts` | 序列化/反序列化 agent 状态 |
| Agent color 管理 | `src/tools/AgentTool/agentColorManager.ts` | 为并发 agent 分配唯一颜色 |
| Coordinator mode | `src/coordinator/coordinatorMode.ts` | 多 agent 编排 + 共享 scratchpad |

### 10.3 Skills 系统 (`claude-skills`)

| 做什么 | 参考源码 | Rust 方案 |
|--------|----------|-----------|
| Skill struct | `src/skills/bundledSkills.ts` | name/content/trigger/allowed_tools |
| 内置 skills | `src/skills/bundled/` (batch, debug, loop, verify, simplify 等) | 静态注册 |
| 自定义 skill 加载 | `src/skills/loadSkillsDir.ts` | `.claude/skills/` 目录 |
| MCP skill builders | `src/skills/mcpSkillBuilders.ts` | 从 MCP 服务器创建 skill |

---

## Phase 11: 后台任务与 SDK

### 11.1 后台任务 (`claude-tasks`)

| 做什么 | 参考源码 | Rust 方案 |
|--------|----------|-----------|
| Task 状态管理 | `src/tasks/types.ts` | enum: Pending/Running/Completed/Failed/Cancelled |
| Shell 后台任务 | `src/tasks/LocalShellTask/` | `tokio::process::Command` 后台执行 |
| Agent 后台任务 | `src/tasks/LocalAgentTask/` | 后台 tokio task 运行子 agent |
| 任务输出流到文件 | `src/tools/TaskOutputTool/` | 追加写日志文件 |
| Todo 列表管理 | `src/tools/TodoWriteTool/` | JSON 文件 I/O |

### 11.2 Agent SDK (`claude-sdk`)

| 做什么 | 参考源码 | Rust 方案 |
|--------|----------|-----------|
| SDK 消息类型 | `src/entrypoints/agentSdkTypes.ts`, `sdk/coreTypes.ts` | serde structs |
| 会话管理 API | `src/entrypoints/sdk/controlSchemas.ts` | create/list/get/fork session |
| 结构化 I/O | `src/cli/` | JSON streaming 输入输出 |
| IDE bridge (Direct Connect) | `src/bridge/` | WebSocket 服务器 |

---

## Phase 12: Feature-Gated 功能补完

所有功能使用 Cargo features 条件编译:

```toml
[features]
default = []
coordinator_mode = []
agent_triggers = []
agent_triggers_remote = ["agent_triggers"]
voice_mode = []
kairos = []
proactive = []
context_collapse = []
transcript_classifier = []
history_snip = []
workflow_scripts = []
skill_search = []
reactive_compact = []
bridge_mode = []
monitor_tool = []
teammem = []
```

| Feature | 做什么 | 参考源码 |
|---------|--------|----------|
| `coordinator_mode` | 完整 coordinator 实现: worker agent spawning, 消息路由, 共享 scratchpad | `src/coordinator/coordinatorMode.ts` |
| `agent_triggers` | Cron 调度 agent: CronCreate/Delete/List Tool, 持久化存储 | `src/tools/ScheduleCronTool/` |
| `agent_triggers_remote` | RemoteTriggerTool, 远程 agent 执行 | `src/tools/RemoteTriggerTool/` |
| `voice_mode` | 语音输入: 捕获→STT→文本 | `src/voice/` |
| `kairos` | 后台助手: session backgrounding, push notification, away summary, PR subscription | `src/assistant/`, `src/tools/SleepTool/`, `src/tools/PushNotificationTool/`, `src/tools/SubscribePRTool/` |
| `context_collapse` | 高级上下文优化 (超越标准 compact) | `src/services/contextCollapse/` |
| `transcript_classifier` | Auto-mode ML 分类器: bash 命令安全分类, 决策缓存 | `src/utils/permissions/yoloClassifier.ts`, `bashClassifier.ts` |
| `history_snip` | 消息裁剪: 选择性从上下文中移除 | 散布于 query.ts 中 |
| `workflow_scripts` | 工作流自动化脚本 | `src/commands/workflows/` |
| `skill_search` | 智能 skill 发现: 相关度评分, 预取 | `src/services/skillSearch/` |
| `reactive_compact` | 响应式上下文压缩 | `src/services/compact/reactiveCompact.ts` |
| `bridge_mode` | IDE bridge: WebSocket 双向通信, JWT 认证 | `src/bridge/` |
| `monitor_tool` | 进程监控工具 | `src/tools/MonitorTool/` |
| `teammem` | Team memory 共享与同步 | `src/services/teamMemorySync/` |

---

## Phase 13: 分析遥测与集成测试

### 13.1 分析与遥测

| 做什么 | 参考源码 | Rust 方案 |
|--------|----------|-----------|
| Feature flag 客户端 (GrowthBook) | `src/services/analytics/growthbook.ts` | HTTP 拉取 + 本地评估 |
| 事件日志 (opt-in) | `src/services/analytics/` | 隐私友好的事件发送 |
| Policy limits | `src/services/policyLimits/` | 组织策略执行 |
| Remote managed settings | `src/services/remoteManagedSettings/` | 远程设置轮询 |
| Auto-updater | `src/utils/autoUpdater.ts` | 检查 GitHub releases |
| Startup profiler | `src/utils/startupProfiler.ts` | 计时检查点 |

### 13.2 集成测试

- 每个工具的 E2E 测试
- API mock 服务器
- 权限系统测试
- MCP 集成测试
- CLI 参数解析测试
- TUI snapshot 测试 (`insta` crate)
- 会话恢复/回放测试

### 13.3 兼容性

- 读取现有 `~/.claude/` 配置
- 读取现有 session transcripts
- Settings 格式兼容
- SDK 消费者 API 兼容

---

## 关键 Crate 汇总

| 用途 | Crate |
|------|-------|
| 异步运行时 | `tokio` |
| CLI 解析 | `clap` (derive) |
| HTTP 客户端 | `reqwest` (with streaming) |
| SSE | `reqwest-eventsource` |
| 终端 UI | `ratatui`, `crossterm` |
| 序列化 | `serde`, `serde_json`, `serde_yaml` |
| 文件 globbing | `globset`, `walkdir` |
| 正则搜索 | `regex` |
| Git | `git2` |
| 密钥存储 | `keyring` |
| Markdown | `pulldown-cmark` |
| 代码高亮 | `syntect` |
| 错误处理 | `thiserror`, `anyhow` |
| 日志/追踪 | `tracing`, `tracing-subscriber` |
| UUID | `uuid` |
| 文件监听 | `notify` |
| Windows 注册表 | `winreg` |
| 测试 mock | `mockall` |
| 快照测试 | `insta` |
| AWS 认证 | `aws-sigv4`, `aws-config` |
| 重试 | `backoff` |
| HTML 解析 | `scraper` |

---

## 验证方案

### 每个 Phase 完成后的验证:

1. **Phase 1**: `cargo build` 通过; 类型可正确序列化/反序列化; 配置文件加载测试
2. **Phase 2**: OAuth flow 可完成; API key 可从 keychain 读取
3. **Phase 3**: 可向 Claude API 发送流式请求并解析响应; 重试逻辑测试
4. **Phase 4**: 权限规则正确评估; 危险命令被检测; 各模式行为正确
5. **Phase 5**: 每个工具单独测试; BashTool 可执行命令; FileEditTool 可精确替换
6. **Phase 6**: 完整查询循环: 用户输入 → API → 工具调用 → 结果 → 循环到停止
7. **Phase 7**: 可连接 MCP 服务器并调用工具
8. **Phase 8**: 斜杠命令可执行; 会话可保存/恢复; Hook 可触发
9. **Phase 9**: TUI 可渲染; 可输入/编辑; 消息可滚动; 权限对话框可交互
10. **Phase 10**: `claude-cli` 二进制可启动完整交互会话; Agent 可 spawn 和执行
11. **Phase 11**: 后台任务可运行; SDK 模式可工作
12. **Phase 12**: 各 feature flag 功能可独立启用和测试
13. **Phase 13**: 遥测发送正确; 兼容现有配置文件

### 端到端测试:
```bash
# 基本交互
cargo run -- "Hello, Claude"

# 非交互模式
cargo run -- -p "What is 2+2?"

# 工具使用
cargo run -- "Read the file Cargo.toml"

# 会话恢复
cargo run -- --resume

# MCP 服务器模式
cargo run -- mcp serve
```
