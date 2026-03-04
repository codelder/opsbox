use clap::{Parser, Subcommand};
use std::net::SocketAddr;
use std::path::PathBuf;

/// 将字符串解析为端口（u16）
fn port_parser(s: &str) -> Result<u16, String> {
  s.parse::<u16>().map_err(|_| format!("无效的端口号：{s}"))
}

/// 验证主机地址（IPv4、IPv6 或主机名）
fn host_parser(s: &str) -> Result<String, String> {
  let s = s.trim();

  if s.is_empty() {
    return Err("主机地址不能为空".to_string());
  }

  // 尝试解析为 IP 地址（IPv4 或 IPv6）
  if s.parse::<std::net::IpAddr>().is_ok() {
    return Ok(s.to_string());
  }

  // 尝试解析为带方括号的 IPv6 地址（如 [::1]）
  if s
    .strip_prefix('[')
    .and_then(|s| s.strip_suffix(']'))
    .and_then(|ipv6_str| ipv6_str.parse::<std::net::Ipv6Addr>().ok())
    .is_some()
  {
    return Ok(s.to_string());
  }

  // 对于主机名，接受非空字符串，让系统在绑定的时候验证
  Ok(s.to_string())
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
    default_value = "0.0.0.0",
    value_parser = host_parser,
    help = "监听地址（IPv4、IPv6 或主机名）"
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
        format!("{}\\.opsbox\\logs", home.trim_end_matches(['\\', '/']))
      }
      #[cfg(not(windows))]
      {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/home/user".to_string());
        format!("{}/.opsbox/logs", home.trim_end_matches('/'))
      }
    },
    help = "日志文件目录"
  )]
  pub log_dir: String,

  #[arg(
    global = true,
    long = "log-retention",
    value_name = "N",
    help = "保留的日志文件数量",
    default_value = "7"
  )]
  pub log_retention: usize,

  // 数据库配置
  #[arg(
    long = "database-url",
    value_name = "URL",
    help = "数据库路径（覆盖 OPSBOX_DATABASE_URL，默认：$HOME/.opsbox/opsbox.db）"
  )]
  pub database_url: Option<String>,

  // 性能配置（LogSeek 模块）
  #[arg(
    long = "io-max-concurrency",
    value_name = "N",
    help = "IO 最大并发数（控制 S3/Local/Agent 等所有数据源的并发访问，默认 12）"
  )]
  pub io_max_concurrency: Option<usize>,

  #[arg(
    long = "io-timeout-sec",
    value_name = "SECS",
    help = "IO 操作超时秒数（适用于所有远程数据源：S3/Agent 等，默认 60 秒）"
  )]
  pub io_timeout_sec: Option<u64>,

  #[arg(
    long = "io-max-retries",
    value_name = "N",
    help = "IO 操作最大重试次数（指数退避，适用于所有远程数据源，默认 5 次）"
  )]
  pub io_max_retries: Option<u32>,

  #[arg(
    long = "server-id",
    value_name = "ID",
    help = "当前服务器的唯一标识（可以是域名或 IP），用于生成跨集群可访问的 FileURL"
  )]
  pub server_id: Option<String>,

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

  /// 获取服务器标识
  pub fn get_server_id(&self) -> Option<String> {
    self
      .server_id
      .clone()
      .or_else(|| std::env::var("LOGSEEK_SERVER_ID").ok())
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

  /// 获取日志目录
  pub fn get_log_dir(&self) -> PathBuf {
    PathBuf::from(self.log_dir.trim())
  }

  /// 获取日志保留数量
  pub fn get_log_retention(&self) -> usize {
    self.log_retention
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  /// Helper function to create a minimal AppConfig for testing
  fn test_config() -> AppConfig {
    AppConfig {
      host: "0.0.0.0".to_string(),
      port: 4000,
      addr: None,
      daemon: false,
      log_level: None,
      verbose: 0,
      log_dir: "/tmp/logs".to_string(),
      log_retention: 7,
      database_url: None,
      io_max_concurrency: None,
      io_timeout_sec: None,
      io_max_retries: None,
      server_id: None,
      cmd: None,
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
    }
  }

  /// Test port_parser validates port numbers correctly
  ///
  /// 业务场景: 确保 CLI 能够正确验证用户输入的端口号
  #[test]
  fn test_port_parser_valid_ports() {
    assert_eq!(port_parser("80").unwrap(), 80);
    assert_eq!(port_parser("8080").unwrap(), 8080);
    assert_eq!(port_parser("65535").unwrap(), 65535);
    assert_eq!(port_parser("1").unwrap(), 1);
  }

  /// Test port_parser rejects invalid ports
  ///
  /// 业务场景: 防止用户输入无效的端口号导致服务启动失败
  #[test]
  fn test_port_parser_invalid_ports() {
    assert!(port_parser("abc").is_err());
    assert!(port_parser("").is_err());
    assert!(port_parser("99999").is_err()); // > u16::MAX
    assert!(port_parser("-1").is_err());
    // Note: port 0 is technically valid u16, but usually reserved
    // We accept it here as the parser doesn't reject it
    assert_eq!(port_parser("0").unwrap(), 0);
  }

  /// Test host_parser validates various host formats
  ///
  /// 业务场景: 支持 IPv4、IPv6、主机名等多种地址格式
  #[test]
  fn test_host_parser_valid_hosts() {
    // IPv4
    assert_eq!(host_parser("127.0.0.1").unwrap(), "127.0.0.1");
    assert_eq!(host_parser("0.0.0.0").unwrap(), "0.0.0.0");
    assert_eq!(host_parser("192.168.1.1").unwrap(), "192.168.1.1");

    // IPv6
    assert_eq!(host_parser("::1").unwrap(), "::1");
    assert_eq!(host_parser("[::1]").unwrap(), "[::1]");
    assert_eq!(host_parser("2001:db8::1").unwrap(), "2001:db8::1");

    // Hostnames
    assert_eq!(host_parser("localhost").unwrap(), "localhost");
    assert_eq!(host_parser("example.com").unwrap(), "example.com");
  }

  /// Test host_parser rejects empty input
  #[test]
  fn test_host_parser_empty() {
    assert!(host_parser("").is_err());
    assert!(host_parser("   ").is_err()); // whitespace only
  }

  /// Test addr_parser validates SocketAddr formats
  ///
  /// 业务场景: 支持完整的 HOST:PORT 地址格式
  #[test]
  fn test_addr_parser_valid() {
    let addr = addr_parser("127.0.0.1:8080").unwrap();
    assert_eq!(addr.ip().to_string(), "127.0.0.1");
    assert_eq!(addr.port(), 8080);

    let addr = addr_parser("[::1]:9000").unwrap();
    assert_eq!(addr.port(), 9000);
  }

  /// Test addr_parser rejects invalid formats
  #[test]
  fn test_addr_parser_invalid() {
    assert!(addr_parser("localhost").is_err()); // missing port
    assert!(addr_parser(":8080").is_err()); // missing host
    assert!(addr_parser("127.0.0.1").is_err()); // missing port
    assert!(addr_parser("").is_err());
  }

  /// Test AppConfig::get_addr combines host and port correctly
  #[test]
  fn test_get_addr_combines_host_port() {
    let config = AppConfig {
      host: "127.0.0.1".to_string(),
      port: 8080,
      ..test_config()
    };

    let addr = config.get_addr().unwrap();
    assert_eq!(addr.ip().to_string(), "127.0.0.1");
    assert_eq!(addr.port(), 8080);
  }

  /// Test AppConfig::get_addr prefers explicit addr over host/port
  #[test]
  fn test_get_addr_prefers_explicit() {
    let explicit = "192.168.1.100:9000".parse().unwrap();
    let config = AppConfig {
      host: "127.0.0.1".to_string(),
      port: 8080,
      addr: Some(explicit),
      ..test_config()
    };

    let addr = config.get_addr().unwrap();
    assert_eq!(addr.ip().to_string(), "192.168.1.100");
    assert_eq!(addr.port(), 9000);
  }

  /// Test get_io_max_concurrency returns clamped values
  ///
  /// 业务场景: 确保并发数在合理范围内（1-128），防止资源耗尽
  #[test]
  fn test_get_io_max_concurrency_clamping() {
    // Too low - should clamp to 1
    let config = AppConfig {
      io_max_concurrency: Some(0),
      ..test_config()
    };
    assert_eq!(config.get_io_max_concurrency(), 1);

    // Normal value
    let config = AppConfig {
      io_max_concurrency: Some(50),
      ..test_config()
    };
    assert_eq!(config.get_io_max_concurrency(), 50);

    // Too high - should clamp to 128
    let config = AppConfig {
      io_max_concurrency: Some(200),
      ..test_config()
    };
    assert_eq!(config.get_io_max_concurrency(), 128);
  }

  /// Test get_io_timeout_sec returns clamped values
  ///
  /// 业务场景: 确保超时时间在合理范围内（5-300秒）
  #[test]
  fn test_get_io_timeout_sec_clamping() {
    // Too low - should clamp to 5
    let config = AppConfig {
      io_timeout_sec: Some(1),
      ..test_config()
    };
    assert_eq!(config.get_io_timeout_sec(), 5);

    // Normal value
    let config = AppConfig {
      io_timeout_sec: Some(120),
      ..test_config()
    };
    assert_eq!(config.get_io_timeout_sec(), 120);

    // Too high - should clamp to 300
    let config = AppConfig {
      io_timeout_sec: Some(600),
      ..test_config()
    };
    assert_eq!(config.get_io_timeout_sec(), 300);
  }

  /// Test get_io_max_retries returns clamped values
  ///
  /// 业务场景: 确保重试次数在合理范围内（1-20次）
  #[test]
  fn test_get_io_max_retries_clamping() {
    // Too low - should clamp to 1
    let config = AppConfig {
      io_max_retries: Some(0),
      ..test_config()
    };
    assert_eq!(config.get_io_max_retries(), 1);

    // Normal value
    let config = AppConfig {
      io_max_retries: Some(10),
      ..test_config()
    };
    assert_eq!(config.get_io_max_retries(), 10);

    // Too high - should clamp to 20
    let config = AppConfig {
      io_max_retries: Some(50),
      ..test_config()
    };
    assert_eq!(config.get_io_max_retries(), 20);
  }

  /// Test get_log_retention returns configured value
  #[test]
  fn test_get_log_retention() {
    let config = AppConfig {
      log_retention: 30,
      ..test_config()
    };
    assert_eq!(config.get_log_retention(), 30);
  }

  /// Test default values from helper function
  #[test]
  fn test_config_defaults() {
    let config = test_config();
    assert_eq!(config.host, "0.0.0.0");
    assert_eq!(config.port, 4000);
    assert!(!config.daemon);
    assert_eq!(config.get_io_max_concurrency(), 12); // default
    assert_eq!(config.get_io_timeout_sec(), 60); // default
    assert_eq!(config.get_io_max_retries(), 5); // default
  }
}
