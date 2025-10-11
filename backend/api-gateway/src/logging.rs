use crate::config::AppConfig;
use log::LevelFilter;

/// 从字符串解析日志级别
fn level_from_str(s: &str) -> Option<LevelFilter> {
  match s.to_ascii_lowercase().as_str() {
    "error" => Some(LevelFilter::Error),
    "warn" | "warning" => Some(LevelFilter::Warn),
    "info" => Some(LevelFilter::Info),
    "debug" => Some(LevelFilter::Debug),
    "trace" => Some(LevelFilter::Trace),
    _ => None,
  }
}

/// 从 -V 参数计数转换为日志级别
fn verbosity_to_level(v: u8) -> LevelFilter {
  match v {
    0 => LevelFilter::Info,
    1 => LevelFilter::Debug,
    _ => LevelFilter::Trace,
  }
}

/// 根据配置选择最终的日志级别
fn choose_level(config: &AppConfig) -> LevelFilter {
  let mut level = config
    .log_level
    .as_deref()
    .and_then(level_from_str)
    .unwrap_or(LevelFilter::Info);

  let vlevel = verbosity_to_level(config.verbose);
  if vlevel > level {
    level = vlevel;
  }
  level
}

/// 初始化日志系统
pub fn init(config: &AppConfig) {
  // 若用户设置了 RUST_LOG，则尊重该环境变量；否则使用我们计算出的 level
  let mut builder = if std::env::var("RUST_LOG").is_ok() {
    env_logger::Builder::from_env(env_logger::Env::default())
  } else {
    let mut b = env_logger::Builder::new();
    b.filter_level(choose_level(config));
    b
  };

  // 初始化（忽略二次初始化错误）
  let _ = builder.try_init();

  log::info!("日志系统初始化完成，级别: {:?}", choose_level(config));
}
