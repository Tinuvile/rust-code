---
sidebar_position: 6
title: 其他 Utils
description: 路径、字符串、文件系统、跨平台工具
---

# 其他 Utils

**目录：** `src/utils/` 的其他子目录

这些是**基础工具函数**——小而关键。

## Path Utils

**目录：** `src/utils/paths/`

### 跨平台路径处理

```typescript
// 规范化分隔符
function normalize(p: string): string {
  return p.replace(/\\/g, '/')  // Windows 下用正斜杠
}

// 相对于 cwd
function relativize(abs: string, cwd: string): string {
  return path.relative(cwd, abs)
}

// Home 展开
function expandHome(p: string): string {
  if (p.startsWith('~/')) return path.join(homedir(), p.slice(2))
  return p
}
```

### 项目根探测

```typescript
async function findProjectRoot(cwd: string): Promise<string> {
  const MARKERS = ['.git', 'package.json', 'Cargo.toml', 'pyproject.toml', 'go.mod']

  let dir = cwd
  while (dir !== path.dirname(dir)) {
    for (const marker of MARKERS) {
      if (await exists(path.join(dir, marker))) return dir
    }
    dir = path.dirname(dir)
  }
  return cwd  // 找不到，用 cwd
}
```

## String Utils

**目录：** `src/utils/strings/`

### Diff 生成

```typescript
function generateDiff(oldStr: string, newStr: string): string {
  // 用 diff 算法生成可读的差异
  const changes = diffLines(oldStr, newStr)
  return changes.map(c => {
    const prefix = c.added ? '+' : c.removed ? '-' : ' '
    return c.value.split('\n').map(l => prefix + l).join('\n')
  }).join('\n')
}
```

### Levenshtein 距离

用于**模糊匹配**：

```typescript
function similarity(a: string, b: string): number {
  const dist = levenshtein(a, b)
  return 1 - dist / Math.max(a.length, b.length)
}

// 用法：用户输入 "reac"，建议 "react"
function suggest(input: string, candidates: string[]): string[] {
  return candidates
    .map(c => ({ c, score: similarity(input, c) }))
    .filter(({ score }) => score > 0.6)
    .sort((a, b) => b.score - a.score)
    .slice(0, 5)
    .map(({ c }) => c)
}
```

### 字符串截断

带中英文宽度感知：

```typescript
function truncateVisual(s: string, maxWidth: number): string {
  let width = 0
  let result = ''
  for (const char of s) {
    const w = isWide(char) ? 2 : 1  // 中日韩宽字符
    if (width + w > maxWidth) break
    result += char
    width += w
  }
  return result
}
```

## File System Utils

**目录：** `src/utils/fs/`

### 安全写入

```typescript
async function atomicWrite(filePath: string, content: string) {
  const tmpPath = filePath + '.tmp-' + crypto.randomUUID()
  await fs.writeFile(tmpPath, content)
  await fs.rename(tmpPath, filePath)  // 原子替换
}
```

**原子写入** 避免并发读到半成品。

### 递归遍历（带忽略）

```typescript
async function* walk(
  dir: string,
  opts: { ignore?: string[] } = {}
): AsyncGenerator<string> {
  const ig = ignore().add(opts.ignore ?? [])

  const entries = await fs.readdir(dir, { withFileTypes: true })
  for (const entry of entries) {
    const full = path.join(dir, entry.name)
    const rel = path.relative(dir, full)

    if (ig.ignores(rel)) continue

    if (entry.isDirectory()) {
      yield* walk(full, opts)
    } else {
      yield full
    }
  }
}
```

### .gitignore 读取

```typescript
async function loadGitignores(root: string): Promise<Ignore> {
  const ig = ignore()

  // 全局
  ig.add(await readMaybe('~/.gitignore_global') ?? '')

  // 项目
  for await (const file of walk(root, { filter: f => f.endsWith('.gitignore') })) {
    const content = await fs.readFile(file, 'utf8')
    ig.add(content)
  }

  return ig
}
```

## Terminal Utils

**目录：** `src/utils/terminal/`

### 宽度探测

```typescript
function terminalWidth(): number {
  return process.stdout.columns ?? 80
}
```

### 色彩支持探测

```typescript
function colorLevel(): 0 | 1 | 2 | 3 {
  if (process.env.NO_COLOR) return 0
  if (process.env.FORCE_COLOR === '3') return 3     // truecolor
  if (process.env.TERM === 'xterm-256color') return 2
  if (process.env.TERM === 'xterm') return 1
  return process.stdout.isTTY ? 1 : 0
}
```

### Cursor 控制

```typescript
const ansi = {
  clearLine: '\x1b[2K',
  clearScreen: '\x1b[2J',
  cursorUp: (n: number) => `\x1b[${n}A`,
  cursorHide: '\x1b[?25l',
  cursorShow: '\x1b[?25h',
}
```

## Date / Time Utils

**目录：** `src/utils/time/`

### 相对时间

```typescript
function relativeTime(ts: number): string {
  const diff = Date.now() - ts
  const minutes = Math.floor(diff / 60_000)
  if (minutes < 1) return 'just now'
  if (minutes < 60) return `${minutes}m ago`
  const hours = Math.floor(minutes / 60)
  if (hours < 24) return `${hours}h ago`
  const days = Math.floor(hours / 24)
  return `${days}d ago`
}
```

### 时区

```typescript
function localTime(ts: number): string {
  return new Date(ts).toLocaleString(undefined, {
    timeZone: Intl.DateTimeFormat().resolvedOptions().timeZone
  })
}
```

## Concurrency Utils

**目录：** `src/utils/concurrency/`

### Mutex

```typescript
class Mutex {
  private queue: Array<() => void> = []
  private locked = false

  async acquire(): Promise<() => void> {
    if (!this.locked) {
      this.locked = true
      return () => this.release()
    }
    return new Promise(resolve => {
      this.queue.push(() => {
        this.locked = true
        resolve(() => this.release())
      })
    })
  }

  private release() {
    this.locked = false
    const next = this.queue.shift()
    if (next) next()
  }
}
```

### Semaphore

```typescript
class Semaphore {
  constructor(private permits: number) {}

  async acquire(): Promise<() => void> {
    while (this.permits <= 0) {
      await new Promise(r => setTimeout(r, 10))
    }
    this.permits--
    return () => this.permits++
  }
}

// 用法
const sem = new Semaphore(5)  // 最多 5 并发

async function limited() {
  const release = await sem.acquire()
  try {
    return await doWork()
  } finally {
    release()
  }
}
```

## Error Utils

**目录：** `src/utils/errors/`

### Error 分类

```typescript
function classifyError(e: Error): ErrorCategory {
  if (e instanceof NetworkError) return 'network'
  if (e instanceof AuthError) return 'auth'
  if (e instanceof RateLimitError) return 'rate_limit'
  if (e instanceof ValidationError) return 'validation'
  return 'unknown'
}
```

### 堆栈清理

```typescript
function cleanStack(stack: string): string {
  return stack
    .split('\n')
    .filter(line => !line.includes('node_modules'))  // 去 node_modules
    .filter(line => !line.includes('internal/'))      // 去 Node 内部
    .join('\n')
}
```

## Debounce / Throttle

```typescript
function debounce<F extends (...args: any[]) => void>(
  fn: F,
  ms: number
): F {
  let timer: NodeJS.Timeout
  return ((...args) => {
    clearTimeout(timer)
    timer = setTimeout(() => fn(...args), ms)
  }) as F
}

// 用法
const saveDebounced = debounce(save, 300)
```

## 值得学习的点

1. **原子写入** — 用 tmp + rename
2. **.gitignore 整合** — 尊重用户忽略规则
3. **宽字符处理** — 中日韩字符宽度
4. **色彩能力探测** — NO_COLOR 等环境变量
5. **Semaphore 并发控制** — 简洁实现
6. **堆栈清理** — 去掉无关帧
7. **项目根探测** — 多 marker 检查

## 相关文档

- [tools/file-tools](../tools/file-tools.md)
- [tools/bash-tool](../tools/bash-tool.md)
- [utils/permissions](./permissions.md)
