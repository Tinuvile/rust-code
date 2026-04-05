---
sidebar_position: 1
title: ink/ — Ink 渲染层
description: Ink 自定义渲染器、布局、样式
---

# ink/ — Ink 渲染层

**目录：** `src/ink/`

**Ink** 是 **React for CLI** ——用 JSX 写终端 UI。但 Ink 默认行为对 Claude Code 不够——这里放自定义扩展。

## Ink 基础

```tsx
import { Box, Text } from 'ink'

function App() {
  return (
    <Box flexDirection="column">
      <Text color="cyan">Hello, Claude Code!</Text>
      <Text>Type your message below:</Text>
    </Box>
  )
}
```

**Box** = flex container，**Text** = 文本。完全模仿 CSS flexbox。

## 为什么要定制 Ink？

Claude Code 的 TUI 需求超出 Ink 默认：

- **可变宽度** — 列宽随终端大小
- **富文本** — 颜色、斜体、链接
- **流式渲染** — token 逐个出现
- **滚动** — 长对话要滚
- **动画** — spinner、打字机
- **覆盖层** — 权限询问框
- **Markdown 渲染** — Claude 的回复

## 自定义组件

### AutoSizer

```tsx
function AutoSizer({ children }) {
  const { cols, rows } = useTerminalSize()
  return children({ width: cols, height: rows })
}

// 用法
<AutoSizer>
  {({ width, height }) => (
    <Box width={width} height={height}>
      ...
    </Box>
  )}
</AutoSizer>
```

### Markdown

```tsx
function Markdown({ content }: { content: string }) {
  const tokens = parseMarkdown(content)
  return (
    <Box flexDirection="column">
      {tokens.map((t, i) => renderToken(t, i))}
    </Box>
  )
}

function renderToken(token: Token, key: number) {
  switch (token.type) {
    case 'heading':
      return <Text key={key} bold color="cyan">{token.content}</Text>
    case 'code':
      return <CodeBlock key={key} language={token.lang} code={token.code} />
    case 'bold':
      return <Text key={key} bold>{token.content}</Text>
    case 'link':
      return <Text key={key} color="blue" underline>{token.text}</Text>
    // ...
  }
}
```

### CodeBlock（带语法高亮）

```tsx
function CodeBlock({ language, code }) {
  const highlighted = highlight(code, language)  // chalk 着色
  return (
    <Box borderStyle="round" padding={1}>
      <Text>{highlighted}</Text>
    </Box>
  )
}
```

### Spinner

```tsx
function Spinner({ label }) {
  const [frame, setFrame] = useState(0)
  const frames = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏']

  useInterval(() => setFrame(f => (f + 1) % frames.length), 80)

  return <Text color="cyan">{frames[frame]} {label}</Text>
}
```

### TypewriterText

```tsx
function TypewriterText({ text }: { text: string }) {
  const [displayed, setDisplayed] = useState('')

  useEffect(() => {
    setDisplayed('')
    let i = 0
    const id = setInterval(() => {
      if (i >= text.length) {
        clearInterval(id)
        return
      }
      setDisplayed(text.slice(0, i + 1))
      i++
    }, 10)
    return () => clearInterval(id)
  }, [text])

  return <Text>{displayed}</Text>
}
```

### Overlay / Modal

```tsx
function Overlay({ children }) {
  return (
    <Box
      position="absolute"
      top={0} left={0} right={0} bottom={0}
      borderStyle="double"
      padding={2}
      justifyContent="center"
      alignItems="center"
    >
      {children}
    </Box>
  )
}

// 权限询问用
{permission.current && (
  <Overlay>
    <PermissionPrompt request={permission.current} />
  </Overlay>
)}
```

## 自定义渲染器

Ink 本身用 `yoga-layout` 做 flex。Claude Code **注入自己的渲染逻辑**：

```typescript
// ink/renderer.ts
class ClaudeRenderer {
  private buffer: Cell[][] = []

  render(tree: Element) {
    this.buffer = []
    this.walk(tree, { x: 0, y: 0, width: cols, height: rows })
    this.flush()
  }

  private flush() {
    // 增量更新：只重绘变化的行
    for (let row = 0; row < this.buffer.length; row++) {
      if (this.rowChanged(row)) {
        this.moveCursor(0, row)
        process.stdout.write(this.renderRow(row))
      }
    }
  }
}
```

**增量渲染** 是性能关键——不能每次全刷。

## 颜色处理

```typescript
import chalk from 'chalk'

const colors = {
  primary: chalk.cyan,
  secondary: chalk.gray,
  success: chalk.green,
  warning: chalk.yellow,
  error: chalk.red,
  muted: chalk.hex('#666'),
}

// 探测色彩能力
const level = chalk.level  // 0 / 1 / 2 / 3

// NO_COLOR 支持
if (process.env.NO_COLOR) {
  chalk.level = 0
}
```

## Layout 技巧

### 固定 footer

```tsx
<Box flexDirection="column" height={rows}>
  <Box flexGrow={1} overflow="hidden">
    <MessageList />
  </Box>
  <Box borderTop>
    <InputBox />
  </Box>
</Box>
```

### 可滚动区域

Ink 不原生支持滚动，**手动实现**：

```tsx
function Scrollable({ children, height }) {
  const [scrollTop, setScrollTop] = useState(0)
  const items = Children.toArray(children)
  const visible = items.slice(scrollTop, scrollTop + height)

  useKeybindings({
    'up': () => setScrollTop(s => Math.max(0, s - 1)),
    'down': () => setScrollTop(s => Math.min(items.length - height, s + 1)),
    'pageup': () => setScrollTop(s => Math.max(0, s - height)),
    'pagedown': () => setScrollTop(s => Math.min(items.length - height, s + height)),
  })

  return <Box flexDirection="column">{visible}</Box>
}
```

## 性能优化

### React.memo

```tsx
const MessageItem = React.memo(({ msg }) => {
  return <Text>{msg.content}</Text>
})
```

### 列表虚拟化

只渲染**可视范围**：

```tsx
function VirtualList({ items, height, rowHeight }) {
  const start = Math.floor(scrollTop / rowHeight)
  const end = start + Math.ceil(height / rowHeight)
  return (
    <Box>
      {items.slice(start, end).map(item => ...)}
    </Box>
  )
}
```

## Focus 管理

```tsx
function App() {
  const { focusNext, focusPrevious } = useFocusManager()

  useKeybindings({
    'tab': focusNext,
    'shift+tab': focusPrevious,
  })

  return (
    <Box>
      <Input focused />
      <TaskList />
    </Box>
  )
}
```

## 测试 Ink 组件

```typescript
import { render } from 'ink-testing-library'

test('renders message', () => {
  const { lastFrame } = render(<MessageItem msg={{ content: 'hi' }} />)
  expect(lastFrame()).toContain('hi')
})
```

## 值得学习的点

1. **React 范式下的 CLI UI** — JSX/Hooks/Components
2. **自定义 AutoSizer** — 响应式布局
3. **增量渲染** — 只更新变化行
4. **NO_COLOR 尊重** — 可访问性
5. **手动滚动** — Ink 不原生支持
6. **虚拟列表** — 长对话性能
7. **Focus 管理** — 键盘导航

## 相关文档

- [hooks/](../hooks/index.md)
- [components/](../components/index.md)
- [screens/](../screens/index.md)
