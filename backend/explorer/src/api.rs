//! Explorer API 路由
//!
//! 提供文件资源列表和下载的 HTTP API

use axum::{
  Router,
  body::Body,
  extract::{Json, Query, State},
  http::header,
  response::IntoResponse,
  routing::{get, post},
};
use opsbox_core::SuccessResponse;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio_util::io::ReaderStream;

use crate::domain::ResourceItem;
use crate::service::ExplorerService;

pub struct AppState {
  service: Arc<ExplorerService>,
}

#[derive(Debug, Deserialize)]
pub struct ListRequest {
  pub orl: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ListResponse {
  pub items: Vec<ResourceItem>,
}

pub fn router(service: Arc<ExplorerService>) -> Router {
  let state = Arc::new(AppState { service });
  Router::new()
    .route("/list", post(list_resources))
    .route("/download", get(download_resource))
    .with_state(state)
}

async fn list_resources(
  State(state): State<Arc<AppState>>,
  Json(payload): Json<ListRequest>,
) -> opsbox_core::Result<impl IntoResponse> {
  // 记录收到的 ORL 请求（截断过长的 ORL 以便于阅读）
  let orl_display = if payload.orl.len() > 100 {
    format!("{}... (total {} chars)", &payload.orl[..100], payload.orl.len())
  } else {
    payload.orl.clone()
  };
  tracing::info!("收到 explorer/list 请求，ORL: {}", orl_display);

  let items = state
    .service
    .list(&payload.orl)
    .await
    .map_err(opsbox_core::AppError::internal)?;

  Ok(Json(SuccessResponse {
    success: true,
    message: Some("success".to_string()),
    data: Some(ListResponse { items }),
  }))
}

async fn download_resource(
  State(state): State<Arc<AppState>>,
  Query(payload): Query<ListRequest>,
) -> Result<impl IntoResponse, opsbox_core::AppError> {
  let (filename, size, reader) = state
    .service
    .download(&payload.orl)
    .await
    .map_err(opsbox_core::AppError::internal)?;

  let stream = ReaderStream::new(reader);
  let body = Body::from_stream(stream);

  let mut headers = header::HeaderMap::new();
  headers.insert(
    header::CONTENT_TYPE,
    header::HeaderValue::from_static("application/octet-stream"),
  );

  // Simple content disposition
  // For proper handling of UTF-8 filenames, we should use RFC 5987.
  // encoding the filename
  let encoded_filename = urlencoding::encode(&filename);
  let disposition = format!("attachment; filename*=UTF-8''{}", encoded_filename);

  headers.insert(
    header::CONTENT_DISPOSITION,
    header::HeaderValue::from_str(&disposition)
      .unwrap_or(header::HeaderValue::from_static("attachment; filename=download")),
  );

  if let Some(s) = size {
    headers.insert(header::CONTENT_LENGTH, header::HeaderValue::from(s));
  }

  Ok((headers, body))
}
