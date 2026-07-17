# Design: 把 OpenSpec 制品桥接为 memora L2 项目记忆的知识源

## Context

memora 仓库目前处于"设计已完成、代码尚未落地"阶段（参见仓库根 `CLAUDE.md`）。
L2 项目记忆本身在 `openspec/memora-proposal.md` 中定义为 Phase 3 才引入。
本变更的目的是**提前把 L2 ↔ OpenSpec 桥接的设计契约固化下来**，让未来 Phase 3
实装时可以直接消费现有的 OpenSpec 制品，而不需要重新决策"是否桥接、桥接哪些、
怎么降级"。

约束：
- 当前 `openspec/specs/` 为空，本变更是首批落地，必须为后续变更立风格、定格式。
- 本变更**只产出设计文档和 spec**，不写 Rust 代码。代码实装等到 memora 的
  Cargo 项目初始化 + L2 提案通过后再走 `openspec-apply-change` 工作流。
- 仓库的 RTK 工具链约定要求命令尽量走 `rtk` proxy，但本变更不涉及 shell 命令，
  只需要 `openspec` CLI。

## Goals / Non-Goals

**Goals:**
- 定义 L2 ↔ OpenSpec 桥接的**契约面**（索引、变更追踪、注入接口、范围边界）。
- 定义渐进式 Phase 路线的**降级语义**，避免未来实现时出现"Phase 升级=破坏
  旧调用"的反模式。
- 把"OpenSpec 是权威事实层"这一项目级约定写入 spec，让所有未来 L2 调用方都
  知道优先消费 OpenSpec 制品而不是会话观察层。

**Non-Goals:**
- 不实装任何 Rust 代码（属于 Phase 3 实装任务范围）。
- 不修改 L1 临时记忆、L3 用户记忆、L4 LLM WIKI 的任何既有设计。
- 不桥接 `openspec/specs/`、`openspec/memora-proposal.md`、`openspec/changes/archive/`，
  这些作为后续独立变更。
- 不引入新的 Cargo 依赖（即使是未来实装，sqlite-vec、tokio 已经在
  `memora-proposal.md` 选型范围内）。

## Decisions

### D1：范围首限 `openspec/changes/` 而非全部 OpenSpec 制品

**选择**：本变更首批范围只覆盖 `openspec/changes/<name>/{proposal,design,tasks}.md`。

**理由**：这是**最活跃**的制品层——每次有提案改动都在这里。`openspec/specs/` 是
真理之源但更新频率极低（一次提案落地才改一次）；`openspec/memora-proposal.md`
近乎冻结；`openspec/changes/archive/` 是历史。

**替代方案考虑**：
- *全部一起桥接*：范围过大，本变更提案会拖到 1500+ 字，难以一次走通工作流。
- *只桥接 `memora-proposal.md`*：信息密度高但更新频次太低，验证机会少。

**结论**：分批桥接，先从最活跃的 `changes/` 开始，验证完契约面后再扩展。

### D2：注入接口采用 MCP tool 而非自动注入

**选择**：Phase 1 暴露 `memora_read_openspec` 和 `memora_list_openspec_changes`
两个 MCP tool，由 Agent 显式调用。

**理由**：
- 自动全量注入（如 `CLAUDE.md` 风格）在 `changes/` 数量超过 5-10 个时
  会撑爆上下文窗口。
- 显式 tool 调用的"按需加载"语义契合 Agent 的"我需要什么调什么"心智模型。
- rmcp crate 已支持 tool 注册，零额外成本。

**替代方案考虑**：
- *启动时全量注入*：实现简单但有扩展性问题。
- *Hook 拦截其他 tool 调用自动追加*：依赖 Phase 2 的 hook 能力（参见
  `memora-proposal.md` 第 7 章设计模式优先级矩阵），过早引入。

### D3：Phase 路线按"能力叠加"而非"接口重塑"

**选择**：每次 Phase 升级都**新增** tool，**不修改**旧 tool 的契约。

**理由**：避免"今天是 phase 2 部署的好日子，结果 phase 3 已经发布且 phase 2
的 tool 被改名"的部署噩梦。每个旧 tool 必须在新 Phase 中继续以相同输入输出
工作。

**替代方案考虑**：
- *接口重塑*（phase 升级时改 tool 名/参数）：破坏调用方兼容性，违背 spec 中
  的"向前兼容"要求。

### D4：制品变更追踪使用 mtime + 内容哈希

**选择**：L2 索引时记录每个制品的 mtime 和 sha256 哈希；下次启动时若任一
变化则刷新。

**理由**：
- mtime 便宜，可快速过滤掉大部分未变化的文件。
- 内容哈希防止"mtime 被 touch 但内容不变"的伪更新，以及"内容被原子写入但
  mtime 倒退"的反向场景。
- 都已是 Rust 标准库或常见 crate（`std::fs::metadata`、`sha2`）的能力，
  无外部依赖。

**替代方案考虑**：
- *纯内容哈希*：每次启动都要读全文+算哈希，对 50+ change 的仓库有可观 I/O。
- *fsnotify 实时监听*：引入 inotify/fsevents 跨平台复杂度，且 memora 通常是
  按需启动而非常驻。

### D5：降级元数据走响应包装层而非 HTTP header 类旁路

**选择**：`phase` 和 `fallback_reason` 作为 tool 响应 JSON 的**字段**返回。

**理由**：MCP tool 响应是 JSON-RPC 结构，没有 HTTP 风格的旁路元数据通道；
字段返回对 Agent 来说是最自然的"读响应时一眼看到"的形式。

## Risks / Trade-offs

- **[风险] OpenSpec 制品本身可能不严谨**（proposal 写得敷衍、tasks 缺漏）。
  → **缓解**：L2 把制品当**事实**而不是**信号**——Agent 调用前应已有能力判断
  制品质量；本变更不引入质量过滤。
- **[风险] Phase 路线冻结过早**（Phase 2 实际不需要 embedding，或 Phase 3
  出现新形态）。
  → **缓解**：spec 写"按 Phase 渐进"而非"Phase 2 必须是 embedding"；后续
  变更可以修订 Phase 定义而不破坏兼容性。
- **[权衡] 首批范围窄**（只覆盖 `changes/`）：意味着 Phase 1 阶段 Agent
  查 `openspec/specs/` 拿不到结果。 → 这是有意的分批策略，已在
  `openspec-as-knowledge-source` spec 中显式声明。
- **[权衡] 没有自动注入导致 Agent 必须记得调 tool**：
  → Phase 3 跨 Agent 协同成熟后可以补一个"会话启动时主动注入进行中
  changes 摘要"的能力，但本变更不在范围。

## Migration Plan

本变更**不部署任何代码**，迁移计划只覆盖文档层级：

1. **本变更落地后**：`openspec/changes/bridge-openspec-as-l2-source/` 下有
   完整的 proposal/design/tasks + 两个 spec 文件，可供未来 Phase 3 L2 实装时
   直接消费。
2. **未来 Phase 3 实装时**：通过 `openspec-apply-change` 技能新建一个
   `implement-l2-project-memory` 变更，引用本变更的 spec 作为契约面。
3. **未来扩展时**：若需要桥接 `openspec/specs/` 或 `archive/`，新建独立
   变更，**不要修改本变更**（保留历史语义）。

**回滚策略**：本变更若被否决，删除 `openspec/changes/bridge-openspec-as-l2-source/`
整个目录即可，对仓库零影响（所有变更都在 `openspec/changes/` 下，未归档前
不属于真理之源）。

## Open Questions

1. **Phase 2 的语义检索 query 接口是否要命名 `memora_search_openspec`？**
   本变更在 spec 中只是举例，未硬性规定；未来实装时再敲定。
2. **跨 Agent 协同时，"权威事实层"是否要可写？** 即其他 Agent 能否通过
   L2 修改 OpenSpec 制品？倾向于**只读**，但需要 Phase 3 提案时再确认。
3. **Phase 1 阶段是否要给 `memora_read_openspec` 加一层结果缓存？**
   当前 spec 要求每次都返回全文；若 `changes/` 文件超过 100KB 级别，缓存
   价值才会显现，先不引入。