## Context

memora 的四层模型目前按作用域和生命周期组织记忆：L1 为会话、L2 为项目、L3 为
用户、L4 为全局知识。现有设计已经定义 observation、summary、checkpoint、handoff、
provenance、authority、冲突可见性和 L1 到 L2 的显式提升，但没有表示“如何重复完成
一类任务”的程序性制品。

Hermes 等 Agent 把这类制品称为 Skill 或 procedural memory。这个方向能减少重复推导，
但也把一次任务中的偶然路径转化为长期行为，因此其风险高于普通记忆写入：错误触发、
权限扩大、环境漂移、秘密泄漏和提示注入都可能被持久化并在未来自动执行。

本变更依赖 `bootstrap-rust-core`、未来 L1 session/record 实现以及
`add-agent-memory-capability-profiles` 中的来源、隔离和能力协商契约。OpenSpec 制品继续
由 `bridge-openspec-as-l2-source` 定义为项目设计意图的高权威来源。当前仓库仍处于
foundation 实现前期，因此本变更先固化领域和协议契约，不提前引入 LLM provider 或
特定 Agent SDK。

约束：
- Local-First、单二进制和 stdio MCP 的基础目标不变。
- memora 是记忆与 Skill 治理服务，不是任务执行器；不得在核心中运行任意 Skill 代码。
- 不假定任意客户端具备 Hook、原生 Skill 目录、自动注入或结果反馈能力。
- Skill 的生成可以由当前 Agent、未来本地模型或可选 provider 完成，但核心正确性不能
  依赖某个特定 LLM。

## Goals / Non-Goals

**Goals:**
- 把程序性知识建模为可审计、可验证、可版本化的派生制品，而不破坏 L1-L4 作用域。
- 让成功经验、失败教训和用户明确指令能形成候选，但不能未经验证直接成为长期行为。
- 提供项目级默认、显式审批、能力匹配、可回滚和反馈降级的安全生命周期。
- 用规范化模型支持跨 Agent 选择与导出，并保留来源、版本和有损转换信息。
- 让 Skill 的复用效果可以被度量，同时阻止输出回填形成自我强化回声。

**Non-Goals:**
- 不在本变更中内置 LLM 推理、embedding、自动轨迹聚类或自动测试环境。
- 不让 memora 直接执行 shell、浏览器、MCP tool 或 Skill 内脚本。
- 不建设在线 Skill 市场、社区评分、跨设备同步或远程发布服务。
- 不把所有会话摘要都转换为 Skill，也不保证一次成功任务具有可泛化性。
- 不读取、修改或同步宿主已有的私有 Skill/原生记忆；adapter 只处理明确导出目标。

## Decisions

### D1：Skill 是正交的知识形态，不是 L5

**选择**：L1-L4 继续表示 scope；记录和派生制品增加知识形态区分。会话中的执行轨迹
可以产生 `skill_candidate`，批准后的 Skill 可以具有 `project` 或 `user` scope；未来
全局共享仍沿用 L4 边界。Skill 不成为新的记忆层，也不自动改变原始证据的 scope。

```
                 episodic          semantic             procedural
session (L1)     observations      summary              skill candidate
project (L2)     decision history  spec/fact            project skill
user (L3)        usage pattern     preference           personal skill
global (L4)      aggregated cases  wiki/reference       imported/shared skill
```

**理由**：作用域和知识形态是两个维度。把 Skill 放入 L5 会让项目 Skill 和个人 Skill
无法表达，也会混淆“保存多久”与“如何使用”。

**替代方案考虑**：
- *增加 L5 Skill 层*：命名直观，但无法自然表达同一流程的项目版与用户版。
- *把 Skill 当普通 L2 Markdown*：存储简单，却缺少状态机、验证、兼容性和执行反馈。

### D2：规范化 Skill 与记忆记录分表建模，版本不可变

**选择**：核心领域包含 `SkillCandidate`、`Skill`、`SkillVersion`、`SkillEvidence`、
`SkillValidation`、`SkillApproval` 和 `SkillExecutionFeedback`。激活后的版本内容不可原地
修改；修订创建新版本并以 `supersedes` 指向旧版本。原始 memory record 只通过 evidence
关系被引用，不复制为 Skill 的新事实来源。

规范化版本至少包含：
- `name`、`intent`、`kind`（`workflow|diagnostic|guardrail`）和 `scope`
- 精确 triggers、preconditions、inputs、steps、expected outputs 和 verification
- required capabilities、permissions、safety constraints、failure modes 和 stop conditions
- evidence refs、authority constraints、environment constraints 和 content hash
- schema version、skill version、confidence state、created/approved identity 与时间

**理由**：Skill 是会影响未来行为的版本化制品，其审计和回滚要求高于普通摘要。
不可变版本能重现“某次执行究竟加载了什么”。

**替代方案考虑**：
- *直接覆盖 SKILL.md*：符合文件心智，但无法可靠关联历史执行与旧内容。
- *把全部字段塞进 memory record JSON*：减少表数量，却把检索记录和行为制品耦合。

### D3：候选生产与核心治理分离

**选择**：核心提供 `SkillCandidateProducer` 输入边界，接受由当前 Agent、用户、未来本地
模型或可选 provider 生成的候选草案。初始 MCP 路径允许 Agent 通过 `skill_propose`
提交结构化候选和 evidence refs；核心负责校验、存储与状态迁移，不负责调用 LLM。

候选必须满足以下至少一个入口条件：用户明确要求沉淀；或提交方提供一个已验证成功的
任务结果；或提供多个独立记录支持重复模式。失败记录可以生成 `diagnostic` 或
`guardrail` 候选，但不得伪装成已成功的 workflow。

**理由**：memora 当前没有确定的 LLM 方案，且不同宿主已具备推理能力。将提炼器设为
port 可以先交付治理价值，也保留未来本地自动提炼的空间。

**替代方案考虑**：
- *memora 内置唯一 LLM compiler*：自动化高，但破坏零网络和 provider 中立性。
- *只允许人工填写 Skill*：安全但无法利用 Agent 已完成任务时的上下文。

### D4：生命周期默认人工审批和项目作用域

**选择**：主状态为 `candidate`、`validated`、`approved`、`active`、`deprecated`，另有
`rejected` 和 `quarantined` 终止/暂停状态。新候选默认 `project` scope，且 validation
完成只进入 `validated`，不得自动批准。初始 activation policy 固定为 `manual` 或
`suggest`；`auto` 必须由用户针对具体 scope 显式启用，并且只适用于 active、兼容、
未冲突的版本。

允许的关键迁移为：

```
candidate -> validated -> approved -> active -> deprecated
    |             |            |         |
    +-> rejected  +-> rejected +---------+-> quarantined
                                      quarantined -> active|deprecated
```

**理由**：候选生成和长期激活是不同信任边界。项目级默认限制错误泛化的影响范围，
人工审批让用户能看到将被持久化的行为变化。

**替代方案考虑**：
- *成功一次即自动激活*：体验顺滑，但会永久化偶然路径和任务特有参数。
- *所有 Skill 只允许手工调用*：安全，但长期无法验证建议式和自动式复用价值。

### D5：验证采用确定性门禁加可选语义验证

**选择**：从 candidate 晋级到 validated 必须通过确定性门禁：schema 完整、evidence
存在且可访问、内容哈希稳定、敏感信息扫描、权限声明、禁止的来源回声检查、作用域
检查和目标 capability 兼容性检查。语义 critique、沙箱 replay 或用户验收可作为附加
validation evidence，但单个 LLM 的“看起来正确”不得替代确定性门禁。

核心不执行 replay。具备执行能力的客户端或测试 adapter 可以返回签名为
`validation_run` 的结果，包含 Skill 版本、环境指纹、验证方法和 outcome。

**理由**：大部分高风险问题可以确定性检测；流程是否真正有效则需要宿主环境或用户
参与。分层验证保持核心 Local-First，同时避免伪造执行能力。

### D6：Skill 运行能力使用独立的可选能力扩展

**选择**：Skill 请求可以携带 `skill_runtime_capabilities`，至少描述
`discovery`（`manual|query`）、`loading`（`manual|startup_hook|on_demand_hook`）、
`formats`、`feedback`（`none|manual|hook`）、`max_skill_tokens` 和可用 tool/capability
标识。缺失时使用 `manual-canonical` 保守模式，只允许显式查询规范化 Skill，不承诺
自动注入、安装或反馈。

该对象是现有 `client_capabilities` 的增量扩展，不改变 session、record、scope 和
authority 语义。未来现有 capability profile 归档后，可通过独立变更合并字段。

**理由**：客户端是否支持 Skill 与是否支持会话 Hook 是相关但不同的问题；独立扩展
避免在当前 active change 之间建立不稳定的修改依赖。

### D7：memora 只选择与交付 Skill，不执行 Skill

**选择**：`skill_prepare` 接受 task、project、可选 session、token budget 和 runtime
capabilities，按 scope、trigger、authority、状态、环境兼容性和预算返回零到多个候选。
响应包含选择理由、版本、provenance、required permissions、冲突和不兼容原因。

memora 不调用 Skill 步骤中的工具，也不把“已交付”记录为“执行成功”。只有客户端或
用户提交 feedback 后，系统才记录结果。

**理由**：执行会引入授权、sandbox、tool 生命周期和宿主上下文问题，超出记忆引擎
边界。选择与执行分离也允许同一 Skill 被不同 Agent 使用。

### D8：adapter 输出必须可追溯且声明有损转换

**选择**：canonical Skill 是唯一一致性来源。adapter 负责渲染 Codex/Claude/Hermes
等目标格式，导出结果必须附带 manifest：canonical Skill ID、version、schema version、
content hash、target format、adapter version、generated_at 和 `lossy_fields`。

adapter 不得自行提升 scope、跳过 validation、删除 required permissions 或把未激活
候选标记为 active。目标格式无法表达的安全字段必须生成 warning；关键安全字段无法
表达时拒绝导出。

**理由**：不同 Skill 格式的字段和触发语义不一致。显式 manifest 让用户能够判断
导出内容是否过期及转换丢失了什么。

### D9：当前用户与项目规格始终高于 Skill

**选择**：Skill 在运行时属于经批准的程序性建议，其 authority 低于当前用户指令和
版本化项目规格。`skill_prepare` 检测到相同约束键冲突时必须返回冲突并禁止 auto
activation；不得让旧 Skill 静默覆盖 OpenSpec 或当前任务要求。

**理由**：Skill 可能随环境过期。复用历史方法不能改变现有 provenance 设计的权威
顺序。

### D10：反馈是幂等事件，不原地改写 Skill

**选择**：`skill_feedback` 接受 `execution_id`、Skill ID/version、outcome
（`success|failure|aborted|false_trigger`）、环境指纹、可选验证引用和
`idempotency_key`。相同执行和 key 只记录一次。反馈更新派生统计与 confidence state，
但不得修改 SkillVersion 内容。

重复 false trigger、相同环境中的重复失败、required capability 消失或验证过期可以把
active 版本转为 quarantined；恢复必须有新的 validation 或显式用户决定。单次失败
默认只记录事件，不自动删除 Skill。

**理由**：结果反馈存在 Hook 重试和噪声。事件化、幂等和可审计的降级优于不透明地
重写提示文本或用一次失败永久删除流程。

### D11：阻止 Skill 自我强化回声和不可信导入

**选择**：由 Skill 输出、memora recall 或 adapter 渲染结果产生的新候选，必须携带
原始 Skill/record 引用并按内容哈希去重，不能仅凭自身输出作为独立 evidence。外部
Skill 文件和市场内容一律以 untrusted candidate 导入，必须经过同样的扫描、验证和
审批流程后才能 active。

**理由**：否则一条错误 Skill 会通过重复执行制造“多次成功证据”，或者外部提示注入
直接进入长期自动行为。

### D12：SQLite 使用增量表与 repository ports

**选择**：未来实现以增量迁移增加 `skill_candidates`、`skills`、`skill_versions`、
`skill_evidence`、`skill_validations`、`skill_approvals`、`skill_exports` 和
`skill_feedback`。domain/application 通过 repository ports 使用这些模型；MCP adapter
不得直接编排 SQL。内容正文与 hash 存于版本表，状态和当前版本指针存于 Skill 聚合。

**理由**：这与 modular-monolith foundation 一致，并能对 evidence、版本和 feedback
分别建立唯一约束与查询索引。

## Risks / Trade-offs

- **[风险] 偶然成功被错误泛化。** → 默认项目级、人工审批、证据入口条件和独立验证；
  自动激活必须显式 opt-in。
- **[风险] Skill 数量膨胀并出现重叠触发。** → 以 intent/trigger/content hash 检测近似
  候选，返回冲突而不自动合并；通过 supersedes 和弃用维持单一推荐版本。
- **[风险] Skill 固化敏感信息或提示注入。** → 候选前后执行敏感信息扫描、来源标记、
  权限检查；外部内容只能以 untrusted candidate 导入。
- **[风险] 目标 Agent 格式丢失安全语义。** → adapter manifest 声明 lossy fields，
  关键字段不可表达时拒绝导出。
- **[风险] 环境升级导致步骤失效。** → 保存 capability 和 environment constraints，
  反馈中记录环境指纹，兼容性变化时 quarantine 而非继续自动加载。
- **[风险] 客户端不提交反馈。** → 将成功率标记为 unknown，不把“被查询/被导出”当作
  成功证据；保留用户手动反馈路径。
- **[权衡] 核心不执行 replay，验证自动化有限。** → 维持清晰授权边界；通过可选测试
  adapter 接收可审计的 validation run。
- **[权衡] canonical model 比单个 SKILL.md 复杂。** → 换取跨 Agent 可移植性、版本审计
  和安全治理；adapter 对用户隐藏大部分结构。

## Migration Plan

1. 先完成 Rust foundation、L1 record/provenance 和客户端 capability profile；未满足时
   Skill tasks 保持阻塞，不复制这些基础模型。
2. 以增量 SQLite migration 和 domain ports 引入 candidate、canonical Skill、版本、
   validation 与 feedback 模型，默认关闭自动选择和导出。
3. 交付手动 `skill_propose`、候选列表、validation 和 review 路径，只允许项目级
   approved/active Skill，不提供 auto activation。
4. 增加 `skill_prepare` 的 canonical 输出、兼容性解释和 token budget；验证无 Skill
   能力客户端仍保持现有记忆行为。
5. 实现首个文件型 adapter 与 manifest，再按实际目标客户端验证 Codex/Claude/Hermes
   格式，不在同一批次承诺全部宿主。
6. 增加幂等 feedback、统计和 quarantine；收集误触发与失败数据后，再单独评审
   `suggest` 或 `auto` activation 的默认策略。

回滚时关闭 Skill MCP tools/adapter 和自动选择开关，保留增量表及已存证据以便兼容
版本恢复。已导出的文件不自动删除；系统将对应版本标记为 inactive/deprecated，并在
后续查询中返回失效状态。数据库迁移不回退或丢弃用户制品。

## Open Questions

1. canonical Skill schema 首版是否直接采用 Agent Skills 开放格式的超集，还是定义
   memora 自有 JSON schema 后由 adapter 映射？
2. 首个 adapter 应优先验证 Codex `SKILL.md`、Claude Code Skill，还是纯文件系统
   canonical Markdown？
3. 自动候选至少需要“两次独立成功”，还是允许一次强验证结果直接进入 validated？
4. confidence/quarantine 的阈值是固定默认值，还是必须按项目配置？
5. L3 用户 Skill 的晋级是否要求跨两个项目的 evidence，还是用户明确批准即可覆盖？
