---
sidebar_position: 1
title: types/ — TypeScript 类型
description: 跨模块共享的类型定义
---

# types/ — TypeScript 类型

**目录：** `src/types/`

`types/` 是**不带运行时代码**的纯类型定义——跨模块共享。

## 与 schemas/ 的区别

| | schemas/ | types/ |
|--|----------|--------|
| 运行时 | 有（Zod 对象） | 无 |
| 作用 | 校验 + 类型 | 仅类型 |
| 示例 | `UserSchema.parse(x)` | `const u: User = ...` |

**规则：**

- 能用 `z.infer` 派生 → 用 schema
- 纯类型（接口、联合、泛型）→ 用 types

## 常见类型定义

### 工具相关

```typescript
// types/tools.ts
export type ToolName =
  | 'Bash' | 'Read' | 'Edit' | 'Write'
  | 'Grep' | 'Glob' | 'Agent' | 'WebFetch'
  | 'TaskCreate' | 'MCPTool'
  // ...

export type ToolCategory = 'readonly' | 'filesystem' | 'shell' | 'network' | 'meta'

export interface ToolMetadata {
  name: ToolName
  category: ToolCategory
  description: string
  permission: 'auto' | 'ask' | 'always'
}
```

### 消息相关

```typescript
// types/messages.ts
export type Role = 'user' | 'assistant' | 'system' | 'tool'

export type ContentBlockType =
  | 'text'
  | 'image'
  | 'tool_use'
  | 'tool_result'
  | 'thinking'

export interface TextBlock {
  type: 'text'
  text: string
}

export interface ImageBlock {
  type: 'image'
  source: {
    type: 'base64' | 'url'
    media_type: string
    data: string
  }
}

export type ContentBlock = TextBlock | ImageBlock | ToolUseBlock | ToolResultBlock
```

### 泛型工具类型

```typescript
// types/utility.ts

// 深度部分可选
export type DeepPartial<T> = {
  [P in keyof T]?: T[P] extends object ? DeepPartial<T[P]> : T[P]
}

// 深度只读
export type DeepReadonly<T> = {
  readonly [P in keyof T]: T[P] extends object ? DeepReadonly<T[P]> : T[P]
}

// 值类型
export type ValueOf<T> = T[keyof T]

// 去除某些字段
export type Omit<T, K extends keyof T> = Pick<T, Exclude<keyof T, K>>

// 必选某些字段
export type RequiredOnly<T, K extends keyof T> = T & Required<Pick<T, K>>

// AsyncIterator 的类型
export type Awaitable<T> = T | Promise<T>
```

### 事件类型

```typescript
// types/events.ts
export type EventMap = {
  'message:new': MessageEvent
  'tool:call': ToolCallEvent
  'tool:result': ToolResultEvent
  'task:created': TaskEvent
  'query:start': QueryEvent
  'query:end': QueryEvent
  'error': ErrorEvent
}

export type EventName = keyof EventMap

export type EventHandler<K extends EventName> = (event: EventMap[K]) => void
```

### Result/Either 类型

```typescript
// types/result.ts
export type Result<T, E = Error> =
  | { ok: true; value: T }
  | { ok: false; error: E }

export function ok<T>(value: T): Result<T, never> {
  return { ok: true, value }
}

export function err<E>(error: E): Result<never, E> {
  return { ok: false, error }
}

// 用法
function divide(a: number, b: number): Result<number, string> {
  if (b === 0) return err('Division by zero')
  return ok(a / b)
}
```

### Branded Types

```typescript
// types/branded.ts
export type Brand<T, B> = T & { __brand: B }

export type UserId = Brand<string, 'UserId'>
export type SessionId = Brand<string, 'SessionId'>
export type FilePath = Brand<string, 'FilePath'>

// 编译时区分
function loadUser(id: UserId) { ... }
function loadSession(id: SessionId) { ... }

const u: UserId = 'user-123' as UserId
const s: SessionId = 'sess-456' as SessionId

loadUser(u)    // ✓
loadUser(s)    // ✗ Type error
```

### 配置类型

```typescript
// types/config.ts
export interface Config {
  theme: Theme
  model: ModelName
  mode: ExecutionMode
  keybindings: KeybindingMap
  hooks: HookConfig
  mcp: MCPConfig
  analytics: AnalyticsConfig
}

export type Theme = 'light' | 'dark' | 'auto'
export type ExecutionMode = 'normal' | 'plan' | 'bypass' | 'safe'
```

## 类型保护函数

```typescript
// types/guards.ts
export function isString(x: unknown): x is string {
  return typeof x === 'string'
}

export function isMessage(x: unknown): x is Message {
  return typeof x === 'object' && x !== null && 'role' in x && 'content' in x
}

export function isToolUse(block: ContentBlock): block is ToolUseBlock {
  return block.type === 'tool_use'
}

// 用法
if (isToolUse(block)) {
  console.log(block.name)  // TypeScript 知道 block 有 name
}
```

## Opaque Types

```typescript
// 不想暴露内部结构
export type ConnectionHandle = Brand<number, 'ConnectionHandle'>

// 只能通过 createConnection 获取
export function createConnection(): ConnectionHandle {
  const handle = internalCreate() as ConnectionHandle
  return handle
}

export function closeConnection(h: ConnectionHandle) {
  internalClose(h as number)
}
```

## Discriminated Unions

```typescript
export type APIError =
  | { type: 'rate_limit', retryAfter: number }
  | { type: 'context_exceeded', tokens: number, limit: number }
  | { type: 'auth', message: string }
  | { type: 'network', cause: Error }

function handleError(e: APIError) {
  switch (e.type) {
    case 'rate_limit':
      return `Retry in ${e.retryAfter}s`  // 只有这个分支有 retryAfter
    case 'context_exceeded':
      return `Too long: ${e.tokens}/${e.limit}`
    // ...
  }
}
```

## Type-Level 运算

```typescript
// 取得对象所有字符串键
export type StringKeys<T> = {
  [K in keyof T]: T[K] extends string ? K : never
}[keyof T]

interface User { id: string; name: string; age: number }
type UserStringKeys = StringKeys<User>  // 'id' | 'name'
```

## 导出约定

```typescript
// 每个文件一个 namespace（可选）
export namespace Task {
  export interface Spec { ... }
  export interface State { ... }
  export type Status = 'running' | 'completed'
}

// 或平铺（更常见）
export interface TaskSpec { ... }
export interface TaskState { ... }
export type TaskStatus = 'running' | 'completed'
```

Claude Code **倾向平铺**。

## 值得学习的点

1. **types vs schemas 分工** — 纯类型 vs 运行时校验
2. **Branded types** — 编译时类型区分
3. **Discriminated union** — 类型安全的错误/状态
4. **类型保护函数** — 运行时类型收窄
5. **泛型工具** — DeepPartial 等
6. **Result 类型** — 函数式错误处理
7. **Opaque handles** — 隐藏内部结构

## 相关文档

- [schemas/](../schemas/index.md)
- [constants/](../constants/index.md)
