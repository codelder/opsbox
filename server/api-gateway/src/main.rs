use axum::http::{Method, StatusCode, header::{CONTENT_TYPE, CACHE_CONTROL}};
use axum::{Router, routing::get, response::{IntoResponse, Response}};
use axum::http;
use logsearch::router as logsearch_router;
use rust_embed::RustEmbed;
use std::borrow::Cow;
use tower_http::cors::{Any, CorsLayer};

// 中文注释：将 server/api-gateway/static 目录在编译期打包进二进制
#[derive(RustEmbed)]
#[folder = "static"]
struct Assets;

fn serve_embedded(path: &str) -> Option<Response> {
  // 去掉路径前导斜杠，避免与嵌入资源键不匹配
  let path = path.trim_start_matches('/');
  // 空路径或目录默认返回 index.html（SPA）
  let candidate = if path.is_empty() { "index.html" } else { path };

  if let Some(content) = Assets::get(candidate) {
    // 识别 MIME 类型
    let mime = mime_guess::from_path(candidate).first_or_octet_stream();

    // 缓存策略：对带哈希文件名启用长期缓存，否则适度缓存
    let cache_header: Cow<'static, str> = if candidate.contains('.') && candidate.contains(".") && candidate.contains(".") {
      // 简化判断：构建产物通常带哈希，允许长缓存（1年）
      Cow::from("public, max-age=31536000, immutable")
    } else {
      Cow::from("public, max-age=300")
    };

    let mut resp = Response::new(axum::body::Body::from(match content.data {
      Cow::Borrowed(b) => b.to_vec(),
      Cow::Owned(b) => b,
    }));
    let headers = resp.headers_mut();
    headers.insert(CONTENT_TYPE, http::HeaderValue::from_str(mime.as_ref()).unwrap_or(http::HeaderValue::from_static("application/octet-stream")));
    headers.insert(CACHE_CONTROL, http::HeaderValue::from_str(&cache_header).unwrap_or(http::HeaderValue::from_static("public, max-age=300")));
    Some(resp)
  } else {
    None
  }
}

async fn spa_fallback(uri: http::Uri) -> Response {
  let path = uri.path();
  if let Some(resp) = serve_embedded(path) {
    return resp;
  }
  // 未命中具体文件则回退到内嵌的 index.html
  if let Some(resp) = serve_embedded("index.html") {
    return resp;
  }
  http::Response::builder()
    .status(StatusCode::NOT_FOUND)
    .header(CONTENT_TYPE, "text/plain; charset=utf-8")
    .body(axum::body::Body::from("404 Not Found"))
    .unwrap()
}

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

  let app = Router::new()
    .route("/healthz", get(|| async { "ok" }))
    .nest("/api/v1/logsearch", logsearch_router())
    .fallback(get(spa_fallback))
    .layer(cors);

  let listener = tokio::net::TcpListener::bind("127.0.0.1:4000").await.unwrap();
  axum::serve(listener, app).await.unwrap();
}
