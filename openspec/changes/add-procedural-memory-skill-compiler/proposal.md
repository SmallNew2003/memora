## Why

memora 当前能够保存、压缩和召回事实与会话经历，但还不能把已经验证有效的工作方法
沉淀为可重复使用的程序性记忆。若只保存历史，Agent 仍需在每次任务中重新推导操作
流程；若未经治理地自动生成 Skill，又会把偶然成功、敏感信息和错误经验固化为长期
行为。

## What Changes

- 把 Skill 定义为从 L1-L4 记忆派生出的程序性制品，而不是新增 L5；记忆层级继续
  表示作用域和生命周期，Skill 表示知识形态。
- 增加证据驱动的 Skill 候选流水线：从成功或失败的任务记录、checkpoint、用户明确
  指令及可验证工具结果中提炼候选，并保留完整 provenance。
- 定义 `candidate -> validated -> approved -> active -> deprecated` 生命周期、项目级
  默认作用域、人工激活默认值、版本替代与可回滚规则。
- 定义规范化 Skill 模型，覆盖触发条件、前置条件、步骤、所需能力、权限、安全约束、
  验证方法、失败模式、证据、置信度和适用环境。
- 增加 capability-aware 的 Skill 选择与 adapter 输出边界，使同一规范化制品可按宿主
  能力导出为 Codex、Claude、Hermes 或其他 Agent 可消费的格式；memora 本身不执行
  Skill。
- 增加执行反馈、误触发、失败与环境漂移记录，使 Skill 能降级、隔离、替代或弃用，
  并阻止召回内容和 Skill 输出形成自我强化回声。
- 在候选晋级和导出前执行来源检查、敏感信息扫描、权限声明检查与兼容性验证；未经
  验证的外部内容不得直接成为 active Skill。
- **BREAKING**: none. 所有能力均为未来 MCP API、领域模型和 adapter 的增量契约。

## Capabilities

### New Capabilities

- `procedural-memory-skill-lifecycle`: 定义 Skill 候选生成、规范化模型、证据要求、
  生命周期、作用域晋级、人工审批、版本替代、安全验证与弃用规则。
- `skill-runtime-adaptation-and-feedback`: 定义按客户端能力选择和导出 Skill、运行时
  provenance、显式激活边界、执行反馈、置信度更新、隔离与环境漂移处理。

### Modified Capabilities

None. 本变更消费现有会话、provenance、L1 到 L2 提升和客户端能力协商契约，但不改变
这些能力已经定义的记忆语义。

## Impact

- 未来 Rust domain/application 层将增加 Skill candidate、canonical Skill、版本、验证
  结果和运行反馈模型，以及相应 repository ports。
- SQLite 将增加候选、Skill 版本、证据关联、兼容性、审批和执行反馈表及索引；现有
  memory record 仍是证据来源，不被复制为新的事实层。
- MCP 层将增量增加候选查询、审核、Skill 获取/导出和反馈工具；无 Skill 能力的客户端
  保持现有记忆行为。
- 客户端 adapter 负责把规范化 Skill 渲染为宿主格式和声明可用能力，不能绕过核心
  验证、作用域与 provenance 规则。
- 本变更不引入新的 LLM provider、网络 Skill 市场、自动发布、任意代码执行或跨设备
  同步；实现依赖 Rust foundation、L1 记录模型和 capability profile 先落地。
