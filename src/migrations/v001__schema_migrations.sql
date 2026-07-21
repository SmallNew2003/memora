-- Migration v001: schema_migrations 表 + 元数据
--
-- 本迁移仅创建迁移元表本身；不创建任何业务表（sessions / observations /
-- summaries / FTS 等）。这些业务 schema 必须由独立 L1 实现变更引入。
--
-- 校验和：迁移 SQL 原始 UTF-8 字节的 SHA-256，由 application 启动期计算并写入。
-- 此处不需要存储任何业务字段。

CREATE TABLE IF NOT EXISTS schema_migrations (
    version    INTEGER PRIMARY KEY,
    checksum   TEXT    NOT NULL,
    applied_at TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);