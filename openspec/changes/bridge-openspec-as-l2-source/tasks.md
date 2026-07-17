# Tasks: bridge-openspec-as-l2-source

> 本变更**只产出文档**（proposal / design / specs / tasks），不实装 Rust 代码。
> 代码实装属于未来 Phase 3 的 `implement-l2-project-memory` 变更。
> 本 tasks.md 列出的"任务"是验证 spec 一致性、归档条件、以及为未来实装铺路的
> 准备项。

## 1. 文档完整性验证

- [ ] 1.1 验证 `proposal.md` 中声明的两个 capability 名称与 `specs/` 目录名完全一致（`openspec-as-knowledge-source`、`phase-progressive-rollout`）
- [ ] 1.2 验证两个 `spec.md` 都遵循 `### Requirement: ... / #### Scenario: ...` 格式，且所有 Scenario 用 4 个 `#`
- [ ] 1.3 验证 `design.md` 的 `Decisions` 段落覆盖了 spec 中提到的关键契约面（索引、变更追踪、tool 暴露、降级元数据）
- [ ] 1.4 运行 `openspec validate bridge-openspec-as-l2-source` 并通过

## 2. 与 memora 总设计的一致性检查

- [ ] 2.1 验证本变更不与 `openspec/memora-proposal.md` 第 4 章（Phase 划分）冲突——L2 仍属于 Phase 3，本变更只是提前固化契约
- [ ] 2.2 验证本变更不与 L1 临时记忆、L3 用户记忆、L4 LLM WIKI 的设计重叠（参见 `memora-proposal.md` 第 4-6 章）
- [ ] 2.3 验证 spec 中"Phase 1/2/3 划分"与 `memora-proposal.md` 第 3 章"Phase 2 = AI 压缩 + 向量搜索"对齐——即 Phase 2 对应 L2 桥接的"语义检索阶段"

## 3. 为未来实装铺路（不实装代码，仅准备）

- [ ] 3.1 在 `openspec/changes/` 下新建 placeholder 文件 `openspec/changes/archive/.gitkeep`（如果 archive 目录为空），方便 Phase 3 实装时验证"归档检测"逻辑
- [ ] 3.2 在仓库根 `CLAUDE.md` 中追加一条引用，指向本变更路径，让未来 Claude 看到"桥接 OpenSpec"的项目级约定
- [ ] 3.3 在 `openspec/config.yaml` 的 `context` 字段填写一段项目级上下文（技术栈、约定、Phase 当前状态），供未来 openspec-propose 技能生成新变更时参考

## 4. 归档与下一步

- [ ] 4.1 当 memora Cargo 项目初始化后（参见仓库根 `CLAUDE.md` 的"Cargo 项目初始化提示"），通过 `/opsx:archive-change` 归档本变更
- [ ] 4.2 归档后由 `/opsx:sync-specs` 把 `specs/openspec-as-knowledge-source/spec.md` 和 `specs/phase-progressive-rollout/spec.md` 同步进 `openspec/specs/`，让它们成为正式的能力规格
- [ ] 4.3 未来 Phase 3 L2 实装时新建 `implement-l2-project-memory` 变更，引用 `openspec-as-knowledge-source` 作为契约来源

## 5. 复核

- [ ] 5.1 用 `openspec status --change bridge-openspec-as-l2-source` 确认所有 artifact 都已 `done`
- [ ] 5.2 用 `openspec validate bridge-openspec-as-l2-source --strict` 做严格校验