// ============================================================================
// LogSeek Agent - 远程搜索代理
// ============================================================================
//
// Agent 安装在远程服务器上，接收来自 Server 的搜索请求，
// 在本地执行搜索并将结果返回给 Server。
//

use axum::{
  Json, Router,
  body::Body,
  extract::{Path, State},
  http::{StatusCode, header::CONTENT_TYPE},
  response::{IntoResponse, Response},
  routing::{get, post},
};
use clap::{Parser, Subcommand};
use futures::StreamExt;
use globset::{Glob, GlobSet, GlobSetBuilder};
use log::{debug, error, info, warn};
use logseek::utils::strings::truncate_utf8;
use logseek::{
  agent::{AgentInfo, AgentMessage, AgentSearchRequest, AgentStatus, SearchScope},
  query::Query,
  service::search::SearchProcessor,
};
use std::{
  net::SocketAddr,
  path::{Path as StdPath, PathBuf},
  sync::Arc,
  time::Duration,
};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;

/// 是否启用与 Server 通讯的“线级”调试日志（打印请求/响应、NDJSON 行等）
/// 通过环境变量 AGENT_DEBUG_WIRE=1 启用
fn wire_debug_enabled() -> bool {
  std::env::var("AGENT_DEBUG_WIRE")
    .map(|v| matches!(v.as_str(), "1" | "true" | "TRUE"))
    .unwrap_or(false)
}

/// LogSeek Agent - 远程搜索代理
#[derive(Parser, Debug)]
#[command(name = "opsbox-agent")]
#[command(about = "Opsbox Agent - 运维工具箱远程代理")]
#[command(version)]
struct Args {
  #[command(subcommand)]
  cmd: Option<Commands>,

  /// Agent ID
  #[arg(global = true, long, default_value_t = {
    let hostname = hostname::get()
      .unwrap_or_else(|_| std::ffi::OsString::from("unknown"))
      .to_string_lossy()
      .to_string();
    format!("agent-{}", hostname)
  })]
  agent_id: String,

  /// Agent 名称
  #[arg(global = true, long, default_value_t = {
    let hostname = hostname::get()
      .unwrap_or_else(|_| std::ffi::OsString::from("unknown"))
      .to_string_lossy()
      .to_string();
    format!("Agent@{}", hostname)
  })]
  agent_name: String,

  /// 服务器端点
  #[arg(global = true, long, default_value = "http://localhost:4000")]
  server_endpoint: String,

  /// 搜索根目录（逗号分隔）
  #[arg(global = true, long, default_value_t = {
    std::env::var("HOME").unwrap_or_else(|_| "/home/user".to_string())
  })]
  search_roots: String,

  /// 监听端口
  #[arg(global = true, long, default_value_t = 4001)]
  listen_port: u16,

  /// 启用心跳
  #[arg(global = true, long, default_value_t = true)]
  enable_heartbeat: bool,

  /// 禁用心跳
  #[arg(global = true, long, action = clap::ArgAction::SetTrue)]
  no_heartbeat: bool,

  /// 心跳间隔（秒）
  #[arg(global = true, long, default_value_t = 30)]
  heartbeat_interval: u64,

  /// 工作线程数
  #[arg(global = true, long)]
  worker_threads: Option<usize>,
}

/// 子命令定义
#[derive(Subcommand, Debug, Clone)]
enum Commands {
  /// 启动 Agent（默认后台运行）
  Start {
    /// 是否后台运行（默认 true，仅类 Unix 支持）
    #[arg(long, short = 'd', default_value_t = true)]
    daemon: bool,
    /// PID 文件路径（默认：~/.opsbox-agent/agent.pid）
    #[arg(long, value_name = "FILE")]
    pid_file: Option<std::path::PathBuf>,
  },
  /// 停止 Agent（通过 PID 文件定位进程）
  Stop {
    /// PID 文件路径（默认：~/.opsbox-agent/agent.pid）
    #[arg(long, value_name = "FILE")]
    pid_file: Option<std::path::PathBuf>,
    /// 强制停止（发送 SIGKILL）
    #[arg(long, short = 'f', default_value_t = false)]
    force: bool,
  },
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
  // 解析命令行参数
  let args = Args::parse();

  // 处理 stop 子命令（优先处理）
  if let Some(Commands::Stop { pid_file, force }) = &args.cmd {
    handle_stop_command(pid_file, *force);
    return Ok(());
  }

  // 处理守护进程模式（在日志初始化之前，避免重复初始化）
  handle_daemon_mode(&args);

  // 初始化日志
  env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

  // 加载配置
  let config = Arc::new(AgentConfig::from_args(args));

  // 创建自定义Tokio运行时，限制工作线程数
  let worker_threads = config.get_worker_threads();
  info!("使用 {} 个工作线程", worker_threads);

  let rt = tokio::runtime::Builder::new_multi_thread()
    .worker_threads(worker_threads)
    .enable_all()
    .build()
    .expect("创建 Tokio 运行时失败");

  rt.block_on(async_main(config))
}

async fn async_main(config: Arc<AgentConfig>) -> Result<(), Box<dyn std::error::Error>> {
  info!("╔══════════════════════════════════════════╗");
  info!("║     Opsbox Agent 启动中...              ║");
  info!("╚══════════════════════════════════════════╝");
  info!("Agent ID: {}", config.agent_id);
  info!("Agent Name: {}", config.agent_name);
  info!("Server: {}", config.server_endpoint);
  info!("LogSeek Search Roots: {:?}", config.search_roots);
  info!("Listen Port: {}", config.listen_port);

  // 向 Server 注册
  if let Err(e) = register_to_server(&config).await {
    error!("注册到 Server 失败: {}", e);
    error!("Agent 将以离线模式运行，仅提供 HTTP 接口");
  }

  // 启动心跳任务
  if config.enable_heartbeat {
    tokio::spawn(heartbeat_loop(config.clone()));
  }

  // 创建任务管理器
  // 构建路由
  let app = Router::new()
    .route("/health", get(health))
    .route("/api/v1/info", get(get_info))
    .route("/api/v1/paths", get(list_available_paths))
    .route("/api/v1/search", post(handle_search))
    .route("/api/v1/cancel/{task_id}", post(handle_cancel))
    .with_state(AppState { config: config.clone() });

  // 启动 HTTP 服务器
  let addr = SocketAddr::from(([0, 0, 0, 0], config.listen_port));
  info!("Agent HTTP 服务监听: {}", addr);
  info!("准备就绪，等待搜索请求...");

  let listener = tokio::net::TcpListener::bind(addr).await?;

  // 支持优雅关闭
  axum::serve(listener, app)
    .with_graceful_shutdown(shutdown_signal())
    .await?;

  info!("Agent 已关闭");
  Ok(())
}

// ============================================================================
// 配置
// ============================================================================

#[derive(Clone)]
struct AgentConfig {
  agent_id: String,
  agent_name: String,
  server_endpoint: String,
  search_roots: Vec<String>,
  listen_port: u16,
  enable_heartbeat: bool,
  heartbeat_interval_secs: u64,
  worker_threads: Option<usize>,
}

impl AgentConfig {
  fn from_args(args: Args) -> Self {
    Self {
      agent_id: args.agent_id,
      agent_name: args.agent_name,
      server_endpoint: args.server_endpoint,
      search_roots: args
        .search_roots
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect(),
      listen_port: args.listen_port,
      enable_heartbeat: args.enable_heartbeat && !args.no_heartbeat,
      heartbeat_interval_secs: args.heartbeat_interval,
      worker_threads: args.worker_threads,
    }
  }

  /// 获取工作线程数（优先级：环境变量 > 默认值）
  ///
  /// 默认策略：保守使用CPU资源，避免影响业务系统
  /// - 单核系统：1个线程
  /// - 2-4核系统：2个线程  
  /// - 5-8核系统：3个线程
  /// - 8核以上：CPU核心数的一半（最大限制）
  fn get_worker_threads(&self) -> usize {
    self
      .worker_threads
      .unwrap_or_else(|| {
        let cpu_count = num_cpus::get();
        match cpu_count {
          1 => 1,
          2..=4 => 2,
          5..=7 => 3,
          _ => cpu_count.div_ceil(2), // 8核以上使用一半核心数（向上取整）
        }
      })
      .clamp(1, 16) // 安全范围：1-16个线程
  }

  fn to_agent_info(&self) -> AgentInfo {
    AgentInfo {
      id: self.agent_id.clone(),
      name: self.agent_name.clone(),
      version: env!("CARGO_PKG_VERSION").to_string(),
      hostname: hostname::get()
        .unwrap_or_else(|_| std::ffi::OsString::from("unknown"))
        .to_string_lossy()
        .to_string(),
      tags: vec![], // Agent 不管理标签，完全由 Agent Manager 负责
      search_roots: self.search_roots.clone(),
      last_heartbeat: chrono::Utc::now().timestamp(),
      status: AgentStatus::Online,
    }
  }

  /// 解析 SearchScope 到实际的文件系统路径
  fn resolve_scope_paths(&self, scope: &SearchScope) -> Result<Vec<PathBuf>, String> {
    match scope {
      SearchScope::Directory { path, recursive: _ } => self.resolve_directory_path(path),
      SearchScope::Files { paths } => self.resolve_file_paths(paths),
      SearchScope::TarGz { path } => self.resolve_targz_path(path),
      SearchScope::All => Ok(self.search_roots.iter().map(PathBuf::from).collect()),
    }
  }

  /// 解析目录路径
  fn resolve_directory_path(&self, relative_path: &str) -> Result<Vec<PathBuf>, String> {
    let mut resolved_paths = Vec::new();

    for root in &self.search_roots {
      let full_path = PathBuf::from(root).join(relative_path);

      if full_path.exists() && full_path.is_dir() {
        resolved_paths.push(full_path);
      } else {
        // 尝试查找匹配的子目录
        if let Ok(entries) = std::fs::read_dir(root) {
          for entry in entries.flatten() {
            if entry.path().is_dir() {
              let sub_path = entry.path().join(relative_path);
              if sub_path.exists() {
                resolved_paths.push(sub_path);
              }
            }
          }
        }
      }
    }

    if resolved_paths.is_empty() {
      Err(format!("未找到目录: {}", relative_path))
    } else {
      Ok(resolved_paths)
    }
  }

  /// 解析文件路径
  fn resolve_file_paths(&self, relative_paths: &[String]) -> Result<Vec<PathBuf>, String> {
    let mut resolved_paths = Vec::new();

    for relative_path in relative_paths {
      for root in &self.search_roots {
        let full_path = PathBuf::from(root).join(relative_path);
        if full_path.exists() && full_path.is_file() {
          resolved_paths.push(full_path);
          break; // 找到第一个匹配的文件就停止
        }
      }
    }

    Ok(resolved_paths)
  }

  /// 解析 tar.gz 路径
  fn resolve_targz_path(&self, relative_path: &str) -> Result<Vec<PathBuf>, String> {
    let mut resolved_paths = Vec::new();

    for root in &self.search_roots {
      let full_path = PathBuf::from(root).join(relative_path);
      if full_path.exists() && full_path.extension().and_then(|s| s.to_str()) == Some("gz") {
        resolved_paths.push(full_path);
        break;
      }
    }

    if resolved_paths.is_empty() {
      Err(format!("未找到 tar.gz 文件: {}", relative_path))
    } else {
      Ok(resolved_paths)
    }
  }

  /// 获取可用的子目录列表（用于错误提示）
  fn get_available_subdirs(&self) -> Vec<String> {
    let mut subdirs = Vec::new();

    for root in &self.search_roots {
      if let Ok(entries) = std::fs::read_dir(root) {
        for entry in entries.flatten() {
          if entry.path().is_dir()
            && let Some(name) = entry.file_name().to_str()
          {
            subdirs.push(name.to_string());
          }
        }
      }
    }

    subdirs.sort();
    subdirs.dedup();
    subdirs
  }
}

// ============================================================================
// 路径过滤功能
// ============================================================================

/// 应用路径过滤器
#[allow(dead_code)]
fn apply_path_filter(paths: &[PathBuf], filter: &str) -> Result<Vec<PathBuf>, String> {
  let glob = Glob::new(filter).map_err(|e| format!("路径过滤器语法错误: {}", e))?;

  let glob_set = GlobSetBuilder::new()
    .add(glob)
    .build()
    .map_err(|e| format!("构建路径过滤器失败: {}", e))?;

  let mut filtered_paths = Vec::new();

  for path in paths {
    if path.is_file() {
      if glob_set.is_match(path) {
        filtered_paths.push(path.clone());
      }
    } else if path.is_dir() {
      // 递归查找匹配的文件
      find_matching_files(path, &glob_set, &mut filtered_paths)?;
    }
  }

  Ok(filtered_paths)
}

/// 在目录中递归查找匹配的文件
#[allow(dead_code)]
fn find_matching_files(dir: &StdPath, glob_set: &GlobSet, results: &mut Vec<PathBuf>) -> Result<(), String> {
  if let Ok(entries) = std::fs::read_dir(dir) {
    for entry in entries.flatten() {
      let path = entry.path();

      if path.is_file() {
        if glob_set.is_match(&path) {
          results.push(path);
        }
      } else if path.is_dir() {
        find_matching_files(&path, glob_set, results)?;
      }
    }
  }

  Ok(())
}

// ============================================================================
// 应用状态
// ============================================================================

#[derive(Clone)]
struct AppState {
  config: Arc<AgentConfig>,
}

// ============================================================================
// 工具函数
// ============================================================================

// ============================================================================
// 路由处理器
// ============================================================================

/// 健康检查
async fn health() -> &'static str {
  "OK"
}

/// 获取 Agent 信息
async fn get_info(State(state): State<AppState>) -> Json<AgentInfo> {
  Json(state.config.to_agent_info())
}

/// 列出可用的搜索路径
async fn list_available_paths(State(state): State<AppState>) -> Json<Vec<String>> {
  let paths = state.config.get_available_subdirs();
  Json(paths)
}

/// 处理搜索请求
async fn handle_search(State(state): State<AppState>, Json(request): Json<AgentSearchRequest>) -> impl IntoResponse {
  info!("收到搜索请求: task_id={}, query={}", request.task_id, request.query);
  if wire_debug_enabled() {
    match serde_json::to_string(&request) {
      Ok(s) => debug!("[Wire] ← /api/v1/search 请求体: {}", s),
      Err(e) => debug!("[Wire] ← /api/v1/search 请求体序列化失败: {}", e),
    }
  } else {
    debug!(
      "搜索参数: ctx={}, path_filter_present={}, scope=...",
      request.context_lines,
      request.path_filter.as_ref().map(|s| !s.is_empty()).unwrap_or(false)
    );
  }

  // 创建结果 channel
  let (tx, rx) = mpsc::channel(128);

  // 在后台执行搜索
  tokio::spawn(execute_search(state.config.clone(), request, tx));

  // 将 channel 转换为 NDJSON 流
  let stream = ReceiverStream::new(rx).map(|msg| {
    let json = serde_json::to_string(&msg).unwrap_or_else(|_| "{}".to_string());
    if wire_debug_enabled() {
      let preview = if json.len() > 512 {
        format!("{}...", truncate_utf8(&json, 512))
      } else {
        json.clone()
      };
      debug!("[Wire] → NDJSON行: {}", preview);
    }
    Ok::<_, std::convert::Infallible>(format!("{}\n", json))
  });

  Response::builder()
    .status(StatusCode::OK)
    .header(CONTENT_TYPE, "application/x-ndjson; charset=utf-8")
    .body(Body::from_stream(stream))
    .unwrap()
}

/// 取消搜索任务
async fn handle_cancel(State(_state): State<AppState>, Path(task_id): Path<String>) -> StatusCode {
  warn!("收到取消请求: task_id={} (暂未实现)", task_id);
  StatusCode::NOT_IMPLEMENTED
}

// ============================================================================
// 核心搜索逻辑
// ============================================================================

/// 执行搜索
async fn execute_search(config: Arc<AgentConfig>, request: AgentSearchRequest, tx: mpsc::Sender<AgentMessage>) {
  let task_id = request.task_id.clone();

  // 1. 解析查询（第三层过滤：query 中的 path: 指令）
  let spec = match Query::parse_github_like(&request.query) {
    Ok(s) => Arc::new(s),
    Err(e) => {
      error!("查询解析失败: {}", e);
      let _ = tx.send(AgentMessage::Error(format!("查询解析失败: {}", e))).await;
      return;
    }
  };

  // 2. 创建搜索处理器
  let processor = Arc::new(SearchProcessor::new(spec.clone(), request.context_lines));

  // 3. 第一层过滤：解析 SearchScope 到实际路径
  let base_paths = match config.resolve_scope_paths(&request.scope) {
    Ok(paths) => {
      info!("SearchScope 解析成功: {:?}", paths);
      paths
    }
    Err(e) => {
      error!("SearchScope 解析失败: {}", e);
      let available_dirs = config.get_available_subdirs();
      let error_msg = if available_dirs.is_empty() {
        format!("SearchScope 解析失败: {}。未找到可用的搜索目录。", e)
      } else {
        format!("SearchScope 解析失败: {}。可用的子目录: {:?}", e, available_dirs)
      };
      let _ = tx.send(AgentMessage::Error(error_msg)).await;
      return;
    }
  };

  // 4. 额外路径过滤：将 path_filter 转为仅含 path: 的 Query，提取 PathFilter 作为“硬性 AND 限定”
  let extra_path_filter: Option<logseek::query::PathFilter> = if let Some(filter) = &request.path_filter {
    match logseek::query::path_glob_to_filter(filter) {
      Ok(f) => Some(f),
      Err(e) => {
        error!("路径过滤器解析失败: {}", e);
        let _ = tx.send(AgentMessage::Error(format!("路径过滤器解析失败: {}", e))).await;
        return;
      }
    }
  } else {
    None
  };

  let filtered_paths = base_paths; // 与 LogSeek 对齐：仅以目录为起点，后置过滤

  if filtered_paths.is_empty() {
    warn!("没有找到匹配的搜索路径");
    let _ = tx.send(AgentMessage::Error("没有找到匹配的搜索路径".to_string())).await;
    return;
  }

  // 5. 执行搜索
  let mut all_processed = 0;
  let mut all_matched = 0;

  for search_path in filtered_paths {
    info!("开始搜索路径: {}", search_path.display());

    // 统一由 logseek 提供的构造器创建本地来源条目流
    let path_str = search_path.to_string_lossy().to_string();
    let scope_hint = match &request.scope {
      SearchScope::TarGz { .. } => Some(request.scope.clone()),
      _ => None,
    };
    let estream = match logseek::service::entry_stream::build_local_entry_stream(&path_str, scope_hint).await {
      Ok(s) => s,
      Err(e) => {
        warn!("构建本地条目流失败 {}: {}", search_path.display(), e);
        continue;
      }
    };

    if let Err(e) = search_with_entry_stream(
      estream,
      processor.clone(),
      &task_id,
      &tx,
      &mut all_processed,
      &mut all_matched,
      extra_path_filter.clone(),
    )
    .await
    {
      warn!("处理条目流失败 {}: {}", search_path.display(), e);
    }
  }

  // 发送完成消息
  let _ = tx.send(AgentMessage::Complete).await;

  info!(
    "搜索完成: task_id={}, processed={}, matched={}",
    task_id, all_processed, all_matched
  );
}

/// 通用条目流搜索辅助函数
/// 使用通用处理函数并自动处理消息发送
async fn search_with_entry_stream(
  stream: Box<dyn logseek::service::entry_stream::EntryStream>,
  processor: Arc<SearchProcessor>,
  _task_id: &str,
  tx: &mpsc::Sender<AgentMessage>,
  all_processed: &mut usize,
  all_matched: &mut usize,
  extra_path_filter: Option<logseek::query::PathFilter>,
) -> Result<(), String> {
  // 使用通用条目流处理函数
  let tx_clone = tx.clone();

  let (processed, matched) = logseek::service::entry_stream::process_entry_stream_with_callback(
    stream,
    processor,
    extra_path_filter,
    move |result| {
      // 发送结果到 channel
      let tx_ref = &tx_clone;
      match futures::executor::block_on(async { tx_ref.send(AgentMessage::Result(result)).await }) {
        Ok(_) => true, // 继续处理
        Err(_) => {
          debug!("接收端已关闭");
          false // 停止处理
        }
      }
    },
  )
  .await?;

  *all_processed += processed;
  *all_matched += matched;

  Ok(())
}

// ============================================================================
// 优雅关闭
// ============================================================================

/// 等待关闭信号
#[cfg(unix)]
async fn shutdown_signal() {
  use tokio::signal::unix::{SignalKind, signal};

  let mut sigterm = signal(SignalKind::terminate()).expect("无法监听 SIGTERM");
  let mut sigint = signal(SignalKind::interrupt()).expect("无法监听 SIGINT");

  let signal_name = tokio::select! {
    _ = sigterm.recv() => "SIGTERM",
    _ = sigint.recv() => "SIGINT (Ctrl-C)",
  };

  info!("收到关闭信号 [{}]，开始优雅关闭...", signal_name);
}

/// 等待关闭信号 (Windows)
#[cfg(not(unix))]
async fn shutdown_signal() {
  tokio::signal::ctrl_c().await.expect("无法监听 Ctrl-C 信号");
  info!("收到关闭信号 [Ctrl-C]，开始优雅关闭...");
}

// ============================================================================
// Server 通信
// ============================================================================

/// 向 Server 注册
async fn register_to_server(config: &AgentConfig) -> Result<(), Box<dyn std::error::Error>> {
  let client = reqwest::Client::builder().timeout(Duration::from_secs(10)).build()?;

  #[derive(serde::Serialize)]
  struct AgentRegisterPayload {
    #[serde(flatten)]
    info: AgentInfo,
    listen_port: u16,
  }

  let payload = AgentRegisterPayload {
    info: config.to_agent_info(),
    listen_port: config.listen_port,
  };
  let url = format!("{}/api/v1/agents/register", config.server_endpoint);

  debug!("向 Server 注册: {}", url);

  let response = client.post(&url).json(&payload).send().await?;

  if response.status().is_success() {
    info!("✓ 已成功向 Server 注册");
    Ok(())
  } else {
    let status = response.status();
    let body_text = response.text().await.unwrap_or_default();
    error!("注册失败: {} - {}", status, body_text);
    Err(format!("注册失败: {} - {}", status, body_text).into())
  }
}

/// 心跳循环
async fn heartbeat_loop(config: Arc<AgentConfig>) {
  let client = reqwest::Client::builder()
    .timeout(Duration::from_secs(5))
    .build()
    .unwrap();

  let mut interval = tokio::time::interval(Duration::from_secs(config.heartbeat_interval_secs));

  loop {
    interval.tick().await;

    let url = format!("{}/api/v1/agents/{}/heartbeat", config.server_endpoint, config.agent_id);

    match client.post(&url).send().await {
      Ok(response) if response.status().is_success() => {
        debug!("心跳发送成功");
      }
      Ok(response) => {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        warn!("心跳失败: {} - {}", status, body);
      }
      Err(e) => {
        warn!("心跳发送出错: {}", e);
      }
    }
  }
}

// ============================================================================
// 守护进程相关功能
// ============================================================================

use std::fs;
use std::io;

#[cfg(unix)]
use nix::sys::signal::{self, Signal};
#[cfg(unix)]
use nix::unistd::Pid;

/// 默认 PID 文件路径
fn default_pid_file() -> PathBuf {
  let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
  let dir = PathBuf::from(home).join(".opsbox-agent");
  let _ = fs::create_dir_all(&dir);
  dir.join("agent.pid")
}

/// 默认日志文件路径
fn default_log_file() -> PathBuf {
  let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
  let dir = PathBuf::from(home).join(".opsbox-agent");
  let _ = fs::create_dir_all(&dir);
  dir.join("agent.log")
}

/// 确保父目录存在
fn ensure_parent_dir(path: &std::path::Path) {
  if let Some(parent) = path.parent() {
    let _ = fs::create_dir_all(parent);
  }
}

/// 解析 PID 文件路径（处理 ~ 前缀）
fn resolve_pid_path(opt: &Option<PathBuf>) -> PathBuf {
  if let Some(p) = opt {
    let s = p.to_string_lossy();
    if let Some(stripped) = s.strip_prefix("~/") {
      let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
      return PathBuf::from(home).join(stripped);
    }
    p.clone()
  } else {
    default_pid_file()
  }
}

#[cfg(unix)]
fn signal_name(force: bool) -> &'static str {
  if force { "SIGKILL" } else { "SIGTERM" }
}

#[cfg(unix)]
fn send_signal_to_process(pid: Pid, sig: Signal) -> io::Result<()> {
  signal::kill(pid, sig).map_err(|e| io::Error::other(format!("发送信号失败: {}", e)))
}

#[cfg(unix)]
fn check_process_alive(pid: Pid) -> bool {
  // 发送信号 0 来检查进程是否存活
  signal::kill(pid, None).is_ok()
}

/// 停止守护进程（Unix）
#[cfg(unix)]
fn stop_daemon(pid_path: PathBuf, force: bool) -> io::Result<()> {
  let txt = fs::read_to_string(&pid_path)?;
  let pid_num: i32 = txt
    .trim()
    .parse()
    .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "PID 文件内容无效"))?;
  let pid = Pid::from_raw(pid_num);

  // 发送信号
  let signal = if force { Signal::SIGKILL } else { Signal::SIGTERM };
  send_signal_to_process(pid, signal)?;

  println!("已发送 {} 到进程 {}", signal_name(force), pid_num);

  // 等待最多 5 秒确认进程退出
  let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
  while std::time::Instant::now() < deadline {
    if !check_process_alive(pid) {
      println!("进程 {} 已退出", pid_num);
      break;
    }
    std::thread::sleep(std::time::Duration::from_millis(100));
  }

  // 移除 PID 文件
  let _ = fs::remove_file(&pid_path);
  Ok(())
}

/// 启动守护进程（Unix）
#[cfg(unix)]
fn start_daemon(pid_path: PathBuf) -> io::Result<()> {
  use daemonize::Daemonize;

  // 保持当前工作目录
  let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/"));
  ensure_parent_dir(&pid_path);

  // 准备日志文件
  let log_path = default_log_file();
  let _ = fs::create_dir_all(log_path.parent().unwrap_or(std::path::Path::new(".")));
  let stdout = fs::OpenOptions::new().create(true).append(true).open(&log_path)?;
  let stderr = fs::OpenOptions::new().create(true).append(true).open(&log_path)?;

  let daemon = Daemonize::new()
    .pid_file(pid_path.clone())
    .working_directory(cwd)
    .stdout(stdout)
    .stderr(stderr);

  daemon
    .start()
    .map_err(|e| io::Error::other(format!("后台运行失败: {}", e)))?;

  Ok(())
}

/// 处理停止命令
fn handle_stop_command(pid_file: &Option<PathBuf>, force: bool) {
  #[cfg(unix)]
  {
    let pid_path = resolve_pid_path(pid_file);
    if let Err(e) = stop_daemon(pid_path, force) {
      eprintln!("停止 Agent 失败: {}", e);
      std::process::exit(1);
    }
  }
  #[cfg(not(unix))]
  {
    eprintln!("停止命令仅在 Unix 系统上支持");
    std::process::exit(1);
  }
}

/// 处理守护进程模式
fn handle_daemon_mode(args: &Args) {
  #[cfg(unix)]
  {
    // 检查是否需要后台运行
    let should_daemon = match &args.cmd {
      Some(Commands::Start { daemon, .. }) => *daemon,
      _ => false, // 如果没有子命令，默认前台运行
    };

    if should_daemon {
      let pid_path = match &args.cmd {
        Some(Commands::Start { pid_file, .. }) => resolve_pid_path(pid_file),
        _ => default_pid_file(),
      };

      if let Err(e) = start_daemon(pid_path) {
        eprintln!("启动守护进程失败: {}", e);
        std::process::exit(1);
      }
    }
  }
  #[cfg(not(unix))]
  {
    if let Some(Commands::Start { daemon, .. }) = &args.cmd {
      if *daemon {
        eprintln!("守护进程模式仅在 Unix 系统上支持");
        std::process::exit(1);
      }
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_config_from_args() {
    let args = Args {
      cmd: None,
      agent_id: "test-agent".to_string(),
      agent_name: "Test Agent".to_string(),
      server_endpoint: "http://test-server:4000".to_string(),
      search_roots: "/var/log,/opt/logs".to_string(),
      listen_port: 9090,
      enable_heartbeat: true,
      no_heartbeat: false,
      heartbeat_interval: 60,
      worker_threads: Some(4),
    };

    let config = AgentConfig::from_args(args);

    assert_eq!(config.agent_id, "test-agent");
    assert_eq!(config.agent_name, "Test Agent");
    assert_eq!(config.server_endpoint, "http://test-server:4000");
    assert_eq!(config.search_roots.len(), 2);
    assert_eq!(config.search_roots[0], "/var/log");
    assert_eq!(config.search_roots[1], "/opt/logs");
    assert_eq!(config.listen_port, 9090);
    assert!(config.enable_heartbeat);
    assert_eq!(config.heartbeat_interval_secs, 60);
    assert_eq!(config.worker_threads, Some(4));
  }

  #[test]
  fn test_resolve_directory_path() {
    let config = AgentConfig {
      agent_id: "test-agent".to_string(),
      agent_name: "Test Agent".to_string(),
      server_endpoint: "http://test-server:4000".to_string(),
      search_roots: vec!["/tmp".to_string()],
      listen_port: 9090,
      enable_heartbeat: true,
      heartbeat_interval_secs: 60,
      worker_threads: Some(4),
    };

    // 测试不存在的目录
    let result = config.resolve_directory_path("nonexistent");
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("未找到目录"));

    // 测试存在的目录（如果 /tmp 存在）
    if std::path::Path::new("/tmp").exists() {
      let result = config.resolve_directory_path(".");
      assert!(result.is_ok());
      assert!(result.unwrap().iter().any(|p| p.to_string_lossy().contains("/tmp")));
    }
  }

  #[test]
  fn test_resolve_file_paths() {
    let config = AgentConfig {
      agent_id: "test-agent".to_string(),
      agent_name: "Test Agent".to_string(),
      server_endpoint: "http://test-server:4000".to_string(),
      search_roots: vec!["/tmp".to_string()],
      listen_port: 9090,
      enable_heartbeat: true,
      heartbeat_interval_secs: 60,
      worker_threads: Some(4),
    };

    // 测试不存在的文件
    let result = config.resolve_file_paths(&["nonexistent.txt".to_string()]);
    assert!(result.is_ok());
    assert!(result.unwrap().is_empty());

    // 测试存在的文件（如果 /tmp 存在）
    if std::path::Path::new("/tmp").exists() {
      let result = config.resolve_file_paths(&[".".to_string()]);
      assert!(result.is_ok());
    }
  }

  #[test]
  fn test_resolve_targz_path() {
    let config = AgentConfig {
      agent_id: "test-agent".to_string(),
      agent_name: "Test Agent".to_string(),
      server_endpoint: "http://test-server:4000".to_string(),
      search_roots: vec!["/tmp".to_string()],
      listen_port: 9090,
      enable_heartbeat: true,
      heartbeat_interval_secs: 60,
      worker_threads: Some(4),
    };

    // 测试不存在的 tar.gz 文件
    let result = config.resolve_targz_path("nonexistent.tar.gz");
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("未找到 tar.gz 文件"));
  }

  #[test]
  fn test_resolve_scope_paths() {
    let config = AgentConfig {
      agent_id: "test-agent".to_string(),
      agent_name: "Test Agent".to_string(),
      server_endpoint: "http://test-server:4000".to_string(),
      search_roots: vec!["/tmp".to_string()],
      listen_port: 9090,
      enable_heartbeat: true,
      heartbeat_interval_secs: 60,
      worker_threads: Some(4),
    };

    // 测试 SearchScope::All
    let scope = SearchScope::All;
    let result = config.resolve_scope_paths(&scope);
    assert!(result.is_ok());
    assert_eq!(result.unwrap().len(), 1);

    // 测试 SearchScope::Directory
    let scope = SearchScope::Directory {
      path: "nonexistent".to_string(),
      recursive: true,
    };
    let result = config.resolve_scope_paths(&scope);
    assert!(result.is_err());

    // 测试 SearchScope::Files
    let scope = SearchScope::Files {
      paths: vec!["nonexistent.txt".to_string()],
    };
    let result = config.resolve_scope_paths(&scope);
    assert!(result.is_ok());
    assert!(result.unwrap().is_empty());

    // 测试 SearchScope::TarGz
    let scope = SearchScope::TarGz {
      path: "nonexistent.tar.gz".to_string(),
    };
    let result = config.resolve_scope_paths(&scope);
    assert!(result.is_err());
  }

  #[test]
  fn test_apply_path_filter() {
    // 创建临时目录和文件进行测试
    let temp_dir = std::env::temp_dir().join("opsbox-agent-test");
    std::fs::create_dir_all(&temp_dir).unwrap();

    let test_file1 = temp_dir.join("test.log");
    let test_file2 = temp_dir.join("debug.txt");
    std::fs::write(&test_file1, "test content").unwrap();
    std::fs::write(&test_file2, "debug content").unwrap();

    let paths = vec![test_file1.clone(), test_file2.clone()];

    // 测试匹配 .log 文件
    let result = apply_path_filter(&paths, "**/*.log");
    assert!(result.is_ok());
    let filtered = result.unwrap();
    assert_eq!(filtered.len(), 1);
    assert!(filtered[0].to_string_lossy().contains("test.log"));

    // 测试匹配 .txt 文件
    let result = apply_path_filter(&paths, "**/*.txt");
    assert!(result.is_ok());
    let filtered = result.unwrap();
    assert_eq!(filtered.len(), 1);
    assert!(filtered[0].to_string_lossy().contains("debug.txt"));

    // 测试无效的 glob 模式
    let result = apply_path_filter(&paths, "[invalid");
    assert!(result.is_err());

    // 清理
    std::fs::remove_dir_all(&temp_dir).unwrap();
  }
}
