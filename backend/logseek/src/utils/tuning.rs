use once_cell::sync::OnceCell;
use std::sync::Arc;

/// 运行期可调参数（仅包含 S3 相关的关键项）
#[derive(Debug, Clone)]
pub struct Tuning {
  pub server_id: Option<String>,
  pub io_max_concurrency: usize,
  pub io_timeout_sec: u64,
  pub io_max_retries: u32,
}

static TUNING: OnceCell<Arc<Tuning>> = OnceCell::new();

/// 设置全局调参（仅第一次成功）
pub fn set(t: Tuning) -> bool {
  TUNING.set(Arc::new(t)).is_ok()
}

/// 获取只读调参（若未设置返回 None）
pub fn get() -> Option<Arc<Tuning>> {
  TUNING.get().cloned()
}
