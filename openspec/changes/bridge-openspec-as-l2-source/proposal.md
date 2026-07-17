# Proposal: 把 OpenSpec 制品桥接为 memora L2 项目记忆的知识源

## Why

memora 计划在 Phase 3 引入 L2 项目记忆（绑定代码库、跨会话持久化项目上下文）。
与此同时，仓库已经使用 OpenSpec 工作流管理所有设计决策：`openspec/changes/` 里有
进行中的提案，`openspec/specs/` 里沉淀已确认的能力规格，`openspec/memora-proposal.md`
定义了项目整体蓝图。

这两者在形态上天然相似——都是给代码库外挂结构化、跨会话持久化的上下文——但
OpenSpec 是**人写的、宏观的、低频**的设计意图层，memora L2 是**机器写的、微观、
高频**的会话观察层。两者应该**互补而非替代**：

- 没有 L2 桥接：Agent 启动时不知道「这个项目正处于 Phase 1，AI 压缩在 Phase 2」，
  容易踩已经被设计提案否决过的方案。
- 没有 OpenSpec 作为知识源：L2 只能从零观察已有规范，无法获得"为什么这么设计"
  的设计意图，注入的上下文会沦为事实罗列。

## What Changes

- 新增一个 capability：`openspec-as-knowledge-source`，定义 memora L2 如何把
  `openspec/changes/`（本变更确认的首批范围）作为权威知识源消费。
- 新增一个 capability：`phase-progressive-rollout`，定义 memora L2 ↔ OpenSpec 桥接
  按 Phase 渐进推进的路线：Phase 1（同步索引 + MCP tool 暴露）→
  Phase 2（embedding 语义检索）→ Phase 3（跨 Agent 协同 + 设计意图注入）。
- 不修改任何现有 spec（`openspec/specs/` 当前为空，本变更是首批落地）。
- **BREAKING**：无（本变更只新增设计文档和未来实现的 spec 约定，不动现有代码）。

## Capabilities

### New Capabilities

- `openspec-as-knowledge-source`：定义 memora L2 对 OpenSpec 制品（首批范围：
  `openspec/changes/<name>/proposal.md` + `design.md` + `tasks.md`）的**索引**、
  **版本追踪**（proposal 修订、归档、重新激活时同步更新）、**注入接口**三个
  契约面。
- `phase-progressive-rollout`：定义上述能力按 Phase 推进的**触发条件**和
  **降级策略**（例如 Phase 1 没有 embedding 时，MCP tool 返回全文而不是向量
  检索结果）。

### Modified Capabilities

（无 — `openspec/specs/` 为空。）

## Impact

- **新增文件**：
  - `openspec/changes/bridge-openspec-as-l2-source/specs/openspec-as-knowledge-source/spec.md`
  - `openspec/changes/bridge-openspec-as-l2-source/specs/phase-progressive-rollout/spec.md`
  - `openspec/changes/bridge-openspec-as-l2-source/design.md`
  - `openspec/changes/bridge-openspec-as-l2-source/tasks.md`
- **未来代码影响**（不在本次落地，仅做规划声明）：
  - Phase 1 实装后：新增 MCP tool `memora_read_openspec`（按 change 名读取
    proposal/design/tasks）和 `memora_list_openspec_changes`（列出当前进行中
    与已归档的 change）。
  - Phase 2 实装后：在 `sqlite-vec` 表中索引 OpenSpec 制品，加入 `phase`
    和 `change_name` 元数据。
  - Phase 3 实装后：跨 Agent（Claude Code + Codex + Cursor）共享 L2 项目记忆时，
    OpenSpec 制品作为权威事实层被广播。
- **不受影响**：memora L1 临时记忆、L3 用户记忆、L4 LLM WIKI 的现有设计均不变化。