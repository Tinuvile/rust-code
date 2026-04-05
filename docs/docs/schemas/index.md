---
sidebar_position: 1
title: schemas/ — Zod Schemas
description: 集中的数据校验 schemas
---

# schemas/ — Zod Schemas

**目录：** `src/schemas/`

`schemas/` 是 Claude Code **所有 Zod 校验定义**的集合。

## 为什么集中 Schemas？

### 分散 schema 的问题

```typescript
// tools/bash.ts
const args = z.object({ command: z.string() })

// services/mcp.ts
const args = z.object({ command: z.string() })  // 重复

// utils/exec.ts
const args = z.object({ command: z.string() })  // 又重复
```

### 集中定义

```typescript
// schemas/common.ts
export const ShellCommandSchema = z.object({
  command: z.string(),
  cwd: z.string().optional(),
  shell: z.enum(['bash', 'zsh', 'pwsh', 'cmd']).optional(),
  timeout: z.number().positive().optional()
})

// 使用
import { ShellCommandSchema } from '~/schemas/common'
```

## Schema 分类

### 1. API Schemas

```typescript
// schemas/api.ts
export const MessageSchema = z.object({
  role: z.enum(['user', 'assistant', 'system']),
  content: z.union([z.string(), z.array(ContentBlockSchema)])
})

export const UsageSchema = z.object({
  input_tokens: z.number(),
  output_tokens: z.number(),
  cache_creation_input_tokens: z.number().optional(),
  cache_read_input_tokens: z.number().optional(),
})
```

### 2. Tool Schemas

```typescript
// schemas/tools.ts
export const EditToolSchema = z.object({
  file_path: z.string(),
  old_string: z.string(),
  new_string: z.string(),
  replace_all: z.boolean().default(false)
})

export const BashToolSchema = z.object({
  command: z.string(),
  description: z.string().optional(),
  run_in_background: z.boolean().optional(),
  timeout: z.number().positive().max(600_000).optional(),
})
```

### 3. Config Schemas

```typescript
// schemas/config.ts
export const ConfigSchema = z.object({
  theme: z.enum(['light', 'dark', 'auto']).default('auto'),
  model: z.string(),
  mode: z.enum(['normal', 'plan', 'bypass', 'safe']).default('normal'),
  keybindings: z.record(z.string()).optional(),
  hooks: HooksSchema.optional(),
  mcp: z.object({
    servers: z.record(MCPServerSchema)
  }).optional(),
})
```

### 4. Plugin Schemas

```typescript
// schemas/plugins.ts
export const PluginManifestSchema = z.object({
  name: z.string().min(1).max(64).regex(/^[a-z0-9-]+$/),
  version: z.string().regex(/^\d+\.\d+\.\d+/),
  description: z.string().max(200),
  permissions: z.array(PermissionSchema),
  // ...
})
```

### 5. Permission Schemas

```typescript
// schemas/permissions.ts
export const PermissionRuleSchema = z.object({
  tool: z.string(),
  pattern: z.string().optional(),
  pathPattern: z.string().optional(),
  decision: z.enum(['allow_once', 'allow_session', 'allow_always', 'deny_once', 'deny_always'])
})
```

## Schema 组合

Zod 支持**组合**：

```typescript
const UserPublicSchema = z.object({
  name: z.string(),
  email: z.string().email(),
})

const UserPrivateSchema = UserPublicSchema.extend({
  apiKey: z.string(),
  sessionId: z.string(),
})
```

## 类型派生

从 schema **自动派生 TypeScript 类型**：

```typescript
export const TaskSchema = z.object({
  id: z.string(),
  command: z.string(),
  status: z.enum(['running', 'completed', 'failed']),
})

export type Task = z.infer<typeof TaskSchema>
```

**单一源头** — schema 改，类型自动跟着改。

## 验证错误格式化

```typescript
// schemas/validate.ts
export function formatValidationError(error: z.ZodError): string {
  return error.errors
    .map(e => `${e.path.join('.')}: ${e.message}`)
    .join('\n')
}

// 用法
try {
  TaskSchema.parse(data)
} catch (e) {
  if (e instanceof z.ZodError) {
    console.error(formatValidationError(e))
  }
}
```

## 安全解析

```typescript
// 不抛异常
const result = TaskSchema.safeParse(data)
if (result.success) {
  console.log(result.data)
} else {
  console.log(result.error)
}
```

## Schema Transform

```typescript
const TimestampSchema = z.string()
  .datetime()
  .transform(s => new Date(s))

// 输入 "2026-01-01T00:00:00Z"
// 输出 Date 对象
```

## 生成 JSON Schema

Zod 4 支持转 JSON Schema：

```typescript
import { toJsonSchema } from 'zod/v4'

const json = toJsonSchema(TaskSchema)
// {
//   "type": "object",
//   "properties": {
//     "id": { "type": "string" },
//     ...
//   }
// }
```

**给 LLM 的 tool schema 直接从 Zod 生成。**

## Schema 版本化

```typescript
// 每个版本的 schema
export const TaskSchemaV1 = z.object({...})
export const TaskSchemaV2 = z.object({...})

// 当前版本
export const TaskSchema = TaskSchemaV2
```

配合 migrations 使用。

## Schema 命名约定

```typescript
// 以 Schema 结尾
const UserSchema = z.object({...})

// 类型去掉后缀
type User = z.infer<typeof UserSchema>

// 可选变体
const UserPartialSchema = UserSchema.partial()
const UserOptionalSchema = UserSchema.optional()
```

## 常用工具函数

```typescript
// schemas/helpers.ts
export const nonEmptyString = z.string().min(1)

export const absolutePath = z.string().refine(
  p => path.isAbsolute(p),
  'Must be absolute path'
)

export const positiveInt = z.number().int().positive()

export const semver = z.string().regex(/^\d+\.\d+\.\d+/)
```

## Schema 单元测试

```typescript
test('TaskSchema validates', () => {
  expect(TaskSchema.safeParse({
    id: 'abc',
    command: 'ls',
    status: 'running'
  }).success).toBe(true)

  expect(TaskSchema.safeParse({
    id: 'abc',
    status: 'invalid'
  }).success).toBe(false)
})
```

## 值得学习的点

1. **集中 schema** — 避免重复
2. **从 schema 派生类型** — 单一源头
3. **Schema 是运行时合约** — 不只 TypeScript
4. **JSON Schema 导出** — 给 LLM 用
5. **Transform** — schema 带转换
6. **safeParse** — 函数式错误处理
7. **组合优先** — extend/merge/pick/omit

## 相关文档

- [types/](../types/index.md)
- [constants/](../constants/index.md)
- [Tool 工具框架](../root-files/tool-framework.md)
