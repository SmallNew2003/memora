## 1. 前置条件与契约定稿

- [ ] 1.1 确认 Rust foundation、L1 session/record、provenance、L1 到 L2 提升和客户端 capability profile 已实现；缺失部分继续由各自变更交付，不在本变更重复建设
- [ ] 1.2 为 canonical Skill schema 选择首版版本策略，记录 Agent Skills 兼容范围和不能安全映射的字段
- [ ] 1.3 选择首个目标 adapter 和默认 quarantine 阈值，并把尚未支持的 Agent 格式标记为显式 out of scope
- [ ] 1.4 定义 Skill MCP tool 名称、错误码、operation mode 和 feature flag，确保禁用 Skill 功能时现有记忆工具契约不变

## 2. Domain 模型与状态机

- [ ] 2.1 定义 `SkillCandidate`、`Skill`、不可变 `SkillVersion`、`SkillEvidence`、`SkillValidation`、`SkillApproval`、`SkillExport` 和 `SkillExecutionFeedback` 领域类型
- [ ] 2.2 定义 workflow、diagnostic、guardrail kind 以及 project/user scope，禁止通过类型转换把 Skill 建模为 L5
- [ ] 2.3 为 canonical SkillVersion 实现必填字段、schema version、skill version、content hash 和 environment constraints 校验
- [ ] 2.4 实现 candidate、validated、approved、active、rejected、quarantined、deprecated 状态机及非法跳转错误
- [ ] 2.5 实现 active 版本不可变、新版本 `supersedes`、当前版本切换和满足验证要求的历史版本回滚
- [ ] 2.6 定义 Skill repository ports 与 application command/query 类型，保持 domain/application 不依赖 SQLite、RMCP 或具体 Agent 格式

## 3. SQLite 迁移与持久化

- [ ] 3.1 增加 skill_candidates、skills、skill_versions、skill_evidence、skill_validations、skill_approvals、skill_exports 和 skill_feedback 的增量迁移
- [ ] 3.2 为 Skill 名称/scope、状态、当前版本、content hash、trigger 查询、evidence record、execution ID 和 idempotency key 建立唯一约束与索引
- [ ] 3.3 实现 Skill 聚合、不可变版本、evidence、validation、approval、export 和 feedback repository adapter
- [ ] 3.4 保证删除或归档 session 不会破坏 Skill evidence 审计关系，并为不可访问 evidence 返回明确状态
- [ ] 3.5 测试迁移重复执行、校验和漂移、旧数据库升级、事务回滚和并发写入幂等性

## 4. Candidate 提交与生命周期用例

- [ ] 4.1 实现 `skill_propose` application use case，要求生成原因、结构化草案和可访问 evidence refs
- [ ] 4.2 校验候选至少来自用户明确指令、已验证任务结果或多个独立记录之一，并区分成功 workflow 与失败 diagnostic/guardrail
- [ ] 4.3 对 memora recall、Skill 输出和同源派生候选按原始 ID/content hash 去重，阻止它们单独构成独立 evidence
- [ ] 4.4 实现候选列表、详情、拒绝和重新提交路径，并在每次状态变化中保存 actor、reason 和时间
- [ ] 4.5 实现 validation 完成、显式 approval、manual activation、quarantine、恢复和 deprecation 用例
- [ ] 4.6 实现 project 默认 scope、跨 session 可见性和 project 到 user 的显式提升，保留原 Skill/evidence provenance
- [ ] 4.7 实现外部 Skill 导入为 untrusted candidate，保存来源 URI 或文件 hash，禁止绕过本地验证和审批

## 5. 确定性验证与安全门禁

- [ ] 5.1 实现 schema 完整性、evidence 可访问性、content hash、scope 和来源回声验证器
- [ ] 5.2 实现本地敏感信息扫描，使凭证和固定秘密在参数化或移除前不能进入 validated/active
- [ ] 5.3 实现 required permissions 与 steps/tool usage 的一致性检查，并返回缺失或过宽权限
- [ ] 5.4 实现 required capabilities、environment constraints 和目标 runtime capability 的兼容性检查
- [ ] 5.5 定义可选 semantic critique 与外部 validation run 的输入契约，确保它们只增加 evidence 而不能覆盖确定性失败
- [ ] 5.6 实现当前用户指令、OpenSpec 项目规格与 Skill constraint 的冲突检查，并阻止冲突版本自动激活
- [ ] 5.7 为每个 validation gate 添加通过、失败和可诊断错误测试，包括提示注入式外部 Skill 与未声明权限场景

## 6. Skill 选择、预算与激活策略

- [ ] 6.1 定义 `SkillRuntimeCapabilities`、`manual-canonical` 默认模式和能力缺失 fallback reason
- [ ] 6.2 实现 `skill_prepare`，按 task、project、session、scope、trigger specificity、状态、authority compatibility 和环境兼容性筛选
- [ ] 6.3 实现 Skill token 估算、预算排序、截断原因和未激活候选的可诊断提示
- [ ] 6.4 在 prepare 响应中返回 Skill/version/hash、选择理由、provenance、permissions、冲突、不兼容项和 operation mode
- [ ] 6.5 实现 manual 默认、scope 级 suggest/auto opt-in，以及权限不足、冲突、quarantine 或能力缺失时的确定性降级
- [ ] 6.6 确保 Skill 交付、查询和导出都不会调用 steps 中的工具，也不会产生隐式 success feedback

## 7. Adapter 与可移植导出

- [ ] 7.1 定义 canonical-to-target adapter port 和包含 Skill/version/schema/hash/target/adapter version/generated_at/lossy fields 的 export manifest
- [ ] 7.2 实现首个文件型 adapter，将 active canonical Skill 渲染为选定目标格式并保存 export 记录
- [ ] 7.3 对无法表达 required permissions、stop conditions 等关键安全字段的目标格式拒绝导出
- [ ] 7.4 对非关键有损字段生成 manifest warning，并验证 adapter 不提升 scope、不跳过 validation、不改变 active 状态
- [ ] 7.5 实现 canonical/adapter version 变化后的过期检测和重新导出路径，不自动删除用户已有导出文件
- [ ] 7.6 添加 adapter golden tests，验证输出稳定、manifest 可追溯、content hash 正确及危险有损转换被拒绝

## 8. Feedback、置信度与环境漂移

- [ ] 8.1 实现 `skill_feedback`，接受 execution ID、Skill/version、success/failure/aborted/false_trigger、环境指纹、验证引用和 idempotency key
- [ ] 8.2 为相同 execution ID/key 实现幂等返回，并确保查询、导出或 Hook 重试不会重复增加统计
- [ ] 8.3 实现基于事件的派生统计与 confidence state，保证单次 failure 不删除或改写 SkillVersion
- [ ] 8.4 实现相同环境重复失败、重复 false trigger、validation 过期和 required capability 消失触发 quarantine
- [ ] 8.5 实现 quarantine 后通过新 validation 或用户决定恢复 active 的路径，并保留失败、隔离和恢复审计事件
- [ ] 8.6 在 adapter/runtime capability 变化时重新评估兼容性，排除不兼容版本的 auto activation
- [ ] 8.7 测试无 feedback 客户端保持 unknown、用户确认形成独立 feedback、Skill 输出回填不增加 confidence

## 9. MCP API、集成测试与发布门禁

- [ ] 9.1 在 RMCP adapter 注册候选提交/查询、review/activation、skill_prepare、export 和 feedback 工具，并只调用 application ports
- [ ] 9.2 为所有响应统一返回 operation mode、provenance、conflict/incompatibility 状态和可操作错误，禁止泄漏秘密或本地绝对路径
- [ ] 9.3 添加从 L1 evidence 到 project Skill candidate、validation、approval、prepare、adapter export 和 feedback 的端到端 MCP 测试
- [ ] 9.4 添加跨 Agent 能力差异测试，覆盖无 Skill 能力、手动 canonical、按需 Hook、无 feedback 和格式不兼容客户端
- [ ] 9.5 添加 scope 隔离、项目到用户提升、OpenSpec 冲突、版本回滚、外部导入、回声去重和 quarantine 恢复测试
- [ ] 9.6 运行格式化、clippy、Rust 单元/集成/MCP contract tests、SQLite migration tests 和 OpenSpec strict validation
- [ ] 9.7 默认关闭 auto activation，并记录首个 adapter 的真实兼容性、误触发、成功率和回滚结果，后续通过独立变更评审默认策略
