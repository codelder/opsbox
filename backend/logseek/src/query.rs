mod lexer;
pub mod parser;

use globset::GlobSet;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ParseError {
  #[error("无效正则，位置 {span:?}：{message}")]
  InvalidRegex { message: String, span: (usize, usize) },
  #[error("无效路径模式，位置 {span:?}：{pattern}")]
  InvalidPathPattern {
    pattern: String,
    span: Option<(usize, usize)>,
  },
  #[error("意外的记号，位置 {span:?}")]
  UnexpectedToken { span: (usize, usize) },
  #[error("括号不匹配，起始于 {span:?}")]
  UnbalancedParens { span: (usize, usize) },
}

#[derive(Debug, Clone)]
pub enum Expr {
  And(Vec<Expr>),
  Or(Vec<Expr>),
  Not(Box<Expr>),
  Atom(usize), // 索引到 Query.terms（关键字列表）
}

#[derive(Debug, Clone)]
pub enum Term {
  // 匹配简单子串
  Literal(String),
  // 匹配精确短语（子串语义）
  Phrase(String),
  // 标准 regex 引擎（性能更好，不支持 look-around）
  RegexStd { pattern: String, re: regex::Regex },
  // fancy-regex 引擎（支持 look-around，可能有回溯开销）
  RegexFancy { pattern: String, re: fancy_regex::Regex },
}

impl Term {
  pub fn matches(&self, line: &str) -> bool {
    match self {
      // Literal: 默认不区分大小写
      Term::Literal(s) => {
        let line_lower = line.to_lowercase();
        let s_lower = s.to_lowercase();
        line_lower.contains(&s_lower)
      }
      // Phrase: 引号内的短语区分大小写
      Term::Phrase(p) => line.contains(p),
      Term::RegexStd { re, .. } => re.is_match(line),
      Term::RegexFancy { re, .. } => re.is_match(line).unwrap_or(false),
    }
  }

  pub fn highlight(&self) -> Option<KeywordHighlight> {
    match self {
      Term::Literal(s) => Some(KeywordHighlight::Literal(s.clone())),
      Term::Phrase(p) => Some(KeywordHighlight::Phrase(p.clone())),
      Term::RegexStd { pattern, .. } | Term::RegexFancy { pattern, .. } => {
        Some(KeywordHighlight::Regex(pattern.clone()))
      }
    }
  }
}

#[derive(Debug, Clone, Default)]
pub struct PathFilter {
  pub include: Option<GlobSet>,
  pub exclude: Option<GlobSet>,
  // 无通配符时的简单包含判断
  pub include_contains: Vec<String>,
  pub exclude_contains: Vec<String>,
}

impl PathFilter {
  pub fn is_allowed(&self, path: &str) -> bool {
    // 排除规则：任一命中即排除（并集）
    if let Some(ex) = &self.exclude
      && ex.is_match(path)
    {
      return false;
    }
    if self.exclude_contains.iter().any(|s| path.contains(s)) {
      return false;
    }

    // 包含规则：如果没有任何包含规则，则允许
    // 如果有包含规则，任一命中即允许（并集）
    let has_include = self.include.is_some();
    let has_include_contains = !self.include_contains.is_empty();

    if !has_include && !has_include_contains {
      return true;
    }

    // 检查是否命中任一包含规则
    let matches_include = self.include.as_ref().is_some_and(|inc| inc.is_match(path));
    let matches_include_contains = self.include_contains.iter().any(|s| path.contains(s));

    matches_include || matches_include_contains
  }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "text", rename_all = "lowercase")]
pub enum KeywordHighlight {
  Literal(String),
  Phrase(String),
  Regex(String),
}

#[derive(Debug, Clone, Default)]
pub struct Query {
  pub terms: Vec<Term>,
  pub expr: Option<Expr>,
  pub path_filter: PathFilter,
  pub highlights: Vec<KeywordHighlight>, // 带类型信息的高亮列表（仅正向项）
  pub byte_matchers: Vec<Option<regex::bytes::Regex>>,
}

impl Query {
  pub fn new(terms: Vec<Term>) -> Self {
    let expr = if terms.is_empty() {
      None
    } else {
      let atoms: Vec<Expr> = (0..terms.len()).map(Expr::Atom).collect();
      Some(Expr::And(atoms))
    };
    let highlights = terms.iter().flat_map(|t| t.highlight()).collect();
    Self {
      terms,
      expr,
      path_filter: PathFilter::default(),
      highlights,
      byte_matchers: vec![],
    }
  }

  pub fn with_path_filter(mut self, pattern: Option<String>) -> Result<Self, String> {
    if let Some(p) = pattern {
      if let Some(stripped) = p.strip_prefix('!') {
        let glob = globset::GlobBuilder::new(stripped).build().map_err(|e| e.to_string())?;
        let mut builder = globset::GlobSetBuilder::new();
        builder.add(glob);
        let set = builder.build().map_err(|e| e.to_string())?;
        self.path_filter.exclude = Some(set);
      } else {
        let glob = globset::GlobBuilder::new(&p).build().map_err(|e| e.to_string())?;
        let mut builder = globset::GlobSetBuilder::new();
        builder.add(glob);
        let set = builder.build().map_err(|e| e.to_string())?;
        self.path_filter.include = Some(set);
      }
    }
    Ok(self)
  }

  pub fn from_keywords(keywords: &[String]) -> Self {
    let mut terms: Vec<Term> = Vec::new();
    for s in keywords.iter().filter(|s| !s.is_empty()) {
      terms.push(Term::Literal(s.clone()));
    }
    let expr = if terms.is_empty() {
      None
    } else {
      let atoms: Vec<Expr> = (0..terms.len()).map(Expr::Atom).collect();
      Some(Expr::And(atoms))
    };
    let highlights: Vec<KeywordHighlight> = keywords
      .iter()
      .filter(|s| !s.is_empty())
      .map(|s| KeywordHighlight::Literal(s.clone()))
      .collect();
    Self {
      terms,
      expr,
      path_filter: PathFilter::default(),
      highlights,
      byte_matchers: keywords
        .iter()
        .filter(|s| !s.is_empty())
        .map(|s| {
          regex::bytes::RegexBuilder::new(&regex::escape(s))
            .case_insensitive(true)
            .build()
            .ok()
        })
        .collect(),
    }
  }

  pub fn parse_github_like(input: &str) -> Result<Self, ParseError> {
    parser::parse_github_like(input)
  }

  pub fn positive_term_indices(&self) -> Vec<usize> {
    let mut indices = Vec::new();
    if let Some(ref e) = self.expr {
      collect_positive_atoms(e, false, &mut indices);
    }
    indices.sort();
    indices.dedup();
    indices
  }

  pub fn eval_file(&self, occurs: &[bool]) -> bool {
    if let Some(ref e) = self.expr {
      eval_expr(e, &|i| occurs.get(i).copied().unwrap_or(false))
    } else {
      false
    }
  }
}

fn collect_positive_atoms(expr: &Expr, neg: bool, out: &mut Vec<usize>) {
  match expr {
    Expr::Atom(i) => {
      if !neg {
        out.push(*i);
      }
    }
    Expr::Not(inner) => collect_positive_atoms(inner, !neg, out),
    Expr::And(v) | Expr::Or(v) => {
      for e in v {
        collect_positive_atoms(e, neg, out);
      }
    }
  }
}

fn eval_expr(expr: &Expr, f: &dyn Fn(usize) -> bool) -> bool {
  match expr {
    Expr::Atom(i) => f(*i),
    Expr::Not(inner) => !eval_expr(inner, f),
    Expr::And(v) => v.iter().all(|e| eval_expr(e, f)),
    Expr::Or(v) => v.iter().any(|e| eval_expr(e, f)),
  }
}

/// 将 includes 和 excludes 列表组合成一个 PathFilter
///
/// 该函数用于统一 Local/S3/Agent 的路径过滤逻辑。
/// 无效的 glob 模式会被记录警告并跳过。
///
/// # Arguments
/// * `includes` - 包含的 glob 模式列表
/// * `excludes` - 排除的 glob 模式列表
///
/// # Returns
/// * `Some(PathFilter)` - 如果有任何有效的过滤规则
/// * `None` - 如果没有有效的过滤规则
pub fn combine_path_filters(includes: &[String], excludes: &[String]) -> Option<PathFilter> {
  let mut filter = PathFilter::default();
  let mut has_filter = false;

  // 处理 includes
  if !includes.is_empty() {
    let mut builder = globset::GlobSetBuilder::new();
    let mut has_include_glob = false;
    for p in includes {
      match classify_path_pattern(p) {
        PathPatternRule::Glob(pat) => match build_strict_path_glob(&pat) {
          Ok(g) => {
            builder.add(g);
            has_include_glob = true;
            has_filter = true;
          }
          Err(e) => tracing::warn!("无效的 path glob: {} ({})", p, e),
        },
        PathPatternRule::Contains(raw) => {
          filter.include_contains.push(raw);
          has_filter = true;
        }
      }
    }
    if has_include_glob && let Ok(set) = builder.build() {
      filter.include = Some(set);
    }
  }

  // 处理 excludes
  if !excludes.is_empty() {
    let mut builder = globset::GlobSetBuilder::new();
    let mut has_exclude_glob = false;
    for p in excludes {
      match classify_path_pattern(p) {
        PathPatternRule::Glob(pat) => match build_strict_path_glob(&pat) {
          Ok(g) => {
            builder.add(g);
            has_exclude_glob = true;
            has_filter = true;
          }
          Err(e) => tracing::warn!("无效的 -path glob: {} ({})", p, e),
        },
        PathPatternRule::Contains(raw) => {
          filter.exclude_contains.push(raw);
          has_filter = true;
        }
      }
    }
    if has_exclude_glob && let Ok(set) = builder.build() {
      filter.exclude = Some(set);
    }
  }

  if has_filter { Some(filter) } else { None }
}

pub(super) enum PathPatternRule {
  Glob(String),
  Contains(String),
}

pub(super) fn classify_path_pattern(pattern: &str) -> PathPatternRule {
  if is_glob_pattern(pattern) {
    PathPatternRule::Glob(pattern.to_string())
  } else {
    PathPatternRule::Contains(pattern.to_string())
  }
}

pub(super) fn build_strict_path_glob(pattern: &str) -> Result<globset::Glob, globset::Error> {
  globset::GlobBuilder::new(pattern).literal_separator(true).build()
}

fn is_glob_pattern(pattern: &str) -> bool {
  // 与 parser::parse_github_like 保持同一判定规则，避免语义漂移。
  pattern.contains('*') || pattern.contains('?') || pattern.contains('[')
}

/// 将 glob 表达式转换为 PathFilter（仅包含 include 规则）
pub fn path_glob_to_filter(glob: &str) -> Result<PathFilter, String> {
  // 使用 strict glob 模式：literal_separator(true)
  // 这意味着 * 不能匹配路径分隔符，必须使用 ** 才能跨目录匹配
  let glob = build_strict_path_glob(glob).map_err(|e| format!("无效路径模式: {}", e))?;

  let mut builder = globset::GlobSetBuilder::new();
  builder.add(glob);
  let set = builder.build().map_err(|e| format!("构建路径过滤器失败: {}", e))?;
  Ok(PathFilter {
    include: Some(set),
    exclude: None,
    include_contains: Vec::new(),
    exclude_contains: Vec::new(),
  })
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_combine_path_filters_empty() {
    let result = combine_path_filters(&[], &[]);
    assert!(result.is_none());
  }

  #[test]
  fn test_combine_path_filters_with_includes() {
    let result = combine_path_filters(&["*.log".to_string()], &[]);
    assert!(result.is_some());
    let filter = result.unwrap();
    assert!(filter.include.is_some());
    assert!(filter.exclude.is_none());
  }

  #[test]
  fn test_combine_path_filters_with_excludes() {
    let result = combine_path_filters(&[], &["*.tmp".to_string()]);
    assert!(result.is_some());
    let filter = result.unwrap();
    assert!(filter.include.is_none());
    assert!(filter.exclude.is_some());
  }

  #[test]
  fn test_combine_path_filters_with_both() {
    let result = combine_path_filters(&["*.log".to_string()], &["*.tmp".to_string()]);
    assert!(result.is_some());
    let filter = result.unwrap();
    assert!(filter.include.is_some());
    assert!(filter.exclude.is_some());
  }

  #[test]
  fn test_combine_path_filters_invalid_glob() {
    // Invalid glob patterns should be handled gracefully.
    // 无有效规则时返回 None。
    let result = combine_path_filters(&["[invalid".to_string()], &[]);
    assert!(result.is_none());
  }

  #[test]
  fn test_combine_path_filters_plain_exclude_uses_contains() {
    let filter = combine_path_filters(&[], &["vendor/".to_string()]).unwrap();
    assert!(filter.exclude.is_none());
    assert_eq!(filter.exclude_contains, vec!["vendor/".to_string()]);
    assert!(!filter.is_allowed("lib/vendor/a.js"));
    assert!(filter.is_allowed("lib/src/a.js"));
  }

  #[test]
  fn test_combine_path_filters_plain_include_uses_contains() {
    let filter = combine_path_filters(&["src/".to_string()], &[]).unwrap();
    assert!(filter.include.is_none());
    assert_eq!(filter.include_contains, vec!["src/".to_string()]);
    assert!(filter.is_allowed("app/src/main.rs"));
    assert!(!filter.is_allowed("app/tests/main.rs"));
  }

  #[test]
  fn test_path_glob_filter_strict() {
    // 1. *.log 不应匹配子目录 (如果 separator 是严格的)
    let filter = path_glob_to_filter("*.log").unwrap();
    assert!(filter.is_allowed("error.log"));
    assert!(!filter.is_allowed("var/error.log")); // / 被视为分隔符，* 无法匹配
    assert!(!filter.is_allowed("/var/error.log"));

    // 2. */*.log 匹配一级子目录
    let filter = path_glob_to_filter("*/*.log").unwrap();
    assert!(!filter.is_allowed("error.log"));
    assert!(filter.is_allowed("var/error.log"));
    assert!(!filter.is_allowed("var/log/error.log"));

    // 3. **/*.log 递归匹配
    let filter = path_glob_to_filter("**/*.log").unwrap();
    assert!(filter.is_allowed("error.log")); // ** 可以匹配空
    assert!(filter.is_allowed("var/error.log"));
    assert!(filter.is_allowed("var/log/error.log"));
    assert!(filter.is_allowed("/abs/path/to/error.log"));
  }

  #[test]
  fn test_term_matches() {
    // Literal is case-insensitive
    let term = Term::Literal("ERROR".to_string());
    assert!(term.matches("error occurred"));
    assert!(term.matches("ERROR occurred"));
    assert!(!term.matches("warning"));

    // Phrase is case-sensitive
    let term = Term::Phrase("Error".to_string());
    assert!(term.matches("Error occurred"));
    assert!(!term.matches("error occurred"));

    // Regex
    let re = regex::Regex::new(r"\d+").unwrap();
    let term = Term::RegexStd {
      pattern: r"\d+".to_string(),
      re,
    };
    assert!(term.matches("line 123"));
    assert!(!term.matches("no numbers"));
  }

  #[test]
  fn test_from_keywords() {
    let keywords = vec!["foo".to_string(), "bar".to_string(), "".to_string()];
    let query = Query::from_keywords(&keywords);

    assert_eq!(query.terms.len(), 2); // Empty string filtered out
    assert_eq!(query.highlights.len(), 2);
    assert!(matches!(query.expr, Some(Expr::And(_))));

    // Empty keywords
    let query = Query::from_keywords(&[]);
    assert!(query.terms.is_empty());
    assert!(query.expr.is_none());
  }

  #[test]
  fn test_eval_expr_logic() {
    // AND
    let expr = Expr::And(vec![Expr::Atom(0), Expr::Atom(1)]);
    assert!(eval_expr(&expr, &|i| i < 2)); // Both true
    assert!(!eval_expr(&expr, &|i| i == 0)); // Only first true

    // OR
    let expr = Expr::Or(vec![Expr::Atom(0), Expr::Atom(1)]);
    assert!(eval_expr(&expr, &|i| i == 0)); // First true
    assert!(eval_expr(&expr, &|i| i == 1)); // Second true
    assert!(!eval_expr(&expr, &|_| false)); // Both false

    // NOT
    let expr = Expr::Not(Box::new(Expr::Atom(0)));
    assert!(!eval_expr(&expr, &|_| true));
    assert!(eval_expr(&expr, &|_| false));
  }

  #[test]
  fn test_collect_positive_atoms() {
    let mut out = Vec::new();

    // Simple atom
    collect_positive_atoms(&Expr::Atom(0), false, &mut out);
    assert_eq!(out, vec![0]);

    // Negated atom should not be collected
    out.clear();
    collect_positive_atoms(&Expr::Not(Box::new(Expr::Atom(1))), false, &mut out);
    assert!(out.is_empty());

    // Mixed
    out.clear();
    let expr = Expr::And(vec![Expr::Atom(0), Expr::Not(Box::new(Expr::Atom(1))), Expr::Atom(2)]);
    collect_positive_atoms(&expr, false, &mut out);
    assert_eq!(out, vec![0, 2]);
  }

  #[test]
  fn test_path_filter_is_allowed() {
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
  fn test_keyword_highlight_serialization() {
    let hl = KeywordHighlight::Literal("test".to_string());
    let json = serde_json::to_string(&hl).unwrap();
    assert!(json.contains("literal"));
    assert!(json.contains("test"));

    let deserialized: KeywordHighlight = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized, hl);
  }

  // ========== 语义对齐测试 ==========

  /// 测试：combine_path_filters 与 parse_github_like 对无通配符 pattern 语义一致
  #[test]
  fn test_combine_and_parser_semantics_aligned_for_plain_path() {
    let combined = combine_path_filters(&[], &["vendor/".to_string()]).unwrap();
    let parsed = Query::parse_github_like("error -path:vendor/").unwrap();

    // 两者都应走 contains 语义
    let path = "lib/vendor/a.js";
    assert_eq!(combined.is_allowed(path), parsed.path_filter.is_allowed(path));
    assert!(!combined.is_allowed(path), "vendor/ 应该排除包含 vendor/ 的路径");
  }

  /// 测试：**/vendor/** glob 等价于 vendor/ contains
  #[test]
  fn test_glob_vendor_recursive_excludes() {
    let filter = combine_path_filters(&[], &["**/vendor/**".to_string()]).unwrap();
    assert!(!filter.is_allowed("lib/vendor/a.js"), "应排除 vendor 目录下的文件");
    assert!(filter.is_allowed("lib/src/a.js"), "不应排除非 vendor 目录");
  }

  /// 测试：**ptcr** (glob) 与 ptcr (contains) 语义不等价
  /// glob **ptcr** 只匹配文件名包含 ptcr，不匹配目录名
  #[test]
  fn test_glob_star_ptcr_not_equivalent_to_contains_ptcr() {
    let filter_contains = combine_path_filters(&["ptcr".to_string()], &[]).unwrap();
    let filter_glob = combine_path_filters(&["**ptcr**".to_string()], &[]).unwrap();

    // 目录名是 ptcr：contains 匹配，glob 不匹配
    assert!(filter_contains.is_allowed("lib/ptcr/a.js"));
    assert!(!filter_glob.is_allowed("lib/ptcr/a.js"));

    // 目录名包含 ptcr：contains 匹配，glob 不匹配
    assert!(filter_contains.is_allowed("lib/myptcr/a.js"));
    assert!(!filter_glob.is_allowed("lib/myptcr/a.js"));

    // 文件名包含 ptcr：两者都匹配
    assert!(filter_contains.is_allowed("ptcr.js"));
    assert!(filter_glob.is_allowed("ptcr.js"));
    assert!(filter_contains.is_allowed("myptcrfile.js"));
    assert!(filter_glob.is_allowed("myptcrfile.js"));
  }

  /// 测试：contains ptcr 可用 4 个 glob 模式组合等价
  #[test]
  fn test_contains_ptcr_equivalent_to_glob_combination() {
    let filter_contains = combine_path_filters(&["ptcr".to_string()], &[]).unwrap();
    let filter_glob_combo = combine_path_filters(
      &[
        "**/ptcr/**".to_string(),   // 目录名精确是 ptcr
        "**/*ptcr*/**".to_string(), // 目录名包含 ptcr
        "**/*ptcr*".to_string(),    // 文件名包含 ptcr
        "ptcr".to_string(),         // 根目录精确匹配
      ],
      &[],
    )
    .unwrap();

    // 验证所有场景语义等价
    let test_cases = [
      "ptcr",              // 精确匹配
      "ptcr.js",           // 文件名以 ptcr 开头
      "lib/ptcr/a.js",     // 目录名是 ptcr
      "lib/myptcr/a.js",   // 目录名包含 ptcr
      "myptcrfile.js",     // 文件名包含 ptcr
      "lib/myptcrfile.js", // 嵌套路径文件名包含 ptcr
    ];

    for path in test_cases {
      assert_eq!(
        filter_contains.is_allowed(path),
        filter_glob_combo.is_allowed(path),
        "路径 '{}' 语义不等价",
        path
      );
      assert!(filter_contains.is_allowed(path), "路径 '{}' 应被允许", path);
    }
  }

  /// 测试：验证 glob 模式原始语义（去掉 normalize 后）
  #[test]
  fn test_glob_raw_semantics_no_normalize() {
    let path = "Users/wangyue/Downloads/dir22/home/bbipadm/logs/msk/nohup-route.log";

    // 测试 1: **/nohup*.log 应该匹配（任意目录下的 nohup*.log）
    let filter1 = combine_path_filters(&["**/nohup*.log".to_string()], &[]).unwrap();
    assert!(filter1.is_allowed(path), "**/nohup*.log 应匹配 {}", path);

    // 测试 2: **/*22*/**/nohup*.log 应该匹配（包含 22 的目录）
    let filter2 = combine_path_filters(&["**/*22*/**/nohup*.log".to_string()], &[]).unwrap();
    assert!(filter2.is_allowed(path), "**/*22*/**/nohup*.log 应匹配 {}", path);

    // 测试 3: abc/nohup*.log 不应该匹配（根目录是 Users 不是 abc）
    let filter3 = combine_path_filters(&["abc/nohup*.log".to_string()], &[]).unwrap();
    assert!(!filter3.is_allowed(path), "abc/nohup*.log 不应匹配 {}", path);

    // 测试 4: nohup*.log 不应该匹配（没有 **/ 前缀，只匹配根目录)
    let filter4 = combine_path_filters(&["nohup*.log".to_string()], &[]).unwrap();
    assert!(!filter4.is_allowed(path), "nohup*.log 不应匹配 {}", path);

  }

}
