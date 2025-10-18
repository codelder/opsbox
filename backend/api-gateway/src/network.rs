//! 网络环境初始化模块
//!
//! 负责代理设置、环境变量处理等网络相关配置

/// 初始化网络环境（代理设置等）
pub fn init_network_env() {
  // 打印并标准化代理相关环境变量
  let get = |k: &str| std::env::var(k).ok();
  let http_proxy = get("HTTP_PROXY").or_else(|| get("http_proxy"));
  let https_proxy = get("HTTPS_PROXY").or_else(|| get("https_proxy"));
  let all_proxy = get("ALL_PROXY").or_else(|| get("all_proxy"));
  let no_proxy = get("NO_PROXY").or_else(|| get("no_proxy"));

  log::debug!(
    "代理环境: HTTP_PROXY={:?} HTTPS_PROXY={:?} ALL_PROXY={:?} NO_PROXY={:?}",
    http_proxy
      .as_deref()
      .unwrap_or("")
      .replace(|c: char| c.is_ascii_control(), ""),
    https_proxy
      .as_deref()
      .unwrap_or("")
      .replace(|c: char| c.is_ascii_control(), ""),
    all_proxy.as_deref().unwrap_or(""),
    no_proxy.as_deref().unwrap_or("")
  );

  // 如果 NO_PROXY 未设置，自动设置默认值（内网与本地地址范围）
  if no_proxy.is_none() {
    let defaults = "localhost,127.0.0.1,::1,10.0.0.0/8,172.16.0.0/12,192.168.0.0/16";
    unsafe {
      std::env::set_var("NO_PROXY", defaults);
      std::env::set_var("no_proxy", defaults);
    }
    log::info!("NO_PROXY 未设置，已自动设为: {}", defaults);
  }

  // 清除 all_proxy，避免其覆盖 NO_PROXY 设置
  if all_proxy.is_some() {
    unsafe {
      std::env::set_var("ALL_PROXY", "");
      std::env::set_var("all_proxy", "");
    }
    log::info!("检测到 all_proxy 设置，已清除以确保 NO_PROXY 生效");
  }

  // 如检测到空的 HTTP(S)_PROXY 值，主动移除
  let is_empty = |v: &Option<String>| v.as_ref().map(|s| s.trim().is_empty()).unwrap_or(false);
  if is_empty(&http_proxy) || is_empty(&https_proxy) {
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
