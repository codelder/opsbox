use axum::http::Method;
use axum::{Router, routing::get, routing::get_service};
use logsearch::router as logsearch_router;
use std::path::PathBuf;
use tower_http::cors::{Any, CorsLayer};
use tower_http::services::{ServeDir, ServeFile};

#[tokio::main]
async fn main() {
  logsearch::ensure_initialized()
    .await
    .expect("初始化设置存储失败");

  // CORS：允许任意来源跨域访问（生产环境请按需收紧）
  let cors = CorsLayer::new()
    .allow_methods([Method::GET, Method::POST, Method::OPTIONS])
    .allow_headers(Any)
    .allow_origin(Any);

  // 静态目录与 SPA 回退：当路径未命中具体文件时，返回 index.html
  let static_root: &'static str = concat!(env!("CARGO_MANIFEST_DIR"), "/static");
  let index_file: PathBuf = PathBuf::from(static_root).join("index.html");
  let static_service = ServeDir::new(static_root)
    .append_index_html_on_directories(true)
    .not_found_service(ServeFile::new(index_file));

  let app = Router::new()
    .route("/healthz", get(|| async { "ok" }))
    .nest("/api/v1/logsearch", logsearch_router())
    .fallback_service(
      get_service(static_service)
        .handle_error(|_| async { (axum::http::StatusCode::INTERNAL_SERVER_ERROR, "静态资源错误") }),
    )
    .layer(cors);

  let listener = tokio::net::TcpListener::bind("127.0.0.1:4000").await.unwrap();
  axum::serve(listener, app).await.unwrap();
}
