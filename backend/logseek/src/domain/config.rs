// 源配置：用于描述 /search 路由要处理的来源
use serde::{Deserialize, Serialize};

/// 存储源配置
///
/// 用于从请求参数描述需要搜索的存储源
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum SourceConfig {
  /// 本地文件系统配置
  Local {
    /// 根目录路径
    path: String,
    /// 是否递归搜索
    #[serde(default = "default_true")]
    recursive: bool,
  },

  /// S3 配置(使用 profile 名称)
  S3 {
    /// Profile 名称
    profile: String,
    /// Bucket 名称 (用于 FileUrl 构造)
    #[serde(skip_serializing_if = "Option::is_none")]
    bucket: Option<String>,
    /// 路径前缀(可选) - 当 key 为 None 时使用
    #[serde(skip_serializing_if = "Option::is_none")]
    prefix: Option<String>,
    /// 路径过滤正则(可选) - 当 key 为 None 时使用
    #[serde(skip_serializing_if = "Option::is_none")]
    pattern: Option<String>,
    /// 特定对象键(可选) - 当指定时，只搜索该对象，忽略 prefix 和 pattern
    #[serde(skip_serializing_if = "Option::is_none")]
    key: Option<String>,
  },

  /// Agent 配置
  Agent {
    /// Agent 标识 ID（例如: "agent-localhost" 或 "192.168.1.10:8090"）
    agent_id: String,
    /// 作用域根目录（可选），例如 "logs"
    #[serde(skip_serializing_if = "Option::is_none")]
    scope_root: Option<String>,
    /// 额外路径过滤（glob，可选），例如 "**/2025-10-19/**"
    #[serde(skip_serializing_if = "Option::is_none")]
    path_filter_glob: Option<String>,
  },
}

fn default_true() -> bool {
  true
}
