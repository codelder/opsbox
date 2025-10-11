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
use futures::StreamExt;
use log::{debug, error, info, warn};
use logseek::{
  agent::{AgentInfo, AgentMessage, AgentSearchRequest, AgentStatus, SearchProgress, SearchStatus},
  query::Query,
  service::{
    entry_stream::{EntryStreamProcessor, FsEntryStream},
    search::SearchProcessor,
  },
};
use std::{net::SocketAddr, path::PathBuf, sync::Arc, time::Duration};
use tokio::sync::{RwLock, mpsc};
use tokio_stream::wrappers::ReceiverStream;

fn main() -> Result<(), Box<dyn std::error::Error>> {
  // 初始化日志
  env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

  // 加载配置
  let config = Arc::new(AgentConfig::from_env());

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
  info!("║     LogSeek Agent 启动中...              ║");
  info!("╚══════════════════════════════════════════╝");
  info!("Agent ID: {}", config.agent_id);
  info!("Agent Name: {}", config.agent_name);
  info!("Server: {}", config.server_endpoint);
  info!("Search Roots: {:?}", config.search_roots);
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
  let task_manager = Arc::new(TaskManager::new());

  // 构建路由
  let app = Router::new()
    .route("/health", get(health))
    .route("/api/v1/info", get(get_info))
    .route("/api/v1/search", post(handle_search))
    .route("/api/v1/progress/{task_id}", get(handle_progress))
    .route("/api/v1/cancel/{task_id}", post(handle_cancel))
    .with_state(AppState {
      config: config.clone(),
      task_manager,
    });

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
  fn from_env() -> Self {
    let hostname = hostname::get()
      .unwrap_or_else(|_| std::ffi::OsString::from("unknown"))
      .to_string_lossy()
      .to_string();

    Self {
      agent_id: std::env::var("AGENT_ID").unwrap_or_else(|_| format!("agent-{}", hostname)),
      agent_name: std::env::var("AGENT_NAME").unwrap_or_else(|_| format!("Agent @ {}", hostname)),
      server_endpoint: std::env::var("SERVER_ENDPOINT").unwrap_or_else(|_| "http://localhost:4000".to_string()),
      search_roots: std::env::var("SEARCH_ROOTS")
        .unwrap_or_else(|_| "/var/log".to_string())
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect(),
      listen_port: std::env::var("AGENT_PORT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(8090),
      enable_heartbeat: std::env::var("ENABLE_HEARTBEAT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(true),
      heartbeat_interval_secs: std::env::var("HEARTBEAT_INTERVAL")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(30),
      worker_threads: std::env::var("AGENT_WORKER_THREADS").ok().and_then(|s| s.parse().ok()),
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
      tags: vec!["production".to_string()],
      search_roots: self.search_roots.clone(),
      last_heartbeat: chrono::Utc::now().timestamp(),
      status: AgentStatus::Online,
    }
  }
}

// ============================================================================
// 应用状态
// ============================================================================

#[derive(Clone)]
struct AppState {
  config: Arc<AgentConfig>,
  task_manager: Arc<TaskManager>,
}

/// 任务管理器
struct TaskManager {
  tasks: RwLock<std::collections::HashMap<String, TaskInfo>>,
}

struct TaskInfo {
  task_id: String,
  status: SearchStatus,
  processed: usize,
  matched: usize,
}

impl TaskManager {
  fn new() -> Self {
    Self {
      tasks: RwLock::new(std::collections::HashMap::new()),
    }
  }

  async fn add_task(&self, task_id: String) {
    self.tasks.write().await.insert(
      task_id.clone(),
      TaskInfo {
        task_id,
        status: SearchStatus::Running,
        processed: 0,
        matched: 0,
      },
    );
  }

  async fn update_progress(&self, task_id: &str, processed: usize, matched: usize) {
    if let Some(task) = self.tasks.write().await.get_mut(task_id) {
      task.processed = processed;
      task.matched = matched;
    }
  }

  async fn complete_task(&self, task_id: &str) {
    if let Some(task) = self.tasks.write().await.get_mut(task_id) {
      task.status = SearchStatus::Completed;
    }
  }

  async fn get_progress(&self, task_id: &str) -> Option<SearchProgress> {
    self.tasks.read().await.get(task_id).map(|task| SearchProgress {
      task_id: task.task_id.clone(),
      processed_files: task.processed,
      matched_files: task.matched,
      total_files: None,
      status: task.status.clone(),
    })
  }
}

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

/// 处理搜索请求
async fn handle_search(State(state): State<AppState>, Json(request): Json<AgentSearchRequest>) -> impl IntoResponse {
  info!("收到搜索请求: task_id={}, query={}", request.task_id, request.query);

  // 添加任务
  state.task_manager.add_task(request.task_id.clone()).await;

  // 创建结果 channel
  let (tx, rx) = mpsc::channel(128);

  // 在后台执行搜索
  tokio::spawn(execute_search(
    state.config.clone(),
    state.task_manager.clone(),
    request,
    tx,
  ));

  // 将 channel 转换为 NDJSON 流
  let stream = ReceiverStream::new(rx).map(|msg| {
    let json = serde_json::to_string(&msg).unwrap_or_else(|_| "{}".to_string());
    Ok::<_, std::convert::Infallible>(format!("{}\n", json))
  });

  Response::builder()
    .status(StatusCode::OK)
    .header(CONTENT_TYPE, "application/x-ndjson; charset=utf-8")
    .body(Body::from_stream(stream))
    .unwrap()
}

/// 获取搜索进度
async fn handle_progress(State(state): State<AppState>, Path(task_id): Path<String>) -> Json<Option<SearchProgress>> {
  Json(state.task_manager.get_progress(&task_id).await)
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
async fn execute_search(
  config: Arc<AgentConfig>,
  task_manager: Arc<TaskManager>,
  request: AgentSearchRequest,
  tx: mpsc::Sender<AgentMessage>,
) {
  let task_id = request.task_id.clone();

  // 1. 解析查询
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

  // 3. 遍历所有搜索根目录
  let mut all_processed = 0;
  let mut all_matched = 0;

  for root in &config.search_roots {
    info!("开始搜索目录: {}", root);

    // 使用 FsEntryStream + EntryStreamProcessor 遍历并搜索
    let mut stream = match FsEntryStream::new(PathBuf::from(root)).await {
      Ok(s) => s,
      Err(e) => {
        warn!("无法读取目录 {}: {}", root, e);
        continue;
      }
    };

    let mut proc_stream = EntryStreamProcessor::new(processor.clone());
    let (sr_tx, mut sr_rx) = mpsc::channel::<logseek::service::search::SearchResult>(128);

    // 后台处理条目流
    let handle = tokio::spawn(async move {
      let _ = proc_stream.process_stream(&mut stream, sr_tx).await;
    });

    while let Some(result) = sr_rx.recv().await {
      all_processed += 1; // 以条目为单位统计处理量
      if tx.send(AgentMessage::Result(result)).await.is_err() {
        debug!("接收端已关闭");
        return;
      }
      all_matched += 1; // 仅在有结果时递增匹配（此处等同于 processed，因为只有匹配才有结果）

      if all_processed % 100 == 0 {
        task_manager.update_progress(&task_id, all_processed, all_matched).await;
        let _ = tx
          .send(AgentMessage::Progress(SearchProgress {
            task_id: task_id.clone(),
            processed_files: all_processed,
            matched_files: all_matched,
            total_files: None,
            status: SearchStatus::Running,
          }))
          .await;
      }
    }

    let _ = handle.await; // 等待条目处理结束
  }

  // 标记任务完成
  task_manager.complete_task(&task_id).await;

  // 发送完成消息
  let _ = tx.send(AgentMessage::Complete).await;

  info!(
    "搜索完成: task_id={}, processed={}, matched={}",
    task_id, all_processed, all_matched
  );
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

  let info = config.to_agent_info();
  let url = format!("{}/api/v1/agents/register", config.server_endpoint);

  debug!("向 Server 注册: {}", url);

  let response = client.post(&url).json(&info).send().await?;

  if response.status().is_success() {
    info!("✓ 已成功向 Server 注册");
    Ok(())
  } else {
    Err(format!("注册失败: {}", response.status()).into())
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
        warn!("心跳失败: {}", response.status());
      }
      Err(e) => {
        warn!("心跳发送出错: {}", e);
      }
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_config_from_env() {
    // 设置环境变量
    unsafe {
      std::env::set_var("AGENT_ID", "test-agent");
      std::env::set_var("SEARCH_ROOTS", "/var/log,/opt/logs");
    }

    let config = AgentConfig::from_env();

    assert_eq!(config.agent_id, "test-agent");
    assert_eq!(config.search_roots.len(), 2);
    assert_eq!(config.search_roots[0], "/var/log");
    assert_eq!(config.search_roots[1], "/opt/logs");

    // 清理
    unsafe {
      std::env::remove_var("AGENT_ID");
      std::env::remove_var("SEARCH_ROOTS");
    }
  }
}
