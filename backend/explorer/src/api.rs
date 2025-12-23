use axum::{
  Router,
  extract::{Json, State},
  response::IntoResponse,
  routing::post,
};
use opsbox_core::SuccessResponse;
use opsbox_core::odfi::Odfi;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::domain::ResourceItem;
use crate::service::ExplorerService;

pub struct AppState {
  service: Arc<ExplorerService>,
}

#[derive(Debug, Deserialize)]
pub struct ListRequest {
  pub odfi: String,
}

#[derive(Debug, Serialize)]
pub struct ListResponse {
  pub items: Vec<ResourceItem>,
}

pub fn router(service: Arc<ExplorerService>) -> Router {
  let state = Arc::new(AppState { service });
  Router::new().route("/list", post(list_resources)).with_state(state)
}

async fn list_resources(
  State(state): State<Arc<AppState>>,
  Json(payload): Json<ListRequest>,
) -> opsbox_core::Result<impl IntoResponse> {
  // Parse ODFI
  let odfi: Odfi = payload
    .odfi
    .parse()
    .map_err(|e| opsbox_core::AppError::bad_request(format!("Invalid ODFI: {}", e)))?;

  let items = state
    .service
    .list(&odfi)
    .await
    .map_err(opsbox_core::AppError::internal)?;

  Ok(Json(SuccessResponse {
    success: true,
    message: Some("success".to_string()),
    data: Some(ListResponse { items }),
  }))
}
