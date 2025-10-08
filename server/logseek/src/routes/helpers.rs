//! 路由辅助函数
//! 
//! 提供路由共享的配置和工具函数

/// 获取流式响应通道容量
///
/// 简化策略：固定容量，避免不必要的调参与链路复杂度
pub fn stream_channel_capacity() -> usize {
  256usize
}

/// 读取 S3 IO 并发上限（限制同时打开/读取的对象数）
pub fn s3_max_concurrency() -> usize {
  if let Some(t) = crate::utils::tuning::get() {
    return t.s3_max_concurrency.clamp(1, 128);
  }
  std::env::var("LOGSEEK_S3_MAX_CONCURRENCY")
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
