use once_cell::sync::OnceCell;
use std::sync::Arc;

/// 运行期可调参数（由上层网关注入；优先级：命令行 > 环境变量 > 默认值）
#[derive(Debug, Clone)]
pub struct Tuning {
  pub s3_max_concurrency: usize,
  pub cpu_concurrency: usize,
  pub stream_ch_cap: usize,
  pub s3_timeout_sec: u64,
  pub s3_max_retries: u32,
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
