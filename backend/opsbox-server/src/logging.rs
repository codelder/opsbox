use crate::config::AppConfig;
use opsbox_core::logging::{LogConfig, LogLevel, ReloadHandle, init as core_init};
use std::str::FromStr;

/// 初始化日志系统
///
/// 使用 opsbox-core 的 logging 模块初始化日志系统
/// 返回 ReloadHandle 用于动态修改日志级别
pub fn init(config: &AppConfig) -> Result<ReloadHandle, opsbox_core::logging::LogError> {
  // 确定日志级别
  let level = if let Some(ref level_str) = config.log_level {
    LogLevel::from_str(level_str)?
  } else {
    // 根据 verbose 参数确定日志级别
    match config.verbose {
      0 => LogLevel::Info,
      1 => LogLevel::Debug,
      _ => LogLevel::Trace,
    }
  };

  // 构建日志配置
  let log_config = LogConfig {
    level,
    log_dir: config.get_log_dir(),
    retention_count: config.get_log_retention(),
    enable_console: true,
    enable_file: true,
    file_prefix: "opsbox-server".to_string(),
  };

  // 初始化日志系统
  core_init(log_config)
}
