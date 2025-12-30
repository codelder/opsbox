use serde::{Deserialize, Serialize};

/// 新的来源描述模型：拆分“端点/根路径”、“目标集合”、“过滤器”
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Source {
  pub endpoint: Endpoint,
  pub target: Target,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub filter_glob: Option<String>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub display_name: Option<String>,
}

/// 端点（在哪里查）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Endpoint {
  /// 本地：root为绝对路径
  Local { root: String },
  /// Agent：subpath为相对该Agent search_roots的子路径；"." 表示不限制
  Agent { agent_id: String, subpath: String },
  /// S3：选择配置与桶
  S3 { profile: String, bucket: String },
}

/// 目标集合（查什么），路径均相对 endpoint.root（Local）或 endpoint.subpath（Agent）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Target {
  /// 目录
  Dir {
    path: String,
    #[serde(default = "default_true")]
    recursive: bool,
  },
  /// 文件清单
  Files { paths: Vec<String> },
  /// 归档（自动探测 tar/tar.gz/gz/zip；zip 暂不支持）
  Archive {
    path: String,
    /// 归档内的条目路径（如果要读取归档内的特定文件）
    #[serde(skip_serializing_if = "Option::is_none")]
    entry: Option<String>,
  },
}

fn default_true() -> bool {
  true
}
