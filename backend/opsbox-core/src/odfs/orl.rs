use std::borrow::Cow;
use std::fmt;
use std::str::FromStr;

use fluent_uri::Uri;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use thiserror::Error;

/// 资源端点类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EndpointType {
  Local,
  Agent,
  S3,
}

impl FromStr for EndpointType {
  type Err = OrlError;

  fn from_str(s: &str) -> Result<Self, Self::Err> {
    match s {
      "local" => Ok(EndpointType::Local),
      "agent" => Ok(EndpointType::Agent),
      "s3" => Ok(EndpointType::S3),
      _ => Err(OrlError::InvalidEndpointType(s.to_string())),
    }
  }
}

impl fmt::Display for EndpointType {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    match self {
      EndpointType::Local => write!(f, "local"),
      EndpointType::Agent => write!(f, "agent"),
      EndpointType::S3 => write!(f, "s3"),
    }
  }
}

/// 目标资源类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TargetType {
  /// 普通文件或目录
  Dir,
  /// 归档文件内的条目
  Archive,
}

/// OpsBox 资源定位符 (ORL)
///
/// 轻量级封装：底层维护单一的符合 RFC 3986 规范的 URI 字符串 (scheme 固定为 `orl`)。
/// 所有的属性通过 On-demand parsing 获取，不再维护冗余的 struct 字段。
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct ORL(String);

impl fmt::Debug for ORL {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.debug_tuple("ORL").field(&self.0).finish()
  }
}

#[derive(Debug, Error)]
pub enum OrlError {
  #[error("Invalid URL format: {0}")]
  InvalidFormat(String),
  #[error("Unsupported scheme: {0}")]
  UnsupportedScheme(String),
  #[error("Invalid endpoint type: {0}")]
  InvalidEndpointType(String),
  #[error("Missing authority/host")]
  MissingAuthority,
}

impl ORL {
  /// 从字符串解析并校验
  pub fn parse(s: impl Into<String>) -> Result<Self, OrlError> {
    let s = s.into();

    // 1. 基本 URI 格式校验
    match Uri::parse(s.as_str()) {
      Ok(uri) => {
        // 2. Scheme 校验
        if uri.scheme().as_str() != "orl" {
          return Err(OrlError::UnsupportedScheme(
            uri.scheme().as_str().to_string(),
          ));
        }
        // 3. Authority 校验 (必须存在)
        if uri.authority().is_none() {
          return Err(OrlError::MissingAuthority);
        }
      }
      Err(e) => return Err(OrlError::InvalidFormat(e.to_string())),
    }

    Ok(Self(s))
  }

  /// 获取原始 URI 字符串
  pub fn as_str(&self) -> &str {
    &self.0
  }

  /// 获取 fluent_uri::Uri 视图（内部使用，unwrap 保证 safe，因为 parse 时已校验）
  pub fn uri(&self) -> Uri<&str> {
    Uri::parse(self.0.as_str()).expect("ORL internal string should be valid URI")
  }

  // --- Accessors ---

  /// 获取端点类型 (local/agent/s3)
  /// 解析 host 的第一部分，如 `agent.web-01` -> `agent`
  pub fn endpoint_type(&self) -> Result<EndpointType, OrlError> {
    let auth = self.uri().authority().ok_or(OrlError::MissingAuthority)?;
    let host = auth.host();

    // 简单策略：Host 即 Type (针对 `orl://local` 或 `orl://agent`)
    // 或者 Host 是 `type.addr` (针对 `orl://agent.10.0.1.5`)
    let type_str = host.split('.').next().unwrap_or(host);

    EndpointType::from_str(type_str)
  }

  /// 获取端点 ID (AgentID / ProfileName)
  /// 对应 userinfo 部分
  pub fn endpoint_id(&self) -> Option<&str> {
    self.uri().authority()?.userinfo().map(|u| u.as_str())
  }

  /// 对于 S3，ProfileName 就在 id 里
  /// 对于 Local，如果 id 为空则意味着 localhost
  /// 对于 Agent，如果 id 为空则意味着 root
  pub fn effective_id(&self) -> Cow<'_, str> {
    match self.endpoint_id() {
      Some(id) => Cow::Borrowed(id),
      None => {
        // 根据端点类型决定默认ID
        if let Ok(endpoint_type) = self.endpoint_type() {
          match endpoint_type {
            EndpointType::Agent => Cow::Borrowed("root"),
            _ => Cow::Borrowed("localhost"),
          }
        } else {
          Cow::Borrowed("localhost")
        }
      }
    }
  }

  /// 获取资源路径
  pub fn path(&self) -> &str {
    self.uri().path().as_str()
  }

  /// 获取完整路径 (含 Bucket 处理逻辑)
  /// S3: `orl://profile@s3/bucket/path` -> Bucket="bucket", Key="path"
  /// 这部分高层逻辑是否要下沉到 ORL 还有待商榷，目前先提供基础 Path
  pub fn path_decoded(&self) -> Cow<'_, str> {
    self.uri().path().as_str().into()
  }

  /// 获取查询参数 `entry` (归档内部路径)
  pub fn entry_path(&self) -> Option<Cow<'_, str>> {
    self.query_param("entry")
  }

  /// 获取查询参数 `glob` (过滤通配符)
  pub fn filter_glob(&self) -> Option<Cow<'_, str>> {
    self.query_param("glob")
  }

  /// 辅助：获取 Query 参数
  fn query_param(&self, key: &str) -> Option<Cow<'_, str>> {
    let uri = self.uri();
    let query = uri.query()?;
    // fluent-uri 暂时没提供便捷的 query pair iterator，手动解析
    for pair in query.as_str().split('&') {
      let mut parts = pair.splitn(2, '=');
      if let Some(k) = parts.next()
        && k == key
      {
          let v = parts.next().unwrap_or("");
          // 可以在这里做 URL decode
          return Some(Cow::Borrowed(v));
      }
    }
    None
  }

  /// 判断目标类型
  pub fn target_type(&self) -> TargetType {
    let path = self.path();
    if self.entry_path().is_some()
      || path.ends_with(".tar")
      || path.ends_with(".tar.gz")
      || path.ends_with(".tgz")
      || path.ends_with(".zip")
    {
      TargetType::Archive
    } else {
      TargetType::Dir
    }
  }

  /// 获取显示名称
  pub fn display_name(&self) -> String {
    if let Some(entry) = self.entry_path() {
      entry.split('/').next_back().unwrap_or(&entry).to_string()
    } else {
      let p = self.path();
      p.split('/').next_back().unwrap_or(p).to_string()
    }
  }

  /// Builder 模式：修改 Path
  /// 注意：这对 String wrapper 来说开销较大，因为需要重建字符串
  pub fn join(&self, subpath: &str) -> Result<Self, OrlError> {
    let mut s = self.0.clone();
    // 粗暴简单的实现，实际可能需要更健壮的 path join
    if !s.contains('?') && !s.contains('#') {
      let sep = if s.ends_with('/') { "" } else { "/" };
      s.push_str(sep);
      s.push_str(subpath.trim_start_matches('/'));
      return Self::parse(s);
    }
    // 暂时如果带 query 就不支持简单的 join，或者需要更复杂的重组逻辑
    // 实际生产建议引入 url crate 的 builder 功能进行辅助构造，或者 fluent-uri 的 builder（如果有）
    // 这里为简化暂略
    Err(OrlError::InvalidFormat("Cannot join path on complex URI currently".into()))
  }
}

// --- Trait Impls ---

impl fmt::Display for ORL {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(f, "{}", self.0)
  }
}

impl FromStr for ORL {
  type Err = OrlError;

  fn from_str(s: &str) -> Result<Self, Self::Err> {
    Self::parse(s)
  }
}

impl Serialize for ORL {
  fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
  where
    S: Serializer,
  {
    serializer.serialize_str(&self.0)
  }
}

impl<'de> Deserialize<'de> for ORL {
  fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
  where
    D: Deserializer<'de>,
  {
    let s = String::deserialize(deserializer)?;
    Self::parse(s).map_err(serde::de::Error::custom)
  }
}

/// 抽象路径封装
///
/// `OpsPath` 始终表示**当前文件系统层级**下的相对路径。
///
/// 在归档处理（Archive Overlay）场景中，它通过递归分层工作：
/// 假设访问 `orl://local/data.zip?entry=inner/file.log`
///
/// 1. **第一层 (底层 FS)**:
///    Router 看到宿主文件 `data.zip`，调用底层 `open_read(OpsPath("data.zip"))`。
///
/// 2. **第二层 (Overlay FS)**:
///    Router 识别出 ZIP 格式，挂载 `ZipOpsFS`。
///    `ZipOpsFS` 接收到的路径是归档内的相对路径：`OpsPath("inner/file.log")`。
///    它不需要关心 `data.zip` 这个前缀，因为它的根就是这个归档文件。
///
/// 保持为 String wrapper，方便 Trait 使用
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpsPath(String);

impl OpsPath {
  pub fn new(path: impl Into<String>) -> Self {
    Self(path.into())
  }

  pub fn as_str(&self) -> &str {
    &self.0
  }

  pub fn join(&self, other: &str) -> Self {
    let other = other.trim_start_matches('/');
    if self.0.ends_with('/') {
      Self(format!("{}{}", self.0, other))
    } else {
      Self(format!("{}/{}", self.0, other))
    }
  }
}

impl AsRef<str> for OpsPath {
  fn as_ref(&self) -> &str {
    &self.0
  }
}

impl fmt::Display for OpsPath {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(f, "{}", self.0)
  }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_endpoint_type_from_str() {
        assert!(matches!(EndpointType::from_str("local"), Ok(EndpointType::Local)));
        assert!(matches!(EndpointType::from_str("agent"), Ok(EndpointType::Agent)));
        assert!(matches!(EndpointType::from_str("s3"), Ok(EndpointType::S3)));
        assert!(EndpointType::from_str("invalid").is_err());
    }

    #[test]
    fn test_endpoint_type_display() {
        assert_eq!(EndpointType::Local.to_string(), "local");
        assert_eq!(EndpointType::Agent.to_string(), "agent");
        assert_eq!(EndpointType::S3.to_string(), "s3");
    }

    #[test]
    fn test_orl_parse() {
        let orl = ORL::parse("orl://local/var/log").unwrap();
        assert_eq!(orl.as_str(), "orl://local/var/log");

        // Invalid scheme
        assert!(ORL::parse("http://local/path").is_err());
    }

    #[test]
    fn test_orl_endpoint_type() {
        let orl = ORL::parse("orl://agent.web-01/path").unwrap();
        assert_eq!(orl.endpoint_type().unwrap(), EndpointType::Agent);

        let orl = ORL::parse("orl://local/path").unwrap();
        assert_eq!(orl.endpoint_type().unwrap(), EndpointType::Local);
    }

    #[test]
    fn test_orl_endpoint_id() {
        let orl = ORL::parse("orl://user@agent/path").unwrap();
        assert_eq!(orl.endpoint_id(), Some("user"));

        let orl = ORL::parse("orl://local/path").unwrap();
        assert_eq!(orl.endpoint_id(), None);
        assert_eq!(orl.effective_id(), "localhost");

        // Test agent endpoint without ID
        let orl = ORL::parse("orl://agent/path").unwrap();
        assert_eq!(orl.endpoint_id(), None);
        assert_eq!(orl.effective_id(), "root");

        // Test agent endpoint with hostname
        let orl = ORL::parse("orl://agent.web-01/path").unwrap();
        assert_eq!(orl.endpoint_id(), None);
        assert_eq!(orl.effective_id(), "root"); // Still root because no userinfo
    }

    #[test]
    fn test_orl_path() {
        let orl = ORL::parse("orl://local/var/log/app.log").unwrap();
        assert_eq!(orl.path(), "/var/log/app.log");
    }

    #[test]
    fn test_orl_entry_path() {
        let orl = ORL::parse("orl://local/archive.tar?entry=inner/file.log").unwrap();
        assert_eq!(orl.entry_path(), Some(Cow::Borrowed("inner/file.log")));

        let orl = ORL::parse("orl://local/file.log").unwrap();
        assert_eq!(orl.entry_path(), None);
    }

    #[test]
    fn test_orl_target_type() {
        let orl = ORL::parse("orl://local/file.tar").unwrap();
        assert_eq!(orl.target_type(), TargetType::Archive);

        let orl = ORL::parse("orl://local/file.log").unwrap();
        assert_eq!(orl.target_type(), TargetType::Dir);

        let orl = ORL::parse("orl://local/file.log?entry=inner").unwrap();
        assert_eq!(orl.target_type(), TargetType::Archive);
    }

    #[test]
    fn test_orl_display_name() {
        let orl = ORL::parse("orl://local/var/log/app.log").unwrap();
        assert_eq!(orl.display_name(), "app.log");

        let orl = ORL::parse("orl://local/archive.tar?entry=inner/file.log").unwrap();
        assert_eq!(orl.display_name(), "file.log");
    }

    #[test]
    fn test_orl_join() {
        let orl = ORL::parse("orl://local/var/log").unwrap();
        let joined = orl.join("app.log").unwrap();
        assert_eq!(joined.path(), "/var/log/app.log");

        // With trailing slash
        let orl = ORL::parse("orl://local/var/log/").unwrap();
        let joined = orl.join("app.log").unwrap();
        assert_eq!(joined.path(), "/var/log/app.log");
    }

    #[test]
    fn test_orl_serialization() {
        let orl = ORL::parse("orl://local/path").unwrap();
        let json = serde_json::to_string(&orl).unwrap();
        assert_eq!(json, "\"orl://local/path\"");

        let deserialized: ORL = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, orl);
    }

    #[test]
    fn test_ops_path() {
        let path = OpsPath::new("/var/log");
        assert_eq!(path.as_str(), "/var/log");
        assert_eq!(path.to_string(), "/var/log");

        let joined = path.join("app.log");
        assert_eq!(joined.as_str(), "/var/log/app.log");

        // With trailing slash
        let path = OpsPath::new("/var/log/");
        let joined = path.join("/app.log");
        assert_eq!(joined.as_str(), "/var/log/app.log");
    }
}
