use clap::{Parser, Subcommand};
use std::net::SocketAddr;
use std::path::PathBuf;

/// 将字符串解析为端口（u16）
fn port_parser(s: &str) -> Result<u16, String> {
  s.parse::<u16>().map_err(|_| format!("无效的端口号：{s}"))
}

/// 将字符串解析为 SocketAddr
fn addr_parser(s: &str) -> Result<SocketAddr, String> {
  s.parse::<SocketAddr>()
    .map_err(|_| format!("无效的地址：{s}，请使用 HOST:PORT 或 [IPv6]:PORT 格式"))
}

/// 子命令定义
#[derive(Subcommand, Debug, Clone)]
pub enum Commands {
  /// 启动服务（默认后台运行，可通过 --daemon=false 前台运行）
  Start {
    /// 是否后台运行（默认 true，仅类 Unix 支持）
    #[arg(long, short = 'd', default_value_t = true)]
    daemon: bool,
    /// PID 文件路径（默认：~/.opsbox/opsbox.pid）
    #[arg(long, value_name = "FILE")]
    pid_file: Option<PathBuf>,
  },
  /// 停止服务（通过 PID 文件定位进程）
  Stop {
    /// PID 文件路径（默认：~/.opsbox/opsbox.pid）
    #[arg(long, value_name = "FILE")]
    pid_file: Option<PathBuf>,
    /// 强制停止（发送 SIGKILL）
    #[arg(long, short = 'f', default_value_t = false)]
    force: bool,
  },
}

/// 命令行配置
#[derive(Parser, Debug, Clone)]
#[command(
  author = "wangyue",
  // name = "opsbox",
  version,
  about = "OpsBox 运维工具箱（内置前端静态资源）",
  long_about = "OpsBox 是一个集成了多个运维功能的工具箱，包括日志检索（LogSeek）等模块。\n支持通过参数设置监听地址/端口，并可选择后台运行（Unix）。",
  help_template = "{about}\n\nAuthor: {author}\n\n{usage-heading}\n{usage}\n\n{all-args}{subcommands}"
)]
pub struct AppConfig {
  // 服务器配置
  #[arg(
    global = true,
    long,
    short = 'H',
    value_name = "HOST",
    default_value = "127.0.0.1",
    help = "监听地址"
  )]
  pub host: String,

  #[arg(global = true, long, short = 'P', value_name = "PORT", default_value_t = 4000, value_parser = port_parser, help = "监听端口")]
  pub port: u16,

  #[arg(global = true, long, short = 'a', value_name = "HOST:PORT", value_parser = addr_parser, help = "完整地址（优先于 --host/--port）")]
  pub addr: Option<SocketAddr>,

  #[arg(long, short = 'd', default_value_t = false, help = "后台运行（仅 Unix 支持）")]
  pub daemon: bool,

  // 日志配置
  #[arg(
    global = true,
    long = "log-level",
    value_name = "LEVEL",
    help = "日志级别：error|warn|info|debug|trace"
  )]
  pub log_level: Option<String>,

  #[arg(global = true, short = 'v', action = clap::ArgAction::Count, help = "增加日志详细程度（-v/-vv/-vvv）")]
  pub verbose: u8,

  // 数据库配置
  #[arg(
    long = "database-url",
    value_name = "URL",
    help = "数据库路径（覆盖 OPSBOX_DATABASE_URL）"
  )]
  pub database_url: Option<String>,

  // 性能配置（LogSeek 模块）
  #[arg(
    long = "io-max-concurrency",
    value_name = "N",
    help = "IO 最大并发数（控制 S3/Local/Agent 等所有数据源的并发访问）"
  )]
  pub io_max_concurrency: Option<usize>,

  #[arg(
    long = "io-timeout-sec",
    value_name = "SECS",
    help = "IO 操作超时秒数（适用于所有远程数据源：S3/Agent 等）"
  )]
  pub io_timeout_sec: Option<u64>,

  #[arg(
    long = "io-max-retries",
    value_name = "N",
    help = "IO 操作最大重试次数（指数退避，适用于所有远程数据源）"
  )]
  pub io_max_retries: Option<u32>,

  /// 管理子命令（start/stop）
  #[command(subcommand)]
  pub cmd: Option<Commands>,

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

impl AppConfig {
  /// 获取最终监听地址
  pub fn get_addr(&self) -> Result<SocketAddr, String> {
    if let Some(a) = self.addr {
      Ok(a)
    } else {
      format!("{}:{}", self.host, self.port)
        .parse::<SocketAddr>()
        .map_err(|e| format!("组合地址无效: {}", e))
    }
  }

  /// 获取用户主目录（跨平台）
  fn get_user_home() -> String {
    #[cfg(windows)]
    {
      std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .unwrap_or_else(|_| ".".into())
    }
    #[cfg(not(windows))]
    {
      std::env::var("HOME").unwrap_or_else(|_| ".".into())
    }
  }

  /// 获取数据库 URL（优先级：CLI > 环境变量 > 默认值）
  pub fn get_database_url(&self) -> String {
    self
      .database_url
      .clone()
      .or_else(|| std::env::var("OPSBOX_DATABASE_URL").ok())
      .or_else(|| std::env::var("DATABASE_URL").ok())
      .unwrap_or_else(|| {
        let home = Self::get_user_home();
        let dir = std::path::PathBuf::from(home).join(".opsbox");
        let _ = std::fs::create_dir_all(&dir);
        dir.join("opsbox.db").to_string_lossy().to_string()
      })
  }

  /// 获取 IO 最大并发（用于所有数据源：S3/Local/Agent）
  pub fn get_io_max_concurrency(&self) -> usize {
    self
      .io_max_concurrency
      .or_else(|| Self::env_usize("LOGSEEK_IO_MAX_CONCURRENCY"))
      .unwrap_or(12)
      .clamp(1, 128)
  }

  /// 获取 IO 超时时间（用于所有远程数据源）
  pub fn get_io_timeout_sec(&self) -> u64 {
    self
      .io_timeout_sec
      .or_else(|| Self::env_u64("LOGSEEK_IO_TIMEOUT_SEC"))
      .unwrap_or(60)
      .clamp(5, 300)
  }

  /// 获取 IO 最大重试次数（用于所有远程数据源）
  pub fn get_io_max_retries(&self) -> u32 {
    self
      .io_max_retries
      .or_else(|| Self::env_u32("LOGSEEK_IO_MAX_RETRIES"))
      .unwrap_or(5)
      .clamp(1, 20)
  }

  fn env_usize(key: &str) -> Option<usize> {
    std::env::var(key).ok().and_then(|s| s.parse().ok())
  }

  fn env_u64(key: &str) -> Option<u64> {
    std::env::var(key).ok().and_then(|s| s.parse().ok())
  }

  fn env_u32(key: &str) -> Option<u32> {
    std::env::var(key).ok().and_then(|s| s.parse().ok())
  }
}
