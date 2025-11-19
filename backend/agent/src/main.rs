// ============================================================================
// LogSeek Agent - 远程搜索代理
// ============================================================================
//
// Agent 安装在远程服务器上，接收来自 Server 的搜索请求，
// 在本地执行搜索并将结果返回给 Server。
//

mod api;
mod config;
mod daemon;
mod path;
mod routes;
mod search;
mod server;
mod shutdown;

#[cfg(windows)]
mod daemon_windows;

use clap::Parser;
use opsbox_core::logging::{LogConfig, LogLevel};
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::Notify;
use tracing::{error, info};

use config::{Args, Commands};
#[cfg(unix)]
use daemon::{default_pid_file, resolve_pid_path, start_daemon, stop_daemon};
use routes::create_router;
use server::{heartbeat_loop, register_to_server};
use shutdown::shutdown_signal;

fn main() -> Result<(), Box<dyn std::error::Error>> {
  // 解析命令行参数
  let args = Args::parse();

  // 处理 Windows 服务相关命令（优先处理）
  #[cfg(windows)]
  {
    use daemon_windows::{handle_install_service, handle_start_service, handle_stop_service, handle_uninstall_service};

    if args.install_service {
      handle_install_service("OpsBoxAgent", "OpsBox Agent", &args);
      return Ok(());
    }
    if args.uninstall_service {
      handle_uninstall_service("OpsBoxAgent");
      return Ok(());
    }
    if args.start_service {
      handle_start_service("OpsBoxAgent");
      return Ok(());
    }
    if args.stop_service {
      handle_stop_service("OpsBoxAgent");
      return Ok(());
    }
    if args.service_mode {
      use daemon_windows::run_windows_service_with_dispatcher;
      run_windows_service_with_dispatcher("OpsBoxAgent", args);
      return Ok(());
    }
  }

  // 处理 stop 子命令（优先处理）
  if let Some(Commands::Stop { pid_file, force }) = &args.cmd {
    handle_stop_command(pid_file, force);
    return Ok(());
  }

  // 处理守护进程模式（在日志初始化之前，避免重复初始化）
  handle_daemon_mode(&args);

  // 加载配置
  let mut config = config::AgentConfig::from_args(args.clone());

  // 初始化日志系统
  let log_config = LogConfig {
    level: LogLevel::Info,
    log_dir: config.log_dir.clone(),
    retention_count: config.log_retention,
    enable_console: true,
    enable_file: true,
    file_prefix: "opsbox-agent".to_string(),
  };

  match opsbox_core::logging::init(log_config) {
    Ok(reload_handle) => {
      config.set_reload_handle(reload_handle);
      // 如果环境变量 RUST_LOG 设置了日志级别，更新 current_log_level
      if let Ok(rust_log) = std::env::var("RUST_LOG") {
        // 解析 RUST_LOG 环境变量（可能是 "info"、"opsbox_agent=debug" 等格式）
        let level = rust_log
          .split(',')
          .find_map(|s| {
            let s = s.trim();
            // 处理 "opsbox_agent=debug" 格式
            if let Some((_, level)) = s.split_once('=') {
              let level = level.trim().to_lowercase();
              if matches!(level.as_str(), "error" | "warn" | "info" | "debug" | "trace") {
                return Some(level);
              }
            }
            // 处理纯级别字符串 "info"
            let s_lower = s.to_lowercase();
            if matches!(s_lower.as_str(), "error" | "warn" | "info" | "debug" | "trace") {
              return Some(s_lower);
            }
            None
          })
          .unwrap_or_else(|| "info".to_string());
        *config.current_log_level.lock().unwrap() = level;
      }
      info!("日志系统初始化成功");
    }
    Err(e) => {
      eprintln!("日志系统初始化失败: {}", e);
      std::process::exit(1);
    }
  }

  let config = Arc::new(config);

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

pub(crate) async fn async_main(config: Arc<config::AgentConfig>) -> Result<(), Box<dyn std::error::Error>> {
  info!("╔══════════════════════════════════════════╗");
  info!("║     Opsbox Agent 启动中...               ║");
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

  // 构建全局关闭通知器（供优雅关闭时停止后台任务）
  let shutdown_notify = Arc::new(Notify::new());

  // 启动 OS 信号监听任务，收到 SIGINT/SIGTERM 后唤醒所有等待者
  let sn = shutdown_notify.clone();
  tokio::spawn(async move {
    shutdown_signal().await;
    info!("收到关闭信号，开始优雅关闭……");
    sn.notify_waiters();
  });

  // 启动心跳任务（可被关闭通知打断）
  if config.enable_heartbeat {
    let sn = shutdown_notify.clone();
    tokio::spawn(heartbeat_loop(config.clone(), sn));
  }

  // 构建路由
  let app = create_router(config.clone());

  // 启动 HTTP 服务器
  let addr = SocketAddr::from(([0, 0, 0, 0], config.listen_port));
  info!("Agent HTTP 服务监听: {}", addr);
  info!("准备就绪，等待搜索请求...");

  let listener = tokio::net::TcpListener::bind(addr).await?;

  // 支持优雅关闭
  axum::serve(listener, app)
    .with_graceful_shutdown({
      let sn = shutdown_notify.clone();
      async move {
        sn.notified().await;
      }
    })
    .await?;

  info!("Agent 已关闭");
  Ok(())
}

// ============================================================================
// 守护进程相关功能
// ============================================================================

/// 处理停止命令
fn handle_stop_command(pid_file: &Option<std::path::PathBuf>, force: &bool) {
  #[cfg(unix)]
  {
    let pid_path = resolve_pid_path(pid_file);
    if let Err(e) = stop_daemon(pid_path, *force) {
      eprintln!("停止 Agent 失败: {}", e);
      std::process::exit(1);
    }
  }
  #[cfg(all(not(unix), not(windows)))]
  {
    let _ = (pid_file, force); // 避免未使用变量警告
    eprintln!("停止命令仅在 Unix 系统上支持");
    std::process::exit(1);
  }
  #[cfg(windows)]
  {
    let _ = (pid_file, force); // 避免未使用变量警告
    eprintln!("在 Windows 上，请使用 --stop-service 或 sc stop 命令停止服务");
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
  #[cfg(all(not(unix), not(windows)))]
  {
    if let Some(Commands::Start { daemon, .. }) = &args.cmd
      && *daemon
    {
      eprintln!("守护进程模式仅在 Unix 系统上支持");
      std::process::exit(1);
    }
  }
  #[cfg(windows)]
  {
    if let Some(Commands::Start { daemon, .. }) = &args.cmd
      && *daemon
    {
      eprintln!("在 Windows 上，请使用 --service-mode 或安装为 Windows 服务");
      std::process::exit(1);
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
      log_dir: "/custom/logs".to_string(),
      log_retention: 14,
      #[cfg(windows)]
      service_mode: false,
      #[cfg(windows)]
      install_service: false,
      #[cfg(windows)]
      uninstall_service: false,
      #[cfg(windows)]
      start_service: false,
      #[cfg(windows)]
      stop_service: false,
    };

    let config = config::AgentConfig::from_args(args);

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
    assert_eq!(config.log_dir, std::path::PathBuf::from("/custom/logs"));
    assert_eq!(config.log_retention, 14);
    assert!(config.reload_handle.is_none());
  }
}
