use crate::config::AppConfig;
use crate::server;
use opsbox_core::SqlitePool;
use opsbox_core::logging::{LogConfig, LogLevel, ReloadHandle, init as core_init};
use std::str::FromStr;

/// 初始化日志系统
///
/// 使用 opsbox-core 的 logging 模块初始化日志系统
/// 返回 ReloadHandle 用于动态修改日志级别
///
/// 注意：此函数在数据库初始化之前调用，只能从命令行参数确定日志级别
/// 数据库初始化后应调用 `setup_logging_config` 来同步数据库配置
pub fn init(config: &AppConfig) -> Result<ReloadHandle, opsbox_core::logging::LogError> {
  // 确定日志级别（仅从命令行参数，不涉及数据库）
  let level = determine_log_level_from_config(config);

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

/// 从配置确定日志级别（不涉及数据库）
///
/// 优先级：
/// 1. 命令行参数 (--log-level)
/// 2. verbose 参数 (-v, -vv)
/// 3. 默认值 (Info)
///
/// 此函数用于数据库初始化之前的场景（如 `init` 函数）
fn determine_log_level_from_config(config: &AppConfig) -> LogLevel {
  if let Some(ref level_str) = config.log_level {
    // 命令行参数指定了日志级别
    LogLevel::from_str(level_str).unwrap_or_else(|_| {
      tracing::warn!("无效的日志级别 '{}'，使用默认值 Info", level_str);
      LogLevel::Info
    })
  } else {
    // 根据 verbose 参数确定日志级别
    match config.verbose {
      0 => LogLevel::Info,
      1 => LogLevel::Debug,
      _ => LogLevel::Trace,
    }
  }
}

/// 设置日志配置（在数据库初始化后调用）
///
/// 处理日志级别的优先级：
/// 1. 命令行参数 (--log-level)
/// 2. verbose 参数 (-v, -vv)
/// 3. 数据库中的配置
///
/// 并将确定的日志级别同步到数据库和日志系统
pub async fn setup_logging_config(
  db_pool: &SqlitePool,
  config: &AppConfig,
) -> Result<LogLevel, opsbox_core::logging::LogError> {
  // 1. 初始化日志配置数据库
  opsbox_core::logging::run_migration(db_pool).await?;

  // 2. 创建仓库
  let repo = opsbox_core::logging::repository::LogConfigRepository::new(db_pool.clone());

  // 3. 确定实际使用的日志级别
  let actual_level = determine_log_level(config, &repo).await;

  // 4. 更新日志系统
  update_log_system(actual_level)?;

  Ok(actual_level)
}

/// 确定日志级别（优先级：命令行 > verbose > 数据库）
///
/// 此函数在数据库初始化之后调用，可以从数据库读取配置
async fn determine_log_level(
  config: &AppConfig,
  repo: &opsbox_core::logging::repository::LogConfigRepository,
) -> LogLevel {
  // 首先尝试从配置确定（命令行参数或 verbose）
  let config_level = determine_log_level_from_config(config);

  // 如果命令行参数或 verbose 指定了级别，同步到数据库
  if config.log_level.is_some() || config.verbose > 0 {
    // 命令行参数或 verbose 指定了级别，同步到数据库
    if let Err(e) = repo.update_level("server", config_level).await {
      tracing::warn!("同步日志级别到数据库失败: {}，继续使用配置的级别", e);
    } else {
      if config.log_level.is_some() {
        tracing::info!("已将命令行日志级别 '{}' 同步到数据库", config_level);
      } else {
        tracing::info!("已将 verbose 参数对应的日志级别 '{}' 同步到数据库", config_level);
      }
    }
    return config_level;
  }

  // verbose 为 0 且没有命令行参数，从数据库读取
  match repo.get("server").await {
    Ok(log_config) => {
      if let Ok(db_level) = LogLevel::from_str(&log_config.level) {
        tracing::info!("从数据库加载日志级别: {}", log_config.level);
        db_level
      } else {
        tracing::warn!("数据库中的日志级别 '{}' 无效，使用默认值", log_config.level);
        LogLevel::Info
      }
    }
    Err(e) => {
      tracing::debug!("从数据库加载日志配置失败（将使用默认值）: {}", e);
      LogLevel::Info
    }
  }
}

/// 更新日志系统
fn update_log_system(level: LogLevel) -> Result<(), opsbox_core::logging::LogError> {
  let reload_handle = server::get_log_reload_handle()
    .ok_or_else(|| opsbox_core::logging::LogError::InvalidConfig("日志重载句柄未初始化".to_string()))?;

  reload_handle
    .update_level(level)
    .map_err(|e| opsbox_core::logging::LogError::ReloadFailed(e.to_string()))?;

  tracing::info!("日志级别已设置为: {}", level);
  Ok(())
}
