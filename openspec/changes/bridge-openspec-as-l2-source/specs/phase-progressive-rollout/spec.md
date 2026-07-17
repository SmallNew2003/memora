# phase-progressive-rollout

## ADDED Requirements

### Requirement: 三阶段渐进路线

`openspec-as-knowledge-source` 的实装 MUST 按以下三个 Phase 渐进推进，
每个 Phase 都有明确的进入条件与降级策略：

| Phase | 进入条件 | 核心能力 | 缺失时降级行为 |
|:------|:---------|:---------|:---------------|
| Phase 1 | L2 项目记忆首次落地（Phase 3 提案通过） | 同步全文索引 + MCP tool 暴露 | —（这是基线） |
| Phase 2 | memora 集成 embedding（Phase 2 提案通过） | 向量语义检索 + FTS5 混合排序 | 退回 Phase 1 行为（全文匹配） |
| Phase 3 | 跨 Agent 协同提案（Phase 3 协同提案）通过 | 跨 Agent 广播 + 设计意图主动注入 | 退回 Phase 2 行为（单 Agent 语义检索） |

#### Scenario: Phase 1 启用
- **WHEN** Phase 1 实现完成且 L2 项目记忆模块激活
- **THEN** Agent 可通过 `memora_read_openspec` 和 `memora_list_openspec_changes`
  两个 tool 获取 OpenSpec 制品全文，无 embedding、无跨 Agent 能力

#### Scenario: Phase 2 启用前查询
- **WHEN** Agent 在 Phase 2 启用前调用任何"语义检索"接口
- **THEN** 系统返回 Phase 1 行为（基于 change_name 精确匹配的全文返回）
  并在响应元数据中标注 `phase: 1, fallback_reason: "embedding_unavailable"`

#### Scenario: Phase 3 启用前查询
- **WHEN** Agent 在 Phase 3 启用前调用任何"跨 Agent 广播"接口
- **THEN** 系统返回 Phase 2 行为（单 Agent 本地检索结果）并在响应元数据中
  标注 `phase: 2, fallback_reason: "cross_agent_unavailable"`

### Requirement: Phase 切换时的版本兼容

每次 Phase 切换 MUST 保持向前兼容：旧 Phase 的所有调用方式 MUST 继续可用，
不得因 Phase 升级而出现 breaking change。

#### Scenario: Phase 1 → Phase 2 切换
- **WHEN** embedding 模块集成完成，Phase 2 启用
- **THEN** Phase 1 的 `memora_read_openspec` tool MUST 继续可用且行为不变；
  新的语义检索能力以新 tool 名（例如 `memora_search_openspec`）暴露

#### Scenario: Phase 2 → Phase 3 切换
- **WHEN** 跨 Agent 协同启用，Phase 3 启用
- **THEN** Phase 2 的所有 tool MUST 继续可用；新能力以新 tool 名暴露

### Requirement: 降级元数据可见性

当 L2 因为当前 Phase 不足而触发降级时，调用响应 MUST 包含 `phase` 和
（必要时）`fallback_reason` 字段，让调用方 Agent 能识别自己拿到的是
"次优结果"而非"完整结果"。

#### Scenario: Phase 不足时的响应
- **WHEN** Agent 在 Phase 1 阶段发出语义检索意图的请求
- **THEN** 响应中 MUST 包含 `phase: 1` 与 `fallback_reason: "embedding_unavailable"`
  两个字段，且 MUST NOT 隐藏降级事实

### Requirement: Phase 进度可见性

L2 MUST 暴露一种方式让 Agent 了解当前所处的 Phase（1/2/3）以及距离下一 Phase
还缺什么条件。

#### Scenario: 查询当前 Phase
- **WHEN** Agent 调用任意 L2 接口
- **THEN** 响应中 SHOULD 包含 `current_phase` 字段，值为 1、2 或 3