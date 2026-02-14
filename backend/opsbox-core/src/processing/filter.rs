//! Path Filter - 路径过滤器
//!
//! 提供基于 glob 模式和字符串包含的路径过滤功能。

/// 路径过滤器
///
/// 支持两种过滤方式：
/// 1. Glob 模式匹配（include/exclude）
/// 2. 字符串包含判断（include_contains/exclude_contains）
#[derive(Clone, Default)]
pub struct PathFilter {
  /// 包含 glob 模式
  pub include: Option<globset::GlobSet>,
  /// 排除 glob 模式
  pub exclude: Option<globset::GlobSet>,
  /// 包含字符串（路径必须包含其中之一）
  pub include_contains: Vec<String>,
  /// 排除字符串（路径包含其中之一则排除）
  pub exclude_contains: Vec<String>,
}

impl PathFilter {
  /// 检查路径是否被允许
  ///
  /// 过滤顺序：
  /// 1. 检查排除 glob
  /// 2. 检查排除 contains
  /// 3. 检查包含 glob
  /// 4. 检查包含 contains
  pub fn is_allowed(&self, path: &str) -> bool {
    // 1. 检查排除 glob
    if let Some(ref exclude) = self.exclude
      && exclude.is_match(path)
    {
      return false;
    }
    // 2. 检查排除 contains
    if self.exclude_contains.iter().any(|s| path.contains(s)) {
      return false;
    }
    // 3. 检查包含 glob
    if let Some(ref include) = self.include
      && !include.is_match(path)
    {
      return false;
    }
    // 4. 检查包含 contains
    if !self.include_contains.is_empty() && !self.include_contains.iter().any(|s| path.contains(s)) {
      return false;
    }
    true
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_path_filter_glob_only() {
    let mut builder = globset::GlobSetBuilder::new();
    builder.add(globset::Glob::new("*.log").unwrap());
    let include = builder.build().unwrap();

    let filter = PathFilter {
      include: Some(include),
      exclude: None,
      include_contains: Vec::new(),
      exclude_contains: Vec::new(),
    };

    assert!(filter.is_allowed("test.log"));
    assert!(filter.is_allowed("path/to/test.log"));
    assert!(!filter.is_allowed("test.txt"));
  }

  #[test]
  fn test_path_filter_with_exclude() {
    let mut include_builder = globset::GlobSetBuilder::new();
    include_builder.add(globset::Glob::new("*").unwrap());
    let include = include_builder.build().unwrap();

    let mut exclude_builder = globset::GlobSetBuilder::new();
    exclude_builder.add(globset::Glob::new("*.tmp").unwrap());
    let exclude = exclude_builder.build().unwrap();

    let filter = PathFilter {
      include: Some(include),
      exclude: Some(exclude),
      include_contains: Vec::new(),
      exclude_contains: Vec::new(),
    };

    assert!(filter.is_allowed("test.log"));
    assert!(!filter.is_allowed("test.tmp"));
  }

  #[test]
  fn test_path_filter_contains() {
    let mut filter = PathFilter::default();

    // No filters - allow all
    assert!(filter.is_allowed("any/path"));

    // Exclude contains
    filter.exclude_contains.push("node_modules".to_string());
    assert!(!filter.is_allowed("src/node_modules/lib.js"));
    assert!(filter.is_allowed("src/lib.js"));

    // Include contains
    filter = PathFilter::default();
    filter.include_contains.push("src".to_string());
    assert!(filter.is_allowed("src/main.rs"));
    assert!(!filter.is_allowed("tests/main.rs"));
  }

  #[test]
  fn test_path_filter_combined() {
    let mut include_builder = globset::GlobSetBuilder::new();
    include_builder.add(globset::Glob::new("*.rs").unwrap());
    let include = include_builder.build().unwrap();

    let filter = PathFilter {
      include: Some(include),
      exclude: None,
      include_contains: vec!["src".to_string()],
      exclude_contains: vec!["target".to_string()],
    };

    // Must match glob AND contain "src" AND not contain "target"
    assert!(filter.is_allowed("src/main.rs"));
    assert!(!filter.is_allowed("tests/main.rs")); // Doesn't contain "src"
    assert!(!filter.is_allowed("src/target/mod.rs")); // Contains "target"
    assert!(!filter.is_allowed("src/main.txt")); // Doesn't match glob
  }
}
