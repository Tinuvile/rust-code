---
sidebar_position: 6
title: 其他服务
description: LSP、SessionMemory、Telemetry 等辅助服务
---

# 其他服务

**目录：** `src/services/` 的其他子目录

这些是**支撑型服务**——Claude Code 正常运转需要，但不是核心。

## LSP Service

**目录：** `src/services/lsp/`

跑 Language Server 协议客户端，支持多语言。

### 支持的语言

```typescript
const LSP_SERVERS = {
  typescript: 'typescript-language-server',
  python: 'pylsp',
  rust: 'rust-analyzer',
  go: 'gopls',
  java: 'jdtls',
  cpp: 'clangd',
  // ...
}
```

### 按需启动

不是所有 LSP 都启动——**只在文件被触及时启动对应的 server**：

```typescript
async function getLSP(filePath: string): Promise<LSPClient> {
  const lang = detectLanguage(filePath)
  if (!lsp[lang]) {
    lsp[lang] = await startLSP(LSP_SERVERS[lang])
  }
  return lsp[lang]
}
```

**延迟启动** 节省内存和启动时间。

### 超时保护

LSP 可能挂起：

```typescript
async function getDefinition(uri: string, pos: Position) {
  return Promise.race([
    lsp.request('textDocument/definition', { uri, pos }),
    timeout(5000).then(() => null)
  ])
}
```

5 秒没响应 → 放弃，继续工作。

## Session Memory

**目录：** `src/services/sessionMemory/`

**会话内**的短期记忆（不跨对话）。

### 存什么？

```typescript
interface SessionMemory {
  userFacts: string[]           // "用户偏好 TypeScript"
  projectContext: string[]      // "这是个 React 项目"
  recentFiles: string[]         // 最近打开的文件
  errorHistory: Error[]         // 最近的错误
  decisions: Decision[]         // 已做的决策
}
```

### 与长期 Memory 的区别

| | Session Memory | Long-term Memory |
|--|----------------|------------------|
| 范围 | 单次对话 | 跨对话 |
| 存储 | 内存 | `~/.claude/memory/*.md` |
| 大小 | 无限（不写盘） | 有限制 |
| 生命周期 | 对话结束即清 | 永久 |

**Session Memory 在 compaction 时部分保留**——最重要的决策会**升级为 Long-term**。

## Telemetry Service

**目录：** `src/services/telemetry/`

和 Analytics 区分：

- **Analytics** — 用户行为（打点）
- **Telemetry** — 系统诊断（性能、错误、状态）

### 诊断指标

```typescript
interface Telemetry {
  memoryUsage: NodeJS.MemoryUsage
  cpuTime: number
  openFileDescriptors: number
  taskCount: number
  mcpServerHealth: Record<string, Health>
}
```

### Debug 命令

```bash
claude diag
```

输出：

```
System:
  Platform: darwin 14.1
  Node: v20.10.0
  Memory: 234MB / 8GB

Services:
  API: healthy (34ms p50)
  MCP servers:
    github: healthy
    postgres: DEGRADED (timeout 3/5 requests)

Active tasks: 2
Open file handles: 45
```

## Cost Service

**目录：** `src/services/cost/`

计算 token 成本。详见 [setup-and-cost](../root-files/setup-and-cost.md)。

核心逻辑：

```typescript
function computeCost(usage: Usage, model: string): number {
  const pricing = PRICING[model]

  return (
    usage.input_tokens * pricing.input_per_million / 1_000_000 +
    usage.output_tokens * pricing.output_per_million / 1_000_000 +
    usage.cache_creation_input_tokens * pricing.cache_write_per_million / 1_000_000 +
    usage.cache_read_input_tokens * pricing.cache_read_per_million / 1_000_000
  )
}
```

**Cache read 仅为 input 的 10%**——Prompt Caching 的经济价值。

## Session Service

**目录：** `src/services/session/`

管理对话会话的**生命周期**。

### 会话恢复

每次对话保存到 `~/.claude/sessions/<id>.jsonl`：

```jsonl
{"type":"user","content":"..."}
{"type":"assistant","content":"..."}
{"type":"tool_use","name":"Read",...}
```

支持**从中间断点继续**：

```bash
claude --continue
# 恢复上次会话

claude --resume <session-id>
# 恢复指定会话
```

### 会话查询

```bash
claude sessions list
# 列出最近会话

claude sessions show <id>
# 查看特定会话
```

## Notification Service

**目录：** `src/services/notifications/`

**桌面通知**——长任务完成时告诉用户。

```typescript
async function notify(title: string, body: string) {
  if (platform === 'darwin') {
    await exec(`osascript -e 'display notification "${body}" with title "${title}"'`)
  } else if (platform === 'linux') {
    await exec(`notify-send "${title}" "${body}"`)
  } else if (platform === 'win32') {
    await exec(`powershell -Command "New-BurntToastNotification ..."`)
  }
}
```

### 触发场景

- 长任务完成（>30s）
- 需要用户输入
- 错误发生

### 用户控制

```bash
claude config set notifications.enabled false
```

## Cache Service

**目录：** `src/services/cache/`

多层缓存管理：

```
Memory cache (hot, <100ms)
     ↓ miss
Disk cache (warm, <1s)
     ↓ miss
Network (cold, >1s)
```

### 缓存对象

- **文件内容**（按 mtime 失效）
- **Token counts**（按 content hash）
- **LSP 响应**（按 position）
- **API 响应**（conditional, 限定使用）

### LRU 驱逐

```typescript
class LRUCache<K, V> {
  private max = 1000
  private map = new Map<K, V>()

  get(key: K): V | undefined {
    const val = this.map.get(key)
    if (val !== undefined) {
      this.map.delete(key)
      this.map.set(key, val)  // 移到末尾
    }
    return val
  }

  set(key: K, val: V) {
    if (this.map.size >= this.max) {
      const oldest = this.map.keys().next().value
      this.map.delete(oldest)
    }
    this.map.set(key, val)
  }
}
```

**Map 天然有序**（插入顺序），JS LRU 写起来很简单。

## Health Check Service

定期检查关键子系统：

```typescript
const healthChecks = {
  api: async () => fetch('https://api.anthropic.com/health').ok,
  mcp: async () => Promise.all(mcpServers.map(s => s.ping())),
  fs: async () => fs.access('~/.claude'),
}

setInterval(async () => {
  for (const [name, check] of Object.entries(healthChecks)) {
    try {
      const ok = await check()
      if (!ok) warn(`${name} unhealthy`)
    } catch (e) {
      warn(`${name} check failed: ${e}`)
    }
  }
}, 60_000)  // 每分钟
```

**主动发现问题**——比用户报 bug 早。

## 值得学习的点

1. **按需启动 LSP** — 延迟节省资源
2. **Session vs Long-term memory** — 生命周期区分
3. **Telemetry vs Analytics** — 诊断 vs 行为
4. **多层缓存** — memory → disk → network
5. **LRU with Map** — JS 的简洁实现
6. **Session 恢复** — 长期对话的保障
7. **主动健康检查** — 比被动报错早

## 相关文档

- [services/api](./api.md)
- [services/compact](./compact.md)
- [services/analytics](./analytics.md)
- [memdir/ - 长期记忆](../memdir/index.md)
