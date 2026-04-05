---
sidebar_position: 1
title: cli/ — CLI 参数解析
description: Commander.js 集成、子命令、全局选项
---

# cli/ — CLI 参数解析

**目录：** `src/cli/`

`cli/` 负责**命令行参数**的定义、解析、帮助生成——让 `claude` 命令成为合格的 CLI 工具。

## 库选择：Commander.js

```typescript
import { Command } from 'commander'

const program = new Command()
  .name('claude')
  .version(VERSION)
  .description('Claude Code — AI-powered CLI')
```

**为什么 Commander？**

- 最成熟的 Node CLI 框架
- 自动生成 help
- 子命令嵌套
- 类型友好

## 全局选项

```typescript
program
  .option('-m, --model <model>', 'Model to use')
  .option('--mode <mode>', 'Execution mode', 'normal')
  .option('-p, --prompt <prompt>', 'Non-interactive prompt')
  .option('-c, --continue', 'Continue last session')
  .option('--resume <id>', 'Resume specific session')
  .option('--debug', 'Enable debug output')
  .option('--no-color', 'Disable color output')
  .option('--quiet', 'Minimal output')
  .option('--version', 'Print version')
  .option('--help', 'Print help')
```

## 子命令

```typescript
program
  .command('config')
  .description('Manage configuration')
  .action(configCommand)

program
  .command('mcp')
  .description('Manage MCP servers')
  .action(mcpCommand)

program
  .command('agents')
  .description('Manage agents')
  .action(agentsCommand)

program
  .command('memory')
  .description('Manage memory')
  .action(memoryCommand)

program
  .command('remote')
  .description('Manage remote triggers')
  .action(remoteCommand)
```

## 子子命令

```typescript
const mcp = program.command('mcp')
mcp.command('list').action(mcpList)
mcp.command('add <name>').action(mcpAdd)
mcp.command('remove <name>').action(mcpRemove)
mcp.command('debug <name>').action(mcpDebug)
```

## 非交互模式

```bash
claude -p "explain this code"
# 不进入 REPL，直接执行，输出结果，退出
```

```typescript
if (options.prompt) {
  await runNonInteractive(options.prompt)
  process.exit(0)
}
```

**CI/CD 场景**必备。

## Stdin 支持

```bash
cat file.py | claude -p "review this"
```

```typescript
if (!process.stdin.isTTY) {
  const input = await readStdin()
  prompt = `${prompt}\n\n${input}`
}
```

## 管道输出

```bash
claude -p "generate SQL" | psql mydb
```

非交互模式 → 纯文本输出到 stdout。

## 环境变量

```typescript
const config = {
  apiKey: process.env.ANTHROPIC_API_KEY,
  model: process.env.CLAUDE_MODEL ?? 'claude-opus-4-6',
  useBedrock: !!process.env.CLAUDE_CODE_USE_BEDROCK,
  useVertex: !!process.env.CLAUDE_CODE_USE_VERTEX,
  noColor: !!process.env.NO_COLOR,
  disableAnalytics: process.env.CLAUDE_CODE_ANALYTICS === '0',
}
```

## 退出码

```typescript
enum ExitCode {
  Success = 0,
  GeneralError = 1,
  AuthError = 2,
  RateLimited = 3,
  ContextExceeded = 4,
  UserCancelled = 130,  // SIGINT
}
```

**CI 脚本可以判断**：

```bash
claude -p "..." || {
  case $? in
    2) echo "Auth failed" ;;
    3) echo "Rate limited" ;;
  esac
}
```

## Shell 自动补全

```bash
claude completion bash > /etc/bash_completion.d/claude
claude completion zsh > ~/.zsh/completions/_claude
claude completion fish > ~/.config/fish/completions/claude.fish
```

生成 shell 补全脚本。

## 自动更新检查

```typescript
async function checkUpdate() {
  const latest = await fetch('https://claude.ai/code/latest.json')
  if (latest.version > currentVersion) {
    console.log(`Update available: ${latest.version}. Run: npm update -g @anthropic-ai/claude-code`)
  }
}
```

**异步检查**——不阻塞启动。

## Telemetry opt-out

```bash
claude --no-telemetry
# 或
export CLAUDE_CODE_ANALYTICS=0
```

## 一些高级 flags

```typescript
program
  .option('--dangerously-bypass-permissions', 'Skip all permission prompts')
  .option('--dry-run', 'Print commands without executing')
  .option('--mcp-config <path>', 'Path to MCP config')
  .option('--no-hooks', 'Disable all hooks')
  .option('--strict-mode', 'Fail on any warning')
```

**危险选项命名要长** — `--dangerously-bypass-permissions` 而非 `-D`。

## 错误处理

```typescript
program.configureOutput({
  outputError: (str, write) => write(chalk.red(str))
})

program.exitOverride()

try {
  program.parse()
} catch (e) {
  if (e.code === 'commander.help') process.exit(0)
  console.error(e.message)
  process.exit(1)
}
```

## 生成 man page

```bash
claude --print-man > /usr/local/share/man/man1/claude.1
```

## 值得学习的点

1. **Commander 是 CLI 标配** — 不要自写 parser
2. **子命令嵌套** — 清晰组织
3. **非交互模式** — CI/CD 场景
4. **stdin 支持** — 管道集成
5. **语义化退出码** — 脚本友好
6. **危险选项命名长** — 故意难打
7. **自动补全** — UX 提升

## 相关文档

- [main-entry](../root-files/main-entry.md)
- [entrypoints/](../entrypoints/index.md)
- [commands/](../commands/index.md)
