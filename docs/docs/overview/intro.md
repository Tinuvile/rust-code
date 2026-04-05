---
sidebar_position: 1
title: 项目总览
description: Claude Code 的规模、技术栈与设计哲学
---

# 项目总览

## 项目是什么

**Claude Code** 是 Anthropic 推出的终端 AI 编码助手，用户通过命令行与 Claude 协作完成软件工程任务。它并非一个简单的命令行包装器，而是一个**工业级的 AI Agent 系统**，具备完整的工具调用、权限控制、多 Agent 协作、持久化记忆、远程会话、MCP 协议集成等能力。

这份文档基于 `F:\claude-code` 源码（去混淆后的 TypeScript 源码）编写，目的是拆解其中值得学习的**工业级系统设计**和**AI Agent 领域先行者方案**。

## 项目规模

| 维度 | 数值 |
|------|------|
| 源文件总数 | **1,884 个** TypeScript/TSX 文件 |
| 源代码行数 | **512,000+ 行** |
| 顶层目录数 | **35 个** 子目录 |
| 工具实现 | **40+ 个** 独立工具 |
| 斜杠命令 | **66+ 个** 用户命令 |
| 内置技能 | **15+ 个** bundled skills |
| React 组件 | **100+ 个** 终端 UI 组件 |
| 自定义 Hooks | **90+ 个** React hooks |
| 自研 Ink 框架 | **85+ 个** 文件 |
| 工具函数 | **298 个** util 文件 |

这是一个**有大量工业用户**的生产级 CLI 系统，具有**严肃的工程质量要求**。

## 技术栈

### 运行时与语言
- **Bun** — JavaScript/TypeScript 运行时（替代 Node.js）
- **TypeScript (strict mode)** — 类型严格的 TS
- **Zod v4** — 运行时类型校验

### 终端 UI
- **React** — 用 React 编写终端界面（是的，React！）
- **Ink (自研 fork)** — React → 终端的 reconciler
- **Yoga** — Flexbox 布局引擎
- **crossterm 级别的键盘处理** — 自研事件系统

### CLI 与交互
- **Commander.js** — CLI 参数解析
- **custom bash parser** — Tree-sitter AST 级别的命令解析

### 协议与集成
- **Anthropic SDK** — Claude API 客户端
- **MCP SDK** — Model Context Protocol（Anthropic 开放协议）
- **LSP** — Language Server Protocol 集成
- **OAuth 2.0 + JWT** — 多种认证方式

### 云与存储
- 多云 API 抽象：**Anthropic / AWS Bedrock / Azure Foundry / Google Vertex AI**
- **macOS Keychain / Windows Credential Manager** — 凭据持久化
- **IndexedDB-like** 本地会话存储

### 分析与可观测
- **Datadog** — 商业 APM
- **GrowthBook** — Feature Flags
- **OpenTelemetry + gRPC** — 分布式追踪

### 构建
- **Bun bundler** — 死代码消除（基于 Feature Flags）
- **Biome** — 代码格式化

## 设计哲学

从源码中能看出的核心设计理念：

### 1. 安全第一
所有可能影响系统的操作（命令执行、文件写入、网络请求）都走**多层权限栈**。以 Bash 工具为例：

```
用户请求 → Tree-sitter AST 解析（FAIL-CLOSED 白名单）
        → 权限规则匹配（allow/deny/ask）
        → ML 分类器（可选）
        → 沙箱隔离（可选）
        → 破坏性命令警告
        → 实际执行
```

任何一环拦截都会中断执行。**宁可误拦，不可误放。**

### 2. 可扩展第一
核心抽象都通过**数据驱动**和**Hook 机制**扩展：
- **工具**：通过 `buildTool<D>()` 泛型工厂构建，支持自定义工具
- **命令**：5 来源发现（bundled/plugins/skills/workflows/dynamic）
- **Hook**：20+ 生命周期事件点，插件可审批/拦截/注入
- **MCP**：第三方可通过 MCP 协议注入任意工具
- **Agent**：用户可通过 Markdown 定义子 Agent

### 3. 性能第一
- **并行预取**：模块加载前并行启动 MDM 子进程 / Keychain 读取（节省 ~65ms）
- **死代码消除**：Bun feature flags 在构建时剥离未启用代码
- **懒加载**：重型模块（Ink 渲染、React 组件）通过动态导入延迟
- **LRU 缓存**：文件状态、messages、settings 全部缓存
- **增量压缩**：micro → auto → consolidation 三阶段上下文压缩

### 4. 可观测第一
- **零依赖分析**：`services/analytics/` 不依赖任何模块以避免循环依赖
- **PII 安全**：`_PROTO_*` 字段前缀 + 类型强制校验防止泄漏代码/路径
- **Feature Flags**：GrowthBook + kill-switch，支持 A/B 实验和紧急关闭
- **诊断工具**：`/doctor` 命令、堆转储、性能剖析

### 5. AI Agent 优先
整个架构围绕 Agent 场景设计：
- **递归 Agent**：Agent 可以派生子 Agent，共享/隔离上下文
- **持久化记忆**：`memdir/` 跨会话记忆系统
- **多 Agent 协调**：`coordinator/` 模式支持 Agent swarm
- **Agent 定义**：Markdown + frontmatter 声明式配置
- **Skill 系统**：Agent 可以加载技能扩展能力

## 这份文档的阅读路径

推荐的阅读顺序：

1. **全局架构**（4 篇）：先建立整体印象
   - [分层架构](./architecture.md) — 模块如何分层组织
   - [请求生命周期](./data-flow.md) — 一次用户请求如何穿越系统
   - [设计亮点](./design-highlights.md) — 十大值得关注的工程模式

2. **根文件**（5 篇）：核心入口
   - `QueryEngine`、`Tool`、`commands` 等定义了系统骨架

3. **tools/ services/ utils/**：三大核心子系统
   - 按关注点深入其中的设计

4. **Agent 核心**：理解 Agent 领域设计
   - `bootstrap/`、`coordinator/`、`memdir/`、`tasks/`、`skills/`

5. **UI 与其他**：按需阅读

## 鸣谢

本文档配套的 Rust 重写项目见 [rust-code 仓库](https://github.com/Tinuvile/rust-code)。
