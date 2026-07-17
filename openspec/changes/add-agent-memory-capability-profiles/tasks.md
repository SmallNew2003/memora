## 1. 前置条件与数据模型

- [ ] 1.1 确认 Cargo workspace 与 Phase 1 的 session/observation 基础存储已落地；未满足时先完成其独立变更，不在本变更中重复初始化项目
- [ ] 1.2 定义并校验 `ClientCapabilities` 与 `OperationMode` 类型，为缺失能力声明实现 `stateless-manual` 保守默认值
- [ ] 1.3 为 session 持久化 `agent_id`、可选 `external_session_ref`、客户端能力快照、运行模式、活跃时间和归档状态
- [ ] 1.4 为记忆记录持久化 scope、kind、origin、project、内容哈希、authority、来源引用、过期与替代关系元数据，并为查询字段建立索引
- [ ] 1.5 设计并实施兼容迁移，确保已有 session 与 observation 在新字段缺失时可按保守默认值读取

## 2. MCP 会话与能力协商

- [ ] 2.1 扩展 `session_start` 以接受可选 client capabilities 和 external session ref，并在响应中返回 operation mode 及 fallback reason
- [ ] 2.2 实现按能力组合选择 `native-opaque`、`stateless-hooked` 与 `stateless-manual` 的纯逻辑，禁止依赖客户端产品名称
- [ ] 2.3 为 `observe` 实现 idempotency key 验证与重复调用返回既有记录的行为
- [ ] 2.4 实现相同项目与 external session ref 的会话恢复，并为无法恢复的长期未活动会话实现归档路径
- [ ] 2.5 为所有自动化能力缺失的响应统一返回 operation mode 和可识别的 fallback reason

## 3. 上下文连续性与 handoff

- [ ] 3.1 实现 `prepare_context`，按 project、task 和 token budget 检索并返回带 provenance 的有预算上下文包
- [ ] 3.2 实现 authority 排序、token 估算和截断原因，确保完整 L1 历史不会作为默认启动上下文返回
- [ ] 3.3 实现带任务、状态、决策、阻塞项、下一步、文件和过期时间的 `checkpoint` handoff 模型
- [ ] 3.4 为 `checkpoint` 实现幂等写入，并让未过期的 `in_progress`/`blocked` handoff 可被后续上下文恢复
- [ ] 3.5 确保 `completed` handoff 不进入默认恢复上下文，并为归档或过期 handoff 提供显式检索路径

## 4. 隔离、提升与冲突可见性

- [ ] 4.1 在检索与上下文准备中默认按 session 隔离 L1，禁止仅因 project 相同而跨 Agent 自动共享 L1 观察
- [ ] 4.2 实现 `promote`，要求提升原因并保留原始 L1 record、agent 和 source refs 作为 L2 provenance
- [ ] 4.3 对 `memora_recall` 来源通过原始 record ID 或内容哈希去重，阻止召回内容形成记忆回声
- [ ] 4.4 实现相同 fact key 不同值的确定性冲突标记，并在响应中返回 authority、origin、conflict state 与 provenance
- [ ] 4.5 将版本化项目规格置于会话观察之前，但保持低权威冲突记录可审计且不被静默删除

## 5. Adapter 边界与验证

- [ ] 5.1 定义可选 client adapter 接口或配置生成边界，使 Hook、包装器和项目指令模板不改变核心记忆语义
- [ ] 5.2 为没有 Hook 的客户端提供可执行的手动启动、上下文准备、检查点和结束调用说明，不承诺自动捕获
- [ ] 5.3 测试无能力声明、无原生记忆带 Hook、无原生记忆手动模式和原生记忆不透明模式的能力协商与降级响应
- [ ] 5.4 测试无状态客户端的新会话恢复、检查点重试、超预算截断、陈旧会话归档和 completed handoff 排除
- [ ] 5.5 测试跨 Agent L1 隔离、显式 L1 到 L2 提升、召回回声去重及 OpenSpec/会话观察冲突排序
- [ ] 5.6 运行 Rust 单元测试、集成 MCP 测试和 OpenSpec 严格校验，并记录首个目标客户端 adapter 的实际能力验证结果
