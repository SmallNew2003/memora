## ADDED Requirements

### Requirement: Skill 作为与记忆层级正交的派生制品
memora MUST 将 Skill 建模为程序性知识制品，并为其保存独立的 scope。系统 MUST NOT
把 Skill 定义为 L5，也 MUST NOT 因创建 Skill candidate 或 Skill 而改变原始 evidence
record 的 L1-L4 scope、authority 或可见性。

#### Scenario: 从会话记录创建项目候选
- **WHEN** Agent 使用一个 L1 session record 创建项目级 Skill candidate
- **THEN** 系统保存 candidate 与原记录的 evidence 关系，且原记录仍保持 session scope 和原有可见性

### Requirement: 规范化 Skill 模型包含可执行前提与安全元数据
每个 SkillVersion MUST 包含名称、intent、kind、scope、triggers、preconditions、steps、
expected outputs、verification、required capabilities、permissions、safety constraints、
failure modes、stop conditions、evidence refs、environment constraints、schema version、
skill version 和 content hash。`kind` MUST 至少支持 `workflow`、`diagnostic` 和
`guardrail`。

#### Scenario: 候选缺少验证方法
- **WHEN** 提交方创建没有 verification 或 expected outputs 的 workflow candidate
- **THEN** 系统保留可诊断的草稿错误或拒绝提交，且不得把该候选标记为 validated

#### Scenario: 失败教训形成防护 Skill
- **WHEN** evidence 描述一次已确认失败及其避免条件
- **THEN** 系统允许创建 diagnostic 或 guardrail candidate，但不得把该 evidence 表示为已验证成功的 workflow

### Requirement: Skill 候选必须具有独立且可审计的 evidence
`skill_propose` 或等价接口 MUST 要求 candidate 提供 evidence refs 和生成原因。evidence
MUST 引用可访问的用户指令、task outcome、checkpoint、tool result、validation run 或
其他有 provenance 的记录。只引用 `memora_recall`、Skill 输出或同一 candidate 派生内容
的候选 MUST NOT 被视为拥有独立 evidence。

#### Scenario: 用户明确要求沉淀流程
- **WHEN** 用户明确要求把当前已完成任务的方法保存为 Skill，且 candidate 引用该用户指令和任务结果
- **THEN** 系统接受 candidate 并在 provenance 中保存用户指令、任务结果和提交 Agent

#### Scenario: Skill 输出尝试证明自身有效
- **WHEN** candidate 的全部 evidence 都来自同一 SkillVersion 的输出或其 memora recall 回填
- **THEN** 系统标记 evidence 不独立，且 candidate 不得晋级为 validated

### Requirement: Skill 生命周期使用受约束的状态迁移
系统 MUST 支持 `candidate`、`validated`、`approved`、`active`、`rejected`、
`quarantined` 和 `deprecated` 状态。新 candidate MUST 默认为项目 scope 和
`candidate` 状态。validation 成功 MUST NOT 自动表示 approved；approval MUST 记录
actor、reason 和时间。未经 approved 的版本 MUST NOT 进入 active。

#### Scenario: 验证通过但尚未审批
- **WHEN** candidate 通过所有强制 validation gates
- **THEN** 系统将其标记为 validated，且在没有审批记录时不得返回 active 状态

#### Scenario: 尝试跳过审批直接激活
- **WHEN** 调用方请求把 candidate 或 validated Skill 直接转换为 active
- **THEN** 系统拒绝非法状态迁移并返回缺失 approval 的原因

#### Scenario: 已隔离版本恢复
- **WHEN** quarantined SkillVersion 获得新的有效 validation 或用户显式恢复决定
- **THEN** 系统允许其恢复为 active，并保留隔离原因和恢复记录

### Requirement: 晋级前执行确定性安全与完整性门禁
candidate 进入 validated 前 MUST 通过 schema 完整性、evidence 可访问性、内容哈希、
敏感信息扫描、权限声明、scope、来源回声和 capability 兼容性检查。可选的 LLM critique
或 replay 结果 MAY 作为附加 validation evidence，但 MUST NOT 替代上述确定性门禁。

#### Scenario: 候选包含未脱敏的凭证
- **WHEN** 敏感信息扫描在 candidate 内容或固定参数中发现 API key、token 或其他凭证
- **THEN** 系统阻止 validated 和 active 状态，并返回需要移除或参数化的验证错误

#### Scenario: 候选使用未声明权限
- **WHEN** Skill steps 要求写文件或调用网络工具，但 permissions 中没有对应声明
- **THEN** 系统阻止晋级并指出缺失的权限声明

#### Scenario: 只有 LLM 自评通过
- **WHEN** candidate 获得语义 critique 的通过结论但确定性 schema 或 evidence gate 失败
- **THEN** 系统保持 candidate 状态并返回确定性 gate 的失败原因

### Requirement: Skill scope 提升必须显式且保留来源
项目 Skill MUST NOT 因在多个 session 被读取而自动成为用户 Skill。项目到用户 scope
的提升 MUST 要求显式操作、提升原因和审批 actor，并保留所有原始 evidence、项目来源
和旧 Skill 标识。跨项目默认检索 MUST NOT 返回未提升的项目 Skill。

#### Scenario: 同一项目多次成功使用
- **WHEN** 一个 project Skill 在同一项目中收到多次 success feedback
- **THEN** 系统可以建议提升，但仍保持 project scope，直到收到显式的用户级提升操作

#### Scenario: 显式提升为用户 Skill
- **WHEN** 用户批准将 project Skill 提升到 user scope 并提供理由
- **THEN** 系统创建用户 scope 的新 Skill 或版本关系，并保留项目 Skill 和 evidence provenance

### Requirement: Active SkillVersion 不可原地修改且可回滚
active SkillVersion 的内容 MUST 不可原地修改。任何步骤、触发器、权限、验证或环境约束
变化 MUST 创建新版本并记录 `supersedes`。系统 MUST 保留旧版本及其状态，使用户能够
把当前版本回滚到仍满足 validation 和 approval 要求的历史版本。

#### Scenario: 修改 active Skill 的步骤
- **WHEN** 用户或 Agent 修改 active Skill 的 steps
- **THEN** 系统创建新的 candidate SkillVersion，旧 active 版本内容和 content hash 保持不变

#### Scenario: 回滚到历史版本
- **WHEN** 当前版本失败且用户选择一个仍有效的已审批历史版本
- **THEN** 系统把该历史版本设为当前 active 版本，并记录回滚 actor、原因和被替代版本

### Requirement: Skill 不得覆盖更高权威的当前要求
当前用户指令和版本化项目规格 MUST 高于 Skill 的程序性建议。系统检测到 Skill 的
precondition、step 或 constraint 与更高权威记录冲突时，MUST 返回 conflict state，
且 MUST NOT 允许该版本自动激活或静默覆盖高权威内容。

#### Scenario: 旧 Skill 与 OpenSpec 冲突
- **WHEN** project Skill 要求使用已被当前 OpenSpec design 禁止的技术方案
- **THEN** 系统返回该 Skill 与项目规格的冲突及双方 provenance，并排除 auto activation

### Requirement: 外部 Skill 导入默认不可信
从文件、仓库、Skill 市场或其他 Agent 导入的 Skill MUST 以 untrusted candidate 创建，
并记录来源 URI 或文件 hash。外部 Skill MUST 经过与本地 candidate 相同的 validation、
approval 和 activation 流程，且 MUST NOT 因来源受欢迎或已签名而直接 active。

#### Scenario: 导入外部 SKILL.md
- **WHEN** 用户导入一个此前不在 memora 一致性域内的 SKILL.md
- **THEN** 系统创建带外部来源的 untrusted candidate，并在验证和审批完成前禁止运行时交付

