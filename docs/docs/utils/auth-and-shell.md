---
sidebar_position: 5
title: 认证与 Shell 工具
description: Token 管理、凭证存储、Shell 探测
---

# 认证与 Shell 工具

**目录：** `src/utils/auth/`、`src/utils/shell/`

## Auth 工具

### Token 存储

`utils/auth/storage.ts`：

```typescript
interface StoredCredentials {
  anthropic?: {
    apiKey?: string
    oauthToken?: OAuthToken
  }
  bedrock?: AWSCredentials
  vertex?: GCPCredentials
}
```

### 平台化存储

不同 OS 有不同的**安全存储 API**：

```typescript
async function saveCredential(key: string, value: string) {
  switch (platform) {
    case 'darwin':
      return saveToKeychain(key, value)       // Keychain
    case 'win32':
      return saveToDPAPI(key, value)          // Windows Credential Store
    case 'linux':
      return saveToSecretService(key, value)  // libsecret
    default:
      return saveToFile(key, value, 0o600)    // 降级到文件
  }
}
```

**优先使用 OS 原生加密存储**，降级到 0600 文件。

### Token 优先级

```typescript
function resolveToken(): Token {
  // 1. 环境变量（最高）
  if (process.env.ANTHROPIC_API_KEY) {
    return { source: 'env', token: process.env.ANTHROPIC_API_KEY }
  }

  // 2. OAuth token
  const oauth = loadOAuthToken()
  if (oauth && !isExpired(oauth)) {
    return { source: 'oauth', token: oauth.access_token }
  }

  // 3. 配置文件
  const file = loadFromFile()
  if (file) return { source: 'file', token: file }

  // 4. 需要登录
  throw new NotAuthenticatedError()
}
```

### Token 屏蔽

日志中**绝不打印完整 token**：

```typescript
function maskToken(token: string): string {
  if (token.length < 10) return '***'
  return token.slice(0, 6) + '...' + token.slice(-4)
}

// ant-api-abc...xyz9
```

### Token 轮换

```typescript
async function rotateToken(oldToken: string): Promise<string> {
  const newToken = await createNewToken()
  await saveCredential('anthropic', newToken)
  await revokeToken(oldToken)  // 立即作废旧的
  return newToken
}
```

## Shell 探测与选择

### 探测可用 Shell

`utils/shell/detect.ts`：

```typescript
async function detectShells(): Promise<Shell[]> {
  const shells: Shell[] = []

  if (await isInstalled('bash')) shells.push('bash')
  if (await isInstalled('zsh')) shells.push('zsh')
  if (await isInstalled('fish')) shells.push('fish')
  if (await isInstalled('pwsh')) shells.push('pwsh')
  if (platform === 'win32') shells.push('cmd')

  return shells
}

async function isInstalled(shell: string): Promise<boolean> {
  try {
    await exec(`${shell} -c "echo ok"`)
    return true
  } catch {
    return false
  }
}
```

### 默认 Shell

```typescript
function defaultShell(): Shell {
  if (platform === 'darwin') return 'zsh'
  if (platform === 'linux') return 'bash'
  if (platform === 'win32') return 'pwsh'
  return 'bash'
}
```

### Shell 切换

```bash
claude config set shell bash
```

## Shell 转义

**用户输入/文件名**可能含 shell 元字符：

```typescript
function shellEscape(arg: string, shell: Shell): string {
  switch (shell) {
    case 'bash':
    case 'zsh':
      return "'" + arg.replace(/'/g, "'\\''") + "'"
    case 'pwsh':
      return '"' + arg.replace(/"/g, '`"') + '"'
    case 'cmd':
      return '"' + arg.replace(/"/g, '""') + '"'
  }
}
```

**不同 shell 转义规则不同**——必须分别处理。

## Shell 环境

```typescript
function buildShellEnv(base: Env): Env {
  return {
    ...base,
    CLAUDE_SESSION_ID: session.id,
    CLAUDE_CWD: cwd,
    PATH: enrichPath(base.PATH),  // 加入 ~/.claude/bin
  }
}
```

### PATH 增强

```typescript
function enrichPath(path: string): string {
  const additions = [
    `${homedir()}/.claude/bin`,
    `${homedir()}/.local/bin`,
  ]
  return [...additions, ...path.split(':')].join(':')
}
```

## Shell 命令构建

```typescript
interface ShellCommand {
  command: string
  args: string[]
  cwd: string
  env: Env
  shell: Shell
}

function buildCommand(spec: ShellCommand): string[] {
  switch (spec.shell) {
    case 'bash':
      return ['bash', '-c', `${spec.command} ${spec.args.map(a => shellEscape(a, 'bash')).join(' ')}`]
    case 'pwsh':
      return ['pwsh', '-Command', `& "${spec.command}" ${spec.args.join(' ')}`]
    // ...
  }
}
```

## Signal 处理

```typescript
function killProcess(pid: number, signal = 'SIGTERM') {
  if (platform === 'win32') {
    // Windows 没有 POSIX signal
    return exec(`taskkill /F /PID ${pid}`)
  } else {
    return process.kill(pid, signal)
  }
}
```

**Windows 特殊处理** — 没有 SIGTERM/SIGKILL。

## Shell History 集成

```typescript
// 读取 bash/zsh 历史
async function recentCommands(shell: Shell): Promise<string[]> {
  const histFile = {
    bash: '~/.bash_history',
    zsh: '~/.zsh_history',
    fish: '~/.local/share/fish/fish_history',
  }[shell]

  if (!histFile) return []

  const content = await readFile(expandHome(histFile))
  return content.split('\n').slice(-100)
}
```

**Claude 可以读取用户历史命令**——但需要用户批准。

## Interactive Shell 检测

```typescript
function isInteractive(): boolean {
  return process.stdout.isTTY && process.stdin.isTTY
}
```

非交互模式（CI）**跳过用户确认**——给环境变量决策。

## 值得学习的点

1. **OS 原生凭证存储** — Keychain/DPAPI/libsecret
2. **Token 屏蔽** — 日志中绝不全量打印
3. **Token 优先级链** — env > oauth > file
4. **Shell 探测** — 自动发现可用 shell
5. **平台特定转义** — bash/pwsh/cmd 不同规则
6. **PATH 增强** — 注入 ~/.claude/bin
7. **Windows signal 适配** — taskkill 代替 kill

## 相关文档

- [services/oauth-and-plugins](../services/oauth-and-plugins.md)
- [tools/bash-tool](../tools/bash-tool.md)
- [utils/permissions](./permissions.md)
