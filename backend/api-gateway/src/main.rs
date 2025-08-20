
use axum::http::Method;
use axum::{Router, routing::get, routing::get_service};
use logsearch::router as logsearch_router;
use tower_http::cors::{Any, CorsLayer};
use tower_http::services::ServeDir;

#[tokio::main]
async fn main() {

    // CORS: allow cross-origin from any origin (adjust in production)
    let cors = CorsLayer::new()
        .allow_methods([Method::GET, Method::POST, Method::OPTIONS])
        .allow_headers(Any)
        .allow_origin(Any);

    let app = Router::new()
        .route("/healthz", get(|| async { "ok" }))
        .nest("/api/v1/logsearch", logsearch_router())
        .fallback_service(
            get_service(
                ServeDir::new(concat!(env!("CARGO_MANIFEST_DIR"), "/static"))
                    .append_index_html_on_directories(true),
            )
            .handle_error(|_| async {
                (
                    axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                    "static error",
                )
            }),
        )
        .layer(cors);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:4000")
        .await
        .unwrap();
    axum::serve(listener, app).await.unwrap();
}
