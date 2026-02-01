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

  tracing::debug!(
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
    tracing::info!("NO_PROXY 未设置，已自动设为: {}", defaults);
  }

  // 将 ALL_PROXY 转换为 HTTP_PROXY/HTTPS_PROXY，避免其覆盖 NO_PROXY 设置
  if let Some(proxy_value) = all_proxy.as_deref() {
    let proxy_value = proxy_value.to_string();
    unsafe {
      // 只在未设置协议特定代理时才设置
      if http_proxy.is_none() {
        std::env::set_var("HTTP_PROXY", &proxy_value);
        std::env::set_var("http_proxy", &proxy_value);
      }
      if https_proxy.is_none() {
        std::env::set_var("HTTPS_PROXY", &proxy_value);
        std::env::set_var("https_proxy", &proxy_value);
      }
      // 移除 ALL_PROXY，使用更明确的协议代理配合 NO_PROXY
      std::env::remove_var("ALL_PROXY");
      std::env::remove_var("all_proxy");
    }
    tracing::info!(
      "检测到 ALL_PROXY={}，已转换为 HTTP_PROXY/HTTPS_PROXY 以确保 NO_PROXY 生效",
      proxy_value
    );
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
    tracing::info!("检测到空代理环境变量，已移除空的 HTTP(S)_PROXY");
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use std::env;
  use std::sync::LazyLock;
  use std::sync::Mutex;

  static ENV_MUTEX: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

  #[test]
  fn test_init_network_env_defaults() {
    let _guard = ENV_MUTEX.lock().unwrap();
    // 清理环境变量
    // SAFETY: ENV_MUTEX 保证测试串行运行，无并发修改环境变量的风险。
    unsafe {
      env::remove_var("HTTP_PROXY");
      env::remove_var("http_proxy");
      env::remove_var("HTTPS_PROXY");
      env::remove_var("https_proxy");
      env::remove_var("ALL_PROXY");
      env::remove_var("all_proxy");
      env::remove_var("NO_PROXY");
      env::remove_var("no_proxy");
    }

    init_network_env();

    // 验证默认 NO_PROXY 是否被设置
    assert!(env::var("NO_PROXY").is_ok());
    assert!(env::var("NO_PROXY").unwrap().contains("localhost"));
  }

  #[test]
  fn test_init_network_env_all_proxy_conversion() {
    let _guard = ENV_MUTEX.lock().unwrap();
    // SAFETY: ENV_MUTEX 保证测试串行运行，无并发修改环境变量的风险。
    unsafe {
      env::remove_var("HTTP_PROXY");
      env::remove_var("http_proxy");
      env::remove_var("ALL_PROXY");
      env::set_var("all_proxy", "http://proxy:8080");
    }

    init_network_env();

    // 验证 all_proxy (小写) 也能被正确读取并转换为大写 HTTP_PROXY
    assert_eq!(
      env::var("HTTP_PROXY").expect("HTTP_PROXY should be set"),
      "http://proxy:8080"
    );
    assert!(env::var("ALL_PROXY").is_err());
    assert!(env::var("all_proxy").is_err());
  }

  #[test]
  fn test_init_network_env_empty_proxy_cleanup() {
    let _guard = ENV_MUTEX.lock().unwrap();
    // SAFETY: ENV_MUTEX 保证测试串行运行，无并发修改环境变量的风险。
    unsafe {
      env::set_var("HTTP_PROXY", " ");
    }

    init_network_env();

    // 验证空代理被移除
    assert!(env::var("HTTP_PROXY").is_err());
  }
}
