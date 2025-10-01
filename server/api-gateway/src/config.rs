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
  name = "opsbox",
  version,
  disable_version_flag = true,
  about = "OpsBox 运维工具箱（内置前端静态资源）",
  long_about = "OpsBox 是一个集成了多个运维功能的工具箱，包括日志检索（LogSeek）等模块。\n支持通过参数设置监听地址/端口，并可选择后台运行（Unix）。"
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

  #[arg(global = true, short = 'V', action = clap::ArgAction::Count, help = "增加日志详细程度（-V/-VV/-VVV）")]
  pub verbose: u8,

  // 数据库配置
  #[arg(
    long = "database-url",
    value_name = "URL",
    help = "数据库路径（覆盖 OPSBOX_DATABASE_URL）"
  )]
  pub database_url: Option<String>,

  // 性能配置（LogSeek 模块）
  #[arg(long = "worker-threads", value_name = "N", help = "Tokio 工作线程数")]
  pub worker_threads: Option<usize>,

  #[arg(long = "s3-max-concurrency", value_name = "N", help = "S3/MinIO 最大并发")]
  pub s3_max_concurrency: Option<usize>,

  #[arg(long = "cpu-concurrency", value_name = "N", help = "CPU 解压/检索最大并发")]
  pub cpu_concurrency: Option<usize>,

  #[arg(long = "stream-ch-cap", value_name = "N", help = "NDJSON 输出通道容量")]
  pub stream_ch_cap: Option<usize>,

  #[arg(long = "minio-timeout-sec", value_name = "SECS", help = "MinIO 操作超时秒数")]
  pub minio_timeout_sec: Option<u64>,

  #[arg(long = "minio-max-attempts", value_name = "N", help = "MinIO 获取对象最大重试次数")]
  pub minio_max_attempts: Option<u32>,

  /// 管理子命令（start/stop）
  #[command(subcommand)]
  pub cmd: Option<Commands>,
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

  /// 获取数据库 URL（优先级：CLI > 环境变量 > 默认值）
  pub fn get_database_url(&self) -> String {
    self
      .database_url
      .clone()
      .or_else(|| std::env::var("OPSBOX_DATABASE_URL").ok())
      .or_else(|| std::env::var("DATABASE_URL").ok())
      .unwrap_or_else(|| "./opsbox.db".to_string())
  }

  /// 获取工作线程数
  pub fn get_worker_threads(&self) -> usize {
    self
      .worker_threads
      .or_else(|| Self::env_usize("LOGSEEK_WORKER_THREADS"))
      .unwrap_or_else(|| {
        let phys = num_cpus::get_physical().max(1);
        let cpu_conc = self.get_cpu_concurrency();
        phys.min(cpu_conc + 2).clamp(2, 18)
      })
      .clamp(2, 64)
  }

  /// 获取 S3 最大并发
  pub fn get_s3_max_concurrency(&self) -> usize {
    self
      .s3_max_concurrency
      .or_else(|| Self::env_usize("LOGSEEK_S3_MAX_CONCURRENCY"))
      .unwrap_or(12)
      .clamp(1, 128)
  }

  /// 获取 CPU 并发数
  pub fn get_cpu_concurrency(&self) -> usize {
    self
      .cpu_concurrency
      .or_else(|| Self::env_usize("LOGSEEK_CPU_CONCURRENCY"))
      .unwrap_or(16)
      .clamp(1, 128)
  }

  /// 获取流通道容量
  pub fn get_stream_ch_cap(&self) -> usize {
    self
      .stream_ch_cap
      .or_else(|| Self::env_usize("LOGSEEK_STREAM_CH_CAP"))
      .unwrap_or(256)
      .clamp(8, 10_000)
  }

  /// 获取 MinIO 超时时间
  pub fn get_minio_timeout_sec(&self) -> u64 {
    self
      .minio_timeout_sec
      .or_else(|| Self::env_u64("LOGSEEK_MINIO_TIMEOUT_SEC"))
      .unwrap_or(60)
      .clamp(5, 300)
  }

  /// 获取 MinIO 最大重试次数
  pub fn get_minio_max_attempts(&self) -> u32 {
    self
      .minio_max_attempts
      .or_else(|| Self::env_u32("LOGSEEK_MINIO_MAX_ATTEMPTS"))
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
