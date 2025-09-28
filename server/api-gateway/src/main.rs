use axum::http;
use axum::http::{
  header::{CACHE_CONTROL, CONTENT_TYPE},
  StatusCode,
};
use axum::{response::Response, routing::get, Router};
use logsearch::router as logsearch_router;
use rust_embed::RustEmbed;
use std::borrow::Cow;
use std::net::SocketAddr;
use clap::{Parser, Subcommand};
use std::path::{Path, PathBuf};
use std::{fs, io};
use log::LevelFilter;
use tower_http::cors::{Any, CorsLayer};
use http::header::{ACCEPT};

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
    let cache_header: Cow<'static, str> =
      if candidate.contains('.') && candidate.contains(".") && candidate.contains(".") {
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
    headers.insert(
      CONTENT_TYPE,
      http::HeaderValue::from_str(mime.as_ref()).unwrap_or(http::HeaderValue::from_static("application/octet-stream")),
    );
    headers.insert(
      CACHE_CONTROL,
      http::HeaderValue::from_str(&cache_header).unwrap_or(http::HeaderValue::from_static("public, max-age=300")),
    );
    Some(resp)
  } else {
    None
  }
}

/// 中文注释：将字符串解析为端口（u16），提供中文错误信息
fn port_parser(s: &str) -> Result<u16, String> {
  s.parse::<u16>()
    .map_err(|_| format!("无效的端口号：{s}"))
}

/// 中文注释：将字符串解析为 SocketAddr，提供中文错误信息
fn addr_parser(s: &str) -> Result<SocketAddr, String> {
  s.parse::<SocketAddr>()
    .map_err(|_| format!("无效的地址：{s}，请使用 HOST:PORT 或 [IPv6]:PORT 格式"))
}

/// 中文注释：子命令定义（使用 clap）
#[derive(Subcommand, Debug)]
pub enum Commands {
  /// 中文注释：启动服务（默认后台运行，可通过 --daemon=false 前台运行）
  Start {
    /// 中文注释：是否后台运行（默认 true，仅类 Unix 支持）
    #[arg(long, short = 'd', default_value_t = true)]
    daemon: bool,
    /// 中文注释：PID 文件路径（默认：~/.opsbox/api-gateway.pid）
    #[arg(long, value_name = "FILE")]
    pid_file: Option<PathBuf>,
  },
  /// 中文注释：停止服务（通过 PID 文件定位进程）
  Stop {
    /// 中文注释：PID 文件路径（默认：~/.opsbox/api-gateway.pid）
    #[arg(long, value_name = "FILE")]
    pid_file: Option<PathBuf>,
    /// 中文注释：强制停止（发送 SIGKILL）
    #[arg(long, short = 'f', default_value_t = false)]
    force: bool,
  },
}

/// 中文注释：命令行选项（使用 clap）
#[derive(Parser, Debug)]
#[command(
  author = "wangyue",
  name = "api-gateway",
  version,
  disable_version_flag = true,
  about = "LogSearch API 网关（内置前端静态资源）。支持通过参数设置监听地址/端口，并可选择后台运行（Unix）。",
  long_about = None
)]
struct Cli {
  #[arg(global = true, long, short = 'H', value_name = "HOST", default_value = "127.0.0.1", help = "监听地址（默认 127.0.0.1）")]
  host: String,

  #[arg(global = true, long, short = 'P', value_name = "PORT", default_value_t = 4000, value_parser = port_parser, help = "监听端口（默认 4000）")]
  port: u16,

  #[arg(global = true, long, short = 'a', value_name = "HOST:PORT", value_parser = addr_parser, help = "完整地址（优先于 --host/--port），如 0.0.0.0:8080 或 [::]:8080")]
  addr: Option<SocketAddr>,

  #[arg(long, short = 'd', default_value_t = false, help = "后台运行（仅 类Unix 支持）")]
  daemon: bool,

  #[arg(global = true, long = "log-level", value_name = "LEVEL", help = "日志级别：error|warn|info|debug|trace")]
  log_level: Option<String>,

  #[arg(global = true, short = 'V', action = clap::ArgAction::Count, help = "增加日志详细程度（可叠加，如 -V/-VV/-VVV）")]
  verbose: u8,

  // ====== 性能与资源参数（命令行>环境变量>默认值）======
  #[arg(long = "worker-threads", value_name = "N", help = "Tokio 工作线程数（覆盖 LOGSEARCH_WORKER_THREADS）")]
  worker_threads: Option<usize>,

  #[arg(long = "s3-max-concurrency", value_name = "N", help = "S3/MinIO 最大并发（覆盖 LOGSEARCH_S3_MAX_CONCURRENCY）")]
  s3_max_concurrency: Option<usize>,

  #[arg(long = "cpu-concurrency", value_name = "N", help = "CPU 解压/检索最大并发（覆盖 LOGSEARCH_CPU_CONCURRENCY）")]
  cpu_concurrency: Option<usize>,

  #[arg(long = "stream-ch-cap", value_name = "N", help = "NDJSON 输出通道容量（覆盖 LOGSEARCH_STREAM_CH_CAP）")]
  stream_ch_cap: Option<usize>,

  #[arg(long = "minio-timeout-sec", value_name = "SECS", help = "MinIO 操作超时秒数（覆盖 LOGSEARCH_MINIO_TIMEOUT_SEC）")]
  minio_timeout_sec: Option<u64>,

  #[arg(long = "minio-max-attempts", value_name = "N", help = "MinIO 获取对象最大重试次数（覆盖 LOGSEARCH_MINIO_MAX_ATTEMPTS）")]
  minio_max_attempts: Option<u32>,

  /// 中文注释：管理子命令（start/stop）
  #[command(subcommand)]
  cmd: Option<Commands>,
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

#[cfg(unix)]
fn default_pid_file() -> PathBuf {
  let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
  let dir = PathBuf::from(home).join(".opsbox");
  let _ = fs::create_dir_all(&dir);
  dir.join("api-gateway.pid")
}

#[cfg(unix)]
fn default_log_file() -> PathBuf {
  let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
  let dir = PathBuf::from(home).join(".opsbox");
  let _ = fs::create_dir_all(&dir);
  dir.join("api-gateway.log")
}

#[cfg(unix)]
fn ensure_parent_dir(path: &Path) {
  if let Some(parent) = path.parent() {
    let _ = fs::create_dir_all(parent);
  }
}

#[cfg(unix)]
fn resolve_pid_path(opt: &Option<PathBuf>) -> PathBuf {
  if let Some(p) = opt {
    // 简单处理 ~ 前缀（zsh/bash 一般会自行展开，此处兜底）
    let s = p.to_string_lossy();
    if s.starts_with("~/") {
      let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
      return PathBuf::from(home).join(&s[2..]);
    }
    p.clone()
  } else {
    default_pid_file()
  }
}

#[cfg(unix)]
use nix::sys::signal::{self, Signal};
#[cfg(unix)]
use nix::unistd::Pid;

#[cfg(unix)]
fn signal_name(force: bool) -> &'static str { if force { "SIGKILL" } else { "SIGTERM" } }

#[cfg(unix)]
fn send_signal_to_process(pid: Pid, sig: Signal) -> io::Result<()> {
  signal::kill(pid, sig).map_err(|e| {
    io::Error::new(
      io::ErrorKind::Other,
      format!("发送信号失败: {}", e)
    )
  })
}

#[cfg(unix)] 
fn check_process_alive(pid: Pid) -> bool {
  // 发送信号0来检查进程是否存活（不会实际杀死进程）
  signal::kill(pid, None).is_ok()
}

/// 中文注释：停止进程（Unix），通过 PID 文件发送 SIGTERM/SIGKILL（同步实现，无unsafe）
#[cfg(unix)]
fn stop_unix(pid_path: PathBuf, force: bool) -> io::Result<()> {
  let txt = fs::read_to_string(&pid_path)?;
  let pid_num: i32 = txt.trim().parse().map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "PID 文件内容无效"))?;
  let pid = Pid::from_raw(pid_num);
  
  // 发送信号
  let signal = if force { Signal::SIGKILL } else { Signal::SIGTERM };
  send_signal_to_process(pid, signal)?;
  
  // 等待最多 5 秒确认进程退出
  let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
  while std::time::Instant::now() < deadline {
    if !check_process_alive(pid) { 
      break; 
    }
    std::thread::sleep(std::time::Duration::from_millis(100));
  }
  
  // 移除 PID 文件
  let _ = fs::remove_file(&pid_path);
  Ok(())
}

async fn run_server(addr: SocketAddr) {
  // 初始化依赖（例如存储等）
  log::info!("启动服务，监听地址 = {}", addr);
  logsearch::ensure_initialized().await.expect("初始化设置存储失败");

  // CORS 已禁用：如需启用，请在此处添加 CorsLayer

  let app = Router::new()
    .route("/healthy", get(|| async { "ok" }))
    .nest("/api/v1/logsearch", logsearch_router())
    .fallback(get(spa_fallback));

  // 中文注释：启用 CORS（主要用于开发阶段前端与后端不同源时读取自定义头 X-Logsearch-SID）
  let cors = CorsLayer::new()
    .allow_origin(Any)
    .allow_methods([http::Method::GET, http::Method::POST, http::Method::OPTIONS])
    .allow_headers([CONTENT_TYPE, ACCEPT])
    .expose_headers([http::header::HeaderName::from_static("x-logsearch-sid")]);
  let app = app.layer(cors);

  // 中文注释：优雅关闭信号（支持 Unix 的 SIGTERM/SIGINT 以及通用的 Ctrl-C）
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
    log::info!("收到关闭信号，开始优雅关闭 ...");
    // 中文注释：通知后台清理任务退出
    logsearch::simple_cache::Cache::stop_cleaner();
  }

  let listener = tokio::net::TcpListener::bind(addr).await.expect("监听地址绑定失败");
  axum::serve(listener, app)
    .with_graceful_shutdown(shutdown_signal())
    .await
    .expect("服务启动失败");
}

fn level_from_str(s: &str) -> Option<LevelFilter> {
  match s.to_ascii_lowercase().as_str() {
    "error" => Some(LevelFilter::Error),
    "warn" | "warning" => Some(LevelFilter::Warn),
    "info" => Some(LevelFilter::Info),
    "debug" => Some(LevelFilter::Debug),
    "trace" => Some(LevelFilter::Trace),
    _ => None,
  }
}

fn verbosity_to_level(v: u8) -> LevelFilter {
  match v {
    0 => LevelFilter::Info,
    1 => LevelFilter::Debug,
    _ => LevelFilter::Trace,
  }
}

fn choose_level(cli: &Cli) -> LevelFilter {
  let mut level = cli
    .log_level
    .as_deref()
    .and_then(level_from_str)
    .unwrap_or(LevelFilter::Info);
  let vlevel = verbosity_to_level(cli.verbose);
  if vlevel > level { level = vlevel; }
  level
}

fn init_logger(cli: &Cli) {
  // 若用户设置了 RUST_LOG，则尊重该环境变量；否则使用我们计算出的 level 作为全局默认
  let mut builder = if std::env::var("RUST_LOG").is_ok() {
    env_logger::Builder::from_env(env_logger::Env::default())
  } else {
    let mut b = env_logger::Builder::new();
    b.filter_level(choose_level(cli));
    b
  };
  // 初始化（忽略二次初始化错误）
  let _ = builder.try_init();
}

fn init_network_env() {
  // 中文注释：打印并标准化代理相关环境变量，便于定位 release 与 debug 行为差异
  let get = |k: &str| std::env::var(k).ok();
  let http_proxy = get("HTTP_PROXY").or_else(|| get("http_proxy"));
  let https_proxy = get("HTTPS_PROXY").or_else(|| get("https_proxy"));
  let no_proxy = get("NO_PROXY").or_else(|| get("no_proxy"));
  log::info!(
    "代理环境: HTTP_PROXY={:?} HTTPS_PROXY={:?} NO_PROXY={:?}",
    http_proxy.as_deref().unwrap_or("").replace(|c: char| c.is_ascii_control(), ""),
    https_proxy.as_deref().unwrap_or("").replace(|c: char| c.is_ascii_control(), ""),
    no_proxy.as_deref().unwrap_or("")
  );

  // 中文注释：当显式开启 LOGSEARCH_AUTO_NO_PROXY，且 NO_PROXY 未设置时，自动填入内网与本地网段
  let auto = std::env::var("LOGSEARCH_AUTO_NO_PROXY")
    .map(|v| v == "1" || v.eq_ignore_ascii_case("true") || v.eq_ignore_ascii_case("yes"))
    .unwrap_or(false);
  if auto && no_proxy.is_none() {
    // 常见内网与本地地址范围
    let defaults = "localhost,127.0.0.1,::1,10.0.0.0/8,172.16.0.0/12,192.168.0.0/16";
    // 中文注释：在 Rust 2024 + 受限环境下 set_var 可能被标记为 unsafe；此处仅影响当前进程环境
    unsafe {
      std::env::set_var("NO_PROXY", defaults);
      // 同时设置小写以适配部分依赖库的读取习惯
      std::env::set_var("no_proxy", defaults);
    }
    log::warn!("NO_PROXY 未设置，已根据 LOGSEARCH_AUTO_NO_PROXY 自动设为: {}", defaults);
  }

  // 中文注释：如检测到空的 HTTP(S)_PROXY 值，主动移除，避免底层库解析异常
  let is_empty = |v: &Option<String>| v.as_ref().map(|s| s.trim().is_empty()).unwrap_or(false);
  if auto && (is_empty(&http_proxy) || is_empty(&https_proxy)) {
    unsafe {
      if is_empty(&http_proxy) {
        std::env::remove_var("HTTP_PROXY");
        std::env::remove_var("http_proxy");
      }
      if is_empty(&https_proxy) {
        std::env::remove_var("HTTPS_PROXY");
        std::env::remove_var("https_proxy");
      }
    }
    log::info!("检测到空代理环境变量，已移除空的 HTTP(S)_PROXY 以避免误解析");
  }
}

fn main() {
  // 中文注释：解析命令行参数（地址、后台模式、以及子命令）
  let cli = Cli::parse();

  // 子命令：stop（优先处理后直接退出）
  if let Some(Commands::Stop { pid_file, force }) = &cli.cmd {
    #[cfg(unix)]
    {
      let path = resolve_pid_path(pid_file);
      match stop_unix(path.clone(), *force) {
        Ok(()) => {
          eprintln!("已发送 {}，服务停止流程已触发（如仍存活请使用 --force）", signal_name(*force));
          return;
        }
        Err(e) => {
          eprintln!("停止失败：{e}");
          std::process::exit(2);
        }
      }
    }
    #[cfg(not(unix))]
    {
      eprintln!("当前操作系统不支持内置 stop，请使用系统服务管理或任务管理器");
      std::process::exit(2);
    }
  }

  // 计算最终监听地址
  let addr = if let Some(a) = cli.addr { a } else {
    format!("{}:{}", cli.host, cli.port)
      .parse::<SocketAddr>()
      .expect("组合地址无效")
  };

  // 判断是否需要后台运行，以及 PID 文件
  #[cfg(unix)]
  {
    let mut need_daemon = cli.daemon;
    let mut pid_path: PathBuf = default_pid_file();
    if let Some(Commands::Start { daemon, pid_file }) = &cli.cmd {
      need_daemon = *daemon;
      if let Some(p) = pid_file {
        pid_path = resolve_pid_path(&Some(p.clone()));
      }
    }

    if need_daemon {
      use daemonize::Daemonize;
      // 中文注释：保持当前工作目录，避免因 chdir("/") 导致的相对路径问题
      let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/"));
      ensure_parent_dir(&pid_path);
      // 中文注释：准备日志文件，重定向 stdout/stderr，便于排障
      let log_path = default_log_file();
      let _ = fs::create_dir_all(log_path.parent().unwrap_or(Path::new(".")));
      let stdout = fs::OpenOptions::new().create(true).append(true).open(&log_path).unwrap();
      let stderr = fs::OpenOptions::new().create(true).append(true).open(&log_path).unwrap();

      let d = Daemonize::new()
        .pid_file(pid_path.clone())
        .working_directory(cwd)
        .stdout(stdout)
        .stderr(stderr);

      if let Err(e) = d.start() {
        eprintln!("后台运行失败：{e}");
        std::process::exit(1);
      }
    }
  }
  #[cfg(not(unix))]
  {
    if cli.daemon || matches!(cli.cmd, Some(Commands::Start { daemon: true, .. })) {
      eprintln!("当前操作系统不支持内置后台运行，请使用 nohup/& 或系统服务方式。");
      std::process::exit(2);
    }
  }

  // 中文注释：初始化日志（使用 env_logger），允许通过 --log-level 与 -V/-VV/-VVV 控制详细程度
  init_logger(&cli);
  // 中文注释：初始化网络环境（打印代理设置；可选自动填充 NO_PROXY）
  init_network_env();

  // ====== 参数整合（命令行 > 环境变量 > 默认值）======
  let env_or = |k: &str| std::env::var(k).ok();

  // S3/CPU/通道容量/MinIO 参数
  let s3_max_conc = cli.s3_max_concurrency
    .or_else(|| env_or("LOGSEARCH_S3_MAX_CONCURRENCY").and_then(|s| s.parse().ok()))
    .unwrap_or(12)
    .clamp(1, 128);
  let cpu_conc = cli.cpu_concurrency
    .or_else(|| env_or("LOGSEARCH_CPU_CONCURRENCY").and_then(|s| s.parse().ok()))
    .unwrap_or_else(|| {
      // 默认：16（保守）；如需自动按核数推导可在此改进
      16
    })
    .clamp(1, 128);
  let stream_ch_cap = cli.stream_ch_cap
    .or_else(|| env_or("LOGSEARCH_STREAM_CH_CAP").and_then(|s| s.parse().ok()))
    .unwrap_or(256)
    .clamp(8, 10_000);
  let minio_timeout_sec = cli.minio_timeout_sec
    .or_else(|| env_or("LOGSEARCH_MINIO_TIMEOUT_SEC").and_then(|s| s.parse().ok()))
    .unwrap_or(60)
    .clamp(5, 300);
  let minio_max_attempts = cli.minio_max_attempts
    .or_else(|| env_or("LOGSEARCH_MINIO_MAX_ATTEMPTS").and_then(|s| s.parse().ok()))
    .unwrap_or(5)
    .clamp(1, 20);

  // worker_threads 默认：min(物理核数, cpu_conc+2, 18)
  let phys = num_cpus::get_physical().max(1);
  let default_workers = phys.min(cpu_conc + 2).min(18).max(2);
  let worker_threads = cli.worker_threads
    .or_else(|| env_or("LOGSEARCH_WORKER_THREADS").and_then(|s| s.parse().ok()))
    .unwrap_or(default_workers)
    .clamp(2, 64);

  // 将最终值注入 logsearch 调参（避免通过环境变量、也无需 unsafe）
  let _ = logsearch::tuning::set(logsearch::tuning::Tuning {
    s3_max_concurrency: s3_max_conc,
    cpu_concurrency: cpu_conc,
    stream_ch_cap: stream_ch_cap,
    minio_timeout_sec: minio_timeout_sec,
    minio_max_attempts: minio_max_attempts,
  });

  // 中文注释：在（可能的）守护化之后，再创建 Tokio 运行时并启动服务器
  let rt = tokio::runtime::Builder::new_multi_thread()
    .worker_threads(worker_threads)
    .enable_all()
    .build()
    .expect("创建 Tokio 运行时失败");
  rt.block_on(run_server(addr));
}
