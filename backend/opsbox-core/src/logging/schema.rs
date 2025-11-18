//! 日志配置数据库 Schema

/// 日志配置表的 SQL Schema
pub const LOG_CONFIG_SCHEMA: &str = r#"
-- 日志配置表
CREATE TABLE IF NOT EXISTS log_config (
    id INTEGER PRIMARY KEY AUTOINCREMENT,    -- 自增主键
    component TEXT NOT NULL UNIQUE,          -- 组件名称: 'server' 或 'agent'（唯一）
    level TEXT NOT NULL,                     -- 日志级别: 'error', 'warn', 'info', 'debug', 'trace'
    retention_count INTEGER NOT NULL,        -- 保留文件数量（天）
    updated_at INTEGER NOT NULL              -- 更新时间戳（Unix 时间戳）
);

-- 插入默认配置（如果不存在）
INSERT OR IGNORE INTO log_config (component, level, retention_count, updated_at)
VALUES ('server', 'info', 7, strftime('%s', 'now'));
"#;
