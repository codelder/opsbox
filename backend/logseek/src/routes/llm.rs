//! 大模型（LLM）设置路由
//!
//! 提供 LLM 后端的增删改查，以及默认后端设置

use axum::{
  extract::{Json, Path, State},
  http::StatusCode,
};
use opsbox_core::SqlitePool;
use serde::{Deserialize, Serialize};

use crate::api::LogSeekApiError;
use crate::repository::llm::{self, LlmBackend, LlmBackendPublic, ProviderKind};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProviderKindPayload {
  Ollama,
  OpenAI,
}

impl From<ProviderKindPayload> for ProviderKind {
  fn from(p: ProviderKindPayload) -> Self {
    match p {
      ProviderKindPayload::Ollama => ProviderKind::Ollama,
      ProviderKindPayload::OpenAI => ProviderKind::OpenAI,
    }
  }
}

impl From<ProviderKind> for ProviderKindPayload {
  fn from(p: ProviderKind) -> Self {
    match p {
      ProviderKind::Ollama => ProviderKindPayload::Ollama,
      ProviderKind::OpenAI => ProviderKindPayload::OpenAI,
    }
  }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmBackendUpsertPayload {
  /// 唯一名称
  pub name: String,
  /// 提供方类型
  pub provider: ProviderKindPayload,
  /// 基础地址
  pub base_url: String,
  /// 默认模型名
  pub model: String,
  /// 超时时间（秒），可选，默认 60
  #[serde(default)]
  pub timeout_secs: i64,
  /// OpenAI: API Key（可选；更新时留空表示不修改）
  #[serde(default)]
  pub api_key: Option<String>,
  /// OpenAI: 组织 ID（可选）
  #[serde(default)]
  pub organization: Option<String>,
  /// OpenAI: 项目 ID（可选）
  #[serde(default)]
  pub project: Option<String>,
  /// 是否进行严格校验（会做一次最小 chat 探针），默认否
  #[serde(default)]
  pub verify_strict: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmBackendListItem {
  pub name: String,
  pub provider: ProviderKindPayload,
  pub base_url: String,
  pub model: String,
  pub timeout_secs: i64,
  pub has_api_key: bool,
}

impl From<LlmBackendPublic> for LlmBackendListItem {
  fn from(v: LlmBackendPublic) -> Self {
    Self {
      name: v.name,
      provider: v.provider.into(),
      base_url: v.base_url,
      model: v.model,
      timeout_secs: v.timeout_secs,
      has_api_key: v.has_api_key,
    }
  }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmBackendListResponse {
  pub backends: Vec<LlmBackendListItem>,
  pub default: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmModelsResponse {
  pub models: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmModelsParamsPayload {
  pub provider: ProviderKindPayload,
  pub base_url: String,
  #[serde(default)]
  pub api_key: Option<String>,
  #[serde(default)]
  pub organization: Option<String>,
  #[serde(default)]
  pub project: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DefaultLlmPayload {
  pub name: String,
}

/// 列出所有 LLM 后端
pub async fn list_backends(State(pool): State<SqlitePool>) -> Result<Json<LlmBackendListResponse>, LogSeekApiError> {
  let list = llm::list_backends(&pool).await?;
  let default = llm::get_default(&pool).await?;
  Ok(Json(LlmBackendListResponse {
    backends: list.into_iter().map(Into::into).collect(),
    default,
  }))
}

/// 新建或更新 LLM 后端
pub async fn upsert_backend(
  State(pool): State<SqlitePool>,
  Json(payload): Json<LlmBackendUpsertPayload>,
) -> Result<StatusCode, LogSeekApiError> {
  let provider: ProviderKind = payload.provider.into();
  let timeout = if payload.timeout_secs <= 0 {
    60
  } else {
    payload.timeout_secs
  };

  // 构造内部对象
  let mut backend = LlmBackend {
    name: payload.name.trim().to_string(),
    provider,
    base_url: payload.base_url.trim().to_string(),
    model: payload.model.trim().to_string(),
    timeout_secs: timeout,
    api_key: payload
      .api_key
      .clone()
      .map(|s| s.trim().to_string())
      .filter(|s| !s.is_empty()),
    organization: payload
      .organization
      .clone()
      .map(|s| s.trim().to_string())
      .filter(|s| !s.is_empty()),
    project: payload
      .project
      .clone()
      .map(|s| s.trim().to_string())
      .filter(|s| !s.is_empty()),
  };

  // 检查是否已有存在记录
  let existing = llm::get_backend(&pool, &backend.name).await?;
  let update_secret = match (
    existing.as_ref().and_then(|b| b.api_key.as_ref()),
    backend.api_key.as_ref(),
  ) {
    (_, Some(_)) => true,     // 显式提供了新密钥 → 覆盖
    (Some(_), None) => false, // 未提供 → 保留旧密钥
    (None, None) => false,
  };
  if !update_secret {
    // 将 api_key 设回原值（防止被置空）
    backend.api_key = existing.and_then(|b| b.api_key);
  }

  // 保存前验证：基于模型列表；可选严格探针
  llm::verify_backend(&backend, payload.verify_strict).await?;

  // 持久化
  llm::save_backend(&pool, &backend, update_secret).await?;

  Ok(StatusCode::NO_CONTENT)
}

/// 删除 LLM 后端
pub async fn delete_backend(
  State(pool): State<SqlitePool>,
  Path(name): Path<String>,
) -> Result<StatusCode, LogSeekApiError> {
  llm::delete_backend(&pool, &name).await?;
  Ok(StatusCode::NO_CONTENT)
}

/// 列出指定已保存后端的可用模型
pub async fn list_models_by_backend(
  State(pool): State<SqlitePool>,
  Path(name): Path<String>,
) -> Result<Json<LlmModelsResponse>, LogSeekApiError> {
  let models = llm::list_models_for_backend(&pool, &name).await?;
  Ok(Json(LlmModelsResponse { models }))
}

/// 基于临时参数列出可用模型（用于前端编辑未保存配置时）
pub async fn list_models_by_params(
  Json(payload): Json<LlmModelsParamsPayload>,
) -> Result<Json<LlmModelsResponse>, LogSeekApiError> {
  let provider: ProviderKind = payload.provider.into();
  let models = llm::list_models_with_params(
    provider,
    &payload.base_url,
    payload.api_key.as_deref(),
    payload.organization.as_deref(),
    payload.project.as_deref(),
  )
  .await?;
  Ok(Json(LlmModelsResponse { models }))
}

/// 获取默认 LLM 后端
pub async fn get_default(State(pool): State<SqlitePool>) -> Result<Json<Option<String>>, LogSeekApiError> {
  let name = llm::get_default(&pool).await?;
  Ok(Json(name))
}

/// 设置默认 LLM 后端
pub async fn set_default(
  State(pool): State<SqlitePool>,
  Json(body): Json<DefaultLlmPayload>,
) -> Result<StatusCode, LogSeekApiError> {
  llm::set_default(&pool, Some(&body.name)).await?;
  Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::repository::llm::init_schema;

  async fn setup_test_db() -> SqlitePool {
    let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
    init_schema(&pool).await.unwrap();
    pool
  }

  #[test]
  fn test_payload_conversions() {
    let p = ProviderKindPayload::Ollama;
    let k: ProviderKind = p.into();
    assert_eq!(k, ProviderKind::Ollama);

    let p2: ProviderKindPayload = k.into();
    assert!(matches!(p2, ProviderKindPayload::Ollama));
  }

  #[tokio::test]
  async fn test_llm_default_routes() {
    let pool = setup_test_db().await;

    // Initially none
    let resp = get_default(State(pool.clone())).await.unwrap();
    assert!(resp.0.is_none());

    // We can't easily set default to a non-existent backend because set_default checks for existence
    // But we can test the API error if it fails
    let res = set_default(
      State(pool.clone()),
      Json(DefaultLlmPayload {
        name: "non-existent".into(),
      }),
    )
    .await;
    assert!(res.is_err());
  }

  #[tokio::test]
  async fn test_list_backends_empty() {
    let pool = setup_test_db().await;
    let resp = list_backends(State(pool)).await.unwrap();
    assert_eq!(resp.backends.len(), 0);
    assert!(resp.default.is_none());
  }

  #[test]
  fn test_provider_kind_payload_openai() {
    let p = ProviderKindPayload::OpenAI;
    let k: ProviderKind = p.into();
    assert_eq!(k, ProviderKind::OpenAI);

    let p2: ProviderKindPayload = k.into();
    assert!(matches!(p2, ProviderKindPayload::OpenAI));
  }

  #[test]
  fn test_llm_backend_list_item_from_public() {
    let public = crate::repository::llm::LlmBackendPublic {
      name: "test".to_string(),
      provider: ProviderKind::Ollama,
      base_url: "http://localhost:11434".to_string(),
      model: "llama2".to_string(),
      timeout_secs: 60,
      has_api_key: false,
    };

    let item: LlmBackendListItem = public.into();
    assert_eq!(item.name, "test");
    assert!(matches!(item.provider, ProviderKindPayload::Ollama));
    assert_eq!(item.base_url, "http://localhost:11434");
    assert_eq!(item.model, "llama2");
    assert_eq!(item.timeout_secs, 60);
    assert!(!item.has_api_key);
  }

  #[test]
  fn test_llm_models_response() {
    let resp = LlmModelsResponse {
      models: vec!["model1".to_string(), "model2".to_string()],
    };
    let json = serde_json::to_string(&resp).unwrap();
    assert!(json.contains("model1"));
    assert!(json.contains("model2"));
  }

  #[test]
  fn test_default_llm_payload() {
    let payload = DefaultLlmPayload {
      name: "my-backend".to_string(),
    };
    assert_eq!(payload.name, "my-backend");
  }

  #[tokio::test]
  async fn test_list_models_by_backend_not_found() {
    let pool = setup_test_db().await;
    let result = list_models_by_backend(State(pool), Path("non-existent".to_string())).await;
    // Should return error because backend doesn't exist
    assert!(result.is_err());
  }

  #[tokio::test]
  async fn test_upsert_backend_validation() {
    // Test payload with empty name (should still work but trimmed)
    let payload = LlmBackendUpsertPayload {
      name: "  test-backend  ".to_string(),
      provider: ProviderKindPayload::Ollama,
      base_url: "  http://localhost:11434  ".to_string(),
      model: "  llama2  ".to_string(),
      timeout_secs: 0,                 // Will be clamped to 60
      api_key: Some("  ".to_string()), // Will be filtered out
      organization: None,
      project: None,
      verify_strict: false,
    };

    assert_eq!(payload.name.trim(), "test-backend");
    assert_eq!(payload.base_url.trim(), "http://localhost:11434");
    assert_eq!(payload.model.trim(), "llama2");
  }
}
