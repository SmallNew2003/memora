## Context

memora 是面向 Claude Code、Codex、Cursor、OpenCode 等多个 Agent 的本地 MCP
记忆服务。宿主的内建记忆能力并不一致：有的客户端维护私有且不可读写的记忆，有的
客户端完全无跨会话记忆，有的客户端还不能提供会话或工具调用 Hook。MCP 服务不能
可靠地从客户端名称推断这些能力，也不能读取或同步宿主的私有记忆。

现有总设计将 L1 定义为会话级记录，将 L2 定义为项目级记忆；正在进行的
`bridge-openspec-as-l2-source` 变更只处理 OpenSpec 到 L2 的知识桥接。本变更
定义所有客户端共享的会话协作契约，不修改该桥接的来源范围或 OpenSpec 权威性。

约束：
- 本变更只产出设计与规格，不实装 Rust、MCP adapter 或客户端配置。
- Local-First 与单二进制目标不变；客户端 adapter 必须是可选层。
- 不能承诺纯 MCP 客户端能够自动触发开始、检查点和结束事件。

## Goals / Non-Goals

**Goals:**
- 让无原生记忆客户端能通过 memora 恢复项目上下文和未完成工作。
- 让有原生记忆客户端把 memora 作为可审计的外部记忆，而不是同步副本。
- 让 L1 默认隔离、L2 显式共享，并能解释每条结果的来源和权威性。
- 在 Host 没有 Hook 时提供完整的手动降级路径，而不虚构自动捕获能力。
- 为未来 Rust 数据模型和 MCP API 固化兼容、去重与恢复约束。

**Non-Goals:**
- 读取、写入、同步或替代任何 Agent 的原生记忆。
- 在本变更中实现特定客户端（包括 OpenCode）的 Hook、插件或包装器。
- 自动判断任意自然语言记录之间的语义矛盾。
- 把完整的 L1 历史自动注入每一个新会话。
- 修改 OpenSpec L2 桥接的读取接口、索引范围或权威来源。

## Decisions

### D1：以能力声明选择运行模式，而非识别 Agent 名称

**选择**：`session_start` 接受可选的 `client_capabilities`，其字段为：
`native_memory`（`absent` 或 `opaque`）、`session_lifecycle`（`manual` 或 `hook`）、
`tool_capture`（`none`、`manual` 或 `hook`）、`context_injection`（`manual` 或
`startup_hook`）和 `max_context_tokens`。服务端从字段组合计算运行模式并在响应中
返回该模式和可用降级原因。

**理由**：同一产品的版本、部署配置和插件可能具有不同能力；按名称分支会失效，且
无法覆盖新的客户端。遗漏声明时采用最保守的 `stateless-manual` 模式，确保服务端
不会假定 Hook 存在。

**替代方案考虑**：
- *Agent 名称白名单*：接入快，但版本和配置变化会造成错误承诺。
- *统一要求 Hook*：会直接排除只有标准 MCP 的客户端。

### D2：运行模式仅改变自动化程度，不改变记忆语义

**选择**：初始版本支持 `native-opaque`、`stateless-hooked`、`stateless-manual`
三个模式。三者使用同一 session、record、scope 和 authority 模型；差异只在谁调用
启动、观察、检查点和结束工具。

**理由**：无原生记忆不应创建第二套存储层。统一模型让同一项目可被不同 Agent
交接，同时保持 L1 默认私有、L2 显式共享。

**替代方案考虑**：
- *为无记忆客户端建立独立“全局缓存”*：会模糊 L1/L2 边界并造成无法解释的泄漏。
- *仅支持有 Hook 的无状态客户端*：丢失最通用的 MCP 兼容性。

### D3：原生记忆保持在 memora 的一致性域之外

**选择**：`native_memory=opaque` 仅表示宿主可能另有记忆；memora 不导入、不导出、
不比较原生记忆内容。所有 memora 响应都含自身记录的 provenance，而不声称覆盖
宿主上下文。

**理由**：原生记忆通常不可见、不可版本化且因 Agent 而异。双向同步既不可验证，
也会放大重复和隐私风险。

### D4：无状态恢复使用有预算的上下文包和显式 handoff

**选择**：增加 `prepare_context` 和 `checkpoint` 的概念接口。`prepare_context`
在调用方提供 task 与 token budget 后，按 authority 返回 L2 项目事实、未过期的
未完成 handoff，以及与任务相关的少量 L1 记录；每条结果带 provenance。`checkpoint`
写入结构化 handoff（任务、状态、文件、决策、阻塞项、下一步、到期时间）。

**理由**：无状态客户端必须恢复工作，但全量回放 L1 会耗尽上下文并混入无关内容。
handoff 是跨会话连续性的最小载体，不等同于把整个 L1 提升为项目记忆。

**替代方案考虑**：
- *每次启动注入完整历史*：成本不可控，且会让过期观察看似仍有效。
- *只依赖 session_end 摘要*：中断、崩溃和长期任务无法可靠交接。

### D5：记录以来源、作用域和显式提升建立边界

**选择**：每条记录持久化 `scope`、`kind`、`origin`、`agent_id`、`project_id`、
可选 `external_session_ref`、`source_refs`、`content_hash`、`authority`、
`expires_at` 与可选 `supersedes`。L1 默认仅对所属 session 可见；只有调用
`promote` 并给出原因时，记录才能成为 L2 项目记忆。

**理由**：来源让跨 Agent 结果可审计；显式提升避免把未经确认的工具输出和思考过程
变成项目事实。

### D6：检查点和写入必须可重试，异常会话可恢复

**选择**：`observe` 和 `checkpoint` 接受调用方生成的 `idempotency_key`；同一会话
内相同 key 只写入一次。对于 `origin=memora_recall` 的内容，服务端还以记录 ID 或
内容哈希去重，防止召回内容被再次记住。相同 `external_session_ref` 的重新启动恢复
尚未结束的 session；无法映射的长期未活动 session 进入已归档可检索状态。

**理由**：Hook 重试和客户端崩溃是常态。幂等写入与恢复优于要求客户端准确执行一次
`session_end`。

### D7：权威顺序可见，冲突不静默解决

**选择**：响应返回 `authority`、`origin` 和 `conflict_state`。初始版本只对相同
`fact_key` 的不同值做确定性冲突标记；排序优先级为当前用户指令、版本化项目规格、
可验证工具事实、Agent 摘要、旧 L1 观察、召回回填内容。OpenSpec 的 L2 权威性继续
由已有桥接规格定义。

**理由**：在无 LLM 推理的 Phase 中，确定性冲突可测试；保留冲突比不透明地选择一条
更安全。

## Risks / Trade-offs

- **[风险] 客户端错误声明能力或 Hook 失效。** → 服务端返回 operation mode 和
  `fallback_reason`；所有自动路径都保留可手动调用的等价工具。
- **[风险] stateless-manual 模式的 Agent 忘记创建检查点。** → 提供启动上下文、
  检查点和结束工具的同等手动路径；未来 adapter 可生成客户端指令或包装器。
- **[风险] 恢复上下文仍超出宿主窗口。** → 强制 token budget、返回估算值和截断原因；
  L1 只经任务相关检索和 handoff 进入上下文包。
- **[风险] 自动捕获工具输出保存敏感信息。** → 捕获模式和来源必须可见；未来实现应在
  写入边界加入敏感度标签与可配置的脱敏策略，不把原始工具输出无条件提升到 L2。
- **[权衡] 外部 session 标识会带来可关联性。** → 仅作为可选的本地 opaque ref 使用，
  不把它作为跨项目或跨用户的身份键。

## Migration Plan

1. 本变更只新增 proposal、design、specs 和 tasks，不影响当前运行时。
2. Phase 1 实装时，先建立统一记录字段和现有工具的可选 capability 参数；未提供新
   参数的客户端得到 `stateless-manual` 行为。
3. 后续新增 `prepare_context`、`checkpoint` 和 `promote` 工具，不重命名已有工具。
4. 在确认目标客户端的实际集成点后，单独提出 adapter 变更；adapter 不能改变核心
   语义或绕过 provenance 规则。

**回滚策略**：部署出现兼容问题时，关闭自动 adapter/Hook 路径并保持手动 MCP 工具；
本次文档变更若被否决，删除该 change 目录即可，不影响现有 OpenSpec 规格。

## Open Questions

1. `external_session_ref` 是否只保存不可逆哈希，还是保留本地原值以便调试？
2. 首个 adapter 应优先支持哪类宿主集成点：启动 Hook、包装 CLI，还是项目级指令模板？
3. handoff 的默认到期时间是否统一为 7 天，还是按项目配置？
4. Phase 1 是否只拒绝已知敏感字段，还是引入可选的本地规则式脱敏器？
