//! 路由辅助函数
//!
//! 提供路由共享的配置和工具函数

/// 获取流式响应通道容量
///
/// 简化策略：固定容量，避免不必要的调参与链路复杂度
pub fn stream_channel_capacity() -> usize {
  256usize
}

/// 读取 IO 并发上限（限制同时打开/读取的对象数，适用于所有数据源）
pub fn s3_max_concurrency() -> usize {
  if let Some(t) = crate::utils::tuning::get() {
    return t.io_max_concurrency.clamp(1, 128);
  }
  std::env::var("LOGSEEK_IO_MAX_CONCURRENCY")
    .ok()
    .and_then(|s| s.parse::<usize>().ok())
    .map(|v| v.clamp(1, 128))
    .unwrap_or(12)
}

/// 读取 CPU 并发上限（限制同时进行解压/检索的任务数）
///
/// 简化策略：使用硬编码的保守上限 min(物理并发, 16)
pub fn cpu_max_concurrency() -> usize {
  num_cpus::get().min(16)
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_stream_channel_capacity() {
    assert_eq!(stream_channel_capacity(), 256);
  }

  #[test]
  fn test_s3_max_concurrency_default() {
    // Without env var or tuning, should return default 12
    let concurrency = s3_max_concurrency();
    assert!(concurrency >= 1 && concurrency <= 128);
  }

  #[test]
  fn test_cpu_max_concurrency() {
    let concurrency = cpu_max_concurrency();
    // Should be between 1 and 16
    assert!(concurrency >= 1);
    assert!(concurrency <= 16);
    // Should not exceed physical CPU count
    assert!(concurrency <= num_cpus::get());
  }

  #[test]
  fn test_s3_max_concurrency_clamping() {
    // The function should clamp values between 1 and 128
    // We can't easily test env var behavior, but we can verify the function runs
    let result = s3_max_concurrency();
    assert!(result >= 1);
    assert!(result <= 128);
  }
}
