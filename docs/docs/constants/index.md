---
sidebar_position: 1
title: constants/ — 全局常量
description: 版本、路径、限制、颜色等常量
---

# constants/ — 全局常量

**目录：** `src/constants/`

`constants/` 是 Claude Code 的**单一真相源 (Single Source of Truth)**——所有常量集中管理。

## 为什么集中？

```typescript
// ❌ 散落各处
// src/services/api.ts
const TIMEOUT = 30000

// src/utils/tasks.ts
const TIMEOUT = 60000  // 和 api.ts 冲突？

// src/components/Spinner.tsx
const SPINNER_INTERVAL = 100  // 硬编码
```

```typescript
// ✅ 集中
// src/constants/timeouts.ts
export const API_TIMEOUT_MS = 30_000
export const TASK_TIMEOUT_MS = 60_000
export const SPINNER_INTERVAL_MS = 80
```

**好处：**

- 一处修改，全局生效
- 容易 review
- 测试时可以 mock

## 分类

### version.ts

```typescript
export const VERSION = '2.0.0'
export const API_VERSION = '2024-10-01'
export const MCP_PROTOCOL_VERSION = '2024-11-05'
```

### paths.ts

```typescript
import { homedir } from 'os'
import path from 'path'

export const CLAUDE_HOME = process.env.CLAUDE_HOME ?? path.join(homedir(), '.claude')
export const MEMORY_DIR = path.join(CLAUDE_HOME, 'memory')
export const SESSIONS_DIR = path.join(CLAUDE_HOME, 'sessions')
export const PLUGINS_DIR = path.join(CLAUDE_HOME, 'plugins')
export const TASKS_DIR = path.join(CLAUDE_HOME, 'tasks')
export const CREDENTIALS_FILE = path.join(CLAUDE_HOME, 'credentials.json')
export const CONFIG_FILE = path.join(CLAUDE_HOME, 'config.json')
export const LOCK_FILE = path.join(CLAUDE_HOME, 'server.lock')
```

### timeouts.ts

```typescript
export const API_TIMEOUT_MS = 30_000
export const MCP_TIMEOUT_MS = 30_000
export const LSP_TIMEOUT_MS = 5_000
export const TASK_TIMEOUT_MS = 600_000
export const HOOK_TIMEOUT_MS = 30_000
export const BASH_TIMEOUT_MS = 120_000
```

### limits.ts

```typescript
export const MAX_CONTEXT_TOKENS = 200_000
export const MAX_OUTPUT_TOKENS = 8_192
export const MAX_TOOL_RESULT_SIZE = 100_000
export const MAX_TASK_OUTPUT_SIZE = 100 * 1024 * 1024  // 100MB
export const MAX_FILE_READ_SIZE = 10 * 1024 * 1024     // 10MB
export const MAX_CONCURRENT_TOOLS = 10
export const MAX_CONCURRENT_TASKS = 20
export const MAX_MCP_SERVERS = 50
```

### compaction.ts

```typescript
export const COMPACT_MICRO_THRESHOLD = 0.55
export const COMPACT_AUTO_THRESHOLD = 0.75
export const COMPACT_DREAM_THRESHOLD = 0.90
export const COMPACT_COOLDOWN_MS = 60_000
```

### models.ts

```typescript
export const DEFAULT_MODEL = 'claude-opus-4-6'
export const COMPACT_MODEL = 'claude-haiku-4-5-20251001'

export const AVAILABLE_MODELS = [
  'claude-opus-4-6',
  'claude-sonnet-4-6',
  'claude-haiku-4-5-20251001',
] as const

export type Model = typeof AVAILABLE_MODELS[number]
```

### pricing.ts

```typescript
export const PRICING: Record<string, ModelPricing> = {
  'claude-opus-4-6': {
    input: 15.0,       // $/M tokens
    output: 75.0,
    cacheWrite: 18.75,
    cacheRead: 1.5,
  },
  'claude-sonnet-4-6': {
    input: 3.0,
    output: 15.0,
    cacheWrite: 3.75,
    cacheRead: 0.3,
  },
  'claude-haiku-4-5-20251001': {
    input: 0.25,
    output: 1.25,
    cacheWrite: 0.3,
    cacheRead: 0.03,
  },
}
```

### colors.ts

```typescript
import chalk from 'chalk'

export const colors = {
  primary: chalk.cyan,
  success: chalk.green,
  warning: chalk.yellow,
  error: chalk.red,
  info: chalk.blue,
  muted: chalk.gray,
  inverse: chalk.inverse,
}
```

### urls.ts

```typescript
export const API_BASE_URL = 'https://api.anthropic.com'
export const AUTH_URL = 'https://auth.anthropic.com'
export const ANALYTICS_URL = 'https://analytics.anthropic.com'
export const MCP_REGISTRY_URL = 'https://registry.modelcontextprotocol.io'
export const PLUGIN_REGISTRY_URL = 'https://plugins.claude.ai'
export const DOCS_URL = 'https://docs.claude.com/claude-code'
```

### feature-flags.ts

```typescript
export const FEATURES = {
  BUDDY_MODE: true,
  REMOTE_TRIGGERS: true,
  EXPERIMENTAL_SKILLS: false,
  VIM_MODE: true,
  VOICE_INPUT: false,  // 实验性
}
```

**编译时常量** — Bun bundler 会 dead-code-eliminate 掉 false 分支。

### regex.ts

```typescript
export const FILE_PATH_REGEX = /^(?:[A-Za-z]:)?[\\/][^\s]+$/
export const URL_REGEX = /^https?:\/\/[^\s]+$/
export const EMAIL_REGEX = /^[^\s@]+@[^\s@]+\.[^\s@]+$/
export const API_KEY_REGEX = /^sk-ant-[a-zA-Z0-9]{40,}$/
```

### keybindings-default.ts

```typescript
export const DEFAULT_KEYBINDINGS = {
  'submit': 'return',
  'cancel': 'ctrl+c',
  'clear': 'ctrl+l',
  'togglePlanMode': 'shift+tab',
  'toggleTaskPanel': 'ctrl+t',
  'editInEditor': 'ctrl+e',
  'historyPrev': 'up',
  'historyNext': 'down',
}
```

### error-codes.ts

```typescript
export enum ErrorCode {
  UNKNOWN = 'E000',
  AUTH_FAILED = 'E001',
  RATE_LIMITED = 'E002',
  CONTEXT_EXCEEDED = 'E003',
  TOOL_FAILED = 'E100',
  PERMISSION_DENIED = 'E101',
  MCP_UNAVAILABLE = 'E200',
  // ...
}
```

## 只读 vs 可配置

```typescript
// 只读 — 代码层面常量
export const MCP_PROTOCOL_VERSION = '2024-11-05' as const

// 可配置 — 从 config 读
export const getMaxConcurrency = () => config.get('concurrency') ?? 10
```

## 常量命名约定

```typescript
// SCREAMING_SNAKE_CASE
export const MAX_TOKENS = 8192

// 带单位后缀
export const TIMEOUT_MS = 30000       // ✓
export const TIMEOUT = 30000          // ✗ 不清晰

// 布尔
export const ENABLE_X = true          // ✓
export const X = true                 // ✗

// 枚举值
export const Status = {
  IDLE: 'idle',
  RUNNING: 'running',
} as const
```

## 测试友好

```typescript
// tests/setup.ts
jest.mock('~/constants/limits', () => ({
  MAX_TOKENS: 100,  // 测试时用小值
}))
```

## 导入策略

```typescript
// 按需
import { API_TIMEOUT_MS } from '~/constants/timeouts'

// 或者批量
import * as C from '~/constants'
C.API_TIMEOUT_MS
```

Claude Code 代码库倾向于**按需导入**——减少 IDE 提示噪音。

## 值得学习的点

1. **单一真相源** — 所有常量集中
2. **按主题分文件** — 不是一个大文件
3. **带单位命名** — TIMEOUT_MS 而非 TIMEOUT
4. **Feature flags 用 const** — bundler 能 DCE
5. **Pricing 可独立维护** — 升级模型价格一处改
6. **URL 集中** — 环境切换方便
7. **as const 类型窄化** — 更好的类型检查

## 相关文档

- [schemas/](../schemas/index.md)
- [types/](../types/index.md)
- [constants 使用示例 - services/api](../services/api.md)
