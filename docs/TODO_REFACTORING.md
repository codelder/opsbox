# OpsBox 重构待办事项

## 当前状态

阶段 2（核心框架创建）已大部分完成，但 logseek 模块需要适配新架构。

## ✅ 已完成

### 核心框架
- [x] 创建 `opsbox-core` 共享库
  - 统一错误处理 (`AppError`, RFC 7807格式)
  - 数据库连接池管理 (SQLite, `opsbox.db`)
  - 标准响应格式 (`SuccessResponse`)
  - 中间件占位符

### OpsBox 主服务重构
- [x] 创建 `config.rs` - 配置管理模块
- [x] 创建 `logging.rs` - 日志初始化模块
- [x] 创建 `daemon.rs` - Unix 守护进程模块
- [x] 创建 `server.rs` - HTTP 服务器和路由聚合
- [x] 重写 `main.rs` - 精简的入口文件（154行，原529行）

### LogSeek 模块（部分）
- [x] 更新 `lib.rs` 添加 `router()` 和 `init_schema()` 导出
- [x] 添加 `opsbox-core` 依赖

## 🔄 待完成工作

### 1. 修复 logseek/routes.rs

**文件**: `server/logseek/src/routes.rs`

**需要修改**:
```rust
// 当前签名
pub fn router() -> Router { ... }

// 修改为
pub fn router(db_pool: SqlitePool) -> Router { ... }
```

**详细步骤**:
1. 修改函数签名接受 `db_pool: SqlitePool` 参数
2. 使用 `axum::extract::State` 共享数据库连接池
3. 更新所有需要数据库访问的路由处理器

**示例代码**:
```rust
use axum::extract::State;
use opsbox_core::SqlitePool;

pub fn router(db_pool: SqlitePool) -> Router {
    Router::new()
        .route("/settings/minio", get(get_minio_settings))
        .route("/settings/minio", post(save_minio_settings))
        .with_state(db_pool)
}

async fn get_minio_settings(
    State(pool): State<SqlitePool>
) -> Result<Json<MinioSettings>, AppError> {
    // 使用 pool 访问数据库
    settings::load_minio_settings(&pool).await
}
```

### 2. 添加 init_schema 到 logseek/settings.rs

**文件**: `server/logseek/src/settings.rs`

**需要添加**:
```rust
use opsbox_core::{Result, SqlitePool, run_migration};

/// 初始化 LogSeek 模块的数据库表
pub async fn init_schema(db_pool: &SqlitePool) -> Result<()> {
    let schema_sql = r#"
        -- LogSeek MinIO 配置表
        CREATE TABLE IF NOT EXISTS logseek_minio_config (
            id INTEGER PRIMARY KEY CHECK (id = 1),
            endpoint TEXT NOT NULL,
            bucket TEXT NOT NULL,
            access_key TEXT NOT NULL,
            secret_key TEXT NOT NULL,
            updated_at INTEGER NOT NULL
        );

        -- LogSeek 通用设置表
        CREATE TABLE IF NOT EXISTS logseek_settings (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL,
            updated_at INTEGER NOT NULL
        );
    "#;

    run_migration(db_pool, schema_sql, "logseek").await
}
```

**注意**: 所有表名必须使用 `logseek_` 前缀！

### 3. 更新 logseek/settings.rs 使用 opsbox-core

**需要修改**:
1. 移除本地定义的错误类型，使用 `opsbox_core::AppError`
2. 移除本地数据库初始化代码
3. 更新所有函数签名接受 `&SqlitePool` 参数
4. 更新表名为 `logseek_` 前缀

**示例**:
```rust
// 之前
pub async fn ensure_store() -> Result<(), SettingsError> { ... }
pub async fn load_minio_settings() -> Result<MinioConfig, SettingsError> { ... }

// 修改为
use opsbox_core::{Result, AppError, SqlitePool};

pub async fn load_minio_settings(pool: &SqlitePool) -> Result<MinioConfig> {
    sqlx::query_as::<_, MinioConfig>(
        "SELECT * FROM logseek_minio_config WHERE id = 1"
    )
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| AppError::not_found("MinIO 配置未设置"))
}
```

### 4. 更新所有 logseek 路由处理器

**涉及文件**: `server/logseek/src/routes.rs` 中的所有处理器函数

**需要修改**:
- 从 `State<SqlitePool>` 提取数据库连接
- 使用 `opsbox_core::AppError` 作为错误类型
- 调用 settings 函数时传递 `&pool`

### 5. 编译测试

**命令**:
```bash
cargo check --manifest-path server/Cargo.toml --workspace
```

**逐步修复编译错误**，直到通过。

### 6. 运行时测试

**启动服务**:
```bash
cargo run --manifest-path server/Cargo.toml -p api-gateway --release
```

**测试项**:
- [ ] 健康检查: `curl http://127.0.0.1:4000/healthy`
- [ ] 前端页面能否访问
- [ ] LogSeek API 是否正常
- [ ] 数据库 `opsbox.db` 是否正确创建
- [ ] 表名是否使用 `logseek_` 前缀

## 📋 后续阶段（未来）

### 阶段 3: 重构 LogSeek 后端（内部分层）
- 创建 `modules/logseek` 目录结构
- 按 API/Service/Domain/Repository 分层
- 优化命名和日志

### 阶段 4: 重构前端
- 调整路由到 `/logseek/*`
- 组件模块化到 `$lib/modules/logseek`

### 阶段 5: 文档和工具
- 更新所有文档
- 更新脚本
- 更新 WARP.md

## 🔧 快速修复脚本

如果需要快速查看所有编译错误:
```bash
cd /Users/wangyue/workspace/codelder/opsboard
cargo check --manifest-path server/Cargo.toml --workspace 2>&1 | grep "error\["
```

## 📝 注意事项

1. **数据库迁移**: 如果已有旧的 `logseek_settings.db`，需要手动迁移数据到新的 `opsbox.db`
2. **表名前缀**: 所有 LogSeek 表必须使用 `logseek_` 前缀
3. **错误处理**: 统一使用 `opsbox_core::AppError`
4. **日志级别**: 
   - `error!` - 系统错误
   - `warn!` - 配置问题
   - `info!` - 关键业务事件
   - `debug!` - 详细流程
   - `trace!` - 性能调试

## 🎯 最终目标

建立清晰的模块化架构：
```
opsbox (主框架)
  ├── opsbox-core (共享库)
  └── modules/
      ├── logseek (日志检索)
      └── (未来模块)
```

每个模块独立、可测试、易扩展。
