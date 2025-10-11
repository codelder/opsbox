use axum::http::{StatusCode, header::ACCEPT, header::CONTENT_TYPE};
use axum::{Router, http, response::Response, routing::get};
use opsbox_core::{Module, SqlitePool};
use rust_embed::RustEmbed;
use std::borrow::Cow;
use std::net::SocketAddr;
use std::sync::Arc;
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
      && candidate
        .split('.')
        .any(|s| s.len() >= 8 && s.chars().all(|c| c.is_ascii_alphanumeric()))
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
      http::HeaderValue::from_str(mime.as_ref()).unwrap_or(http::HeaderValue::from_static("application/octet-stream")),
    );
    headers.insert(
      http::header::CACHE_CONTROL,
      http::HeaderValue::from_str(&cache_header).unwrap_or(http::HeaderValue::from_static("public, max-age=300")),
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

/// 优雅关闭信号（带超时）
///
/// 监听系统信号实现优雅关闭：
/// - Unix: SIGTERM, SIGINT (Ctrl-C)
/// - Windows: Ctrl-C
///
/// 优雅关闭流程：
/// 1. 等待关闭信号
/// 2. 清理模块资源
/// 3. 返回并触发 Axum 停止接受新连接
/// 4. Axum 等待现有连接完成（本函数返回后）
///
/// 注意: 本函数只处理到步骤3，Axum 会在本函数返回后继续等待连接关闭。
/// 为了避免永久等待，我们在外层设置超时。
async fn shutdown_with_timeout(modules: Vec<Arc<dyn Module>>) {
  // 等待关闭信号
  let signal_name = wait_for_shutdown_signal().await;
  log::info!("收到关闭信号 [{}]，开始优雅关闭...", signal_name);

  // 清理所有模块资源
  for module in &modules {
    log::info!("清理模块: {}", module.name());
    module.cleanup();
  }

  log::info!("所有模块已清理完成，等待活跃连接关闭...");

  // 在后台启动超时任务
  // 如果10秒后还有连接未关闭，强制退出进程
  tokio::spawn(async move {
    tokio::time::sleep(std::time::Duration::from_secs(10)).await;
    log::warn!("优雅关闭超时（10秒），仍有活跃连接未关闭");
    log::warn!("强制退出进程...");
    std::process::exit(0);
  });

  // 返回后，Axum 会停止接受新连接并等待现有连接关闭
  // 如果10秒内连接都关闭了，上面的 spawn 任务不会执行 exit
  // 如果10秒后还有连接，上面的 spawn 任务会强制退出
}

/// 等待关闭信号并返回信号名称
#[cfg(unix)]
async fn wait_for_shutdown_signal() -> &'static str {
  use tokio::signal::unix::{SignalKind, signal};

  // 创建信号监听器
  let mut sigterm = signal(SignalKind::terminate()).expect("无法监听 SIGTERM");
  let mut sigint = signal(SignalKind::interrupt()).expect("无法监听 SIGINT");

  tokio::select! {
    _ = sigterm.recv() => "SIGTERM",
    _ = sigint.recv() => "SIGINT (Ctrl-C)",
  }
}

/// 等待关闭信号并返回信号名称 (Windows)
#[cfg(not(unix))]
async fn wait_for_shutdown_signal() -> &'static str {
  tokio::signal::ctrl_c().await.expect("无法监听 Ctrl-C 信号");
  "Ctrl-C"
}

/// 构建应用路由（动态加载所有模块）
fn build_router(db_pool: SqlitePool, modules: &[Arc<dyn Module>]) -> Router {
  let mut app = Router::new()
    // 健康检查
    .route("/healthy", get(|| async { "ok" }));

  // ✅ 动态注册所有模块路由
  for module in modules {
    let prefix = module.api_prefix();
    let router = module.router(db_pool.clone());
    log::info!("注册路由: {} -> {}", module.name(), prefix);
    app = app.nest(prefix, router);
  }

  // SPA fallback（必须放最后）
  app = app.fallback(get(spa_fallback));

  app
}

/// 配置 CORS
fn configure_cors() -> CorsLayer {
  CorsLayer::new()
    .allow_origin(Any)
    .allow_methods([http::Method::GET, http::Method::POST, http::Method::OPTIONS])
    .allow_headers([CONTENT_TYPE, ACCEPT])
    .expose_headers([http::header::HeaderName::from_static("x-logseek-sid")])
}

/// 运行HTTP服务器
pub async fn run(addr: SocketAddr, db_pool: SqlitePool, modules: Vec<Arc<dyn Module>>) {
  log::info!("启动 HTTP 服务器，监听地址: {}", addr);

  // 构建应用
  let app = build_router(db_pool, &modules).layer(configure_cors());

  // 绑定监听
  let listener = tokio::net::TcpListener::bind(addr).await.expect("监听地址绑定失败");

  log::info!("OpsBox 服务启动成功，访问地址: http://{}", addr);

  // 启动服务器并支持优雅关闭
  // 注意: with_graceful_shutdown 只在收到信号后才开始关闭流程
  axum::serve(listener, app)
    .with_graceful_shutdown(shutdown_with_timeout(modules))
    .await
    .expect("服务启动失败");

  log::info!("服务已关闭");
}
