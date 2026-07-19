## ADDED Requirements

### Requirement: Skill 运行能力声明采用保守默认值
memora MUST 允许调用方在 Skill 请求中声明 `discovery`、`loading`、`formats`、
`feedback`、`max_skill_tokens` 和可用 tool/capability 标识。客户端省略声明时，系统
MUST 使用 `manual-canonical` 模式，只提供显式规范化 Skill 查询，且 MUST NOT 宣称
自动注入、安装、执行或反馈 Hook 可用。

#### Scenario: 标准 MCP 客户端未声明 Skill 能力
- **WHEN** 客户端调用 Skill 准备接口但没有提供 `skill_runtime_capabilities`
- **THEN** 系统返回 `operation_mode: "manual-canonical"` 和相应 fallback reason，且只返回可手动消费的规范化结果

#### Scenario: 客户端支持按需加载和反馈 Hook
- **WHEN** 客户端声明 `loading: "on_demand_hook"`、受支持格式和 `feedback: "hook"`
- **THEN** 系统按这些能力评估兼容性，并在响应中返回所选格式和可用反馈契约

### Requirement: Skill 准备按任务、作用域、兼容性和预算选择
`skill_prepare` 或等价接口 MUST 接受 task、project、可选 session、正整数 token budget
和运行能力。系统 MUST 只选择 active、scope 可见、trigger 匹配、环境兼容且未被更高
权威内容阻止的 SkillVersion。响应 MUST 返回选择理由、版本、content hash、provenance、
required permissions、估算 token、冲突和不兼容原因。

#### Scenario: 为项目任务选择匹配 Skill
- **WHEN** 客户端为项目中的数据库迁移任务请求 Skill，且存在 trigger 匹配的 active project Skill
- **THEN** 系统返回该版本及选择理由、evidence provenance、权限和兼容性信息

#### Scenario: 候选超过 token budget
- **WHEN** 所有匹配 Skill 的估算 token 总量超过请求预算
- **THEN** 系统按 scope、trigger specificity、authority compatibility 和验证状态选择预算内结果，并返回 truncation reason

#### Scenario: Skill 仍处于 validated 状态
- **WHEN** trigger 匹配的版本尚未 approved 或 active
- **THEN** 默认 skill_prepare 不交付该版本，并返回存在未激活候选的可诊断提示

### Requirement: memora 不执行 Skill 且交付不等于成功
memora MUST NOT 执行 Skill steps、调用其中声明的工具或把 Skill 交付/导出记录为执行
成功。只有客户端或用户通过反馈接口提交 outcome 后，系统才能记录该次执行结果。

#### Scenario: 客户端获取 Skill 后中止任务
- **WHEN** memora 已返回 Skill，但客户端没有执行并提交 `aborted` feedback
- **THEN** 系统记录中止事件，且不得增加该 Skill 的成功次数

#### Scenario: 客户端不支持反馈
- **WHEN** 客户端声明 `feedback: "none"` 并获取 Skill
- **THEN** 系统把结果状态保持为 unknown，而不得根据查询或导出次数推断成功

### Requirement: 自动激活必须显式启用且满足安全条件
Skill activation policy MUST 默认为 `manual`。`suggest` 或 `auto` MUST 由用户针对明确
scope 显式启用。auto activation MUST 只适用于 active、兼容、未冲突、未 quarantine
且 required permissions 在客户端授权范围内的版本；否则系统 MUST 降级到 suggest 或
manual 并返回原因。

#### Scenario: 项目未启用自动激活
- **WHEN** active Skill 匹配任务但项目 activation policy 仍为默认值
- **THEN** 系统只返回手动加载结果，不声明 Skill 已自动生效

#### Scenario: 自动激活遇到权限不足
- **WHEN** 项目启用了 auto，但匹配 Skill 需要客户端未授权的文件写入权限
- **THEN** 系统阻止 auto activation，降级到 manual 或 suggest，并返回 missing permission

### Requirement: Adapter 导出携带可验证 manifest
每次目标格式导出 MUST 以 canonical SkillVersion 为来源，并生成包含 canonical Skill ID、
version、schema version、content hash、target format、adapter version、generated_at 和
`lossy_fields` 的 manifest。adapter MUST NOT 提升 scope、跳过 validation、删除 required
permissions 或把非 active 版本标记为 active。

#### Scenario: 导出为宿主 SKILL.md
- **WHEN** adapter 把 active canonical Skill 渲染为目标 Agent 的 SKILL.md
- **THEN** 导出结果包含可关联到 canonical 版本的 manifest，且 content hash 和 adapter version 可供后续过期检查

#### Scenario: 目标格式无法表达关键安全字段
- **WHEN** 目标格式无法表达 required permissions 或 stop conditions
- **THEN** adapter 拒绝导出并返回不可安全表示的字段，而不得静默丢弃这些字段

#### Scenario: 只丢失非关键展示字段
- **WHEN** 目标格式只无法表达非关键描述元数据
- **THEN** adapter 可以导出，但必须在 manifest 的 `lossy_fields` 和 warning 中列出这些字段

### Requirement: Skill 执行反馈可幂等重试并保持事件历史
`skill_feedback` MUST 接受 execution ID、Skill ID/version、outcome、环境指纹和
`idempotency_key`。outcome MUST 至少支持 `success`、`failure`、`aborted` 和
`false_trigger`。同一 execution ID 和 key 的重试 MUST 返回原反馈事件，且 MUST NOT
重复增加统计。反馈事件 MUST NOT 原地修改 SkillVersion 内容。

#### Scenario: Hook 重试成功反馈
- **WHEN** 客户端因网络中断以相同 execution ID 和 idempotency key 重试 success feedback
- **THEN** 系统返回首次反馈标识，且成功统计只增加一次

#### Scenario: 失败反馈包含验证引用
- **WHEN** 客户端提交 failure outcome 并引用失败的 verification step
- **THEN** 系统保存该反馈、环境指纹和验证引用，同时保持已执行 SkillVersion 的内容不变

### Requirement: 重复失败与误触发能够隔离 Skill
系统 MUST 根据可审计的反馈事件维护派生 confidence state。单次 failure MUST NOT 自动
删除或改写 Skill。相同环境中的重复 failure、重复 false trigger、强制 validation
过期或 required capability 消失 MUST 能将 active 版本转换为 quarantined，并返回明确
原因。quarantined 版本 MUST 不参与 auto activation。

#### Scenario: 单次偶发失败
- **WHEN** active Skill 收到第一次 failure feedback 且没有安全违规
- **THEN** 系统记录失败并更新统计，但默认保持版本 active

#### Scenario: 相同环境重复失败
- **WHEN** active Skill 在相同环境指纹下达到配置的连续失败阈值
- **THEN** 系统将其标记为 quarantined、保存触发事件，并从后续 auto activation 中排除

#### Scenario: 隔离后重新验证
- **WHEN** quarantined Skill 在目标环境获得新的成功 validation 并被用户批准恢复
- **THEN** 系统允许恢复 active，并保留历史失败、隔离和恢复事件

### Requirement: 环境与能力漂移在交付前可见
系统 MUST 比较 SkillVersion 的 environment constraints 和 required capabilities 与当前
运行能力。缺失或不兼容项 MUST 在 skill_prepare 和 export 响应中可见；不兼容版本
MUST NOT 被自动激活。adapter 或客户端版本变化 MUST NOT 静默沿用先前兼容结论。

#### Scenario: 所需工具在新客户端中不存在
- **WHEN** active Skill 需要一个当前客户端未声明的 MCP tool
- **THEN** 系统返回 incompatible 状态和缺失 capability，且不将该版本自动交付

#### Scenario: Adapter 升级后重新评估
- **WHEN** 目标 adapter version 变化且旧导出 manifest 来自先前版本
- **THEN** 系统重新执行格式兼容性检查，并在需要时生成新导出或返回失效原因

### Requirement: Skill 输出不得形成反馈与 evidence 回声
系统 MUST 区分 Skill 交付、Skill 输出、用户确认、工具验证和独立任务结果。Skill 输出
或 memora recall 的重复写入 MUST 使用原始 Skill/record ID 或内容哈希去重，且 MUST NOT
在没有独立 outcome evidence 时增加 success、confidence 或候选 evidence 数量。

#### Scenario: Agent 把 Skill 文本重新写回观察
- **WHEN** Agent 将刚获取的 Skill 内容以 observation 或新 candidate 形式写回 memora
- **THEN** 系统识别原始 Skill/version 并去重，不把该写入计为独立复用成功或新 evidence

#### Scenario: 用户确认任务成功
- **WHEN** Skill 执行后用户明确确认结果成功并关联 execution ID
- **THEN** 系统可以把该确认作为独立 success feedback，同时保留用户来源和执行 provenance

