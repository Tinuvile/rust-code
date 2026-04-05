---
sidebar_position: 4
title: 消息处理
description: Message 构造、序列化、流式处理
---

# 消息处理工具

**目录：** `src/utils/messages/`

这组工具处理 **LLM 消息的所有操作**——构造、序列化、合并、截断。

## 消息类型

```typescript
type Message =
  | UserMessage
  | AssistantMessage
  | ToolUseMessage
  | ToolResultMessage
  | SystemMessage

interface UserMessage {
  role: 'user'
  content: string | ContentBlock[]
}

interface ContentBlock {
  type: 'text' | 'image' | 'tool_use' | 'tool_result'
  // ...
}
```

## 消息构造

### 简单用户消息

```typescript
function userMessage(text: string): UserMessage {
  return { role: 'user', content: text }
}
```

### 带图片的消息

```typescript
function userMessageWithImage(text: string, imagePath: string): UserMessage {
  const imageData = readFileBase64(imagePath)
  return {
    role: 'user',
    content: [
      { type: 'text', text },
      { type: 'image', source: { type: 'base64', media_type: 'image/png', data: imageData } }
    ]
  }
}
```

### 工具结果

```typescript
function toolResult(toolUseId: string, output: any, isError = false): ContentBlock {
  return {
    type: 'tool_result',
    tool_use_id: toolUseId,
    content: JSON.stringify(output),
    is_error: isError
  }
}
```

## 消息合并

连续的 user / assistant 消息**必须合并**——Claude API 要求：

```typescript
// 输入（违规）
[
  { role: 'user', content: 'A' },
  { role: 'user', content: 'B' },  // 连续 user
  { role: 'assistant', content: 'C' }
]

// 合并后
[
  { role: 'user', content: 'A\n\nB' },
  { role: 'assistant', content: 'C' }
]
```

`utils/messages/merge.ts`：

```typescript
function mergeConsecutive(messages: Message[]): Message[] {
  const result: Message[] = []
  for (const msg of messages) {
    const last = result[result.length - 1]
    if (last?.role === msg.role) {
      last.content = mergeContent(last.content, msg.content)
    } else {
      result.push(msg)
    }
  }
  return result
}
```

## Token 估算

在发送前**估算 token 数**：

```typescript
// 粗略估算（4 chars ≈ 1 token）
function estimateTokens(text: string): number {
  return Math.ceil(text.length / 4)
}

// 精确（用 tiktoken）
async function countTokens(messages: Message[]): Promise<number> {
  const result = await apiClient.countTokens({ messages })
  return result.input_tokens
}
```

**两阶段估算：**

1. 发送前用粗略估算决定要不要 compact
2. 必要时调 API 精确计数

## 序列化到磁盘

会话消息保存为 **JSONL**（每行一条 JSON）：

```jsonl
{"role":"user","content":"help me"}
{"role":"assistant","content":[{"type":"text","text":"Sure"}]}
{"role":"tool_use","id":"t1","name":"Read",...}
```

### 为什么 JSONL？

- **追加高效** — 不用读整个文件
- **流式解析** — 逐行处理
- **容错** — 最后一行损坏不影响前面

## 消息恢复

```typescript
async function loadSession(id: string): Promise<Message[]> {
  const lines = await readLines(`~/.claude/sessions/${id}.jsonl`)
  return lines
    .filter(l => l.trim())
    .map(l => JSON.parse(l))
    .filter(isValidMessage)  // 跳过损坏的行
}
```

## 消息净化

某些消息**不能发给 LLM**：

```typescript
// 过滤掉 tool output 中的二进制
function sanitize(msg: Message): Message {
  if (msg.role === 'tool_result') {
    if (isBinary(msg.content)) {
      return { ...msg, content: '<binary content redacted>' }
    }
  }
  return msg
}
```

## 消息截断

当消息太长时：

```typescript
function truncate(msg: Message, maxTokens: number): Message {
  const tokens = estimateTokens(msg.content)
  if (tokens <= maxTokens) return msg

  const ratio = maxTokens / tokens
  const newLen = Math.floor(msg.content.length * ratio)
  return {
    ...msg,
    content: msg.content.slice(0, newLen) + '\n[truncated]'
  }
}
```

## Content Block 遍历

```typescript
function* walkContent(msg: Message): Generator<ContentBlock> {
  if (typeof msg.content === 'string') {
    yield { type: 'text', text: msg.content }
  } else {
    for (const block of msg.content) {
      yield block
    }
  }
}

// 用法
for (const block of walkContent(msg)) {
  if (block.type === 'tool_use') {
    console.log('Tool:', block.name)
  }
}
```

## Tool Use / Result 配对

每个 `tool_use` 必须跟着对应的 `tool_result`：

```typescript
function validateToolPairs(messages: Message[]): Error[] {
  const errors: Error[] = []
  const pendingUses = new Set<string>()

  for (const msg of messages) {
    if (msg.role === 'assistant') {
      for (const b of walkContent(msg)) {
        if (b.type === 'tool_use') pendingUses.add(b.id)
      }
    } else if (msg.role === 'user') {
      for (const b of walkContent(msg)) {
        if (b.type === 'tool_result') pendingUses.delete(b.tool_use_id)
      }
    }
  }

  for (const id of pendingUses) {
    errors.push(new Error(`Missing tool_result for ${id}`))
  }
  return errors
}
```

## 流式处理

LLM 响应是流：

```typescript
async function* processStream(stream: AsyncIterable<StreamEvent>) {
  let currentText = ''
  let currentToolUse: ToolUse | null = null

  for await (const event of stream) {
    switch (event.type) {
      case 'content_block_start':
        if (event.content_block.type === 'tool_use') {
          currentToolUse = { id: event.content_block.id, ... }
        }
        break

      case 'content_block_delta':
        if (event.delta.type === 'text_delta') {
          currentText += event.delta.text
          yield { type: 'text_chunk', text: event.delta.text }
        } else if (event.delta.type === 'input_json_delta') {
          currentToolUse!.input_json += event.delta.partial_json
        }
        break

      case 'content_block_stop':
        if (currentToolUse) {
          yield { type: 'tool_use_complete', tool: currentToolUse }
          currentToolUse = null
        }
        break
    }
  }
}
```

**关键：tool_use input 是流式 JSON**——要累积后解析。

## 消息压缩

用于 context 压缩（详见 [services/compact](../services/compact.md)）：

```typescript
function compactMessage(msg: Message): Message {
  if (msg.role === 'tool_result' && isLargeOutput(msg)) {
    return {
      ...msg,
      content: `[Tool output compressed: ${summary}]`
    }
  }
  return msg
}
```

## 值得学习的点

1. **严格的消息规范** — API 合约必须满足
2. **JSONL 会话存储** — 追加友好、容错好
3. **两阶段 token 估算** — 粗略 + 精确
4. **消息净化** — 二进制不上传
5. **Tool use/result 配对验证** — 防止协议不一致
6. **流式 JSON 累积** — tool input 特殊处理

## 相关文档

- [QueryEngine 查询引擎](../root-files/query-engine.md)
- [services/compact - 上下文压缩](../services/compact.md)
- [services/api - API 客户端](../services/api.md)
