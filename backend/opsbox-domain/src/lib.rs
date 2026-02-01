//! # OpsBox Domain Models
//!
//! 纯领域模型层，无外部依赖，遵循 DDD 原则。
//!
//! ## 三个界限上下文
//!
//! - **Resource Context**: 资源访问抽象
//! - **Search Context**: 搜索执行抽象
//! - **Agent Context**: 代理管理抽象
//!
//! ## 设计原则
//!
//! - 类型安全：使用值对象替代字符串
//! - 聚合根封装行为：业务逻辑在实体内部
//! - 无基础设施依赖：领域层不依赖具体实现

// Resource 上下文
pub mod resource;

// Search 上下文
pub mod search;

// Agent 上下文
pub mod agent;
