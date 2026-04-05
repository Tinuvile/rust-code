---
sidebar_position: 1
title: hooks/ — React Hooks 集合
description: Claude Code 的自定义 React Hooks 库
---

# hooks/ — React Hooks 集合

**目录：** `src/hooks/`

Claude Code TUI 用 **React + Ink** 渲染。`hooks/` 里是所有**自定义 React Hooks**——连接 UI 与底层状态/服务。

## 为什么这么多 Hooks？

终端 UI 的状态比想象中复杂：

- **对话流** — 消息不断更新
- **任务状态** — 任务随时创建/完成
- **文件状态** — Edit 后要刷新
- **权限询问** — 弹出/关闭
- **流式输出** — token 一个个来

每种状态一个 hook，避免组件**再嵌一层状态地狱**。

## 核心 Hooks

### useMessages

```typescript
function useMessages(): {
  messages: Message[]
  addMessage: (msg: Message) => void
  updateLast: (delta: Partial<Message>) => void
  clear: () => void
} {
  const [messages, setMessages] = useState<Message[]>([])

  const addMessage = useCallback((msg: Message) => {
    setMessages(prev => [...prev, msg])
  }, [])

  // 流式更新最后一条
  const updateLast = useCallback((delta) => {
    setMessages(prev => {
      const last = prev[prev.length - 1]
      return [...prev.slice(0, -1), { ...last, ...delta }]
    })
  }, [])

  return { messages, addMessage, updateLast, clear }
}
```

### useStream

处理 LLM 的流式响应：

```typescript
function useStream(): {
  isStreaming: boolean
  currentText: string
  startStream: () => void
  appendChunk: (chunk: string) => void
  endStream: () => void
} {
  const [isStreaming, setStreaming] = useState(false)
  const [text, setText] = useState('')

  const appendChunk = useCallback((chunk: string) => {
    setText(prev => prev + chunk)
  }, [])

  return { isStreaming, currentText: text, ... }
}
```

### useTasks

任务列表订阅：

```typescript
function useTasks(): {
  tasks: Task[]
  running: Task[]
  completed: Task[]
} {
  const [tasks, setTasks] = useState<Task[]>([])

  useEffect(() => {
    const unsub = taskService.subscribe(setTasks)
    return unsub
  }, [])

  const running = tasks.filter(t => t.status === 'running')
  const completed = tasks.filter(t => t.status === 'completed')

  return { tasks, running, completed }
}
```

### useKeybindings

```typescript
function useKeybindings(bindings: Record<string, () => void>) {
  useEffect(() => {
    const handler = (input: string, key: Key) => {
      const shortcut = keyToString(input, key)
      if (bindings[shortcut]) bindings[shortcut]()
    }
    process.stdin.on('keypress', handler)
    return () => process.stdin.off('keypress', handler)
  }, [bindings])
}

// 用法
useKeybindings({
  'ctrl+c': () => cancel(),
  'shift+tab': () => togglePlanMode(),
  'ctrl+t': () => toggleTaskPanel(),
})
```

### usePermissionPrompt

```typescript
function usePermissionPrompt(): {
  current: PermissionRequest | null
  approve: (decision: Decision) => void
  deny: () => void
} {
  const [current, setCurrent] = useState<PermissionRequest | null>(null)

  useEffect(() => {
    return permissionService.onPrompt(setCurrent)
  }, [])

  const approve = (decision) => {
    current?.resolve(decision)
    setCurrent(null)
  }

  return { current, approve, deny }
}
```

### useTokenCount

```typescript
function useTokenCount(): {
  input: number
  output: number
  cached: number
  cost: number
} {
  const [usage, setUsage] = useState({ ... })

  useEffect(() => {
    return costService.subscribe(setUsage)
  }, [])

  return usage
}
```

### useSession

```typescript
function useSession(): {
  id: string
  model: string
  startedAt: number
  turnCount: number
} {
  return useContext(SessionContext)!
}
```

### useWorkingDirectory

```typescript
function useWorkingDirectory(): {
  cwd: string
  setCwd: (path: string) => void
  projectRoot: string
} {
  const [cwd, setCwd] = useState(process.cwd())

  useEffect(() => {
    process.chdir(cwd)
  }, [cwd])

  return { cwd, setCwd, projectRoot: findProjectRoot(cwd) }
}
```

### useThrottledRender

减少 Ink 重渲染：

```typescript
function useThrottledRender<T>(value: T, ms: number = 50): T {
  const [throttled, setThrottled] = useState(value)
  const lastUpdate = useRef(0)

  useEffect(() => {
    const now = Date.now()
    const elapsed = now - lastUpdate.current
    if (elapsed >= ms) {
      setThrottled(value)
      lastUpdate.current = now
    } else {
      const t = setTimeout(() => {
        setThrottled(value)
        lastUpdate.current = Date.now()
      }, ms - elapsed)
      return () => clearTimeout(t)
    }
  }, [value])

  return throttled
}
```

流式输出时尤其有用——每个 token 都重渲染太慢。

### useTerminalSize

```typescript
function useTerminalSize(): { cols: number; rows: number } {
  const [size, setSize] = useState({
    cols: process.stdout.columns ?? 80,
    rows: process.stdout.rows ?? 24
  })

  useEffect(() => {
    const handler = () => setSize({
      cols: process.stdout.columns ?? 80,
      rows: process.stdout.rows ?? 24
    })
    process.stdout.on('resize', handler)
    return () => process.stdout.off('resize', handler)
  }, [])

  return size
}
```

### useInterval

```typescript
function useInterval(callback: () => void, delay: number | null) {
  useEffect(() => {
    if (delay === null) return
    const id = setInterval(callback, delay)
    return () => clearInterval(id)
  }, [callback, delay])
}

// 用法
useInterval(() => {
  updateTaskStatuses()
}, 1000)
```

## 复合 Hook 案例

### useCoordinatedInput

组合多个 hook：

```typescript
function useCoordinatedInput() {
  const { addMessage } = useMessages()
  const { startStream } = useStream()
  const { prompt } = useUserInput()
  const permission = usePermissionPrompt()

  // 阻塞输入当有权限询问
  const canInput = !permission.current

  const submit = async (text: string) => {
    if (!canInput) return
    addMessage({ role: 'user', content: text })
    startStream()
    await runAgent(text)
  }

  return { canInput, submit, prompt }
}
```

## 订阅模式

Hooks 大量用 **subscribe + cleanup**：

```typescript
useEffect(() => {
  const unsub = service.subscribe(listener)
  return unsub  // cleanup
}, [])
```

## 性能优化

### memoize

```typescript
const filteredMessages = useMemo(
  () => messages.filter(m => m.visible),
  [messages]
)
```

### lazy init

```typescript
const [config, _] = useState(() => loadConfig())  // 只算一次
```

## 测试 Hooks

```typescript
import { renderHook } from '@testing-library/react-hooks'

test('useMessages adds message', () => {
  const { result } = renderHook(() => useMessages())
  act(() => {
    result.current.addMessage({ role: 'user', content: 'hi' })
  })
  expect(result.current.messages).toHaveLength(1)
})
```

## 值得学习的点

1. **按职责分离 hook** — 每个 hook 一个 concern
2. **订阅 + cleanup** — 防止泄漏
3. **throttle 渲染** — 流式输出必备
4. **组合高于继承** — 用 hook 组合而非 HOC
5. **lazy init** — 避免重复计算
6. **useInterval 的陷阱** — closure 过期
7. **Ink 终端特殊性** — resize 监听

## 相关文档

- [state/ - 状态管理](../state/index.md)
- [ink/ - Ink 渲染](../ink/index.md)
- [components/ - UI 组件](../components/index.md)
