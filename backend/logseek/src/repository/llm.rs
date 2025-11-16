use super::RepositoryError;
use super::error::Result;
use tracing::{debug, warn};
use opsbox_core::{SqlitePool, run_migration};
use reqwest::Url;
use serde::{Deserialize, Serialize};

/// 提供方类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProviderKind {
  Ollama,
  OpenAI,
}

impl ProviderKind {
  pub fn as_str(&self) -> &'static str {
    match self {
      ProviderKind::Ollama => "ollama",
      ProviderKind::OpenAI => "openai",
    }
  }

  pub fn parse(s: &str) -> Option<Self> {
    match s.to_ascii_lowercase().as_str() {
      "ollama" => Some(ProviderKind::Ollama),
      "openai" => Some(ProviderKind::OpenAI),
      _ => None,
    }
  }
}

/// 大模型后端配置（持久化）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmBackend {
  /// 唯一名称（用于选择和展示）
  pub name: String,
  /// 提供方：ollama | openai
  pub provider: ProviderKind,
  /// 基础地址（如 http://127.0.0.1:11434 或 https://api.openai.com）
  pub base_url: String,
  /// 默认模型名（如 qwen3:8b, gpt-4o-mini）
  pub model: String,
  /// 超时时间（秒）
  pub timeout_secs: i64,
  /// OpenAI: API Key（可选，Ollama 忽略）
  pub api_key: Option<String>,
  /// OpenAI: 组织 ID（可选）
  pub organization: Option<String>,
  /// OpenAI: 项目 ID（可选）
  pub project: Option<String>,
}

/// 公共展示数据（不包含敏感字段）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmBackendPublic {
  pub name: String,
  pub provider: ProviderKind,
  pub base_url: String,
  pub model: String,
  pub timeout_secs: i64,
  /// 是否已配置密钥（仅 openai 有意义）
  pub has_api_key: bool,
}

impl From<LlmBackend> for LlmBackendPublic {
  fn from(v: LlmBackend) -> Self {
    Self {
      name: v.name,
      provider: v.provider,
      base_url: v.base_url,
      model: v.model,
      timeout_secs: v.timeout_secs,
      has_api_key: v.api_key.as_ref().map(|s| !s.is_empty()).unwrap_or(false),
    }
  }
}

/// 初始化 LLM 配置相关表
pub async fn init_schema(db_pool: &SqlitePool) -> Result<()> {
  let sql_backends = r#"
    CREATE TABLE IF NOT EXISTS llm_backends (
      name TEXT PRIMARY KEY,
      provider TEXT NOT NULL,
      base_url TEXT NOT NULL,
      model TEXT NOT NULL,
      timeout_secs INTEGER NOT NULL DEFAULT 60,
      api_key TEXT,
      organization TEXT,
      project TEXT,
      created_at INTEGER NOT NULL,
      updated_at INTEGER NOT NULL
    );
  "#;

  let sql_default = r#"
    CREATE TABLE IF NOT EXISTS llm_default (
      id INTEGER PRIMARY KEY CHECK (id = 1),
      name TEXT
    );
    INSERT OR IGNORE INTO llm_default (id, name) VALUES (1, NULL);
  "#;

  run_migration(db_pool, sql_backends, "logseek")
    .await
    .map_err(|e| RepositoryError::Database(e.to_string()))?;
  run_migration(db_pool, sql_default, "logseek")
    .await
    .map_err(|e| RepositoryError::Database(e.to_string()))?;
  Ok(())
}

fn now_secs() -> i64 {
  std::time::SystemTime::now()
    .duration_since(std::time::UNIX_EPOCH)
    .unwrap()
    .as_secs() as i64
}

/// 列出所有后端（不含敏感字段）
pub async fn list_backends(pool: &SqlitePool) -> Result<Vec<LlmBackendPublic>> {
  let rows = sqlx::query_as::<_, (String, String, String, String, Option<i64>, Option<String>)>(
    "SELECT name, provider, base_url, model, timeout_secs, api_key FROM llm_backends ORDER BY name",
  )
  .fetch_all(pool)
  .await
  .map_err(|e| RepositoryError::QueryFailed(format!("查询 LLM 后端失败: {}", e)))?;

  Ok(
    rows
      .into_iter()
      .filter_map(|(name, provider, base_url, model, timeout_secs, api_key)| {
        let provider = ProviderKind::parse(&provider)?;
        Some(LlmBackendPublic {
          name,
          provider,
          base_url,
          model,
          timeout_secs: timeout_secs.unwrap_or(60),
          has_api_key: api_key.map(|s| !s.is_empty()).unwrap_or(false),
        })
      })
      .collect(),
  )
}

/// 读取完整后端信息（包含敏感字段）
pub async fn get_backend(pool: &SqlitePool, name: &str) -> Result<Option<LlmBackend>> {
  let row = sqlx::query_as::<_, (String, String, String, String, Option<i64>, Option<String>, Option<String>, Option<String>)>(
    "SELECT name, provider, base_url, model, timeout_secs, api_key, organization, project FROM llm_backends WHERE name = ?",
  )
  .bind(name)
  .fetch_optional(pool)
  .await
  .map_err(|e| RepositoryError::QueryFailed(format!("查询 LLM 后端失败: {}", e)))?;

  Ok(row.map(
    |(name, provider, base_url, model, timeout_secs, api_key, organization, project)| LlmBackend {
      name,
      provider: ProviderKind::parse(&provider).unwrap_or(ProviderKind::Ollama),
      base_url,
      model,
      timeout_secs: timeout_secs.unwrap_or(60),
      api_key,
      organization,
      project,
    },
  ))
}

/// 保存或更新后端
pub async fn save_backend(pool: &SqlitePool, backend: &LlmBackend, update_secret: bool) -> Result<()> {
  let now = now_secs();
  let exists = sqlx::query_scalar::<_, i64>("SELECT COUNT(1) FROM llm_backends WHERE name = ?")
    .bind(&backend.name)
    .fetch_one(pool)
    .await
    .map_err(|e| RepositoryError::QueryFailed(format!("检查 LLM 后端存在性失败: {}", e)))?
    > 0;

  if exists {
    // 更新
    if update_secret {
      sqlx::query(
        r#"UPDATE llm_backends
           SET provider = ?, base_url = ?, model = ?, timeout_secs = ?, api_key = ?, organization = ?, project = ?, updated_at = ?
           WHERE name = ?"#,
      )
      .bind(backend.provider.as_str())
      .bind(&backend.base_url)
      .bind(&backend.model)
      .bind(backend.timeout_secs)
      .bind(&backend.api_key)
      .bind(&backend.organization)
      .bind(&backend.project)
      .bind(now)
      .bind(&backend.name)
      .execute(pool)
      .await
      .map_err(|e| RepositoryError::QueryFailed(format!("更新 LLM 后端失败: {}", e)))?;
    } else {
      sqlx::query(
        r#"UPDATE llm_backends
           SET provider = ?, base_url = ?, model = ?, timeout_secs = ?, organization = ?, project = ?, updated_at = ?
           WHERE name = ?"#,
      )
      .bind(backend.provider.as_str())
      .bind(&backend.base_url)
      .bind(&backend.model)
      .bind(backend.timeout_secs)
      .bind(&backend.organization)
      .bind(&backend.project)
      .bind(now)
      .bind(&backend.name)
      .execute(pool)
      .await
      .map_err(|e| RepositoryError::QueryFailed(format!("更新 LLM 后端失败: {}", e)))?;
    }
  } else {
    // 插入
    sqlx::query(
      r#"INSERT INTO llm_backends
         (name, provider, base_url, model, timeout_secs, api_key, organization, project, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
    )
    .bind(&backend.name)
    .bind(backend.provider.as_str())
    .bind(&backend.base_url)
    .bind(&backend.model)
    .bind(backend.timeout_secs)
    .bind(&backend.api_key)
    .bind(&backend.organization)
    .bind(&backend.project)
    .bind(now)
    .bind(now)
    .execute(pool)
    .await
    .map_err(|e| RepositoryError::QueryFailed(format!("保存 LLM 后端失败: {}", e)))?;
  }

  Ok(())
}

/// 删除后端
pub async fn delete_backend(pool: &SqlitePool, name: &str) -> Result<()> {
  // 若为默认后端，则清空默认
  if get_default(pool).await?.as_deref() == Some(name) {
    set_default(pool, None).await?;
  }

  sqlx::query("DELETE FROM llm_backends WHERE name = ?")
    .bind(name)
    .execute(pool)
    .await
    .map_err(|e| RepositoryError::QueryFailed(format!("删除 LLM 后端失败: {}", e)))?;

  Ok(())
}

/// 设置默认后端（None 表示清空）
pub async fn set_default(pool: &SqlitePool, name: Option<&str>) -> Result<()> {
  if let Some(n) = name {
    // 确认存在
    if get_backend(pool, n).await?.is_none() {
      return Err(RepositoryError::StorageError(format!("默认后端不存在: {}", n)));
    }
  }

  sqlx::query("UPDATE llm_default SET name = ? WHERE id = 1")
    .bind(name.map(|s| s.to_string()))
    .execute(pool)
    .await
    .map_err(|e| RepositoryError::QueryFailed(format!("设置默认 LLM 后端失败: {}", e)))?;
  Ok(())
}

/// 获取默认后端名称
pub async fn get_default(pool: &SqlitePool) -> Result<Option<String>> {
  let row = sqlx::query_scalar::<_, Option<String>>("SELECT name FROM llm_default WHERE id = 1")
    .fetch_one(pool)
    .await
    .map_err(|e| RepositoryError::QueryFailed(format!("查询默认 LLM 后端失败: {}", e)))?;
  Ok(row)
}

/// 基于“列出模型”进行验证；当 strict=true 时再做一次最小对话探针
pub async fn verify_backend(backend: &LlmBackend, strict: bool) -> Result<()> {
  // 1) 列出模型，确保基础连通性与鉴权，并校验模型存在
  let models = match backend.provider {
    ProviderKind::Ollama => list_models_ollama(&backend.base_url).await?,
    ProviderKind::OpenAI => {
      let key = backend
        .api_key
        .as_deref()
        .ok_or_else(|| RepositoryError::StorageError("缺少 OpenAI API Key".to_string()))?;
      list_models_openai(
        &backend.base_url,
        key,
        backend.organization.as_deref(),
        backend.project.as_deref(),
      )
      .await?
    }
  };
  if !models.iter().any(|m| m == &backend.model) {
    return Err(RepositoryError::StorageError(format!(
      "指定模型不存在：{}，可用模型：{}",
      backend.model,
      models.join(", ")
    )));
  }

  // 2) 可选严格校验：对该模型发起一次最小 chat 探针
  if strict {
    match backend.provider {
      ProviderKind::Ollama => verify_chat_ollama(&backend.base_url, &backend.model).await?,
      ProviderKind::OpenAI => {
        let key = backend
          .api_key
          .as_deref()
          .ok_or_else(|| RepositoryError::StorageError("缺少 OpenAI API Key".to_string()))?;
        verify_chat_openai(
          &backend.base_url,
          key,
          backend.organization.as_deref(),
          backend.project.as_deref(),
          &backend.model,
        )
        .await?;
      }
    }
  }

  Ok(())
}

/// 列出 Ollama 模型
async fn list_models_ollama(base_url: &str) -> Result<Vec<String>> {
  #[derive(Deserialize)]
  struct OllamaModelItem {
    name: String,
  }
  #[derive(Deserialize)]
  struct OllamaTagsResp {
    models: Vec<OllamaModelItem>,
  }

  let client = reqwest::Client::builder()
    .timeout(std::time::Duration::from_secs(10))
    .build()
    .map_err(|e| RepositoryError::QueryFailed(format!("创建 HTTP 客户端失败: {}", e)))?;

  let mut url = Url::parse(base_url).map_err(|_| RepositoryError::StorageError("Ollama 地址无效".to_string()))?;
  url.set_path("/api/tags");
  let resp = client
    .get(url)
    .send()
    .await
    .map_err(|e| RepositoryError::StorageError(format!("无法连接 Ollama：{}", e)))?;
  if !resp.status().is_success() {
    let status = resp.status();
    let text = resp.text().await.unwrap_or_default();
    return Err(RepositoryError::StorageError(format!(
      "Ollama 响应失败：{} {}",
      status, text
    )));
  }
  let data: OllamaTagsResp = resp
    .json()
    .await
    .map_err(|e| RepositoryError::StorageError(format!("解析 Ollama 模型列表失败: {}", e)))?;
  Ok(data.models.into_iter().map(|m| m.name).collect())
}

/// 列出 OpenAI 模型
async fn list_models_openai(
  base_url: &str,
  api_key: &str,
  organization: Option<&str>,
  project: Option<&str>,
) -> Result<Vec<String>> {
  #[derive(Deserialize)]
  struct OpenAIModelItem {
    id: String,
  }
  #[derive(Deserialize)]
  struct OpenAIModelsResp {
    data: Vec<OpenAIModelItem>,
  }

  debug!("开始列出 OpenAI 模型: {}", base_url);

  let client = reqwest::Client::builder()
    .timeout(std::time::Duration::from_secs(10))
    .build()
    .map_err(|e| RepositoryError::QueryFailed(format!("创建 HTTP 客户端失败: {}", e)))?;

  let mut base = Url::parse(base_url).map_err(|_| RepositoryError::StorageError("OpenAI 基础地址无效".to_string()))?;
  if !base.path().ends_with('/') {
    base.set_path(&format!("{}/", base.path()));
  }
  let url = base
    .join("models")
    .map_err(|_| RepositoryError::StorageError("无法构建 OpenAI API URL".to_string()))?;

  debug!("OpenAI 列表请求 URL: {}", url);

  let mut req = client.get(url).bearer_auth(api_key.to_string());
  if let Some(org) = organization {
    req = req.header("OpenAI-Organization", org);
  }
  if let Some(proj) = project {
    req = req.header("OpenAI-Project", proj);
  }

  let resp = req
    .send()
    .await
    .map_err(|e| RepositoryError::StorageError(format!("无法连接 OpenAI：{}", e)))?;
  if !resp.status().is_success() {
    let status = resp.status();
    let text = resp.text().await.unwrap_or_default();
    warn!("OpenAI 模型列表失败: {} {}", status, text);
    return Err(RepositoryError::StorageError(format!(
      "OpenAI 列出模型失败：{} {}",
      status, text
    )));
  }

  let data: OpenAIModelsResp = resp
    .json()
    .await
    .map_err(|e| RepositoryError::StorageError(format!("解析 OpenAI 模型列表失败: {}", e)))?;
  Ok(data.data.into_iter().map(|m| m.id).collect())
}

/// 基于已保存后端列出模型
pub async fn list_models_for_backend(pool: &SqlitePool, name: &str) -> Result<Vec<String>> {
  let backend = get_backend(pool, name)
    .await?
    .ok_or_else(|| RepositoryError::NotFound(format!("后端不存在: {}", name)))?;
  match backend.provider {
    ProviderKind::Ollama => list_models_ollama(&backend.base_url).await,
    ProviderKind::OpenAI => {
      let key = backend
        .api_key
        .as_deref()
        .ok_or_else(|| RepositoryError::StorageError("未配置 OpenAI API Key，无法列出模型".to_string()))?;
      list_models_openai(
        &backend.base_url,
        key,
        backend.organization.as_deref(),
        backend.project.as_deref(),
      )
      .await
    }
  }
}

/// 基于临时参数列出模型
pub async fn list_models_with_params(
  provider: ProviderKind,
  base_url: &str,
  api_key: Option<&str>,
  organization: Option<&str>,
  project: Option<&str>,
) -> Result<Vec<String>> {
  match provider {
    ProviderKind::Ollama => list_models_ollama(base_url).await,
    ProviderKind::OpenAI => {
      let key = api_key.ok_or_else(|| RepositoryError::StorageError("缺少 OpenAI API Key".to_string()))?;
      list_models_openai(base_url, key, organization, project).await
    }
  }
}

///// 最小对话探针：Ollama
async fn verify_chat_ollama(base_url: &str, model: &str) -> Result<()> {
  let client = reqwest::Client::builder()
    .timeout(std::time::Duration::from_secs(10))
    .build()
    .map_err(|e| RepositoryError::QueryFailed(format!("创建 HTTP 客户端失败: {}", e)))?;

  let mut url = Url::parse(base_url).map_err(|_| RepositoryError::StorageError("Ollama 地址无效".to_string()))?;
  url.set_path("/api/chat");

  let body = serde_json::json!({
    "model": model,
    "messages": [
      {"role": "user", "content": "ping"}
    ],
    "stream": false
  });

  let resp = client
    .post(url)
    .json(&body)
    .send()
    .await
    .map_err(|e| RepositoryError::StorageError(format!("Ollama 对话探针失败: {}", e)))?;
  if !resp.status().is_success() {
    let status = resp.status();
    let text = resp.text().await.unwrap_or_default();
    return Err(RepositoryError::StorageError(format!(
      "Ollama 对话探针失败：{} {}",
      status, text
    )));
  }
  Ok(())
}

/// 最小对话探针：OpenAI
async fn verify_chat_openai(
  base_url: &str,
  api_key: &str,
  organization: Option<&str>,
  project: Option<&str>,
  model: &str,
) -> Result<()> {
  let client = reqwest::Client::builder()
    .timeout(std::time::Duration::from_secs(10))
    .build()
    .map_err(|e| RepositoryError::QueryFailed(format!("创建 HTTP 客户端失败: {}", e)))?;

  let mut base = Url::parse(base_url).map_err(|_| RepositoryError::StorageError("OpenAI 基础地址无效".to_string()))?;
  let existing = base.path().trim_end_matches('/');
  let chat_path = if existing.is_empty() || existing == "/" {
    "/v1/chat/completions".to_string()
  } else {
    format!("{}/chat/completions", existing)
  };
  base.set_path(&chat_path);

  let body = serde_json::json!({
    "model": model,
    "messages": [
      {"role": "user", "content": "ping"}
    ],
    "max_tokens": 1
  });

  let mut req = client.post(base).json(&body).bearer_auth(api_key.to_string());
  if let Some(org) = organization {
    req = req.header("OpenAI-Organization", org);
  }
  if let Some(proj) = project {
    req = req.header("OpenAI-Project", proj);
  }

  let resp = req
    .send()
    .await
    .map_err(|e| RepositoryError::StorageError(format!("OpenAI 对话探针失败: {}", e)))?;
  if !resp.status().is_success() {
    let status = resp.status();
    let text = resp.text().await.unwrap_or_default();
    return Err(RepositoryError::StorageError(format!(
      "OpenAI 对话探针失败：{} {}",
      status, text
    )));
  }
  Ok(())
}
