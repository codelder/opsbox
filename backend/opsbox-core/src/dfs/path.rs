//! Path 模块 - 资源路径概念
//!
//! 定义了 ResourcePath，表示端点内的路径

/// 资源路径
///
/// 表示端点内部的资源位置，与具体端点解耦
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ResourcePath {
  /// 路径片段
  segments: Vec<String>,
  /// 是否为绝对路径
  is_absolute: bool,
}

impl ResourcePath {
  /// 创建新的 ResourcePath
  pub fn new(segments: Vec<String>, is_absolute: bool) -> Self {
    Self { segments, is_absolute }
  }

  /// 从字符串解析
  pub fn parse(s: &str) -> Self {
    let is_absolute = s.starts_with('/');
    let segments = s
      .trim_start_matches('/')
      .split('/')
      .filter(|s| !s.is_empty())
      .map(String::from)
      .collect();
    Self { segments, is_absolute }
  }

  /// 连接两个路径
  pub fn join(&self, other: &ResourcePath) -> Self {
    let mut segments = self.segments.clone();
    segments.extend(other.segments.iter().cloned());
    Self {
      segments,
      is_absolute: self.is_absolute,
    }
  }

  /// 获取路径片段
  pub fn segments(&self) -> &[String] {
    &self.segments
  }

  /// 是否为绝对路径
  pub fn is_absolute(&self) -> bool {
    self.is_absolute
  }

  /// 获取路径字符串表示 (内部辅助)
  fn render_string(&self) -> String {
    let path = self.segments.join("/");
    if self.is_absolute { format!("/{path}") } else { path }
  }
}

impl std::str::FromStr for ResourcePath {
  type Err = std::convert::Infallible;

  fn from_str(s: &str) -> Result<Self, Self::Err> {
    Ok(Self::parse(s))
  }
}

impl From<&str> for ResourcePath {
  fn from(s: &str) -> Self {
    Self::parse(s)
  }
}

impl From<String> for ResourcePath {
  fn from(s: String) -> Self {
    Self::parse(&s)
  }
}

impl std::fmt::Display for ResourcePath {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    write!(f, "{}", self.render_string())
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_from_str_absolute() {
    let path = ResourcePath::parse("/var/log/app.log");
    assert!(path.is_absolute);
    assert_eq!(path.segments, vec!["var", "log", "app.log"]);
  }

  #[test]
  fn test_from_str_relative() {
    let path = ResourcePath::parse("logs/app.log");
    assert!(!path.is_absolute);
    assert_eq!(path.segments, vec!["logs", "app.log"]);
  }

  #[test]
  fn test_from_str_empty() {
    let path = ResourcePath::parse("/");
    assert!(path.is_absolute);
    assert!(path.segments.is_empty());
  }

  #[test]
  fn test_join() {
    let base = ResourcePath::parse("/var");
    let rel = ResourcePath::parse("log/app.log");
    let joined = base.join(&rel);
    assert!(joined.is_absolute);
    assert_eq!(joined.segments, vec!["var", "log", "app.log"]);
  }

  #[test]
  fn test_to_string() {
    let path = ResourcePath::parse("/var/log/app.log");
    assert_eq!(path.to_string(), "/var/log/app.log");

    let path2 = ResourcePath::parse("logs/app.log");
    assert_eq!(path2.to_string(), "logs/app.log");
  }

  #[test]
  fn test_from_string() {
    let path: ResourcePath = String::from("/data/test.txt").into();
    assert_eq!(path.to_string(), "/data/test.txt");
  }
}
