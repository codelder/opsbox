use std::{collections::HashMap, sync::Arc};

use async_trait::async_trait;
use once_cell::sync::Lazy;
use opsbox_core::SqlitePool;

use crate::{api::models::AppError, domain::config::Source};

// 子模块：通用类型与具体存储源规划器实现
mod bbip;
mod starlark_runtime;
mod types;

pub use starlark_runtime::plan_with_starlark;
pub use types::DateRange;

/// 规划结果：来源配置 + 清理后的查询
pub struct PlanResult {
  pub sources: Vec<Source>,
  pub cleaned_query: String,
}

// 旧的 Rust 规划器机制仍保留，以便回退/测试；默认走 Starlark
#[async_trait]
pub trait SourcePlanner: Send + Sync {
  /// 应用标识（如 "bbip"）
  fn app_id(&self) -> &'static str;
  /// 基于查询与数据库，生成来源配置（可含日期分割、路径模板展开等）
  async fn plan(&self, pool: &SqlitePool, query: &str) -> Result<PlanResult, AppError>;
}

/// 规划器工厂（供 inventory 收集）
pub struct PlannerFactory {
  pub app_id: &'static str,
  pub create: fn() -> Arc<dyn SourcePlanner>,
}

inventory::collect!(PlannerFactory);

/// 内部注册表（首用时从 inventory 构建）
fn registry() -> &'static HashMap<&'static str, Arc<dyn SourcePlanner>> {
  static REG: Lazy<HashMap<&'static str, Arc<dyn SourcePlanner>>> = Lazy::new(|| {
    let mut m: HashMap<&'static str, Arc<dyn SourcePlanner>> = HashMap::new();
    for f in inventory::iter::<PlannerFactory>() {
      m.insert(f.app_id, (f.create)());
    }
    m
  });
  &REG
}

/// 获取默认存储源规划器（当前为 bbip）
pub fn default_planner() -> Arc<dyn SourcePlanner> {
  registry().get("bbip").cloned().expect("默认 bbip 规划器未注册")
}

/// 获取指定业务系统的存储源规划器
#[allow(dead_code)]
pub fn planner_by_app(app: &str) -> Option<Arc<dyn SourcePlanner>> {
  registry().get(app).cloned()
}

/// 便捷注册宏：在各 planner 模块中调用以完成注册
#[macro_export]
macro_rules! register_planner {
  ($app_id:expr, $ctor:expr) => {
    inventory::submit! {
      $crate::domain::source_planner::PlannerFactory {
        app_id: $app_id,
        create: || -> std::sync::Arc<dyn $crate::domain::source_planner::SourcePlanner> { std::sync::Arc::new($ctor) },
      }
    }
  };
}
