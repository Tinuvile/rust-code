---
sidebar_position: 1
title: entrypoints/ — 多入口支持
description: CLI、IDE、Web、MCP 多种启动方式
---

# entrypoints/ — 多入口支持

**目录：** `src/entrypoints/`

Claude Code 不是**单一 CLI**——它有多个**启动方式**，对应不同的使用场景。`entrypoints/` 统一管理这些入口。

## 入口一览

```
entrypoints/
├── cli.ts           - 交互式 CLI (默认)
├── noninteractive.ts - 一次性执行
├── mcp-server.ts    - 作为 MCP Server
├── ide-stdio.ts     - IDE 插件 stdio
├── ide-http.ts      - IDE 插件 HTTP
├── web.ts           - Web UI 后端
├── worker.ts        - 远程 worker
└── test-runner.ts   - 测试用
```

## 1. CLI 入口（默认）

```bash
claude
# 或
claude
```

启动**交互式 REPL**：

```typescript
// entrypoints/cli.ts
async function main() {
  const args = parseArgs(process.argv)
  const ctx = await bootstrap()
  await startREPL(ctx)
}
```

特点：
- TTY 模式
- 完整 TUI
- 长会话

## 2. 非交互入口

```bash
claude -p "fix this bug" --file bug.ts
```

```typescript
// entrypoints/noninteractive.ts
async function main(prompt: string) {
  const ctx = await bootstrap()
  const response = await runOnce(ctx, prompt)
  console.log(response)
  process.exit(0)
}
```

特点：
- 一次执行退出
- 纯文本输出
- CI/CD 友好

## 3. MCP Server 入口

```bash
claude mcp-server
# 或放在其他工具的 mcp config 里
```

```typescript
// entrypoints/mcp-server.ts
async function main() {
  const server = new MCPServer({
    name: 'claude-code',
    version: VERSION,
  })

  server.addTool('refactor', refactorHandler)
  server.addTool('analyze', analyzeHandler)

  await server.start({ transport: 'stdio' })
}
```

特点：
- 把 Claude Code 能力暴露为 MCP 工具
- 给其他 AI 应用使用（比如 Cursor、Cline）

## 4. IDE stdio 入口

VS Code 插件启动 Claude Code 子进程：

```typescript
// entrypoints/ide-stdio.ts
async function main() {
  const bridge = new StdioBridge(process.stdin, process.stdout)
  const ctx = await bootstrap({ mode: 'ide' })

  bridge.on('prompt', async (text) => {
    const response = await runAgent(ctx, text)
    bridge.send({ type: 'response', content: response })
  })
}
```

特点：
- JSON-RPC over stdio
- 无 TUI（UI 在 IDE）
- 事件驱动

## 5. IDE HTTP 入口

替代 stdio——IDE 通过 HTTP 连接：

```typescript
// entrypoints/ide-http.ts
async function main() {
  const port = await findFreePort()
  const app = express()

  app.post('/prompt', async (req, res) => {
    const response = await runAgent(req.body.prompt)
    res.json(response)
  })

  app.listen(port)
}
```

特点：
- 跨语言 IDE 集成
- 本地网络通信

## 6. Web UI 后端

```typescript
// entrypoints/web.ts
async function main() {
  const ctx = await bootstrap()
  const server = createHTTPServer(ctx)
  server.listen(process.env.PORT ?? 8080)
}
```

特点：
- 远程部署
- 多租户（每会话独立 ctx）
- 认证层

## 7. Worker 入口（远程触发）

```typescript
// entrypoints/worker.ts
async function main() {
  const triggerId = process.env.TRIGGER_ID
  const trigger = await loadTrigger(triggerId)
  const ctx = await bootstrap({ mode: 'worker' })

  const result = await runAgent(ctx, trigger.prompt)
  await reportResult(triggerId, result)
  process.exit(0)
}
```

特点：
- 单次执行
- 远程触发
- 结果回调

## 入口选择

```typescript
// 根据 argv[2] 选择入口
const entrypoint = process.argv[2]
switch (entrypoint) {
  case 'mcp-server': return entrypoints.mcpServer()
  case 'worker': return entrypoints.worker()
  case 'web': return entrypoints.web()
  default: return entrypoints.cli()
}
```

或通过环境：

```typescript
if (process.env.CLAUDE_ENTRYPOINT === 'worker') {
  return entrypoints.worker()
}
```

## 入口共享的初始化

所有入口都经过**核心 bootstrap**：

```typescript
async function bootstrap(opts: BootOpts) {
  const config = await loadConfig(opts)
  const auth = await initAuth(config)
  const tools = await loadTools()
  const services = await startServices()
  return { config, auth, tools, services }
}
```

**入口只负责：**
- 参数解析
- UI/通信协议
- 结束处理

**核心逻辑**被所有入口复用。

## 入口特定的优化

### CLI 入口

```typescript
// 尽快显示 TUI，后台继续加载
showWelcome()
const ctx = await quickBoot()
startREPL(ctx)
backgroundLoad(ctx)
```

### Worker 入口

```typescript
// 无 TUI，无交互初始化
const ctx = await fastBoot({ skipUI: true })
```

### MCP Server 入口

```typescript
// stdio 模式，压缩所有日志
process.env.LOG_FILE = '/tmp/mcp-server.log'
```

## 入口的退出处理

```typescript
// CLI — 保存会话后退出
process.on('SIGINT', async () => {
  await saveSession()
  await flushAnalytics()
  process.exit(0)
})

// Worker — 汇报结果后退出
process.on('exit', async () => {
  await reportResult()
})
```

## 测试入口

```typescript
// entrypoints/test-runner.ts
async function main() {
  const ctx = await bootstrap({ mode: 'test' })

  // 跑内置测试
  await runTests(ctx)
  process.exit(failedCount)
}
```

用于 CI 验证。

## 值得学习的点

1. **多入口架构** — 一套核心多种用法
2. **共享 bootstrap** — 避免重复初始化
3. **入口特定优化** — CLI 快显示，worker 无 UI
4. **协议层分离** — stdio/http/JSON-RPC
5. **退出处理差异** — 每种入口不同
6. **测试入口** — CI 友好

## 相关文档

- [bootstrap/](../bootstrap/index.md)
- [cli/](../cli/index.md)
- [server/](../server/index.md)
- [remote/](../remote/index.md)
