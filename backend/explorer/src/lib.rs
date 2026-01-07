pub mod api;
pub mod domain;
pub mod service;

use async_trait::async_trait;
use axum::Router;
use opsbox_core::{Module, SqlitePool, register_module};
use std::sync::Arc;

#[derive(Default)]
pub struct ExplorerModule;

#[async_trait]
impl Module for ExplorerModule {
  fn name(&self) -> &'static str {
    "explorer"
  }

  fn api_prefix(&self) -> &'static str {
    "/api/v1/explorer"
  }

  async fn init_schema(&self, _pool: &SqlitePool) -> Result<(), Box<dyn std::error::Error>> {
    Ok(())
  }

  fn router(&self, pool: SqlitePool) -> Router {
    let service = Arc::new(service::ExplorerService::new(pool));
    api::router(service)
  }
}

register_module!(ExplorerModule);
