---
sidebar_position: 1
title: assistant/ — Assistant 核心
description: LLM 对话循环、思考链、流式响应
---

# assistant/ — Assistant 核心

**目录：** `src/assistant/`

`assistant/` 是 Claude Code 的**LLM 对话引擎核心**——封装 Claude 的调用、思考链、工具使用。

## 职责

和 `query/` 目录互补：

- **query/** — 协调工具调用循环（who/when）
- **assistant/** — 专注 LLM 的调用细节（how）

## 核心接口

```typescript
interface Assistant {
  complete(params: CompleteParams): Promise<Message>
  stream(params: StreamParams): AsyncIterable<StreamEvent>
  countTokens(messages: Message[]): Promise<number>
}
```

## 请求构造

```typescript
async function buildRequest(
  messages: Message[],
  tools: Tool[],
  systemPrompt: string
): APIRequest {
  return {
    model: currentModel,
    system: [
      { type: 'text', text: systemPrompt, cache_control: { type: 'ephemeral' } }
    ],
    messages: normalizeMessages(messages),
    tools: tools.map(toAPITool),
    max_tokens: 8192,
    temperature: 0,  // 默认确定性
    metadata: { user_id: hashedUserId }
  }
}
```

## 流式处理

```typescript
async function* stream(params: StreamParams): AsyncIterable<AssistantEvent> {
  const raw = apiClient.stream(params)

  for await (const event of raw) {
    switch (event.type) {
      case 'message_start':
        yield { type: 'start', message: event.message }
        break

      case 'content_block_start':
        yield { type: 'block_start', block: event.content_block }
        break

      case 'content_block_delta':
        yield { type: 'delta', delta: event.delta }
        break

      case 'content_block_stop':
        yield { type: 'block_end' }
        break

      case 'message_delta':
        yield { type: 'usage', usage: event.usage }
        break

      case 'message_stop':
        yield { type: 'end' }
        break
    }
  }
}
```

## Thinking 支持

Claude 4.6 支持 **thinking**（推理过程）：

```typescript
{
  thinking: { type: 'enabled', budget_tokens: 10000 },
  messages: [...]
}
```

返回：

```typescript
{
  content: [
    { type: 'thinking', thinking: '...' },  // 推理
    { type: 'text', text: '...' },           // 回复
  ]
}
```

Claude Code **默认开启 thinking**——让 Agent 做决策更可靠。

## Reasoning Effort

Opus 4.6 支持**推理深度调节**：

```typescript
{
  reasoning: {
    effort: 'low' | 'medium' | 'high'  // 思考多少
  }
}
```

- **low** — 快速，浅思考
- **medium** — 默认
- **high** — 深度，慢

Claude Code 根据任务复杂度**动态选择**：

```typescript
function selectEffort(task: Task): 'low' | 'medium' | 'high' {
  if (task.type === 'quick_answer') return 'low'
  if (task.type === 'refactor') return 'high'
  return 'medium'
}
```

## 系统提示词

`assistant/systemPrompt.ts` 构造 system prompt：

```typescript
function buildSystemPrompt(ctx: Context): string {
  const sections = [
    corePersona(),              // "You are Claude Code..."
    currentDate(),               // "Today's date is 2026-04-05"
    workingDirectory(ctx.cwd),   // "Primary working directory: ..."
    environment(),               // OS, shell
    memoryIndex(ctx.memory),     // 记忆索引
    availableTools(ctx.tools),
    skillDescriptions(ctx.skills),
    currentMode(ctx.mode),       // "You are in plan mode" (if applicable)
    customInstructions(ctx.config),
  ]
  return sections.join('\n\n')
}
```

## 模型选择

```typescript
function selectModel(task: Task): string {
  // Haiku 用于压缩、简单任务
  if (task.type === 'compact') return 'claude-haiku-4-5-20251001'

  // Sonnet 平衡
  if (task.type === 'routine') return 'claude-sonnet-4-6'

  // Opus 复杂任务
  return 'claude-opus-4-6'
}
```

**多模型策略** — 不是所有任务都用最贵的模型。

## Token 预算

```typescript
async function completeWithBudget(
  messages: Message[],
  budget: number
): Promise<Message> {
  const estimated = await countTokens(messages)
  if (estimated > budget) {
    throw new BudgetExceeded(estimated, budget)
  }

  return complete({ messages, max_tokens: budget - estimated })
}
```

## Stop Sequences

```typescript
{
  stop_sequences: ['</answer>', 'USER:']
}
```

控制生成何时停止。

## Assistant Hooks

响应的**每个事件**都可以被 hook：

```typescript
assistant.on('thinking', (chunk) => logThinking(chunk))
assistant.on('text', (chunk) => updateUI(chunk))
assistant.on('tool_use', (tool) => logToolUse(tool))
```

## 错误恢复

```typescript
async function completeWithRetry(params, attempt = 0) {
  try {
    return await complete(params)
  } catch (e) {
    if (attempt >= 3) throw e

    if (e.type === 'rate_limit') {
      await sleep(e.retryAfter * 1000)
      return completeWithRetry(params, attempt + 1)
    }

    if (e.type === 'overloaded') {
      await sleep(exponentialBackoff(attempt))
      return completeWithRetry(params, attempt + 1)
    }

    throw e  // 其他错误不重试
  }
}
```

## Token 计数缓存

```typescript
const tokenCountCache = new Map<string, number>()

async function countTokensCached(messages: Message[]): Promise<number> {
  const key = hashMessages(messages)
  if (tokenCountCache.has(key)) return tokenCountCache.get(key)!

  const count = await api.countTokens(messages)
  tokenCountCache.set(key, count)
  return count
}
```

Token count 调用**不是免费的**——缓存可省钱。

## 消息前处理

```typescript
function preprocessMessages(messages: Message[]): Message[] {
  let out = messages
  out = mergeConsecutive(out)      // 合并相邻同 role
  out = sanitize(out)              // 过滤不安全内容
  out = truncateTooLong(out)       // 单消息不能过长
  out = validateToolPairs(out)     // 验证 use/result 配对
  return out
}
```

## 值得学习的点

1. **Assistant vs Query 分工** — how vs who/when
2. **Thinking 开启** — 推理可观察
3. **Effort 动态调节** — 按任务复杂度
4. **多模型策略** — Haiku/Sonnet/Opus 分工
5. **Token count 缓存** — 省 API 调用
6. **消息前处理** — 保证协议合规
7. **Hooks 贯穿全程** — 可观察性

## 相关文档

- [query/](../query/index.md)
- [services/api](../services/api.md)
- [utils/messages](../utils/messages.md)
