// 领域层：核心业务模型和逻辑
// 当前 API 模型仍在 routes.rs 中，未来可以逐步提取到此处

pub mod config;
pub mod odfi_builder;
pub mod source_planner;

pub use config::{Endpoint, Source, Target};
pub use odfi_builder::{
  EntrySourceType, build_odfi_for_result, build_odfi_for_result_with_archive_path,
  build_odfi_for_result_with_source_type,
};
pub use opsbox_core::odfi::{EndpointType, Odfi, OdfiError, TargetType};
