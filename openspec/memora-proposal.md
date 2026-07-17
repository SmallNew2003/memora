# Memora — 多层 AI 记忆系统

## 1. 背景与动机

### 1.1 问题

当前 AI Coding Agent（Claude Code、Cursor、Codex 等）普遍存在**上下文失忆**问题：

- 每次新会话，Agent 从零开始，用户需要重复解释项目背景
- 跨会话、跨项目的知识和偏好无法沉淀
- 工具调用结果、决策过程等有价值信息随会话结束而丢失
- 现有方案要么太重量级（需要独立服务），要么太轻量（仅文件存储）

### 1.2 目标

设计一个**多层记忆系统**，让 AI Agent 具备渐进式记忆能力：

| 层级 | 名称 | 范围 | 生命周期 | 类比 |
|:-----|:-----|:-----|:---------|:-----|
| **L1** | 临时记忆 | 单次会话 | 会话结束即归档 | 工作记忆 |
| **L2** | 项目记忆 | 绑定代码库 | 项目周期 | 项目文档 |
| **L3** | 用户记忆 | 跨项目 | 用户周期 | 个人偏好 |
| **L4** | LLM WIKI | 全局 | 永久 | 知识库 |

### 1.3 核心原则

- **Local-First**：数据完全本地，隐私优先，零网络依赖
- **零配置**：单二进制，下载即用，无需安装 Python/Rust 工具链
- **MCP 接入**：通过 MCP 协议被所有主流 Agent 调用
- **渐进式**：从 L1 开始，逐步实现 L2-L4

---

## 2. 调研总结

### 2.1 调研范围

调研了 **50+ 开源项目**，覆盖：

- 海外主流框架：Mem0、Letta、Graphiti、Cognee、Zep、claude-mem
- MCP 记忆工具：codebase-memory-mcp、engram、basic-memory、EverOS 等
- Local-First 方案：SQLite 派、Markdown 派、LanceDB 派
- Rust/Go 引擎：Rig、moltis、Mnemon、Guild、deja-vu 等祖
- 中文社区：Nekro Agent、Echo Agent、Awesome-AI-Memory

完整调研笔记见 Obsidian：`AI Agent Memory Frameworks - Comprehensive Comparison.md`

### 2.2 关键洞察

1. **MCP 已成标配**：2025-2026 年几乎所有新记忆项目都通过 MCP 协议集成
2. **Go 正在占领记忆引擎赛道**：engram (5.5k)、Mnemon、Guild、deja-vu 等一批 Go 项目主打单二进制零依赖
3. **Local-First 三足鼎立**：SQLite 派（90%）、Markdown 派（EverOS 11.1k）、LanceDB 派（新兴）
4. **"遗忘"是核心痛点**：社区共识——记忆系统的关键不是记住更多，而是学会遗忘
5. **claude-mem 模式最实用**：Hook 驱动全自动，会话捕获 → AI 压缩 → 注入，87k stars

### 2.3 竞品对比（与 memora 最相关的）

| 项目 | Stars | 语言 | 存储 | 优势 | 不足 |
|:------|------:|:-----|:-----|:-----|:-----|
| **claude-mem** | 87k | Python | SQLite+Chroma | 全自动，Hook 驱动 | 仅 Claude Code，无分层 |
| **engram** | 5.5k | Go | SQLite+FTS5 | 单二进制，Agent 无关 | 无向量搜索 |
| **EverOS** | 11.1k | Python | Markdown+SQLite+LanceDB | Markdown 原生，Local-First | Python 依赖 |
| **basic-memory** | 3.4k | Python | Markdown | 简单直接 | 功能单一 |
| **Mnemon** | 385 | Go | SQLite | 四图架构，15+ 框架 | 较新，生态小 |
| **Echo Agent** | 648 | Python | - | 四层认知记忆，遗忘曲线 | 非独立记忆工具 |

---

## 3. 技术选型

### 3.1 语言：Rust

**理由：**

- 单二进制部署，零运行时依赖（对标 Go 的优势）
- 内存安全 + 高性能
- sqlite-vec 是 Rust 原生 SQLite 向量扩展，生态无缝
- Rig (5k+) 证明了 Rust 在 AI 工具链的可行性

### 3.2 存储：SQLite + sqlite-vec + FTS5

**理由：**

- SQLite：零配置嵌入式，单文件数据库，90% local-first 方案首选
- sqlite-vec：Rust 原生向量扩展，零依赖，纯 Rust 生态
- FTS5：SQLite 内置全文搜索，BM25 排序
- 一个文件搞定结构化数据 + 向量 + 全文搜索 пуним
- 与 claude-mem、engram、Guild 等成功项目同款路线

### 3.3 接入方式：MCP 协议

**理由：**

- 行业标配，所有主流 Agent 支持
- 一套协议，Claude Code / Codex / Cursor / OpenCode 通用
- rmcp crate 提供 Rust 原生 MCP 实现

### 3.4 MCP 框架：rmcp

**理由：**

- Rust 原生 MCP 实现，支持 SSE 和 Streamable HTTP 传输
- 宏驱动的 Tool 定义，开发体验好
- 活跃维护（0.7 → 2.2 版本迭代快）

---

## 4. 架构设计

### 4.1 整体架构

```
┌──────────────────────────────────────────────────┐
│                   MCP Server                       │
│            (Rust, rmcp, SSE/HTTP)                  │
├──────────────────────────────────────────────────┤
│  MCP Tools:                                        │
│  • session_start  — 开始会话，注入历史上下文       │
│  • session_end    — 结束会话，触发 AI 压缩         │
│  • observe        — 记录观察（工具调用结果）       │
│  • search         — 混合搜索历史记忆               │
│  • recall         — 召回相关上下文                 │
├──────────────────────────────────────────────────┤
│  Memory Engine (Core)                              │
│  • Capture:  自动捕获工具调用                      │
│  • Compress: AI 压缩生成摘要                       │
│  • Index:    sqlite-vec 向量 + FTS5 全文           │
│  • Retrieve: 混合搜索（向量 + 关键词）             │
├──────────────────────────────────────────────────┤
│  Storage: SQLite + sqlite-vec + FTS5               │
│  • sessions       — 会话元数据                     │
│  • observations   — 工具调用记录 + embedding        │
│  • summaries      — AI 压缩摘要 + embedding         │
└──────────────────────────────────────────────────┘
```

### 4.2 数据模型（Phase 1: 临时记忆）

```sql
-- 会话表
CREATE TABLE sessions (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    ended_at TEXT,
    summary TEXT
);

-- 观察表（工具调用、用户输入等）
CREATE TABLE observations (
    id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL REFERENCES sessions(id),
    content TEXT NOT NULL,
    tool_name TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    embedding BLOB  -- sqlite-vec 向量
);

-- 摘要表（AI 压缩后的会话摘要）
CREATE TABLE summaries (
    id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL REFERENCES sessions(id),
    content TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    embedding BLOB  -- sqlite-vec 向量
);

-- FTS5 全文索引
CREATE VIRTUAL TABLE observations_fts USING fts5(id, session_id, content, tool_name);
CREATE VIRTUAL TABLE summaries_fts USING fts5(id, session_id, content);
```

### 4.3 MCP Tools（Phase 1）

| Tool | 描述 | 输入 | 输出 |
|:-----|:-----|:-----|:-----|
| `session_start` | 开始新会话 | name | session_id |
| `session_end` | 结束会话，保存摘要 | session_id, summary | - |
| `observe` | 记录一条观察 | session_id, content, tool_name? | observation |
| `search` | 全文搜索历史记忆 | query, limit? | [search_result] |
| `recent_observations` | 获取最近观察 | session_id, limit? | [observation] |
| `recent_sessions` | 获取最近会话 | limit? | [session] |

---

## 5. 实现计划

### Phase 1：临时记忆（当前）

**目标：** 单会话级别的记忆，Agent 可以记录和检索当前会话的上下文。

**范围：**
- [ ] SQLite + sqlite-vec + FTS5 存储层
- [ ] Memory Engine（start/end session, observe, search）
- [ ] MCP Server（6 个 tools）
- [ ] 单二进制构建
- [ ] 基础测试

**不包含：**
- AI 自动压缩（Phase 2）
- 向量嵌入生成（Phase 2，需要 embedding model）
- 项目记忆 / 用户记忆 / LLM WIKI（Phase 3+）

### Phase 2：AI 压缩 + 向量搜索

- 集成 embedding model（本地 ONNX 或 API）
- 会话结束时自动 AI 压缩生成摘要
- 向量相似度搜索 + FTS5 混合检索再用

### Phase 3：项目记忆（L2）

- 绑定代码库路径
- 自动索引项目结构、关键文件
- 跨会话项目上下文注入

### Phase 4：用户记忆 + LLM WIKI（L3 + L4）

- 跨项目用户偏好存储
- 结构化知识库，多后端插件架构
- 知识图谱（可选）

---

## 6. L4 LLM WIKI 多后端插件架构

### 6.1 设计原则

**不自己造笔记软件。** memora 专注记忆引擎，L4 桥接到已有笔记系统。

### 6.2 WikiBackend Trait

```rust
/// L4 LLM WIKI 的存储后端抽象
trait WikiBackend: Send + Sync {
    /// 写入一篇笔记（创建或覆盖）
    fn write_note(&self, path: &str, content: &str) -> Result<()>;

    /// 读取一篇笔记
    fn read_note(&self, path: &str) -> Result<String>;

    /// 全文搜索笔记
    fn search_notes(&self, query: &str, limit: usize) -> Result<Vec<NoteHit>>;

    /// 创建双向链接（[[wiki-link]]）
    fn link_notes(&self, from: &str, to: &str) -> Result<()>;

    /// 列出目录下的所有笔记
    fn list_notes(&self, dir: &str) -> Result<Vec<String>>;
}

struct NoteHit {
    path: String,
    title: String,
    snippet: String,
    score: f64,
}
```

### 6.3 后端实现优先级

| 优先级 | 后端 | 实现方式 | 理由 |
|:-------|:-----|:---------|:-----|
| **P0** | Obsidian | Local REST API 插件 | 你已在用，MCP 工具链已通 |
| P1 | 纯文件系统 | 直接读写 .md 文件 | 零依赖，通用兜底 |
| P2 | Logseq | Logseq API | 开源，与 Obsidian 生态互补 |
| P3 | Notion | Notion API | 用户量大，但需网络 |

### 6.4 架构图（更新）

```
┌──────────────────────────────────────────┐
│              memora (Rust)                │
├──────────────────────────────────────────┤
│  L1 临时记忆  ──  SQLite (会话级)        │
│  L2 项目记忆  ──  SQLite (项目级)        │
│  L3 用户记忆  ──  SQLite (用户级)        │
│  L4 LLM WIKI  ──  WikiBackend trait      │
│       ├── ObsidianBackend (P0)           │
│       ├── FileSystemBackend (P1)         │
│       └── LogseqBackend (P2)             │
├──────────────────────────────────────────┤
│         Obsidian Local REST API          │
│         读写 .md 笔记、frontmatter       │
└──────────────────────────────────────────┘
```

---

## 7. 优秀设计模式借鉴（来自调研的 50+ 项目）

以下是从调研项目中提炼的可借鉴设计，标注了建议引入的 Phase。

### 7.1 遗忘曲线 + 矛盾检测（Echo Agent, distill）

**来源：** Echo Agent (648⭐)、distill (174⭐)

**核心思想：** 记忆不是越多越好——需要遗忘机制。

- **遗忘曲线**：记忆根据访问频率和时间衰减，久未访问的记忆自动降权/归档
- **矛盾检测**：新记忆与旧记忆冲突时，标记矛盾并触发审查，而非静默覆盖
- **记忆优先级**：高频访问 > 低频，最近的 > 久远的，用户标记 > 自动捕获

**建议：Phase 丛3 引入。** L1/L2 先做全量存储，L3 加入衰减和优先级。

### 7.2 四图架构（Mnemon）

**来源：** Mnemon (385⭐)

**核心思想：** 单一向量搜索不够，需要多维图结构。

| 图类型 | 回答什么问题 | 示例 |
|:-------|:-------------|:-----|
| **时序图** | 什么时候发生的？ | "上周三我们修了那个 bug" |
| **实体图** | 哪些东西相关？ | "User 模块依赖 Auth 模块" |
| **因果图** | 为什么发生？ | "因为改了 API 签名，所以测试挂了" |
| **语义图** | 什么意思？ | "这个错误码表示权限不足" |

**建议：Phase 4（L4 LLM WIKI）引入实体图和因果图。** SQLite 本身可以存图结构（节点表 + 边表），不需要 Neo4j。

### 7.3 Hook 驱动全自动捕获（claude-mem）

**来源：** claude-mem (87k⭐)

**核心思想：** Agent 不需要手动调用 `observe`——自动拦截所有工具调用。

- Hook 机制：拦截 Agent 的每一次工具调用，自动记录输入/输出
- 会话结束时 AI 自动压缩生成摘要
- 下次会话开始时自动注入相关历史

**建议：Phase 2 引入。** MCP 本身支持工具调用拦截，可以在 memora 侧实现"被动观察模式"。

### 7.4 AI 压缩流水线（claude-mem, context-mode, MemOS）

**来源：** claude-mem (87k⭐)、context-mode (19k⭐)、MemOS (10.2k⭐)

**核心思想：** 原始记录 → AI 摘要 → 嵌入 → 分层注入。

```
原始观察 (10k tokens)
    ↓ AI 压缩
会话摘要 (500 tokens)
    ↓ 再次压缩
项目摘要 (200 tokens)
    ↓ 注入上下文
Agent 的系统提示
```

- context-mode：沙箱化工具输出，-98% token 消耗
- MemOS：混合检索，节省 35% token
- token-savior：-77% 活跃 token，-76% 耗时

**建议：Phase 2 引入。** 核心差异化能力——不只是"存"，而是"压缩后存"。

### 7.5 代码库知识图谱（codebase-memory-mcp）

**来源：** codebase-memory-mcp (32k⭐)

**核心思想：** 用 Tree-sitter 解析 AST，构建代码级知识图谱。

- 函数调用关系、类型依赖、模块结构
- C 级性能，大代码库也能秒级查询
- "这个函数被谁调用？" "这个模块依赖哪些包？"

**建议：Phase 3（L2 项目记忆）引入。** Rust 有 tree-sitter binding，天然适配。

### 7.6 自我编辑记忆（Letta）

**来源：** Letta (24k⭐)

**核心思想：** Agent 不只是读取记忆，还能主动编辑、删除、重组记忆块。

- 记忆块（Memory Block）：可独立寻址的记忆单元
- 虚拟上下文管理：Agent 决定哪些块进入当前上下文窗口
- 自我反思：Agent 定期审查并清理过时记忆

**建议：Phase 劃4 引入。** 需要 Agent 具备"元认知"能力，依赖 LLM 自身判断。

### 7.7 写入时去重 + 敏感度标记（distill, deja-vu）

**来源：** distill (174⭐)、deja-vu (242⭐)

**核心思想：** 写入前检查，而非写入后清理。

- **去重**：新记忆与已有记忆语义重复时，合并而非新增
- **敏感度标记**：自动检测 API key、token、密码等，标记为敏感
- **秘密脱敏**：deja-vu 在存储前自动脱敏

**建议：Phase 2 引入去重，Phase 1 就可以加入简单的敏感信息过滤。**

### 7.8 可回滚 / 版本化记忆（nocturne_memory, EverOS）

**来源：** nocturne_memory (1.3k⭐)、EverOS (11.1k⭐)

**核心思想：** 记忆像 Git 一样可版本控制。

- nocturne_memory：可视化回滚，"告别 Vector RAG 失忆"
- EverOS：Markdown + Git = 天然版本化
- 可以回退到"那个 bug 修之前的状态"

**建议：Phase 3 引入。** SQLite 可以加 `version` 字段实现软删除 + 版本链。

### 7.9 意图分析 + 智能合成（context-keeper）

**来源：** context-keeper (153⭐)

**核心思想：** 两阶段 LLM 推理——先分析意图，再合成记忆。

- 第一阶段：分析用户输入/工具输出的**意图**和**重要性**
- 第二阶段：根据意图智能合成记忆条目
- 四维统一上下文模型

**建议：Phase 2 引入。** 比简单的"全存"更智能，减少噪音。

### 7.10 仿生认知架构（Echo Agent, spector）

**来源：** Echo Agent (648⭐)、spector (16⭐)

**核心思想：** 模仿人脑记忆模型。

| 人脑记忆 | Echo Agent | memora 映射 |
|:---------|:-----------|:------------|
| 工作记忆 | Working | L1 临时记忆 |
| 情景记忆 | Episodic | L рођено2 项目记忆 |
| 语义记忆 | Semantic | L3 用户记忆 |
| 档案记忆 | Archival | L4 LLM WIKI |

- spector：4 层 Cortex + Panama SIMD（Java 25），真正的仿生架构
- Echo Agent：遗忘曲线 + 矛盾检测 + 睡眠整合

**建议：** memora 的四层设计本身就与仿生模型对齐，Phase 4 可加入"睡眠整合"（定期 AI 整理记忆）。

### 7.11 跨 Agent 共享 + 任务协调（Guild）

**来源：** Guild (318⭐)

**核心思想：** 多个 Agent 共享同一套记忆，协调任务。

- Claude Code 和 Codex 同时读写同一项目记忆
- 任务状态共享：Agent A 做到一半，Agent B 接手
- 避免重复工作

**建议：Phase 3 引入。** SQLite 支持多读单写，天然适合本地多 Agent 场景。

### 7.12 本体驱动 / 形式化知识（Cortex abbacusgroup）

**来源：** Cortex abbacusgroup (20⭐)

**核心思想：** 不只是自然语言记忆，还有形式化知识表示。

- OWL-RL 推理：自动推导隐含知识
- SPARQL 查询：精确结构化查询
- 22 个 MCP 工具

**建议：Phase 4（L4 LLM WIKI）可选引入。** 对知识库的高级查询很有价值，但实现复杂度高。

---

## 8. 设计模式优先级矩阵

| 设计模式 | 价值 | 复杂度 | 建议 Phase |
|:---------|:----:|:------:|:----------|
| AI 压缩流水线 | ⭐⭐⭐⭐⭐ | 中 | Phase 2 |
| Hook 驱动自动捕获 | ⭐⭐⭐⭐⭐ | 中 | Phase 2 |
| 写入时去重 | ⭐⭐⭐⭐ | 低 | Phase 2 |
| 遗忘曲线 + 优先级 | ⭐⭐⭐⭐ | 中 | Phase 3 |
| 代码库知识图谱 | ⭐⭐⭐⭐ | 高 | Phase 3 |
| 可回滚 / 版本化 | ⭐⭐⭐ | 低 | Phase 3 |
| 跨 Agent 共享 | ⭐⭐⭐ | 中 | Phase 3 |
| 四图架构 | ⭐⭐⭐ | 高 | Phase 4 |
| 自我编辑记忆 | ⭐⭐⭐ | 高 | Phase 4 |
| 矛盾检测 | ⭐⭐⭐ | 中 | Phase 4 |
| 仿生睡眠整合 | ⭐⭐ | 高 | Phase 4 |
| 本体推理 | ⭐⭐ | 很高 | Phase 4+ |

---

## 9. 待讨论

1. **Embedding 方案**：Phase 2 是用本地 ONNX 模型（如 all-MiniLM-L6-v2）还是调 OpenAI API？前者零网络依赖但增加二进制体积，后者简单但有网络依赖。

2. **AI 压缩触发时机**：是 session_end 时由调用方 Agent 自己压缩后传入，还是 memora 内置压缩逻辑？前者更灵活但依赖 Agent 配合，后者更自动化但需要内置 LLM 调用。

3. **与 claude-mem 的关系**：是替代 claude-mem 还是互补？claude-mem 已解决"全自动 Hook 驱动记忆"，memora 的差异化在于分层架构 + Rust 性能 + 跨 Agent 通用。

4. **项目名称**：memora 是否最终确定？是否需要考虑更独特的名称以避免与现有项目混淆？
