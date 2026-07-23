## ADDED Requirements

### Requirement: 本地数据库的可预测初始化
在显式运行 memora server 时，系统 MUST 打开 `MEMORA_DB_PATH` 指定的数据库，或在
其缺失时创建默认本地数据库及必要父目录。系统 MUST 使用 bundled SQLite，验证 FTS5
编译能力，启用 foreign key enforcement 并设置有限的 SQLite busy timeout。数据库
初始化 MUST NOT 在仅编译、格式检查或单元测试阶段隐式写入默认用户数据目录。

#### Scenario: 首次启动
- **WHEN** 用户在不存在数据库文件的默认配置下启动 stdio server
- **THEN** 系统创建本地数据库、应用初始 schema，并使 server 在迁移成功后开始接受 MCP 请求

### Requirement: 有校验和的顺序迁移
系统 MUST 用 `schema_migrations` 记录已应用迁移的版本和校验和。校验和 MUST 是迁移
嵌入 UTF-8 原始字节的 SHA-256，且 MUST NOT 进行空白或换行标准化。启动时，已记录
迁移 MUST 构成当前 binary 内嵌迁移集合的连续版本前缀；校验和漂移、版本缺口或高于
当前 binary 的未知版本 MUST 拒绝启动。未应用迁移 MUST 在 `BEGIN IMMEDIATE` 的单一
事务内按版本顺序执行。

#### Scenario: 迁移成功
- **WHEN** 数据库的 schema version 落后于 binary 内嵌迁移
- **THEN** 系统按版本顺序应用缺失迁移，记录其校验和，并在所有迁移成功后启动 server

#### Scenario: 迁移校验和漂移
- **WHEN** 已应用迁移的记录校验和与 binary 内嵌迁移不匹配
- **THEN** 系统以明确错误中止启动，且不应用后续迁移或启动部分可用的 MCP server

#### Scenario: 旧 binary 打开已升级数据库
- **WHEN** 数据库记录的迁移版本高于当前 binary 内嵌的最高迁移版本
- **THEN** 系统以数据库版本过新的错误中止启动，且不打开 MCP service

#### Scenario: 两个 server 同时首次启动
- **WHEN** 两个进程同时尝试迁移同一空数据库
- **THEN** 一个进程获得 immediate migration lock 并完成迁移，另一个进程在配置的 busy timeout 与退避重试预算内验证已完成的连续迁移而不重复执行或破坏 `schema_migrations`；若预算耗尽，则返回可重试启动错误

### Requirement: 失败不会留下半迁移服务
任一迁移失败时，系统 MUST 回滚该迁移事务并以非零启动结果结束。系统 MUST NOT 在
迁移失败后注册 MCP tools 或接受请求。

#### Scenario: 无效迁移 SQL
- **WHEN** 初始化过程中某个迁移执行失败
- **THEN** 该迁移事务回滚、server 不启动，且错误不包含记忆内容或未脱敏的数据库数据
