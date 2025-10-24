use std::{collections::HashMap, sync::Arc};

use async_trait::async_trait;
use once_cell::sync::Lazy;
use opsbox_core::SqlitePool;

use crate::{api::models::AppError, domain::config::SourceConfig};

// 子模块：通用类型与具体存储源规划器实现
mod bbip;
mod types;

pub use types::DateRange;

/// 规划结果：来源配置 + 清理后的查询
pub struct PlanResult {
  pub sources: Vec<SourceConfig>,
  pub cleaned_query: String,
}

#[async_trait]
pub trait SourcePlanner: Send + Sync {
  /// 应用标识（如 "bbip"）
  fn app_id(&self) -> &'static str;
  /// 基于查询与数据库，生成来源配置（可含日期分割、路径模板展开等）
  async fn plan(&self, pool: &SqlitePool, query: &str) -> Result<PlanResult, AppError>;
}

/// 注册表：按业务系统标识选择存储源规划器
pub static SOURCE_PLANNERS: Lazy<HashMap<&'static str, Arc<dyn SourcePlanner>>> = Lazy::new(|| {
  let mut m: HashMap<&'static str, Arc<dyn SourcePlanner>> = HashMap::new();
  m.insert("bbip", Arc::new(bbip::BbipPlanner));
  m
});

/// 获取默认存储源规划器（当前为 bbip）
pub fn default_planner() -> Arc<dyn SourcePlanner> {
  SOURCE_PLANNERS.get("bbip").cloned().expect("默认 bbip 规划器未注册")
}

/// 获取指定业务系统的存储源规划器
#[allow(dead_code)]
pub fn planner_by_app(app: &str) -> Option<Arc<dyn SourcePlanner>> {
  SOURCE_PLANNERS.get(app).cloned()
}
