use super::RepositoryError;
use super::error::Result;
use opsbox_core::{SqlitePool, run_migration};
use serde::{Deserialize, Serialize};

/// Planner 脚本记录
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlannerScript {
  pub app: String,
  pub script: String,
  pub updated_at: i64,
}

/// 脚本元信息（不含内容）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlannerScriptMeta {
  pub app: String,
  pub updated_at: i64,
}

/// 初始化 planner_scripts 表
pub async fn init_schema(db: &SqlitePool) -> Result<()> {
  let ddl = r#"
    CREATE TABLE IF NOT EXISTS planner_scripts (
      app TEXT PRIMARY KEY,
      script TEXT NOT NULL,
      updated_at INTEGER NOT NULL
    );
  "#;
  let ddl_default = r#"
    CREATE TABLE IF NOT EXISTS planner_default (
      id INTEGER PRIMARY KEY CHECK (id = 1),
      app TEXT
    );
    INSERT OR IGNORE INTO planner_default (id, app) VALUES (1, NULL);
  "#;
  run_migration(db, ddl, "logseek_planners")
    .await
    .map_err(|e| RepositoryError::Database(e.to_string()))?;
  run_migration(db, ddl_default, "logseek_planners_default")
    .await
    .map_err(|e| RepositoryError::Database(e.to_string()))?;
  Ok(())
}

/// 保存/更新脚本
pub async fn upsert_script(db: &SqlitePool, app: &str, script: &str) -> Result<()> {
  let now = std::time::SystemTime::now()
    .duration_since(std::time::UNIX_EPOCH)
    .unwrap()
    .as_secs() as i64;
  sqlx::query(
    "INSERT INTO planner_scripts (app, script, updated_at) VALUES (?, ?, ?) \
     ON CONFLICT(app) DO UPDATE SET script=excluded.script, updated_at=excluded.updated_at",
  )
  .bind(app)
  .bind(script)
  .bind(now)
  .execute(db)
  .await
  .map_err(|e| RepositoryError::QueryFailed(format!("保存脚本失败: {}", e)))?;
  Ok(())
}

/// 加载脚本（含内容）
pub async fn load_script(db: &SqlitePool, app: &str) -> Result<Option<PlannerScript>> {
  let row =
    sqlx::query_as::<_, (String, String, i64)>("SELECT app, script, updated_at FROM planner_scripts WHERE app = ?")
      .bind(app)
      .fetch_optional(db)
      .await
      .map_err(|e| RepositoryError::QueryFailed(format!("查询脚本失败: {}", e)))?;
  Ok(row.map(|(app, script, updated_at)| PlannerScript {
    app,
    script,
    updated_at,
  }))
}

/// 仅获取脚本文本（便于运行时加载）
pub async fn load_script_text(db: &SqlitePool, app: &str) -> Result<Option<String>> {
  let row = sqlx::query_as::<_, (String,)>("SELECT script FROM planner_scripts WHERE app = ?")
    .bind(app)
    .fetch_optional(db)
    .await
    .map_err(|e| RepositoryError::QueryFailed(format!("查询脚本文本失败: {}", e)))?;
  Ok(row.map(|(s,)| s))
}

/// 列表（不含内容）
pub async fn list_scripts(db: &SqlitePool) -> Result<Vec<PlannerScriptMeta>> {
  let rows = sqlx::query_as::<_, (String, i64)>("SELECT app, updated_at FROM planner_scripts ORDER BY app")
    .fetch_all(db)
    .await
    .map_err(|e| RepositoryError::QueryFailed(format!("查询脚本列表失败: {}", e)))?;
  Ok(
    rows
      .into_iter()
      .map(|(app, updated_at)| PlannerScriptMeta { app, updated_at })
      .collect(),
  )
}

/// 删除
pub async fn delete_script(db: &SqlitePool, app: &str) -> Result<()> {
  // 若为默认脚本，则清空默认
  if get_default(db).await?.as_deref() == Some(app) {
    set_default(db, None).await?;
  }

  sqlx::query("DELETE FROM planner_scripts WHERE app = ?")
    .bind(app)
    .execute(db)
    .await
    .map_err(|e| RepositoryError::QueryFailed(format!("删除脚本失败: {}", e)))?;
  Ok(())
}

/// 设置默认规划脚本（None 表示清空）
pub async fn set_default(db: &SqlitePool, app: Option<&str>) -> Result<()> {
  if let Some(a) = app {
    // 确认存在
    if load_script_text(db, a).await?.is_none() {
      return Err(RepositoryError::StorageError(format!("默认规划脚本不存在: {}", a)));
    }
  }

  sqlx::query("UPDATE planner_default SET app = ? WHERE id = 1")
    .bind(app.map(|s| s.to_string()))
    .execute(db)
    .await
    .map_err(|e| RepositoryError::QueryFailed(format!("设置默认规划脚本失败: {}", e)))?;
  Ok(())
}

/// 获取默认规划脚本名称
pub async fn get_default(db: &SqlitePool) -> Result<Option<String>> {
  let row = sqlx::query_scalar::<_, Option<String>>("SELECT app FROM planner_default WHERE id = 1")
    .fetch_one(db)
    .await
    .map_err(|e| RepositoryError::QueryFailed(format!("查询默认规划脚本失败: {}", e)))?;
  Ok(row)
}
