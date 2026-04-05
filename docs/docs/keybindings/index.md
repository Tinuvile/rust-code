---
sidebar_position: 1
title: keybindings/ — 键盘绑定
description: 默认绑定、自定义、vim/emacs 模式
---

# keybindings/ — 键盘绑定

**目录：** `src/keybindings/`

CLI 工具的用户**重度依赖键盘**。Claude Code 的 keybindings 系统支持**自定义、链式、模式**。

## 默认绑定

```typescript
// keybindings/defaults.ts
export const DEFAULTS = {
  // 消息
  'submit': 'return',
  'newline': 'shift+return',
  'cancel': 'ctrl+c',

  // 编辑
  'clear': 'ctrl+l',
  'edit-in-editor': 'ctrl+e',

  // 历史
  'history-prev': 'up',
  'history-next': 'down',
  'search-history': 'ctrl+r',

  // 模式
  'toggle-plan-mode': 'shift+tab',
  'toggle-buddy': 'ctrl+b',

  // 面板
  'toggle-tasks': 'ctrl+t',
  'toggle-cost': 'ctrl+$',
  'show-settings': 'ctrl+,',

  // 导航
  'escape': 'escape',
  'help': 'f1',
  'quit': 'ctrl+d',
}
```

## 自定义绑定

```json
// ~/.claude/keybindings.json
{
  "submit": "ctrl+return",
  "toggle-plan-mode": "ctrl+p",
  "custom-action": "ctrl+shift+x"
}
```

项目级覆盖：

```json
// .claude/keybindings.json
{
  "edit-in-editor": "ctrl+shift+e"
}
```

## 按键解析

```typescript
// keybindings/parser.ts
function parseKey(input: string, key: Key): string {
  const parts: string[] = []
  if (key.ctrl) parts.push('ctrl')
  if (key.shift) parts.push('shift')
  if (key.meta) parts.push('meta')

  if (key.return) parts.push('return')
  else if (key.escape) parts.push('escape')
  else if (key.upArrow) parts.push('up')
  else if (key.downArrow) parts.push('down')
  else parts.push(input)

  return parts.join('+')
}
```

## 链式绑定 (chords)

像 Emacs `C-x C-s` 这样的**双键组合**：

```json
{
  "save-session": "ctrl+x ctrl+s",
  "quit-all": "ctrl+x ctrl+c"
}
```

```typescript
class ChordHandler {
  private pending: string | null = null
  private timeout: NodeJS.Timeout | null = null

  handle(key: string): string | null {
    if (this.pending) {
      // 第二个按键
      const chord = `${this.pending} ${key}`
      this.pending = null
      this.clearTimeout()
      return chord
    }

    // 检查是否是 chord 前缀
    if (this.isChordPrefix(key)) {
      this.pending = key
      this.timeout = setTimeout(() => this.pending = null, 1000)
      return null
    }

    return key  // 普通按键
  }
}
```

**1 秒超时** — 超时未输入第二键，丢弃。

## 模式切换

**Vim 模式：**

```typescript
function vimKeybindings() {
  return {
    // Normal mode
    'enter-insert': 'i',
    'enter-insert-append': 'a',
    'delete-line': 'd d',
    'yank-line': 'y y',
    'paste': 'p',
    'undo': 'u',

    // Insert mode
    'exit-insert': 'escape',
  }
}
```

**Emacs 模式：**

```typescript
function emacsKeybindings() {
  return {
    'forward-word': 'alt+f',
    'backward-word': 'alt+b',
    'delete-word': 'alt+d',
    'kill-line': 'ctrl+k',
  }
}
```

启用：

```bash
claude config set input-mode vim
```

## 绑定冲突检测

```typescript
function validateKeybindings(bindings: Record<string, string>) {
  const reverse = new Map<string, string[]>()

  for (const [action, key] of Object.entries(bindings)) {
    if (!reverse.has(key)) reverse.set(key, [])
    reverse.get(key)!.push(action)
  }

  for (const [key, actions] of reverse) {
    if (actions.length > 1) {
      warn(`Key "${key}" bound to multiple actions: ${actions.join(', ')}`)
    }
  }
}
```

## 上下文相关

不同屏幕有不同绑定：

```typescript
const contextBindings = {
  'main': DEFAULTS,
  'permission-prompt': {
    ...DEFAULTS,
    'submit': 'return',
    'deny': 'escape',
  },
  'task-list': {
    ...DEFAULTS,
    'show-output': 'return',
    'stop-task': 'delete',
  }
}
```

## 动态绑定

```typescript
function useKeybindings(ctx: string) {
  const bindings = contextBindings[ctx]

  useInput((input, key) => {
    const pressed = parseKey(input, key)
    const action = findAction(bindings, pressed)
    if (action) dispatch(action)
  })
}
```

## 快捷键提示

按 `?` 显示当前上下文的所有绑定：

```
┌─ Keybindings ──────────────────────────┐
│                                         │
│ Enter        Submit                     │
│ Ctrl+C       Cancel                     │
│ Ctrl+L       Clear                      │
│ Shift+Tab    Toggle plan mode           │
│ Ctrl+T       Show tasks                 │
│ Ctrl+,       Settings                   │
│ ?            This help                  │
│                                         │
└─────────────────────────────────────────┘
```

## 终端限制

某些按键**终端不能传递**：

```typescript
const UNSUPPORTED = [
  'ctrl+shift+a',  // 大部分终端不传递
  'alt+arrow',     // macOS 特殊
  'f13+',          // 功能键 13+
]

function validate(binding: string) {
  if (UNSUPPORTED.includes(binding)) {
    warn(`Binding "${binding}" may not work in all terminals`)
  }
}
```

## 平台差异

```typescript
function normalizeBinding(binding: string): string {
  if (platform === 'darwin') {
    // Mac 上 Cmd 替换 Ctrl
    return binding.replace('ctrl', 'meta')
  }
  return binding
}
```

## 测试

```typescript
test('submit binding', () => {
  const { result } = renderHook(() => useKeybindings('main'))
  fireEvent.keyPress({ key: { return: true } })
  expect(result.current.submitted).toBe(true)
})
```

## 值得学习的点

1. **默认 + 覆盖** — 系统默认，用户自定义
2. **链式绑定** — Emacs 风格
3. **上下文敏感** — 不同屏幕不同绑定
4. **模式切换** — Vim / Emacs / 默认
5. **冲突检测** — 启动时警告
6. **按 ? 显示绑定** — 自解释
7. **平台适配** — Mac Cmd 替换

## 相关文档

- [hooks/ - useKeybindings](../hooks/index.md)
- [screens/](../screens/index.md)
- [commands/](../commands/index.md)
