//! Agent Manager 数据库操作层
//!
//! 提供 Agent 和标签的持久化存储功能

use crate::models::{AgentInfo, AgentStatus, AgentTag};
use chrono::Utc;
use serde_json;
use sqlx::{Row, sqlite::SqlitePool};
use uuid::Uuid;

/// 数据库操作结构体
pub struct AgentRepository {
  pool: SqlitePool,
}

impl AgentRepository {
  /// 创建新的数据库操作实例（使用外部传入的连接池）
  pub fn new(pool: SqlitePool) -> Self {
    Self { pool }
  }

  /// 初始化数据库表结构
  pub async fn init_schema(&self) -> Result<(), sqlx::Error> {
    // 创建 agents 表
    sqlx::query(
      r#"
      CREATE TABLE IF NOT EXISTS agents (
          id TEXT PRIMARY KEY,
          name TEXT NOT NULL,
          version TEXT NOT NULL,
          hostname TEXT NOT NULL,
          search_roots TEXT NOT NULL,
          last_heartbeat INTEGER NOT NULL,
          status TEXT NOT NULL,
          created_at INTEGER NOT NULL,
          updated_at INTEGER NOT NULL
      )
      "#,
    )
    .execute(&self.pool)
    .await?;

    // 创建 agent_tags 表
    sqlx::query(
      r#"
      CREATE TABLE IF NOT EXISTS agent_tags (
          id TEXT PRIMARY KEY,
          agent_id TEXT NOT NULL,
          tag_key TEXT NOT NULL,
          tag_value TEXT NOT NULL,
          created_at INTEGER NOT NULL,
          FOREIGN KEY (agent_id) REFERENCES agents(id) ON DELETE CASCADE,
          UNIQUE(agent_id, tag_key, tag_value)
      )
      "#,
    )
    .execute(&self.pool)
    .await?;

    // 创建索引
    sqlx::query("CREATE INDEX IF NOT EXISTS idx_agents_status ON agents(status)")
      .execute(&self.pool)
      .await?;

    sqlx::query("CREATE INDEX IF NOT EXISTS idx_agents_hostname ON agents(hostname)")
      .execute(&self.pool)
      .await?;

    sqlx::query("CREATE INDEX IF NOT EXISTS idx_agents_last_heartbeat ON agents(last_heartbeat)")
      .execute(&self.pool)
      .await?;

    sqlx::query("CREATE INDEX IF NOT EXISTS idx_agent_tags_agent_id ON agent_tags(agent_id)")
      .execute(&self.pool)
      .await?;

    sqlx::query("CREATE INDEX IF NOT EXISTS idx_agent_tags_key_value ON agent_tags(tag_key, tag_value)")
      .execute(&self.pool)
      .await?;

    sqlx::query("CREATE INDEX IF NOT EXISTS idx_agent_tags_key ON agent_tags(tag_key)")
      .execute(&self.pool)
      .await?;

    sqlx::query("CREATE INDEX IF NOT EXISTS idx_agent_tags_value ON agent_tags(tag_value)")
      .execute(&self.pool)
      .await?;

    Ok(())
  }

  /// 注册 Agent
  pub async fn register_agent(&self, info: &AgentInfo) -> Result<(), sqlx::Error> {
    let now = Utc::now().timestamp();
    let search_roots_json = serde_json::to_string(&info.search_roots).map_err(|e| sqlx::Error::Decode(Box::new(e)))?;

    sqlx::query(
      r#"
            INSERT INTO agents 
            (id, name, version, hostname, search_roots, last_heartbeat, status, created_at, updated_at)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(id) DO UPDATE SET
              name = excluded.name,
              version = excluded.version,
              hostname = excluded.hostname,
              search_roots = excluded.search_roots,
              last_heartbeat = excluded.last_heartbeat,
              status = excluded.status,
              updated_at = excluded.updated_at
            "#,
    )
    .bind(&info.id)
    .bind(&info.name)
    .bind(&info.version)
    .bind(&info.hostname)
    .bind(&search_roots_json)
    .bind(info.last_heartbeat)
    .bind(info.status.to_string())
    .bind(now)
    .bind(now)
    .execute(&self.pool)
    .await?;

    // 仅当 info.tags 非空时才覆盖（避免空上报清空已存在的标签）
    if !info.tags.is_empty() {
      self.save_agent_tags(&info.id, &info.tags).await?;
    }

    Ok(())
  }

  /// 获取 Agent
  pub async fn get_agent(&self, agent_id: &str) -> Result<Option<AgentInfo>, sqlx::Error> {
    let agent_row = sqlx::query(
      r#"
            SELECT id, name, version, hostname, search_roots, last_heartbeat, status, created_at, updated_at
            FROM agents 
            WHERE id = ?
            "#,
    )
    .bind(agent_id)
    .fetch_optional(&self.pool)
    .await?;

    if let Some(row) = agent_row {
      let search_roots: Vec<String> =
        serde_json::from_str(&row.get::<String, _>("search_roots")).map_err(|e| sqlx::Error::Decode(Box::new(e)))?;

      let status = match row.get::<String, _>("status").as_str() {
        "Online" => AgentStatus::Online,
        "Offline" => AgentStatus::Offline,
        _ => AgentStatus::Offline,
      };

      let tags = self.get_agent_tags(&row.get::<String, _>("id")).await?;

      Ok(Some(AgentInfo {
        id: row.get::<String, _>("id"),
        name: row.get::<String, _>("name"),
        version: row.get::<String, _>("version"),
        hostname: row.get::<String, _>("hostname"),
        tags,
        search_roots,
        last_heartbeat: row.get::<i64, _>("last_heartbeat"),
        status,
      }))
    } else {
      Ok(None)
    }
  }

  /// 获取所有 Agent
  pub async fn list_agents(&self) -> Result<Vec<AgentInfo>, sqlx::Error> {
    let rows = sqlx::query(
      r#"
            SELECT id, name, version, hostname, search_roots, last_heartbeat, status, created_at, updated_at
            FROM agents 
            ORDER BY created_at DESC
            "#,
    )
    .fetch_all(&self.pool)
    .await?;

    let mut agents = Vec::new();
    for row in rows {
      let search_roots: Vec<String> =
        serde_json::from_str(&row.get::<String, _>("search_roots")).map_err(|e| sqlx::Error::Decode(Box::new(e)))?;

      let status = match row.get::<String, _>("status").as_str() {
        "Online" => AgentStatus::Online,
        "Offline" => AgentStatus::Offline,
        _ => AgentStatus::Offline,
      };

      let tags = self.get_agent_tags(&row.get::<String, _>("id")).await?;

      agents.push(AgentInfo {
        id: row.get::<String, _>("id"),
        name: row.get::<String, _>("name"),
        version: row.get::<String, _>("version"),
        hostname: row.get::<String, _>("hostname"),
        tags,
        search_roots,
        last_heartbeat: row.get::<i64, _>("last_heartbeat"),
        status,
      });
    }

    Ok(agents)
  }

  /// 获取在线 Agent
  pub async fn list_online_agents(&self) -> Result<Vec<AgentInfo>, sqlx::Error> {
    let rows = sqlx::query(
      r#"
            SELECT id, name, version, hostname, search_roots, last_heartbeat, status, created_at, updated_at
            FROM agents 
            WHERE status = 'Online'
            ORDER BY created_at DESC
            "#,
    )
    .fetch_all(&self.pool)
    .await?;

    let mut agents = Vec::new();
    for row in rows {
      let search_roots: Vec<String> =
        serde_json::from_str(&row.get::<String, _>("search_roots")).map_err(|e| sqlx::Error::Decode(Box::new(e)))?;

      let status = AgentStatus::Online;
      let tags = self.get_agent_tags(&row.get::<String, _>("id")).await?;

      agents.push(AgentInfo {
        id: row.get::<String, _>("id"),
        name: row.get::<String, _>("name"),
        version: row.get::<String, _>("version"),
        hostname: row.get::<String, _>("hostname"),
        tags,
        search_roots,
        last_heartbeat: row.get::<i64, _>("last_heartbeat"),
        status,
      });
    }

    Ok(agents)
  }

  /// 按标签筛选 Agent
  pub async fn list_agents_by_tags(&self, tag_filters: &[AgentTag]) -> Result<Vec<AgentInfo>, sqlx::Error> {
    if tag_filters.is_empty() {
      return self.list_agents().await;
    }

    // 使用子查询确保 Agent 同时拥有所有指定的标签
    let mut subqueries = Vec::new();
    let mut params: Vec<String> = Vec::new();

    for tag in tag_filters {
      subqueries
        .push("EXISTS (SELECT 1 FROM agent_tags t WHERE t.agent_id = a.id AND t.tag_key = ? AND t.tag_value = ?)");
      params.push(tag.key.clone());
      params.push(tag.value.clone());
    }

    let where_clause = subqueries.join(" AND ");

    let query_str = format!(
      r#"
            SELECT a.id, a.name, a.version, a.hostname, a.search_roots, a.last_heartbeat, a.status, a.created_at, a.updated_at
            FROM agents a
            WHERE {}
            ORDER BY a.created_at DESC
            "#,
      where_clause
    );

    let mut query = sqlx::query(&query_str);
    for param in params {
      query = query.bind(param);
    }

    let rows = query.fetch_all(&self.pool).await?;

    let mut agents = Vec::new();
    for row in rows {
      let search_roots: Vec<String> =
        serde_json::from_str(&row.get::<String, _>("search_roots")).map_err(|e| sqlx::Error::Decode(Box::new(e)))?;

      let status = match row.get::<String, _>("status").as_str() {
        "Online" => AgentStatus::Online,
        "Offline" => AgentStatus::Offline,
        _ => AgentStatus::Offline,
      };

      let tags = self.get_agent_tags(&row.get::<String, _>("id")).await?;

      agents.push(AgentInfo {
        id: row.get::<String, _>("id"),
        name: row.get::<String, _>("name"),
        version: row.get::<String, _>("version"),
        hostname: row.get::<String, _>("hostname"),
        tags,
        search_roots,
        last_heartbeat: row.get::<i64, _>("last_heartbeat"),
        status,
      });
    }

    Ok(agents)
  }

  /// 按标签筛选在线 Agent
  pub async fn list_online_agents_by_tags(&self, tag_filters: &[AgentTag]) -> Result<Vec<AgentInfo>, sqlx::Error> {
    if tag_filters.is_empty() {
      return self.list_online_agents().await;
    }

    // 使用子查询确保 Agent 同时拥有所有指定的标签
    let mut subqueries = Vec::new();
    let mut params: Vec<String> = Vec::new();

    for tag in tag_filters {
      subqueries
        .push("EXISTS (SELECT 1 FROM agent_tags t WHERE t.agent_id = a.id AND t.tag_key = ? AND t.tag_value = ?)");
      params.push(tag.key.clone());
      params.push(tag.value.clone());
    }

    let where_clause = subqueries.join(" AND ");

    let query_str = format!(
      r#"
            SELECT a.id, a.name, a.version, a.hostname, a.search_roots, a.last_heartbeat, a.status, a.created_at, a.updated_at
            FROM agents a
            WHERE a.status = 'Online' AND {}
            ORDER BY a.created_at DESC
            "#,
      where_clause
    );

    let mut query = sqlx::query(&query_str);
    query = query.bind("Online");
    for param in params {
      query = query.bind(param);
    }

    let rows = query.fetch_all(&self.pool).await?;

    let mut agents = Vec::new();
    for row in rows {
      let search_roots: Vec<String> =
        serde_json::from_str(&row.get::<String, _>("search_roots")).map_err(|e| sqlx::Error::Decode(Box::new(e)))?;

      let status = AgentStatus::Online;
      let tags = self.get_agent_tags(&row.get::<String, _>("id")).await?;

      agents.push(AgentInfo {
        id: row.get::<String, _>("id"),
        name: row.get::<String, _>("name"),
        version: row.get::<String, _>("version"),
        hostname: row.get::<String, _>("hostname"),
        tags,
        search_roots,
        last_heartbeat: row.get::<i64, _>("last_heartbeat"),
        status,
      });
    }

    Ok(agents)
  }

  /// 更新 Agent 心跳
  pub async fn update_heartbeat(&self, agent_id: &str) -> Result<(), sqlx::Error> {
    let now = Utc::now().timestamp();

    sqlx::query(
      r#"
            UPDATE agents 
            SET last_heartbeat = ?, status = 'Online', updated_at = ?
            WHERE id = ?
            "#,
    )
    .bind(now)
    .bind(now)
    .bind(agent_id)
    .execute(&self.pool)
    .await?;

    Ok(())
  }

  /// 注销 Agent
  pub async fn unregister_agent(&self, agent_id: &str) -> Result<(), sqlx::Error> {
    sqlx::query(
      r#"
            DELETE FROM agents WHERE id = ?
            "#,
    )
    .bind(agent_id)
    .execute(&self.pool)
    .await?;

    Ok(())
  }

  /// 保存 Agent 标签（部分覆盖）
  /// 仅覆盖本次上报中出现的 tag_key：
  /// - 对于每个 tag_key：先删除该 Agent 下该 key 的所有旧值，再插入新值
  /// - 未出现在本次上报中的其他 key 将被保留，不做变更
  pub async fn save_agent_tags(&self, agent_id: &str, tags: &[AgentTag]) -> Result<(), sqlx::Error> {
    for tag in tags {
      // 删除该 key 既有记录（避免同一 key 存在多个值）
      sqlx::query(
        r#"
              DELETE FROM agent_tags WHERE agent_id = ? AND tag_key = ?
              "#,
      )
      .bind(agent_id)
      .bind(&tag.key)
      .execute(&self.pool)
      .await?;

      // 插入该 key 的新值
      let tag_id = Uuid::new_v4().to_string();
      let now = Utc::now().timestamp();

      sqlx::query(
        r#"
                INSERT INTO agent_tags (id, agent_id, tag_key, tag_value, created_at)
                VALUES (?, ?, ?, ?, ?)
                "#,
      )
      .bind(&tag_id)
      .bind(agent_id)
      .bind(&tag.key)
      .bind(&tag.value)
      .bind(now)
      .execute(&self.pool)
      .await?;
    }

    Ok(())
  }

  /// 获取 Agent 标签
  pub async fn get_agent_tags(&self, agent_id: &str) -> Result<Vec<AgentTag>, sqlx::Error> {
    let rows = sqlx::query(
      r#"
            SELECT tag_key, tag_value FROM agent_tags WHERE agent_id = ? ORDER BY created_at
            "#,
    )
    .bind(agent_id)
    .fetch_all(&self.pool)
    .await?;

    let tags = rows
      .into_iter()
      .map(|row| AgentTag::new(row.get::<String, _>("tag_key"), row.get::<String, _>("tag_value")))
      .collect();

    Ok(tags)
  }

  /// 删除指定的 Agent 标签
  pub async fn delete_agent_tag(&self, agent_id: &str, key: &str, value: &str) -> Result<(), sqlx::Error> {
    sqlx::query(
      r#"
            DELETE FROM agent_tags WHERE agent_id = ? AND tag_key = ? AND tag_value = ?
            "#,
    )
    .bind(agent_id)
    .bind(key)
    .bind(value)
    .execute(&self.pool)
    .await?;

    Ok(())
  }

  /// 获取所有标签
  pub async fn get_all_tags(&self) -> Result<Vec<AgentTag>, sqlx::Error> {
    let rows = sqlx::query(
      r#"
            SELECT DISTINCT tag_key, tag_value FROM agent_tags ORDER BY tag_key, tag_value
            "#,
    )
    .fetch_all(&self.pool)
    .await?;

    let tags = rows
      .into_iter()
      .map(|row| AgentTag::new(row.get::<String, _>("tag_key"), row.get::<String, _>("tag_value")))
      .collect();

    Ok(tags)
  }

  /// 获取所有标签键
  pub async fn get_all_tag_keys(&self) -> Result<Vec<String>, sqlx::Error> {
    let rows = sqlx::query(
      r#"
            SELECT DISTINCT tag_key FROM agent_tags ORDER BY tag_key
            "#,
    )
    .fetch_all(&self.pool)
    .await?;

    let keys = rows.into_iter().map(|row| row.get::<String, _>("tag_key")).collect();
    Ok(keys)
  }

  /// 获取指定键的所有标签值
  pub async fn get_tag_values_by_key(&self, key: &str) -> Result<Vec<String>, sqlx::Error> {
    let rows = sqlx::query(
      r#"
            SELECT DISTINCT tag_value FROM agent_tags WHERE tag_key = ? ORDER BY tag_value
            "#,
    )
    .bind(key)
    .fetch_all(&self.pool)
    .await?;

    let values = rows.into_iter().map(|row| row.get::<String, _>("tag_value")).collect();
    Ok(values)
  }

  /// 清理离线 Agent（超过心跳超时时间）
  pub async fn cleanup_offline_agents(&self, heartbeat_timeout: i64) -> Result<usize, sqlx::Error> {
    let cutoff_time = Utc::now().timestamp() - heartbeat_timeout;

    let result = sqlx::query(
      r#"
            UPDATE agents 
            SET status = 'Offline', updated_at = ?
            WHERE last_heartbeat < ? AND status = 'Online'
            "#,
    )
    .bind(Utc::now().timestamp())
    .bind(cutoff_time)
    .execute(&self.pool)
    .await?;

    Ok(result.rows_affected() as usize)
  }
}
