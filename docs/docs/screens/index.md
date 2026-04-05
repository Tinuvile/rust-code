---
sidebar_position: 1
title: screens/ — 屏幕视图
description: 主 REPL、权限询问、设置等完整屏幕
---

# screens/ — 屏幕视图

**目录：** `src/screens/`

`screens/` 是 **完整的顶级视图**——对应用户看到的"屏幕"。组件是砖块，屏幕是房子。

## 主要屏幕

### 1. MainREPL（主对话界面）

```
┌──────────────────────────────────────────┐
│ Claude Code v2.0.0 - Opus 4.6            │
├──────────────────────────────────────────┤
│ You: 帮我修这个 bug                       │
│                                          │
│ Claude: Let me check the code...         │
│   ▸ Read auth.ts                         │
│   ✓ Result                               │
│                                          │
│ I see the issue. The token validation... │
├──────────────────────────────────────────┤
│ > █                                      │
├──────────────────────────────────────────┤
│ opus-4-6 [normal]       $0.0234   1 task │
└──────────────────────────────────────────┘
```

实现：

```tsx
function MainREPL() {
  return (
    <Box flexDirection="column" height="100%">
      <Header />
      <Box flexGrow={1} overflow="hidden">
        <MessageList />
      </Box>
      <PromptInput />
      <StatusBar />
    </Box>
  )
}
```

### 2. PermissionScreen（权限询问）

当 Agent 要调用需要批准的工具：

```
┌─ ⚠ Permission Required ─────────────────┐
│                                          │
│ Tool: Bash                               │
│                                          │
│ ┌──────────────────────────────────────┐ │
│ │ Execute: rm node_modules             │ │
│ │ Risk: medium                         │ │
│ └──────────────────────────────────────┘ │
│                                          │
│ > Allow once                             │
│   Allow in this session                  │
│   Allow always                           │
│   Deny                                   │
│   Deny and cancel                        │
│                                          │
└──────────────────────────────────────────┘
```

### 3. TaskListScreen

```
┌─ Tasks ─────────────────────────────────┐
│                                          │
│ ● task-abc  npm run dev       2m 15s    │
│ ● task-def  npm test          12s       │
│ ✓ task-xyz  cargo build       (done)    │
│                                          │
│ [Enter] Show output  [Del] Stop          │
└──────────────────────────────────────────┘
```

### 4. SettingsScreen

```
┌─ Settings ──────────────────────────────┐
│                                          │
│ Theme:       > dark                      │
│ Model:         claude-opus-4-6           │
│ Mode:          normal                    │
│                                          │
│ Keybindings...                           │
│ Permissions...                           │
│ MCP Servers...                           │
│                                          │
└──────────────────────────────────────────┘
```

### 5. OnboardingScreen

首次使用：

```
┌─ Welcome to Claude Code ────────────────┐
│                                          │
│ Let's get you set up.                    │
│                                          │
│ 1. Sign in to Anthropic                  │
│    Press [Enter] to open browser         │
│                                          │
│ 2. Choose your model                     │
│                                          │
│ 3. Configure permissions                 │
│                                          │
└──────────────────────────────────────────┘
```

### 6. DiffViewScreen

查看文件修改预览：

```
┌─ src/auth.ts ───────────────────────────┐
│                                          │
│  10  function verifyToken(token) {       │
│ -11    if (token.length < 10) {          │
│ +11    if (!isValidJWT(token)) {         │
│  12      return null                     │
│  13    }                                 │
│                                          │
└──────────────────────────────────────────┘
```

## 屏幕路由

```tsx
// screens/Router.tsx
type ScreenName = 'repl' | 'permission' | 'tasks' | 'settings' | 'onboarding'

function ScreenRouter() {
  const current = useStore(uiStore, s => s.currentScreen)

  switch (current) {
    case 'repl': return <MainREPL />
    case 'permission': return <PermissionScreen />
    case 'tasks': return <TaskListScreen />
    case 'settings': return <SettingsScreen />
    case 'onboarding': return <OnboardingScreen />
  }
}
```

## 屏幕切换

```tsx
const { setScreen } = useUIActions()

useKeybindings({
  'ctrl+t': () => setScreen('tasks'),
  'ctrl+,': () => setScreen('settings'),
  'esc': () => setScreen('repl'),
})
```

**快捷键驱动** — 不需要鼠标。

## 屏幕栈

```tsx
function useScreenStack() {
  const stack = useRef<ScreenName[]>([])

  const push = (screen: ScreenName) => {
    stack.current.push(uiStore.get().currentScreen)
    uiStore.set({ currentScreen: screen })
  }

  const pop = () => {
    const prev = stack.current.pop()
    if (prev) uiStore.set({ currentScreen: prev })
  }

  return { push, pop }
}
```

按 Esc 回退到上一个屏幕。

## Overlay 屏幕

某些屏幕**覆盖**在当前屏幕上（权限询问、diff 查看）：

```tsx
function App() {
  return (
    <>
      <ScreenRouter />
      {overlay && <Overlay>{overlay}</Overlay>}
    </>
  )
}
```

**Overlay 不替换底层屏幕**，只挡住。

## 全屏 vs 分屏

```tsx
function MainREPL() {
  const showTasks = useStore(uiStore, s => s.showTaskPanel)

  if (showTasks) {
    return (
      <SplitPane
        left={<MessageList />}
        right={<TaskListPanel />}
        ratio={0.7}
      />
    )
  }

  return <MessageList />
}
```

## 屏幕生命周期

```tsx
function TaskListScreen() {
  useEffect(() => {
    // enter
    taskService.startPolling()
    return () => {
      // leave
      taskService.stopPolling()
    }
  }, [])

  return <Box>...</Box>
}
```

**进入时启动订阅，离开时清理。**

## 屏幕状态持久化

```typescript
// 用户切出去，切回来保持滚动位置
interface ScreenState {
  [screenName: string]: { scrollTop: number; selection?: any }
}

const screenStateStore = new Store<ScreenState>({})
```

## 调试屏幕

```bash
claude --screen tasks
# 直接启动到任务屏
```

开发时方便。

## 键盘导航规范

```
Tab       - 下一项
Shift+Tab - 上一项
Enter     - 确认
Esc       - 返回/取消
Up/Down   - 列表导航
Left/Right- 展开/折叠
Space     - 切换
```

**一致的导航**让用户不用记。

## 动画/过渡

终端 UI 不像 Web，**过渡谨慎使用**：

```tsx
function FadeIn({ children }) {
  const [opacity, setOpacity] = useState(0)
  useEffect(() => {
    const id = setInterval(() => setOpacity(o => Math.min(1, o + 0.1)), 30)
    return () => clearInterval(id)
  }, [])
  // 伪 opacity：用 gray 梯度模拟
  const dimmed = opacity < 0.5
  return <Box dimColor={dimmed}>{children}</Box>
}
```

## 值得学习的点

1. **屏幕 = 完整视图** — 组件是砖，屏幕是房
2. **Router + Store** — 屏幕切换由状态驱动
3. **快捷键驱动** — 无鼠标交互
4. **屏幕栈** — Esc 回退
5. **Overlay 屏幕** — 覆盖而非替换
6. **分屏支持** — 同时看多个上下文
7. **屏幕状态持久化** — 切回保持滚动
8. **一致导航** — Tab/Enter/Esc 通用

## 相关文档

- [components/](../components/index.md)
- [hooks/](../hooks/index.md)
- [keybindings/](../keybindings/index.md)
