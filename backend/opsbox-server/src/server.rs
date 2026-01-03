use axum::http::{StatusCode, header::CONTENT_TYPE};
use axum::{Router, http, response::Response, routing::get};
use opsbox_core::logging::ReloadHandle;
use opsbox_core::{Module, SqlitePool};
use rust_embed::RustEmbed;
use std::borrow::Cow;
use std::net::SocketAddr;
use std::sync::{Arc, OnceLock};
use tokio::sync::Notify;

/// 全局日志重载句柄
static LOG_RELOAD_HANDLE: OnceLock<ReloadHandle> = OnceLock::new();

/// 全局日志目录
static LOG_DIR: OnceLock<std::path::PathBuf> = OnceLock::new();

/// 设置日志重载句柄（在启动时调用一次）
pub fn set_log_reload_handle(handle: ReloadHandle) {
  LOG_RELOAD_HANDLE.set(handle).unwrap_or_else(|_| {
    panic!("日志重载句柄已被设置");
  });
}

/// 获取日志重载句柄（用于 API 调用）
pub fn get_log_reload_handle() -> Option<&'static ReloadHandle> {
  LOG_RELOAD_HANDLE.get()
}

/// 设置日志目录（在启动时调用一次）
pub fn set_log_dir(log_dir: std::path::PathBuf) {
  LOG_DIR.set(log_dir).expect("日志目录已被设置");
}

/// 获取日志目录（用于 API 调用）
pub fn get_log_dir() -> Option<&'static std::path::PathBuf> {
  LOG_DIR.get()
}

// 将 backend/opsbox-server/static 目录在编译期打包进二进制
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

    // 缓存策略：对带哈希文件名或静态字体启用长期缓存
    let cache_header: Cow<'static, str> = if (candidate.contains('.')
      && candidate
        .split('.')
        .any(|s| s.len() >= 8 && s.chars().all(|c| c.is_ascii_alphanumeric())))
      || candidate.ends_with(".woff2")
    {
      // 构建产物或静态字体，允许长缓存（1年）
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

/// 构建关闭通知器：监听系统信号，清理模块，然后通知 Axum 与后台任务
fn create_shutdown_notify(modules: Vec<Arc<dyn Module>>) -> Arc<Notify> {
  let notify = Arc::new(Notify::new());
  let notify_clone = notify.clone();
  tokio::spawn(async move {
    // 等待系统信号
    let signal_name = wait_for_shutdown_signal().await;
    tracing::info!("收到关闭信号 [{}]，开始优雅关闭...", signal_name);

    // 清理模块资源
    for module in &modules {
      tracing::info!("清理模块: {}", module.name());
      module.cleanup();
    }
    tracing::info!("所有模块已清理完成，通知服务优雅关闭...");

    // 通知 Axum 停止接受新连接
    notify_clone.notify_waiters();

    // 10 秒后若仍未退出，强制结束进程（与旧实现对齐）
    tokio::spawn(async move {
      tokio::time::sleep(std::time::Duration::from_secs(10)).await;
      tracing::warn!("优雅关闭超时（10秒），仍有活跃连接未关闭，强制退出");
      std::process::exit(0);
    });
  });
  notify
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

  // 注册系统级日志配置路由
  let log_dir = get_log_dir()
    .cloned()
    .unwrap_or_else(|| std::path::PathBuf::from("logs"));
  let log_routes = crate::log_routes::create_log_routes(db_pool.clone(), log_dir);
  app = app.merge(log_routes);
  tracing::info!("注册路由: 日志配置 -> /api/v1/log/*");

  // ✅ 动态注册所有模块路由
  for module in modules {
    let prefix = module.api_prefix();
    let router = module.router(db_pool.clone());
    tracing::info!("注册路由: {} -> {}", module.name(), prefix);
    app = app.nest(prefix, router);
  }

  // SPA fallback（必须放最后）
  app = app.fallback(get(spa_fallback));

  app
}

/// 运行HTTP服务器
pub async fn run(addr: SocketAddr, db_pool: SqlitePool, modules: Vec<Arc<dyn Module>>) {
  tracing::info!("启动 HTTP 服务器，监听地址: {}", addr);

  // 构建应用
  let app = build_router(db_pool, &modules);

  // 绑定监听
  let listener = tokio::net::TcpListener::bind(addr).await.expect("监听地址绑定失败");

  let svc = app.into_make_service_with_connect_info::<SocketAddr>();

  tracing::info!("OpsBox 服务启动成功，访问地址: http://{}", addr);

  // 与 Agent 对齐：使用 Notify 驱动优雅关闭
  let shutdown_notify = create_shutdown_notify(modules);
  axum::serve(listener, svc)
    .with_graceful_shutdown(async move {
      shutdown_notify.notified().await;
    })
    .await
    .expect("服务启动失败");

  tracing::info!("服务已关闭");
}
