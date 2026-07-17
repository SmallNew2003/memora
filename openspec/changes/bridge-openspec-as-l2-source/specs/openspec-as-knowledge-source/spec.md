# openspec-as-knowledge-source

## ADDED Requirements

### Requirement: OpenSpec changes 目录作为权威知识源

memora L2 MUST 把 `openspec/changes/<change-name>/` 下的 proposal.md、design.md、
tasks.md 三类制品识别为项目的**权威设计意图层**。当 Agent 询问"为什么这么设计"
或"当前项目处于什么阶段"时，L2 的查询结果 MUST 优先返回这些制品中的内容，而不是
会话观察层的人工总结。

#### Scenario: Agent 询问项目阶段
- **WHEN** Agent 调用 L2 注入上下文并包含"当前 Phase / 进度"类查询
- **THEN** L2 返回来自 `openspec/changes/<active-change>/proposal.md` 的
  `## What Changes` 与 `## Impact` 段落，而非来自会话观察层的归纳

#### Scenario: Agent 询问设计决策原因
- **WHEN** Agent 询问某项技术选型的设计动机
- **THEN** L2 返回 `openspec/changes/<change-name>/design.md` 的
  `## Decisions` 段落中对应决策的 rationale

### Requirement: 制品变更追踪

当 `openspec/changes/` 下的制品被修改、新增、或归档到 `archive/` 时，
memora L2 MUST 在下次启动或下次查询时检测到这一变化，并刷新对应的索引条目。
L2 MUST NOT 缓存过期版本的 proposal.md。

#### Scenario: proposal 修订
- **WHEN** 同一 `<change-name>/proposal.md` 的 `mtime` 或内容哈希发生变化
- **THEN** L2 在下次读取该 change 时返回最新内容，并丢弃旧版本的向量/索引

#### Scenario: change 归档
- **WHEN** `<change-name>/` 目录被移动到 `openspec/changes/archive/`
- **THEN** L2 把该 change 从"进行中"集合移入"已归档"集合，已归档制品默认
  不出现在 Agent 上下文注入中（除非 Agent 显式指定 `--include-archived`）

#### Scenario: 新增 change
- **WHEN** `openspec/changes/<new-name>/` 目录出现且至少包含 `proposal.md`
- **THEN** L2 在下次启动时把新 change 加入"进行中"集合

### Requirement: 注入接口契约

L2 MUST 暴露以下两个 MCP tool（或等价的内部接口）：

- `memora_read_openspec(change_name: string, artifact: "proposal" | "design" | "tasks")`：
  返回指定 change 下指定制品的全文。
- `memora_list_openspec_changes(include_archived: bool)`：
  列出所有 change 的元数据（name、phase、归档状态、最新 mtime）。

#### Scenario: 调用 read tool
- **WHEN** Agent 调用 `memora_read_openspec(change_name="bridge-openspec-as-l2-source", artifact="proposal")`
- **THEN** 系统返回该 proposal.md 的完整内容字符串（不做截断、不做摘要）

#### Scenario: 调用 list tool
- **WHEN** Agent 调用 `memora_list_openspec_changes(include_archived=false)`
- **THEN** 系统返回所有 `openspec/changes/<name>/`（不在 archive/ 下）的
  元数据列表，每项至少包含 `name` 和 `last_modified` 字段

### Requirement: 范围限定声明

本 capability MUST 仅覆盖 `openspec/changes/` 目录（进行中提案）。
`openspec/specs/`、`openspec/memora-proposal.md`、`openspec/changes/archive/`
的消费方式 MUST 作为未来扩展，由后续变更独立提议；本 capability MUST NOT
直接处理这些范围的制品。

#### Scenario: 跨范围查询
- **WHEN** Agent 查询 `openspec/specs/` 下的内容
- **THEN** L2 MUST 返回 "out of scope" 错误或空结果，且 MUST 在错误信息中
  提示该范围尚未被本 capability 覆盖