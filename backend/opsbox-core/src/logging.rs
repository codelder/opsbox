//! 日志系统模块
//!
//! 提供基于 tracing 的日志功能，支持：
//! - 控制台和文件双输出
//! - 按日期滚动日志文件
//! - 动态调整日志级别
//! - 自动清理旧日志文件

pub mod repository;
pub mod schema;

use chrono::Local;
use serde::{Deserialize, Serialize};
use std::fmt as stdfmt;
use std::path::PathBuf;
use std::str::FromStr;
use thiserror::Error;
use tracing::Level;
use tracing_appender::rolling::{RollingFileAppender, Rotation};
use tracing_subscriber::filter::LevelFilter;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{EnvFilter, Registry, fmt, reload};

#[derive(Clone, Copy, Default)]
struct LocalTimer;

impl tracing_subscriber::fmt::time::FormatTime for LocalTimer {
  fn format_time(&self, w: &mut tracing_subscriber::fmt::format::Writer<'_>) -> stdfmt::Result {
    let now = Local::now();
    w.write_str(&now.to_rfc3339())
  }
}

/// 日志错误类型
#[derive(Debug, Error)]
pub enum LogError {
  #[error("日志目录创建失败: {0}")]
  DirectoryCreation(#[from] std::io::Error),

  #[error("日志配置无效: {0}")]
  InvalidConfig(String),

  #[error("日志级别无效: {0}")]
  InvalidLevel(String),

  #[error("重载失败: {0}")]
  ReloadFailed(String),
}

/// 日志级别枚举
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
  Error,
  Warn,
  Info,
  Debug,
  Trace,
}

impl LogLevel {
  /// 转换为 tracing::Level
  pub fn to_tracing_level(&self) -> Level {
    match self {
      LogLevel::Error => Level::ERROR,
      LogLevel::Warn => Level::WARN,
      LogLevel::Info => Level::INFO,
      LogLevel::Debug => Level::DEBUG,
      LogLevel::Trace => Level::TRACE,
    }
  }

  /// 转换为 LevelFilter
  pub fn to_level_filter(&self) -> LevelFilter {
    match self {
      LogLevel::Error => LevelFilter::ERROR,
      LogLevel::Warn => LevelFilter::WARN,
      LogLevel::Info => LevelFilter::INFO,
      LogLevel::Debug => LevelFilter::DEBUG,
      LogLevel::Trace => LevelFilter::TRACE,
    }
  }
}

impl FromStr for LogLevel {
  type Err = LogError;

  fn from_str(s: &str) -> Result<Self, Self::Err> {
    match s.to_lowercase().as_str() {
      "error" => Ok(LogLevel::Error),
      "warn" => Ok(LogLevel::Warn),
      "info" => Ok(LogLevel::Info),
      "debug" => Ok(LogLevel::Debug),
      "trace" => Ok(LogLevel::Trace),
      _ => Err(LogError::InvalidLevel(s.to_string())),
    }
  }
}

impl std::fmt::Display for LogLevel {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      LogLevel::Error => write!(f, "error"),
      LogLevel::Warn => write!(f, "warn"),
      LogLevel::Info => write!(f, "info"),
      LogLevel::Debug => write!(f, "debug"),
      LogLevel::Trace => write!(f, "trace"),
    }
  }
}

/// 日志配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogConfig {
  /// 日志级别
  pub level: LogLevel,
  /// 日志目录
  pub log_dir: PathBuf,
  /// 日志保留数量（天）
  pub retention_count: usize,
  /// 是否启用控制台输出
  pub enable_console: bool,
  /// 是否启用文件输出
  pub enable_file: bool,
  /// 日志文件名前缀
  pub file_prefix: String,
}

impl Default for LogConfig {
  fn default() -> Self {
    Self {
      level: LogLevel::Info,
      log_dir: PathBuf::from("logs"),
      retention_count: 7,
      enable_console: true,
      enable_file: true,
      file_prefix: "opsbox".to_string(),
    }
  }
}

/// 重载句柄，用于动态修改日志配置
pub struct ReloadHandle {
  inner: reload::Handle<EnvFilter, Registry>,
}

impl ReloadHandle {
  /// 更新日志级别
  pub fn update_level(&self, level: LogLevel) -> Result<(), LogError> {
    let filter = EnvFilter::try_new(level.to_string()).map_err(|e| LogError::ReloadFailed(e.to_string()))?;

    self
      .inner
      .reload(filter)
      .map_err(|e| LogError::ReloadFailed(e.to_string()))?;

    Ok(())
  }
}

/// 初始化日志系统
///
/// 返回一个 ReloadHandle，用于运行时动态修改日志级别
pub fn init(config: LogConfig) -> Result<ReloadHandle, LogError> {
  // 创建日志目录
  if config.enable_file {
    std::fs::create_dir_all(&config.log_dir)?;
  }

  // 创建 EnvFilter，支持从环境变量 RUST_LOG 读取配置
  let filter = EnvFilter::try_from_default_env()
    .or_else(|_| EnvFilter::try_new(config.level.to_string()))
    .map_err(|e| LogError::InvalidConfig(e.to_string()))?;

  // 创建可重载的 filter layer
  let (filter_layer, reload_handle) = reload::Layer::new(filter);

  // 创建 Registry
  let registry = Registry::default().with(filter_layer);

  // 统一的本地时间定时器（RFC3339，本地时区）
  let timer = LocalTimer;

  // 创建 Console Layer（带彩色输出）
  if config.enable_console {
    let console_layer = fmt::layer()
      .with_timer(timer)
      .with_target(true)
      .with_thread_ids(false)
      .with_thread_names(false)
      .with_ansi(true)
      .with_file(false)
      .with_line_number(false)
      .compact();

    // 创建 File Layer（使用 RollingFileAppender）
    if config.enable_file {
      let file_appender = RollingFileAppender::builder()
        .rotation(Rotation::DAILY)
        .filename_prefix(&config.file_prefix)
        .filename_suffix("log")
        .max_log_files(config.retention_count)
        .build(&config.log_dir)
        .map_err(|e| LogError::InvalidConfig(format!("创建日志文件失败: {}", e)))?;

      let (non_blocking, _guard) = tracing_appender::non_blocking(file_appender);

      let file_layer = fmt::layer()
        .with_timer(timer)
        .with_writer(non_blocking)
        .with_target(true)
        .with_thread_ids(false)
        .with_thread_names(false)
        .with_ansi(false)
        .with_file(false)
        .with_line_number(false)
        .compact();

      registry.with(console_layer).with(file_layer).init();

      // 防止 _guard 被 drop
      std::mem::forget(_guard);
    } else {
      registry.with(console_layer).init();
    }
  } else if config.enable_file {
    let file_appender = RollingFileAppender::builder()
      .rotation(Rotation::DAILY)
      .filename_prefix(&config.file_prefix)
      .filename_suffix("log")
      .max_log_files(config.retention_count)
      .build(&config.log_dir)
      .map_err(|e| LogError::InvalidConfig(format!("创建日志文件失败: {}", e)))?;

    let (non_blocking, _guard) = tracing_appender::non_blocking(file_appender);

    let file_layer = fmt::layer()
      .with_timer(timer)
      .with_writer(non_blocking)
      .with_target(true)
      .with_thread_ids(false)
      .with_thread_names(false)
      .with_ansi(false)
      .with_file(false)
      .with_line_number(false)
      .compact();

    registry.with(file_layer).init();

    // 防止 _guard 被 drop
    std::mem::forget(_guard);
  } else {
    return Err(LogError::InvalidConfig("至少需要启用控制台或文件输出".to_string()));
  }

  Ok(ReloadHandle { inner: reload_handle })
}

/// 执行日志配置数据库迁移
pub async fn run_migration(pool: &sqlx::SqlitePool) -> Result<(), LogError> {
  sqlx::query(schema::LOG_CONFIG_SCHEMA)
    .execute(pool)
    .await
    .map_err(|e| LogError::InvalidConfig(format!("数据库迁移失败: {}", e)))?;

  tracing::info!("日志配置数据库迁移完成");
  Ok(())
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_log_level_from_str() {
    assert_eq!(LogLevel::from_str("error").unwrap(), LogLevel::Error);
    assert_eq!(LogLevel::from_str("warn").unwrap(), LogLevel::Warn);
    assert_eq!(LogLevel::from_str("info").unwrap(), LogLevel::Info);
    assert_eq!(LogLevel::from_str("debug").unwrap(), LogLevel::Debug);
    assert_eq!(LogLevel::from_str("trace").unwrap(), LogLevel::Trace);
    assert_eq!(LogLevel::from_str("ERROR").unwrap(), LogLevel::Error);
    assert_eq!(LogLevel::from_str("INFO").unwrap(), LogLevel::Info);
    assert!(LogLevel::from_str("invalid").is_err());
  }

  #[test]
  fn test_log_level_from_str_invalid() {
    let result = LogLevel::from_str("invalid");
    assert!(result.is_err());
    match result {
      Err(LogError::InvalidLevel(msg)) => assert_eq!(msg, "invalid"),
      _ => panic!("Expected InvalidLevel error"),
    }
  }

  #[test]
  fn test_log_level_to_string() {
    assert_eq!(LogLevel::Error.to_string(), "error");
    assert_eq!(LogLevel::Warn.to_string(), "warn");
    assert_eq!(LogLevel::Info.to_string(), "info");
    assert_eq!(LogLevel::Debug.to_string(), "debug");
    assert_eq!(LogLevel::Trace.to_string(), "trace");
  }

  #[test]
  fn test_log_level_to_tracing_level() {
    assert_eq!(LogLevel::Error.to_tracing_level(), Level::ERROR);
    assert_eq!(LogLevel::Warn.to_tracing_level(), Level::WARN);
    assert_eq!(LogLevel::Info.to_tracing_level(), Level::INFO);
    assert_eq!(LogLevel::Debug.to_tracing_level(), Level::DEBUG);
    assert_eq!(LogLevel::Trace.to_tracing_level(), Level::TRACE);
  }

  #[test]
  fn test_log_level_to_level_filter() {
    assert_eq!(LogLevel::Error.to_level_filter(), LevelFilter::ERROR);
    assert_eq!(LogLevel::Warn.to_level_filter(), LevelFilter::WARN);
    assert_eq!(LogLevel::Info.to_level_filter(), LevelFilter::INFO);
    assert_eq!(LogLevel::Debug.to_level_filter(), LevelFilter::DEBUG);
    assert_eq!(LogLevel::Trace.to_level_filter(), LevelFilter::TRACE);
  }

  #[test]
  fn test_log_level_serialization() {
    let level = LogLevel::Info;
    let json = serde_json::to_string(&level).unwrap();
    assert_eq!(json, "\"info\"");

    let deserialized: LogLevel = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized, LogLevel::Info);
  }

  #[test]
  fn test_log_level_deserialization_all_levels() {
    assert_eq!(serde_json::from_str::<LogLevel>("\"error\"").unwrap(), LogLevel::Error);
    assert_eq!(serde_json::from_str::<LogLevel>("\"warn\"").unwrap(), LogLevel::Warn);
    assert_eq!(serde_json::from_str::<LogLevel>("\"info\"").unwrap(), LogLevel::Info);
    assert_eq!(serde_json::from_str::<LogLevel>("\"debug\"").unwrap(), LogLevel::Debug);
    assert_eq!(serde_json::from_str::<LogLevel>("\"trace\"").unwrap(), LogLevel::Trace);
  }

  #[test]
  fn test_default_config() {
    let config = LogConfig::default();
    assert_eq!(config.level, LogLevel::Info);
    assert_eq!(config.retention_count, 7);
    assert!(config.enable_console);
    assert!(config.enable_file);
    assert_eq!(config.file_prefix, "opsbox");
    assert_eq!(config.log_dir, PathBuf::from("logs"));
  }

  #[test]
  fn test_log_config_serialization() {
    let config = LogConfig {
      level: LogLevel::Debug,
      log_dir: PathBuf::from("/var/log/test"),
      retention_count: 14,
      enable_console: true,
      enable_file: false,
      file_prefix: "test-app".to_string(),
    };

    let json = serde_json::to_string(&config).unwrap();
    let deserialized: LogConfig = serde_json::from_str(&json).unwrap();

    assert_eq!(deserialized.level, LogLevel::Debug);
    assert_eq!(deserialized.log_dir, PathBuf::from("/var/log/test"));
    assert_eq!(deserialized.retention_count, 14);
    assert!(deserialized.enable_console);
    assert!(!deserialized.enable_file);
    assert_eq!(deserialized.file_prefix, "test-app");
  }

  #[test]
  fn test_log_config_custom_values() {
    let config = LogConfig {
      level: LogLevel::Trace,
      log_dir: PathBuf::from("/custom/path"),
      retention_count: 30,
      enable_console: false,
      enable_file: true,
      file_prefix: "custom".to_string(),
    };

    assert_eq!(config.level, LogLevel::Trace);
    assert_eq!(config.log_dir, PathBuf::from("/custom/path"));
    assert_eq!(config.retention_count, 30);
    assert!(!config.enable_console);
    assert!(config.enable_file);
    assert_eq!(config.file_prefix, "custom");
  }

  #[tokio::test]
  async fn test_init_and_reload() {
    let temp_dir = tempfile::tempdir().unwrap();
    let config = LogConfig {
      level: LogLevel::Info,
      log_dir: temp_dir.path().to_path_buf(),
      enable_console: false,
      enable_file: true,
      file_prefix: "test-init".to_string(),
      ..Default::default()
    };

    let handle = init(config).expect("Init failed");
    handle.update_level(LogLevel::Debug).expect("Reload failed");
    assert!(temp_dir.path().exists());
  }
}
