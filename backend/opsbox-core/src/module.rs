use async_trait::async_trait;
use axum::Router;
use sqlx::SqlitePool;
use std::sync::Arc;

/// 模块接口
#[async_trait]
pub trait Module: Send + Sync {
  /// 模块名称
  fn name(&self) -> &'static str;

  /// API 路由前缀（如 "/api/v1/logseek"）
  fn api_prefix(&self) -> &'static str;

  /// 配置模块（可选）
  ///
  /// 在模块初始化前调用，允许模块从环境变量中读取配置
  /// 默认实现为空操作
  fn configure(&self) {
    // 默认不需要配置
  }

  /// 初始化数据库模式
  async fn init_schema(&self, pool: &SqlitePool) -> Result<(), Box<dyn std::error::Error>>;

  /// 创建路由
  fn router(&self, pool: SqlitePool) -> Router;

  /// 清理资源（可选）
  fn cleanup(&self) {}
}

/// 模块工厂包装器（必须是一个新类型，才能被 inventory 收集）
pub struct ModuleFactory {
  pub create: fn() -> Arc<dyn Module>,
}

impl ModuleFactory {
  pub const fn new(create: fn() -> Arc<dyn Module>) -> Self {
    Self { create }
  }
}

// 使用 inventory 收集所有注册的模块工厂
inventory::collect!(ModuleFactory);

/// 获取所有已注册的模块
pub fn get_all_modules() -> Vec<Arc<dyn Module>> {
  inventory::iter::<ModuleFactory>()
    .map(|factory| (factory.create)())
    .collect()
}

/// 模块注册宏（简化注册流程）
#[macro_export]
macro_rules! register_module {
  ($module_type:ty) => {
    inventory::submit! {
        $crate::module::ModuleFactory::new(|| -> std::sync::Arc<dyn $crate::Module> {
            std::sync::Arc::new(<$module_type>::default())
        })
    }
  };
}
