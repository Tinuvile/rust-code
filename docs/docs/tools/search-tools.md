---
sidebar_position: 4
title: 搜索工具 (Grep/Glob/Web)
description: 文件搜索、内容检索、Web 搜索与延迟工具发现
---

# 搜索工具 (Grep / Glob / WebSearch / WebFetch / ToolSearch)

**目录：** `src/tools/GrepTool/`、`src/tools/GlobTool/`、`src/tools/WebSearchTool/`、`src/tools/WebFetchTool/`、`src/tools/ToolSearchTool/`

这组工具让 Agent **收集信息**。设计重点是**并发安全**和**延迟发现**。

## GrepTool — ripgrep 封装

**特点：比 grep 快 100x，支持 Unicode、.gitignore 感知。**

```typescript
{
  pattern: 'useState',
  path: '/src',
  glob: '*.tsx',
  type: 'typescript',
  outputMode: 'content',  // 或 'files_with_matches' 或 'count'
  '-C': 2,                // context lines
  multiline: false,
}
```

### 输出模式

| mode | 返回 |
|------|------|
| `files_with_matches` | 匹配的文件路径 |
| `content` | 匹配的行内容 + 行号 |
| `count` | 每个文件的匹配次数 |

**默认 `files_with_matches`** 省 token——Agent 先看哪些文件有匹配，再决定读哪些。

### head_limit 与 offset

```typescript
{ head_limit: 50, offset: 100 }
```

分页读取——大项目里的高频词（如 `function`）可能有上千匹配，分页避免爆炸。

### glob 和 type 筛选

```typescript
{ pattern: 'TODO', glob: '*.{ts,tsx}' }
{ pattern: 'TODO', type: 'typescript' }  // 等价
```

`type` 是 ripgrep 的内置类型别名——比 glob 简洁。

## GlobTool — 文件名匹配

**用途：快速找文件，不搜内容。**

```typescript
{
  pattern: 'src/**/*.tsx',
  path: '.'
}
```

### 按 mtime 排序

```typescript
const files = await glob(pattern, { cwd: path })
return files.sort((a, b) => statMtime(b) - statMtime(a))
```

**最近修改的文件排前面**——Agent 更关注用户正在改的代码。

### Grep vs Glob

| 场景 | 工具 |
|------|------|
| 找所有 `.tsx` 文件 | **Glob** |
| 找包含 "useState" 的文件 | **Grep** (files_with_matches) |
| 看 "useState" 的具体用法 | **Grep** (content) |
| 统计代码量 | **Glob** + wc（通过 Bash） |

## WebSearchTool

**集成搜索引擎，返回结果 snippets。**

```typescript
{ query: 'react server components best practices' }
```

这个工具让 Agent 能获取**训练数据截止日期之后的信息**——对快速变化的生态（前端、AI 工具）尤其重要。

## WebFetchTool

**抓取单个 URL 的内容。**

```typescript
{ url: 'https://react.dev/reference/react/useState' }
```

### 安全检查

```typescript
// 拒绝本地地址
if (isPrivateIP(url)) return { error: 'Private IP not allowed' }

// 拒绝非 http(s)
if (!url.startsWith('http')) return { error: 'Only http(s) URLs' }

// 检查 robots.txt
if (await shouldRespectRobots(url)) return { error: 'Blocked by robots.txt' }
```

### Markdown 转换

抓到的 HTML 转换成 Markdown：

```typescript
const html = await fetch(url).then(r => r.text())
const markdown = turndown(html)
return { content: markdown }
```

**给 Claude 的是 Markdown**——比 HTML 更紧凑，保留结构。

## ToolSearchTool — 延迟工具发现

**目的：减少 turn-1 的工具数量，节省 token。**

### 问题

如果 Claude 在 turn 1 就知道所有 100+ 工具（包括 MCP 的工具），每次对话要发送**所有工具定义**——巨大的 token 浪费。

### 解决

Turn 1 只发送**核心工具**（Read、Write、Edit、Bash、Grep、Glob、Agent）。其他工具通过 `ToolSearch` **延迟加载**：

```typescript
// Claude 调用 ToolSearch
{ query: 'need to run SQL queries on postgres' }

// ToolSearch 返回匹配的工具
[
  { name: 'postgres-query', server: 'postgres-mcp', ... },
  { name: 'postgres-schema', server: 'postgres-mcp', ... }
]

// Claude 看到后，可以在下一个 turn 调用这些工具
```

### 实现

```typescript
const deferredTools = allTools.filter(t => t.shouldDefer?.())

// 系统提示词里只列出核心工具
const turnOneTools = allTools.filter(t => !t.shouldDefer?.())

// ToolSearch 在 deferredTools 里搜索
function search(query: string): Tool[] {
  return deferredTools.filter(t =>
    fuzzyMatch(t.name, query) ||
    fuzzyMatch(t.description, query)
  )
}
```

这是**token 和能力的平衡**——Claude 需要时能获得任意工具，但默认不被超多工具定义淹没。

### select: 语法

```typescript
// Claude 知道工具名时可以直接 select
{ query: 'select:postgres-query,postgres-schema' }
```

这比模糊搜索精确、更便宜。

## 并发安全声明

所有搜索工具都标记为 `isReadOnly: true`：

```typescript
isReadOnly: () => true,
isConcurrencySafe: () => true,
```

QueryEngine 会**并发执行**它们（见 [QueryEngine 文档](../root-files/query-engine.md)）：

```typescript
// 这 5 个调用同时跑
const [files, funcs, hooks, tests, docs] = await Promise.all([
  grep({ pattern: 'useState' }),
  grep({ pattern: 'useEffect' }),
  glob({ pattern: '**/hooks/*.ts' }),
  glob({ pattern: '**/*.test.ts' }),
  glob({ pattern: '**/*.md' }),
])
```

**5x 吞吐** = Agent 更快完成探索阶段。

## 值得学习的点

1. **按 mtime 排序** — 近期修改的文件更相关
2. **分页 + head_limit** — 防止输出爆炸
3. **输出模式分层** — files/content/count 按需选择
4. **Markdown 转换** — HTML 不适合 LLM
5. **延迟工具发现** — 节省 turn-1 token
6. **并发声明** — 让 QueryEngine 能并行执行

## 相关文档

- [QueryEngine 并发编排](../root-files/query-engine.md)
- [MCP 工具集成](./mcp-tools.md)
- [Tool 工具框架](../root-files/tool-framework.md)
