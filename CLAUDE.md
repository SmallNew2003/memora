# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## 回复语言

- **默认使用中文回复用户**（见 `AGENTS.md`）。

## 项目状态：预实现设计阶段

当前仓库处于**设计已完成、代码尚未落地**的阶段：

- ✅ 有完整的设计提案：`openspec/memora-proposal.md`（约 450 行，定义了四层架构、Phase 计划、技术选型）
- ✅ 有 OpenSpec 工作流：`openspec/config.yaml`、`openspec/changes/`、`openspec/specs/`
- ✅ 有 Codex 自定义技能：`.codex/skills/openspec-*`（共 5 个）
- ❌ **没有 `Cargo.toml`，没有 `src/`** —— Cargo.lock 和 target/ 是单次命令的残留，可视为尚未建立 Cargo workspace
- ⚠️ 在执行任何 cargo 命令前，先确认 `Cargo.toml` 是否已创建；如果不存在，工作流应该是先 `cargo init --name memora` 再开始

未来 Claude 在这个仓库工作时，应**优先遵循 OpenSpec 工作流**而非直接动手写代码。

---

## 项目简介：memora —— 多层 AI 记忆系统

**核心问题**：AI Coding Agent（Claude Code、Cursor、Codex 等）每次会话都从零开始，跨会话/项目的知识无法沉淀。

**目标**：设计一个 **Local-First、零配置、单二进制** 的多层记忆系统，让 Agent 具备渐进式记忆能力，通过 **MCP 协议**被主流 Agent 调用。

### 四层架构（L1-L4）

| 层级 | 名称 | 范围 | 生命周期 | 类比 |
|:-----|:-----|:-----|:---------|:-----|
| **L1** | 临时记忆 | 单次会话 | 会话结束即归档 | 工作记忆 |
| **L2** | 项目记忆 | 绑定代码库 | 项目周期 | 项目文档 |
| **L3** | 用户记忆 | 跨项目 | 用户周期 | 个人偏好 |
| **L4** | LLM WIKI | 全局 | 永久 | 知识库 |

### 当前 Phase

**Phase 1：临时记忆**（设计中）—— SQLite + sqlite-vec + FTS5 存储，6 个 MCP tools，单二进制。

**后续 Phase**：Phase 2（AI 压缩 + 向量搜索）、Phase 3（项目记忆）、Phase 4（用户记忆 + LLM WIKI）。

---

## 技术选型（已确定）

- **语言**：Rust（单二进制，零运行时依赖）
- **存储**：SQLite + sqlite-vec + FTS5（结构化数据 + 向量 + 全文搜索）
- **MCP 框架**：rmcp crate（Rust 原生 MCP 实现，支持 SSE 和 Streamable HTTP）
- **接入方式**：MCP 协议（Claude Code / Codex / Cursor / OpenCode 通用）

工具链版本（来自 `target/.rustc_info.json`）：
- rustc 1.94.0（aarch64-apple-darwin）
- LLVM 21.1.8

---

## OpenSpec 工作流（重要）

本仓库使用 [OpenSpec](https://openspec.dev/) 作为规格驱动开发（spec-driven）的工作流。**在动手改代码前先走 OpenSpec 流程。**

### 目录结构

```
openspec/
├── config.yaml              # schema: spec-driven（默认值，注释里有示例）
├── memora-proposal.md       # 项目总设计文档（约 450 行）
├── specs/                   # 真理之源：当前系统应该做什么
│   └── (按能力组织的 spec 文件)
└── changes/                 # 提案/变更
    ├── <change-name>/       # 进行中的变更（含 proposal.md / design.md / tasks.md）
    └── archive/             # 已完成/归档的变更
```

### 5 个 Codex 技能（`.codex/skills/`）

| 技能 | 触发场景 |
|:-----|:---------|
| `openspec-propose` | 提议一个完整变更（含 proposal.md、design.md、tasks.md） |
| `openspec-apply-change` | 根据 tasks.md 执行实现 |
| `openspec-explore` | 探索/调研现状 |
| `openspec-sync-specs` | 把已完成变更同步回 specs/ |
| `openspec-update-change` | 更新已有变更 |
| `openspec-archive-change` | 归档已完成变更 |

**每个技能的工作流**：读 `SKILL.md` 了解输入参数和 OpenSpec CLI 命令（如 `openspec new change`、`openspec status --json`、`openspec instructions`）。

### 典型工作流

1. **想清楚要做什么** → `/opsx:propose <name>` 走 `openspec-propose` 技能
2. **实现** → `/opsx:apply` 走 `openspec-apply-change`
3. **完成后** → `/opsx:sync-specs` 和 `/opsx:archive-change`

修改设计前先看 `openspec/config.yaml` 的 `context` 和 `rules` 字段（当前为空，可以填写项目的技术栈、约定、领域知识等约束）。

---

## 重要设计模式（来自调研，提炼自 50+ 开源项目）

`memora-proposal.md` 第 7-8 章有详细的设计模式优先级矩阵，**最相关的几个**：

- **AI 压缩流水线**（⭐⭐⭐⭐⭐ Phase 2）：原始观察 → AI 摘要 → 嵌入 → 分层注入。参考 claude-mem (87k⭐)、context-mode、MemOS。
- **Hook 驱动自动捕获**（⭐⭐⭐⭐⭐ Phase 2）：MCP 工具调用拦截，自动记录。
- **写入时去重 + 敏感度过滤**（⭐⭐⭐⭐ Phase 2）：去重；自动检测 API key/token 等敏感信息。
- **遗忘曲线 + 矛盾检测**（⭐⭐⭐⭐ Phase 3）：记忆会衰减；新旧记忆冲突时标记而非覆盖。
- **四图架构**（⭐⭐⭐ Phase 4）：时序图 + 实体图 + 因果图 + 语义图（参考 Mnemon）。

L4 LLM WIKI 走"**不自己造笔记软件**"路线 —— 通过 `WikiBackend` trait 桥接已有笔记系统（Obsidian P0、纯文件系统 P1、Logseq P2、Notion P3）。

---

## 待讨论问题（设计未拍板）

见 `openspec/memora-proposal.md` 第 9 章。未来 Claude 在被问及时应主动推进这些决策而非默认等用户：

1. **Embedding 方案**：本地 ONNX（all-MiniLM-L6-v2，零网络但二进制大）vs OpenAI API（简单但需网络）
2. **AI 压缩触发时机**：由调用方 Agent 在 `session_end` 时压缩后传入 vs memora 内置 LLM 调用
3. **与 claude-mem 的关系**：替代还是互补（差异化在分层 + Rust + 跨 Agent）
4. **项目名称**：memora 是否最终确认

---

## Cargo 项目初始化提示

首次执行 cargo 命令时，因为没有 `Cargo.toml`，会报 "no targets to build" 之类的错。预期初始化步骤：

```bash
cargo init --name memora           # 在仓库根创建 Cargo.toml 和 src/main.rs
cargo add rmcp tokio rusqlite ...  # 按设计提案添加依赖
```

预期依赖（从 `memora-proposal.md` 推断，未确定版本）：
- `rmcp`（MCP 框架）
- `rusqlite` 或 `sqlx`（SQLite 绑定）
- `sqlite-vec`（向量扩展，纯 Rust 版）
- `tokio`（异步运行时，rmcp 需要）
- `serde` / `serde_json`（序列化）
- `anyhow` / `thiserror`（错误处理）
- `tracing`（日志）

构建/测试命令在 Cargo 项目初始化后即可正常使用（`cargo build`、`cargo test`、`cargo run` 等）。
