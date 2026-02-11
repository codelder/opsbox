use opsbox_core::dfs::Resource;

// 子模块：Starlark 运行时和类型定义
mod starlark_runtime;
mod types;

pub use starlark_runtime::{plan_with_starlark, plan_with_starlark_with_script};
pub use types::DateRange;

/// 规划结果：来源配置 + 清理后的查询 + 调试日志
pub struct PlanResult {
  pub sources: Vec<Resource>,
  pub cleaned_query: String,
  pub debug_logs: Vec<String>,
}
