// 领域层：核心业务模型和逻辑
// 当前 API 模型仍在 routes.rs 中，未来可以逐步提取到此处

pub mod file_url;

pub use file_url::{FileUrl, FileUrlError, TarCompression};
