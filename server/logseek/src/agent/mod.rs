// Agent 别名模块：对外暴露与远程 Agent 搜索相关的类型
// 便于在仅保留 Agent 能力的场景下使用更贴切的命名空间

pub use crate::storage::{
  agent::AgentClient,
  SearchOptions,
  SearchScope,
  SearchService,
  SearchProgress,
  SearchResultStream,
  ServiceCapabilities,
  StorageError,
};
