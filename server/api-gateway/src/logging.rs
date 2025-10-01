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

/// 初始化网络环境（代理设置等）
pub fn init_network_env() {
  // 打印并标准化代理相关环境变量
  let get = |k: &str| std::env::var(k).ok();
  let http_proxy = get("HTTP_PROXY").or_else(|| get("http_proxy"));
  let https_proxy = get("HTTPS_PROXY").or_else(|| get("https_proxy"));
  let no_proxy = get("NO_PROXY").or_else(|| get("no_proxy"));

  log::debug!(
    "代理环境: HTTP_PROXY={:?} HTTPS_PROXY={:?} NO_PROXY={:?}",
    http_proxy
      .as_deref()
      .unwrap_or("")
      .replace(|c: char| c.is_ascii_control(), ""),
    https_proxy
      .as_deref()
      .unwrap_or("")
      .replace(|c: char| c.is_ascii_control(), ""),
    no_proxy.as_deref().unwrap_or("")
  );

  // 当显式开启 LOGSEEK_AUTO_NO_PROXY，且 NO_PROXY 未设置时，自动填入内网与本地网段
  let auto = std::env::var("LOGSEEK_AUTO_NO_PROXY")
    .map(|v| v == "1" || v.eq_ignore_ascii_case("true") || v.eq_ignore_ascii_case("yes"))
    .unwrap_or(false);

  if auto && no_proxy.is_none() {
    // 常见内网与本地地址范围
    let defaults = "localhost,127.0.0.1,::1,10.0.0.0/8,172.16.0.0/12,192.168.0.0/16";
    unsafe {
      std::env::set_var("NO_PROXY", defaults);
      std::env::set_var("no_proxy", defaults);
    }
    log::warn!("NO_PROXY 未设置，已根据 LOGSEEK_AUTO_NO_PROXY 自动设为: {}", defaults);
  }

  // 如检测到空的 HTTP(S)_PROXY 值，主动移除
  let is_empty = |v: &Option<String>| v.as_ref().map(|s| s.trim().is_empty()).unwrap_or(false);
  if auto && (is_empty(&http_proxy) || is_empty(&https_proxy)) {
    unsafe {
      if is_empty(&http_proxy) {
        std::env::remove_var("HTTP_PROXY");
        std::env::remove_var("http_proxy");
      }
      if is_empty(&https_proxy) {
        std::env::remove_var("HTTPS_PROXY");
        std::env::remove_var("https_proxy");
      }
    }
    log::info!("检测到空代理环境变量，已移除空的 HTTP(S)_PROXY");
  }
}
