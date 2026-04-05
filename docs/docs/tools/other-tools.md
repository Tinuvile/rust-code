---
sidebar_position: 8
title: 其他工具
description: REPL、LSP、PowerShell、Notebook、Cron、AskUser、SendMessage、Todo 等
---

# 其他工具

这些工具是**功能型**补充，不像 Bash/Edit 那么核心，但解决特定场景。

## REPLTool

**交互式 Python/Node REPL。**

```typescript
{ language: 'python', code: 'import pandas as pd\ndf = pd.read_csv("data.csv")\ndf.head()' }
```

REPL 保持会话状态——后续调用能访问之前定义的变量。

### 使用场景

- 数据探索（pandas、numpy）
- 快速计算
- 验证代码片段

## LSPTool

**调用 Language Server Protocol 获取代码信息。**

```typescript
{
  action: 'definition',
  filePath: '/src/app.ts',
  line: 42,
  column: 10
}
```

支持的 action：

| action | 返回 |
|--------|------|
| `definition` | 定义位置 |
| `references` | 所有引用 |
| `hover` | 类型信息 + 文档 |
| `diagnostics` | 错误和警告 |
| `completion` | 补全建议 |
| `rename` | 重命名预览 |

### 为什么需要 LSP？

Grep 只能做**文本匹配**，LSP 能做**语义查询**：

```typescript
// Grep 搜 "useState"
grep({ pattern: 'useState' })  // 找所有出现的地方

// LSP 找 useState 的实际引用
lsp({ action: 'references', ... })  // 只找真正引用的地方
```

LSP 知道**导入、作用域、类型**。

## PowerShellTool

**Windows 专用 PowerShell 执行工具。**

```typescript
{
  command: 'Get-Process | Where-Object CPU -gt 100',
  cwd: 'C:/project'
}
```

和 Bash 并存——Windows 用户写 PowerShell 脚本，不需要装 bash。

## ConfigTool

**读写 Claude Code 配置。**

```typescript
// 读
{ action: 'get', key: 'theme' }

// 写
{ action: 'set', key: 'theme', value: 'dark' }

// 列出所有
{ action: 'list' }
```

Agent 能**自主调整配置**——用户说"切深色模式"，Claude 直接调 ConfigTool。

## CronTool (Create/List/Delete)

**定时任务。**

```typescript
// 创建
{
  schedule: '0 9 * * MON',  // 每周一 9 点
  command: 'git pull && npm test',
  description: 'Weekly CI check'
}
```

### 场景

- 每日构建
- 监控脚本
- 提醒 Agent

Cron 任务由 Claude Code 进程**持久管理**——即使主对话关了，任务也会触发。

## SleepTool

**让 Agent 主动等待。**

```typescript
{ duration: 5000 }  // 5 秒
```

### 为什么需要？

有时候需要**等异步操作**：

```typescript
await tool('Bash', { command: 'docker compose up -d' })
await tool('Sleep', { duration: 3000 })  // 等容器启动
await tool('Bash', { command: 'curl http://localhost:8080/health' })
```

比不停重试更优雅。

## NotebookEditTool

**编辑 Jupyter Notebook 的 cells。**

```typescript
{
  notebookPath: '/nb.ipynb',
  action: 'edit',
  cellIndex: 3,
  source: 'df.describe()'
}
```

支持：

- `edit` - 修改 cell
- `insert` - 插入新 cell
- `delete` - 删除 cell

**Notebook 不是 JSON 文本**——直接 Edit 容易损坏格式。专用工具保证结构正确。

## BriefTool

**生成项目简报。**

```typescript
{ focus: 'authentication system' }
```

Claude 会：

1. 扫描项目
2. 提取结构
3. 总结关键模块
4. 返回简报

**实际上是一个"内置 Agent"**——专门用来生成项目摘要。

## AskUserTool

**向用户提问。**

```typescript
{
  question: 'Should I create a new branch or use the current one?',
  options: ['new branch', 'current branch', 'cancel']
}
```

**关键点：**

- 这是 **Agent 主动中断**，等用户输入
- 用户回答后，Agent 带着答案继续
- 是 **Human-in-the-loop** 的实现基础

### 与 Plan Mode 的对比

| 工具 | 时机 | 决策权 |
|------|------|--------|
| AskUser | 单点疑问 | 用户选一个选项 |
| ExitPlanMode | 整体计划 | 用户批准/拒绝/修改 |

## SendMessageTool

**Agent 之间通信。**

```typescript
{
  toAgent: 'database-expert',
  message: 'How should I index the users table for this query?'
}
```

在 **多 Agent 场景**下，子 Agent 之间传递信息。详见 [coordinator/](../coordinator/index.md)。

## TodoWriteTool

**管理任务清单。**

```typescript
{
  todos: [
    { content: 'Read auth.ts', status: 'completed', activeForm: 'Reading auth.ts' },
    { content: 'Refactor login flow', status: 'in_progress', activeForm: 'Refactoring login flow' },
    { content: 'Add tests', status: 'pending', activeForm: 'Adding tests' },
  ]
}
```

### 为什么内置 Todo？

**长任务**需要记录进度：

- 防止 Agent 忘记要做什么
- 让用户看到进度
- 中断后可以恢复

### Todo 不是 memory

- **Todo**：当前对话的任务列表
- **Memory**：跨对话持久化

## 其他零散工具

| 工具 | 用途 |
|------|------|
| `RemoteTriggerTool` | 触发远程 Agent 任务 |
| `PreviewTool` | 浏览器预览（dev server 截图） |
| `ScreenshotTool` | 终端截图 |
| `DiagramTool` | 生成 Mermaid/Excalidraw 图 |

## 值得学习的点

1. **工具职责极致细分** — 每个工具只做一件事
2. **LSP vs Grep** — 语义 vs 文本，互补不替代
3. **Cron 让 Agent 持久化** — 超越单次对话
4. **AskUser 是 Human-in-the-loop 的原子操作**
5. **Todo 是 Agent 的外部记忆**
6. **Notebook 工具保证结构完整性**
7. **跨平台工具共存** — Bash/PowerShell 并存

## 相关文档

- [BashTool 安全栈](./bash-tool.md)
- [Tool 工具框架](../root-files/tool-framework.md)
- [coordinator/ - 多 Agent 协调](../coordinator/index.md)
