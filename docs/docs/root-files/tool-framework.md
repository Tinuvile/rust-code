---
sidebar_position: 3
title: Tool - 工具框架
description: buildTool 泛型工厂、Zod schema、权限模型
---

# Tool — 工具框架

**文件：** `src/Tool.ts`（30 KB）

这是所有工具的**类型契约和工厂函数**。Claude Code 有 40+ 工具，它们都通过这个框架构建。

## 核心问题

如何在 TypeScript 里实现一个工具系统，满足：

1. **类型安全** — 每个工具的 input、output、progress、render 都类型一致
2. **可扩展** — 新增工具不需要修改 core
3. **运行时校验** — 来自 Claude 的 JSON 需要 validate
4. **权限集成** — 每个工具都能声明自己的权限需求
5. **UI 集成** — 工具结果能被终端 UI 渲染
6. **流式进度** — 长任务能报告进度

## buildTool 泛型工厂

```typescript
export function buildTool<D extends AnyToolDef>(def: D): BuiltTool<D> {
  return {
    name: def.name,
    aliases: def.aliases ?? [],
    description: def.description,
    inputSchema: def.inputSchema,
    isReadOnly: def.isReadOnly ?? (() => false),
    isEnabled: def.isEnabled ?? (() => true),
    validate: def.validate ?? ((_) => ({ result: true })),
    call: def.call,
    renderResultForAssistant: def.renderResultForAssistant,
    renderToolUseMessage: def.renderToolUseMessage,
    // ... 更多默认实现
  }
}
```

关键在于 **`<D extends AnyToolDef>`** 这个泛型约束：

- 输入 `D` 是具体的 ToolDef 类型
- 输出 `BuiltTool<D>` 的各个方法签名**根据 D 精确推导**
- 调用者使用时**不需要显式泛型参数**

## ToolDef 的形状

```typescript
type ToolDef<
  InputShape extends ZodRawShape,
  Output,
  ProgressData extends ToolProgressData
> = {
  name: string
  aliases?: string[]
  description: (ctx: DescriptionContext) => Promise<string>
  inputSchema: z.ZodObject<InputShape>

  isReadOnly?: (input: z.infer<z.ZodObject<InputShape>>) => boolean
  isEnabled?: (ctx: EnabledContext) => Promise<boolean>
  isConcurrencySafe?: (input) => boolean
  isDestructive?: (input) => boolean

  validate?: (input, ctx) => Promise<ValidationResult>
  checkPermissions?: (input, ctx) => Promise<PermissionResult>

  call: (input, ctx) => AsyncGenerator<
    { type: 'progress', data: ProgressData } |
    { type: 'result', data: Output }
  >

  renderResultForAssistant: (output: Output) => string | ContentBlock[]
  renderToolUseMessage: (input, ctx) => JSX.Element
  renderToolResultMessage?: (output, ctx) => JSX.Element
  renderToolUseRejectedMessage?: (input, ctx) => JSX.Element
  renderToolUseProgressMessage?: (data, ctx) => JSX.Element

  inputsEquivalent?: (a, b) => boolean  // 去重
  shouldDefer?: (ctx) => boolean        // 延迟加载
}
```

每个字段都有意义：

| 字段 | 作用 |
|------|------|
| `inputSchema` | 用 Zod 定义，同时是**运行时校验**和**类型推导**源 |
| `isReadOnly` | 决定是否走并发批次 |
| `isConcurrencySafe` | 更细粒度的并发控制 |
| `isDestructive` | 破坏性操作需要特殊警告 |
| `checkPermissions` | 工具级权限检查（在全局权限之外） |
| `call` | **async generator** — 支持流式进度 |
| `renderResultForAssistant` | 给 Claude 看的文本结果 |
| `renderToolUseMessage` | 给用户看的 UI（"正在读取 foo.ts..."）|
| `renderToolResultMessage` | 结果的 UI |
| `inputsEquivalent` | 去重：相同输入的调用复用结果 |
| `shouldDefer` | 延迟工具发现：只有询问时才加载 |

## 为什么 `call` 是 async generator？

```typescript
call: async function* (input, ctx) {
  yield { type: 'progress', data: { phase: 'parsing' } }
  const ast = parseBash(input.command)

  yield { type: 'progress', data: { phase: 'executing' } }
  const output = await execBash(input.command)

  yield { type: 'result', data: { stdout: output } }
}
```

Generator 让调用者**可以订阅进度**：

```typescript
for await (const event of tool.call(input, ctx)) {
  if (event.type === 'progress') updateUI(event.data)
  else if (event.type === 'result') return event.data
}
```

长任务（Bash 命令、WebSearch、Agent 调用）都能实时反馈状态。

## ToolUseContext

工具在执行时拿到的上下文对象：

```typescript
type ToolUseContext = {
  abortController: AbortController  // 用户按 Ctrl+C 中断
  readFileCache: FileStateCache     // 继承自父 Agent
  permissionContext: ToolPermissionContext
  denialTrackingState: DenialTrackingState
  canUseTool: CanUseToolFn
  elicit?: (req: ElicitRequestURLParams) => Promise<ElicitResult>
  getAppState: () => AppState
  setAppState: (f) => void
  queryChain?: QueryChainTracking  // 追踪 Agent 调用链
  sourceToolAssistantUUID?: UUID
  // ... 更多字段
}
```

这个 context **在调用工具时构造**，包含工具执行所需的**所有外部依赖**。

## PermissionResult

工具的权限检查返回三选一：

```typescript
type PermissionResult =
  | { behavior: 'allow' }
  | { behavior: 'deny', message: string, reason?: PermissionDenialReason }
  | { behavior: 'ask', message: string }
```

QueryEngine 根据结果决定是否执行、是否弹窗询问。

## 进度类型的集中定义

所有工具的 progress 类型都在 `src/types/tools.ts`：

```typescript
type BashProgress = {
  phase: 'parsing' | 'executing' | 'finalizing'
  partialStdout?: string
  partialStderr?: string
}

type AgentToolProgress = {
  subAgentUuid: string
  currentAction: string
  turns: number
}

type MCPProgress = { serverName: string, method: string }

// 联合类型
type ToolProgressData =
  | BashProgress
  | AgentToolProgress
  | MCPProgress
  | ...
```

**集中定义**避免了循环依赖：工具文件不需要互相 import 进度类型。

## 延迟工具（Deferred Tools）

有些工具（如 MCP 的工具、某些 skill 工具）**在首次用户交互时不加载**，节省 token：

```typescript
shouldDefer: (ctx) => {
  // 只在用户明确询问时才加载
  return !ctx.currentPrompt.includes(this.name)
}
```

当 Claude 通过 `ToolSearch` 工具请求时，才动态注入工具定义。

## 输入去重

```typescript
inputsEquivalent: (a, b) => {
  // 忽略大小写的路径比较
  return a.filePath.toLowerCase() === b.filePath.toLowerCase()
}
```

如果连续两次调用的 input equivalent，第二次直接复用第一次的结果——省 token、省时间。

## 值得学习的点

1. **泛型工厂 + Zod** — 类型和运行时双保险
2. **Async Generator 的 call** — 天然支持流式进度
3. **JSX 渲染方法** — 工具自带 UI 展示逻辑
4. **集中的 Progress 类型** — 打破循环依赖
5. **延迟加载 + 输入去重** — token 优化的工程手段

## 实例：一个工具的完整定义

以 Grep 工具为例（简化版）：

```typescript
export const GrepTool = buildTool({
  name: 'Grep',
  description: async () => 'Search file contents with ripgrep',
  inputSchema: z.object({
    pattern: z.string(),
    path: z.string().optional(),
    glob: z.string().optional(),
  }),
  isReadOnly: () => true,  // 纯读，可并发
  validate: async (input) => {
    try {
      new RegExp(input.pattern)
      return { result: true }
    } catch (e) {
      return { result: false, message: `Invalid regex: ${e.message}` }
    }
  },
  call: async function* (input, ctx) {
    yield { type: 'progress', data: { phase: 'searching' } }
    const result = await ripgrep(input.pattern, input.path)
    yield { type: 'result', data: { matches: result.matches } }
  },
  renderResultForAssistant: (output) => output.matches.join('\n'),
  renderToolUseMessage: (input) => <Text>Searching for "{input.pattern}"...</Text>,
})
```

类型系统保证：这个 Grep 工具的每个方法都正确实现了契约。

## 相关文档

- [tools/ 工具实现](../tools/bash-tool.md)
- [utils/permissions - 权限系统](../utils/permissions.md)
- [QueryEngine 工具调用编排](./query-engine.md)
