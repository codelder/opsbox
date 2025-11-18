//! Agent 配置管理
//!
//! 包含命令行参数解析和配置结构

use clap::{Parser, Subcommand};
use logseek::agent::{AgentInfo, AgentStatus};
use opsbox_core::logging::ReloadHandle;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

/// LogSeek Agent - 远程搜索代理
#[derive(Parser, Debug, Clone)]
#[command(name = "opsbox-agent")]
#[command(about = "Opsbox Agent - 运维工具箱远程代理")]
#[command(version)]
pub struct Args {
  #[command(subcommand)]
  pub cmd: Option<Commands>,

  /// Agent ID
  #[arg(global = true, long, default_value_t = {
    #[cfg(not(windows))]
    let hostname = hostname::get()
      .unwrap_or_else(|_| std::ffi::OsString::from("unknown"))
      .to_string_lossy()
      .to_string();
    #[cfg(windows)]
    let hostname = std::env::var("COMPUTERNAME")
      .unwrap_or_else(|_| "unknown".to_string());
    format!("agent-{}", hostname)
  })]
  pub agent_id: String,

  /// Agent 名称
  #[arg(global = true, long, default_value_t = {
    #[cfg(not(windows))]
    let hostname = hostname::get()
      .unwrap_or_else(|_| std::ffi::OsString::from("unknown"))
      .to_string_lossy()
      .to_string();
    #[cfg(windows)]
    let hostname = std::env::var("COMPUTERNAME")
      .unwrap_or_else(|_| "unknown".to_string());
    format!("Agent@{}", hostname)
  })]
  pub agent_name: String,

  /// 服务器端点
  #[arg(global = true, long, default_value = "http://localhost:4000")]
  pub server_endpoint: String,

  /// 搜索根目录（逗号分隔）
  #[arg(global = true, long, default_value_t = {
    #[cfg(windows)]
    let home = std::env::var("USERPROFILE")
      .or_else(|_| std::env::var("HOME"))
      .unwrap_or_else(|_| "C:\\Users\\User".to_string());
    #[cfg(not(windows))]
    let home = std::env::var("HOME").unwrap_or_else(|_| "/home/user".to_string());
    home
  })]
  pub search_roots: String,

  /// 监听端口
  #[arg(global = true, long, default_value_t = 4001)]
  pub listen_port: u16,

  /// 启用心跳
  #[arg(global = true, long, default_value_t = true)]
  pub enable_heartbeat: bool,

  /// 禁用心跳
  #[arg(global = true, long, action = clap::ArgAction::SetTrue)]
  pub no_heartbeat: bool,

  /// 心跳间隔（秒）
  #[arg(global = true, long, default_value_t = 30)]
  pub heartbeat_interval: u64,

  /// 工作线程数
  #[arg(global = true, long)]
  pub worker_threads: Option<usize>,

  /// 日志目录
  #[arg(
    global = true,
    long = "log-dir",
    value_name = "DIR",
    default_value_t = {
      #[cfg(windows)]
      {
        let home = std::env::var("USERPROFILE")
          .or_else(|_| std::env::var("HOME"))
          .unwrap_or_else(|_| "C:\\Users\\User".to_string());
        format!("{}\\.opsbox-agent\\logs", home.trim_end_matches(['/', '\\']))
      }
      #[cfg(not(windows))]
      {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/home/user".to_string());
        format!("{}/.opsbox-agent/logs", home.trim_end_matches('/'))
      }
    }
  )]
  pub log_dir: String,

  /// 日志保留数量
  #[arg(
    global = true,
    long = "log-retention",
    value_name = "N",
    help = "保留的日志文件数量",
    default_value = "7"
  )]
  pub log_retention: usize,

  /// 以 Windows 服务模式运行
  #[cfg(windows)]
  #[arg(long, help = "以 Windows 服务模式运行")]
  pub service_mode: bool,

  /// 安装 Windows 服务
  #[cfg(windows)]
  #[arg(long, help = "安装为 Windows 服务")]
  pub install_service: bool,

  /// 卸载 Windows 服务
  #[cfg(windows)]
  #[arg(long, help = "卸载 Windows 服务")]
  pub uninstall_service: bool,

  /// 启动 Windows 服务（通过 sc 命令）
  #[cfg(windows)]
  #[arg(long, help = "启动 Windows 服务")]
  pub start_service: bool,

  /// 停止 Windows 服务（通过 sc 命令）
  #[cfg(windows)]
  #[arg(long, help = "停止 Windows 服务")]
  pub stop_service: bool,
}

/// 子命令定义
#[derive(Subcommand, Debug, Clone)]
pub enum Commands {
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

/// Agent 配置
#[derive(Clone)]
pub struct AgentConfig {
  pub agent_id: String,
  pub agent_name: String,
  pub server_endpoint: String,
  pub search_roots: Vec<String>,
  pub listen_port: u16,
  pub enable_heartbeat: bool,
  pub heartbeat_interval_secs: u64,
  pub worker_threads: Option<usize>,
  pub log_dir: PathBuf,
  pub log_retention: usize,
  pub reload_handle: Option<Arc<ReloadHandle>>,
  pub current_log_level: Arc<Mutex<String>>,
}

impl AgentConfig {
  pub fn from_args(args: Args) -> Self {
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
      log_dir: PathBuf::from(args.log_dir),
      log_retention: args.log_retention,
      reload_handle: None,
      current_log_level: Arc::new(Mutex::new("info".to_string())),
    }
  }

  pub fn set_reload_handle(&mut self, handle: ReloadHandle) {
    self.reload_handle = Some(Arc::new(handle));
  }

  pub fn get_reload_handle(&self) -> Option<Arc<ReloadHandle>> {
    self.reload_handle.clone()
  }

  /// 获取工作线程数（优先级：环境变量 > 默认值）
  pub fn get_worker_threads(&self) -> usize {
    self
      .worker_threads
      .unwrap_or_else(|| {
        #[cfg(not(windows))]
        let cpu_count = num_cpus::get();
        #[cfg(windows)]
        let cpu_count = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1);
        match cpu_count {
          1 => 1,
          2..=4 => 2,
          5..=7 => 3,
          _ => cpu_count.div_ceil(2),
        }
      })
      .clamp(1, 16)
  }

  pub fn to_agent_info(&self) -> AgentInfo {
    #[cfg(not(windows))]
    let hostname = hostname::get()
      .unwrap_or_else(|_| std::ffi::OsString::from("unknown"))
      .to_string_lossy()
      .to_string();
    #[cfg(windows)]
    let hostname = std::env::var("COMPUTERNAME").unwrap_or_else(|_| "unknown".to_string());

    AgentInfo {
      id: self.agent_id.clone(),
      name: self.agent_name.clone(),
      version: env!("CARGO_PKG_VERSION").to_string(),
      hostname,
      tags: vec![],
      search_roots: self.search_roots.clone(),
      last_heartbeat: chrono::Utc::now().timestamp(),
      status: AgentStatus::Online,
    }
  }
}
