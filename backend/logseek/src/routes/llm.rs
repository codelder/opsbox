//! 大模型（LLM）设置路由
//!
//! 提供 LLM 后端的增删改查，以及默认后端设置

use axum::{
  extract::{Json, Path, State},
  http::StatusCode,
};
use opsbox_core::SqlitePool;
use problemdetails::Problem;
use serde::{Deserialize, Serialize};

use crate::api::models::AppError;
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
pub async fn list_backends(State(pool): State<SqlitePool>) -> Result<Json<LlmBackendListResponse>, Problem> {
  let list = llm::list_backends(&pool).await.map_err(AppError::Settings)?;
  let default = llm::get_default(&pool).await.map_err(AppError::Settings)?;
  Ok(Json(LlmBackendListResponse {
    backends: list.into_iter().map(Into::into).collect(),
    default,
  }))
}

/// 新建或更新 LLM 后端
pub async fn upsert_backend(
  State(pool): State<SqlitePool>,
  Json(payload): Json<LlmBackendUpsertPayload>,
) -> Result<StatusCode, Problem> {
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
  let existing = llm::get_backend(&pool, &backend.name)
    .await
    .map_err(AppError::Settings)?;
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
  llm::verify_backend(&backend, payload.verify_strict)
    .await
    .map_err(AppError::Settings)?;

  // 持久化
  llm::save_backend(&pool, &backend, update_secret)
    .await
    .map_err(AppError::Settings)?;

  Ok(StatusCode::NO_CONTENT)
}

/// 删除 LLM 后端
pub async fn delete_backend(State(pool): State<SqlitePool>, Path(name): Path<String>) -> Result<StatusCode, Problem> {
  llm::delete_backend(&pool, &name).await.map_err(AppError::Settings)?;
  Ok(StatusCode::NO_CONTENT)
}

/// 列出指定已保存后端的可用模型
pub async fn list_models_by_backend(
  State(pool): State<SqlitePool>,
  Path(name): Path<String>,
) -> Result<Json<LlmModelsResponse>, Problem> {
  let models = llm::list_models_for_backend(&pool, &name)
    .await
    .map_err(AppError::Settings)?;
  Ok(Json(LlmModelsResponse { models }))
}

/// 基于临时参数列出可用模型（用于前端编辑未保存配置时）
pub async fn list_models_by_params(
  Json(payload): Json<LlmModelsParamsPayload>,
) -> Result<Json<LlmModelsResponse>, Problem> {
  let provider: ProviderKind = payload.provider.into();
  let models = llm::list_models_with_params(
    provider,
    &payload.base_url,
    payload.api_key.as_deref(),
    payload.organization.as_deref(),
    payload.project.as_deref(),
  )
  .await
  .map_err(AppError::Settings)?;
  Ok(Json(LlmModelsResponse { models }))
}

/// 获取默认 LLM 后端
pub async fn get_default(State(pool): State<SqlitePool>) -> Result<Json<Option<String>>, Problem> {
  let name = llm::get_default(&pool).await.map_err(AppError::Settings)?;
  Ok(Json(name))
}

/// 设置默认 LLM 后端
pub async fn set_default(
  State(pool): State<SqlitePool>,
  Json(body): Json<DefaultLlmPayload>,
) -> Result<StatusCode, Problem> {
  llm::set_default(&pool, Some(&body.name))
    .await
    .map_err(AppError::Settings)?;
  Ok(StatusCode::NO_CONTENT)
}
