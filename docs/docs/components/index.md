---
sidebar_position: 1
title: components/ — UI 组件库
description: 可复用的 Ink 组件
---

# components/ — UI 组件库

**目录：** `src/components/`

`components/` 是 Claude Code 的**UI 组件仓库**——所有重复使用的 TUI 元素。

## 组件分类

### 1. 原子组件（atoms）

基础元素：

- `Button` - 按钮
- `Input` - 文本输入
- `Divider` - 分割线
- `Badge` - 标签
- `Spinner` - 加载动画

### 2. 分子组件（molecules）

组合多个原子：

- `PromptInput` - 输入框 + 历史 + autocomplete
- `MessageItem` - 消息渲染（含 avatar + content）
- `TaskItem` - 任务行（status + name + duration）

### 3. 有机体（organisms）

完整模块：

- `MessageList` - 对话历史
- `TaskListPanel` - 任务面板
- `PermissionPrompt` - 权限询问框
- `CostDisplay` - 成本面板
- `StatusBar` - 底部状态栏

## 代表组件详解

### PromptInput

```tsx
function PromptInput({ onSubmit }) {
  const [value, setValue] = useState('')
  const [history, setHistory] = useState<string[]>([])
  const [historyIdx, setHistoryIdx] = useState(-1)

  const handleKey = (input: string, key: Key) => {
    if (key.return) {
      onSubmit(value)
      setHistory(h => [...h, value])
      setValue('')
      setHistoryIdx(-1)
    } else if (key.upArrow) {
      // 历史回溯
      const newIdx = historyIdx + 1
      if (newIdx < history.length) {
        setHistoryIdx(newIdx)
        setValue(history[history.length - 1 - newIdx])
      }
    } else if (key.downArrow) {
      // ...
    } else if (key.ctrl && input === 'l') {
      // Clear screen
    } else {
      setValue(v => v + input)
    }
  }

  useInput(handleKey)

  return (
    <Box borderStyle="round">
      <Text>{'> '}</Text>
      <Text>{value}</Text>
      <Cursor />
    </Box>
  )
}
```

### MessageItem

```tsx
function MessageItem({ msg }: { msg: Message }) {
  if (msg.role === 'user') {
    return (
      <Box>
        <Text color="cyan">You: </Text>
        <Text>{msg.content}</Text>
      </Box>
    )
  }

  if (msg.role === 'assistant') {
    return (
      <Box flexDirection="column">
        <Text color="green">Claude:</Text>
        <Box marginLeft={2}>
          <Markdown content={msg.content as string} />
        </Box>
      </Box>
    )
  }

  if (msg.role === 'tool_use') {
    return <ToolCallItem call={msg} />
  }

  if (msg.role === 'tool_result') {
    return <ToolResultItem result={msg} />
  }
}
```

### ToolCallItem

```tsx
function ToolCallItem({ call }: { call: ToolCall }) {
  const [expanded, setExpanded] = useState(false)

  return (
    <Box flexDirection="column" marginY={1}>
      <Box>
        <Text color="yellow">▸ {call.name}</Text>
        <Text color="gray"> ({call.id.slice(0, 8)})</Text>
      </Box>
      {expanded && (
        <Box marginLeft={2}>
          <Text>{JSON.stringify(call.args, null, 2)}</Text>
        </Box>
      )}
    </Box>
  )
}
```

### ToolResultItem

```tsx
function ToolResultItem({ result }) {
  const truncated = result.content.length > 500
  const display = truncated ? result.content.slice(0, 500) + '\n...' : result.content

  return (
    <Box flexDirection="column" marginY={1}>
      <Text color={result.isError ? 'red' : 'green'}>
        {result.isError ? '✗' : '✓'} Result
      </Text>
      <Box marginLeft={2} borderStyle="round">
        <Text>{display}</Text>
      </Box>
    </Box>
  )
}
```

### PermissionPrompt

```tsx
function PermissionPrompt({ request }) {
  const [selected, setSelected] = useState(0)
  const options = [
    'Allow once',
    'Allow in this session',
    'Allow always',
    'Deny',
    'Deny and cancel'
  ]

  useInput((input, key) => {
    if (key.upArrow) setSelected(s => Math.max(0, s - 1))
    if (key.downArrow) setSelected(s => Math.min(options.length - 1, s + 1))
    if (key.return) request.resolve(options[selected])
  })

  return (
    <Overlay>
      <Box flexDirection="column">
        <Text bold color="yellow">⚠ Permission Required</Text>
        <Box marginY={1}>
          <Text>Tool: <Text bold>{request.tool}</Text></Text>
        </Box>
        <Box flexDirection="column" borderStyle="round">
          <Text>{request.preview}</Text>
        </Box>
        <Box marginTop={1} flexDirection="column">
          {options.map((opt, i) => (
            <Text key={i} color={i === selected ? 'cyan' : undefined}>
              {i === selected ? '> ' : '  '}{opt}
            </Text>
          ))}
        </Box>
      </Box>
    </Overlay>
  )
}
```

### StatusBar

```tsx
function StatusBar() {
  const cost = useStore(costStore, s => s.totalCost)
  const model = useStore(sessionStore, s => s.model)
  const mode = useStore(uiStore, s => s.mode)
  const tasks = useStore(taskStore, s => s.tasks.filter(t => t.status === 'running'))

  return (
    <Box borderTop paddingX={1} justifyContent="space-between">
      <Box gap={2}>
        <Text color="gray">{model}</Text>
        <Text color={mode === 'plan' ? 'magenta' : 'gray'}>[{mode}]</Text>
        {tasks.length > 0 && (
          <Text color="yellow">{tasks.length} tasks running</Text>
        )}
      </Box>
      <Text color="gray">${cost.toFixed(4)}</Text>
    </Box>
  )
}
```

### TaskListPanel

```tsx
function TaskListPanel() {
  const tasks = useStore(taskStore, s => s.tasks)

  if (tasks.length === 0) return null

  return (
    <Box flexDirection="column" borderStyle="round" padding={1}>
      <Text bold>Tasks</Text>
      {tasks.map(t => (
        <Box key={t.id}>
          <Text color={statusColor(t.status)}>●</Text>
          <Text> {t.command}</Text>
          <Text color="gray"> ({formatDuration(t.duration)})</Text>
        </Box>
      ))}
    </Box>
  )
}

function statusColor(status: string): string {
  return { running: 'yellow', completed: 'green', failed: 'red' }[status] ?? 'gray'
}
```

## 布局组件

### SplitPane

```tsx
function SplitPane({ left, right, ratio = 0.6 }) {
  const { cols } = useTerminalSize()
  const leftWidth = Math.floor(cols * ratio)
  const rightWidth = cols - leftWidth - 1

  return (
    <Box flexDirection="row">
      <Box width={leftWidth}>{left}</Box>
      <Box width={1}><Text>│</Text></Box>
      <Box width={rightWidth}>{right}</Box>
    </Box>
  )
}
```

### Tabs

```tsx
function Tabs({ tabs, activeTab, onSwitch }) {
  return (
    <Box>
      {tabs.map((tab, i) => (
        <Box key={i} marginRight={1}>
          <Text
            color={i === activeTab ? 'cyan' : 'gray'}
            underline={i === activeTab}
          >
            {tab}
          </Text>
        </Box>
      ))}
    </Box>
  )
}
```

## 主题

```tsx
const theme = {
  colors: {
    primary: 'cyan',
    secondary: 'gray',
    success: 'green',
    warning: 'yellow',
    error: 'red',
  },
  borders: {
    normal: 'round',
    focused: 'double',
    danger: 'bold',
  }
}

// 用法
<Text color={theme.colors.primary}>...</Text>
```

## 组件命名约定

- 原子组件：单数名词（`Button`、`Spinner`）
- 容器：以 Panel/Container 结尾（`TaskListPanel`）
- 列表项：以 Item 结尾（`MessageItem`、`TaskItem`）
- 覆盖层：以 Prompt/Modal 结尾（`PermissionPrompt`）

## 组件测试

```tsx
import { render } from 'ink-testing-library'

test('MessageItem renders user', () => {
  const { lastFrame } = render(
    <MessageItem msg={{ role: 'user', content: 'Hi' }} />
  )
  expect(lastFrame()).toContain('You:')
  expect(lastFrame()).toContain('Hi')
})
```

## 值得学习的点

1. **原子/分子/有机体** 分层
2. **组件订阅 store** — 自动响应状态
3. **键盘优先** — 每个组件支持导航
4. **Markdown 渲染** — Claude 回复的核心
5. **主题化** — 颜色/样式集中
6. **可折叠内容** — 长输出默认折叠
7. **边框区分区域** — CLI 布局技巧

## 相关文档

- [ink/](../ink/index.md)
- [hooks/](../hooks/index.md)
- [state/](../state/index.md)
- [screens/](../screens/index.md)
