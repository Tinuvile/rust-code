---
sidebar_position: 2
title: Proxy 与 Native 模块
description: proxy/ 与 native/ 目录
---

# Proxy 与 Native 模块

**目录：** `src/proxy/`、`src/native/`

这两个目录处理 **网络代理** 和 **原生代码集成**——企业/性能场景必备。

## Proxy 模块

### 为什么需要代理？

企业网络常有**HTTP 代理**：

- 安全审计 — 所有出站流量通过代理
- 访问控制 — 白名单域名
- 缓存 — 省带宽

Claude Code 要能**透过代理**访问 Anthropic API。

### 代理配置

```bash
# 环境变量（标准）
export HTTPS_PROXY=http://proxy.company.com:8080
export NO_PROXY=localhost,127.0.0.1,*.internal

# 或 config
claude config set proxy.https http://proxy:8080
```

### 代理实现

```typescript
// proxy/agent.ts
import { HttpsProxyAgent } from 'https-proxy-agent'

function getProxyAgent(url: string): Agent | undefined {
  const proxy = process.env.HTTPS_PROXY ?? config.proxy?.https
  if (!proxy) return undefined

  // 检查 NO_PROXY
  const hostname = new URL(url).hostname
  const noProxy = (process.env.NO_PROXY ?? '').split(',')
  if (noProxy.some(p => matchDomain(hostname, p))) return undefined

  return new HttpsProxyAgent(proxy)
}

// 用法
const agent = getProxyAgent('https://api.anthropic.com')
fetch(url, { agent })
```

### PAC（Proxy Auto-Config）

某些企业用 PAC 脚本动态决定代理：

```javascript
// proxy.pac
function FindProxyForURL(url, host) {
  if (host === 'api.anthropic.com') {
    return 'PROXY proxy.company.com:8080'
  }
  return 'DIRECT'
}
```

```typescript
import pacProxyAgent from 'pac-proxy-agent'

const agent = new PacProxyAgent('http://pac.company.com/proxy.pac')
```

### 代理认证

```typescript
// 基础认证
const proxy = 'http://user:pass@proxy.company.com:8080'

// NTLM（Windows 企业）
import { HttpsProxyAgent } from 'https-proxy-agent'
import { NtlmClient } from 'ntlm-client'
```

### SSL/TLS 考量

```typescript
// 企业 CA 证书
https.globalAgent.options.ca = [
  ...tls.rootCertificates,
  fs.readFileSync('/etc/ssl/company-ca.pem')
]

// 或配置
export NODE_EXTRA_CA_CERTS=/etc/ssl/company-ca.pem
```

### 代理诊断

```bash
claude proxy diagnose
```

输出：

```
Proxy Configuration:
  HTTPS_PROXY: http://proxy.company.com:8080
  HTTP_PROXY: (not set)
  NO_PROXY: localhost,*.internal

Testing proxy:
  → Connect to proxy.company.com:8080 ... OK
  → Tunnel to api.anthropic.com:443 ... OK
  → Certificate valid ... OK
  → API reachable ... OK

Tests passed.
```

**启动前诊断**节省大量 debug 时间。

## Native 模块

### 为什么需要原生代码？

某些功能**纯 JS 太慢**或**无法实现**：

- **Tree-sitter** — C 语言解析器，高性能
- **文件监控** — 平台原生 API（FSEvents/inotify）
- **键盘捕获** — 底层 TTY 操作
- **加密** — 硬件加速

### 原生依赖

```json
// package.json
{
  "dependencies": {
    "tree-sitter": "^0.21.0",
    "tree-sitter-bash": "^0.21.0",
    "node-pty": "^1.0.0",
    "chokidar": "^3.6.0",
    "@napi-rs/keyring": "^1.1.0"
  }
}
```

### Tree-sitter

```typescript
// native/treeSitter.ts
import Parser from 'tree-sitter'
import Bash from 'tree-sitter-bash'
import JavaScript from 'tree-sitter-javascript'
import TypeScript from 'tree-sitter-typescript'

const parsers = new Map<string, Parser>()

export function getParser(lang: string): Parser {
  if (!parsers.has(lang)) {
    const parser = new Parser()
    parser.setLanguage(loadLanguage(lang))
    parsers.set(lang, parser)
  }
  return parsers.get(lang)!
}
```

详见 [utils/bash-security](../utils/bash-security.md) 的 AST 解析章节。

### node-pty

**真正的 PTY**（pseudo-terminal）——比 spawn 更像真终端：

```typescript
import * as pty from 'node-pty'

const shell = pty.spawn('bash', [], {
  name: 'xterm-color',
  cols: 80,
  rows: 30,
  cwd: process.cwd(),
  env: process.env
})

shell.onData(data => process.stdout.write(data))
shell.write('ls\r')
```

**对交互式程序（vim、top）必需** — 普通 spawn 不行。

### keyring

OS 原生**密钥存储**：

```typescript
import { Entry } from '@napi-rs/keyring'

const entry = new Entry('claude-code', 'anthropic-token')

// 保存
entry.setPassword('sk-ant-...')

// 读取
const token = entry.getPassword()

// 删除
entry.deletePassword()
```

不同 OS 用不同存储：
- macOS: Keychain
- Windows: Credential Manager
- Linux: libsecret

### 文件监控

```typescript
import chokidar from 'chokidar'

chokidar.watch('src/**/*.ts', {
  ignored: /node_modules/,
  persistent: true
}).on('change', (path) => {
  console.log(`Changed: ${path}`)
})
```

chokidar 内部用 **FSEvents (macOS)** / **inotify (Linux)** / **ReadDirectoryChangesW (Windows)**。

### 原生模块的打包问题

Bun bundle 时**原生模块不能打包进单文件**：

```
my-app.js (4MB)
+ node_modules/tree-sitter/build/Release/*.node
+ node_modules/node-pty/build/Release/*.node
+ ...
```

解决方案：

```typescript
// 发布时包含原生模块
import path from 'path'
import { createRequire } from 'module'

const require = createRequire(import.meta.url)
const treeSitter = require(path.join(
  process.env.CLAUDE_RUNTIME_DIR,
  'tree-sitter.node'
))
```

### 跨平台构建

```json
// package.json scripts
{
  "build:darwin-arm64": "pkg . --target darwin-arm64 ...",
  "build:darwin-x64": "pkg . --target darwin-x64 ...",
  "build:linux-x64": "pkg . --target linux-x64 ...",
  "build:win32-x64": "pkg . --target win32-x64 ..."
}
```

每个平台**独立打包**——原生模块不通用。

## 值得学习的点

**Proxy：**

1. **尊重 HTTPS_PROXY/NO_PROXY** — 标准环境变量
2. **PAC 支持** — 企业必备
3. **CA 证书注入** — 企业自签证书
4. **启动前诊断** — 省 debug 时间

**Native：**

1. **Tree-sitter** — AST 性能关键
2. **node-pty** — 真 PTY 支持
3. **OS keyring** — 原生密钥存储
4. **平台适配打包** — 原生模块不通用
5. **chokidar** — 跨平台文件监控

## 相关文档

- [utils/bash-security](../utils/bash-security.md)
- [services/api](../services/api.md)
- [services/oauth-and-plugins](../services/oauth-and-plugins.md)
- [utils/auth-and-shell](../utils/auth-and-shell.md)
