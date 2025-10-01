use axum::http::{header::ACCEPT, header::CONTENT_TYPE, StatusCode};
use axum::{http, response::Response, routing::get, Router};
use opsbox_core::SqlitePool;
use rust_embed::RustEmbed;
use std::borrow::Cow;
use std::net::SocketAddr;
use tower_http::cors::{Any, CorsLayer};

// 将 server/api-gateway/static 目录在编译期打包进二进制
#[derive(RustEmbed)]
#[folder = "static"]
struct Assets;

/// 服务嵌入的静态资源
fn serve_embedded(path: &str) -> Option<Response> {
    // 去掉路径前导斜杠
    let path = path.trim_start_matches('/');
    // 空路径或目录默认返回 index.html（SPA）
    let candidate = if path.is_empty() { "index.html" } else { path };

    if let Some(content) = Assets::get(candidate) {
        // 识别 MIME 类型
        let mime = mime_guess::from_path(candidate).first_or_octet_stream();

        // 缓存策略：对带哈希文件名启用长期缓存
        let cache_header: Cow<'static, str> = if candidate.contains('.') 
            && candidate.split('.').any(|s| s.len() >= 8 && s.chars().all(|c| c.is_ascii_alphanumeric())) 
        {
            // 构建产物通常带哈希，允许长缓存（1年）
            Cow::from("public, max-age=31536000, immutable")
        } else {
            Cow::from("public, max-age=300")
        };

        let mut resp = Response::new(axum::body::Body::from(match content.data {
            Cow::Borrowed(b) => b.to_vec(),
            Cow::Owned(b) => b,
        }));
        
        let headers = resp.headers_mut();
        headers.insert(
            http::header::CONTENT_TYPE,
            http::HeaderValue::from_str(mime.as_ref())
                .unwrap_or(http::HeaderValue::from_static("application/octet-stream")),
        );
        headers.insert(
            http::header::CACHE_CONTROL,
            http::HeaderValue::from_str(&cache_header)
                .unwrap_or(http::HeaderValue::from_static("public, max-age=300")),
        );
        Some(resp)
    } else {
        None
    }
}

/// SPA fallback处理器
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

/// 优雅关闭信号
async fn shutdown_signal() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{signal, SignalKind};
        let mut sigterm = signal(SignalKind::terminate()).expect("无法监听 SIGTERM");
        let mut sigint = signal(SignalKind::interrupt()).expect("无法监听 SIGINT");
        tokio::select! {
            _ = sigterm.recv() => {},
            _ = sigint.recv() => {},
            _ = tokio::signal::ctrl_c() => {},
        }
    }
    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
    }
    log::info!("收到关闭信号，开始优雅关闭...");
    // 通知后台清理任务退出
    logseek::simple_cache::Cache::stop_cleaner();
}

/// 构建应用路由
fn build_router(db_pool: SqlitePool) -> Router {
    Router::new()
        // 健康检查
        .route("/healthy", get(|| async { "ok" }))
        // LogSeek 模块路由
        .nest("/api/v1/logseek", logseek::router(db_pool))
        // SPA fallback（必须放最后）
        .fallback(get(spa_fallback))
}

/// 配置 CORS
fn configure_cors() -> CorsLayer {
    CorsLayer::new()
        .allow_origin(Any)
        .allow_methods([
            http::Method::GET,
            http::Method::POST,
            http::Method::OPTIONS,
        ])
        .allow_headers([CONTENT_TYPE, ACCEPT])
        .expose_headers([http::header::HeaderName::from_static("x-logseek-sid")])
}

/// 运行HTTP服务器
pub async fn run(addr: SocketAddr, db_pool: SqlitePool) {
    log::info!("启动 HTTP 服务器，监听地址: {}", addr);

    // 构建应用
    let app = build_router(db_pool).layer(configure_cors());

    // 绑定监听
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .expect("监听地址绑定失败");

    log::info!("OpsBox 服务启动成功，访问地址: http://{}", addr);

    // 启动服务器并支持优雅关闭
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .expect("服务启动失败");

    log::info!("服务已关闭");
}
