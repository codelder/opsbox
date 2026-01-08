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
  include: Option<GlobSet>,
  exclude: Option<GlobSet>,
  // 无通配符时的简单包含判断
  include_contains: Vec<String>,
  exclude_contains: Vec<String>,
}

impl PathFilter {
  pub fn is_allowed(&self, path: &str) -> bool {
    if let Some(ex) = &self.exclude
      && ex.is_match(path)
    {
      return false;
    }
    if self.exclude_contains.iter().any(|s| path.contains(s)) {
      return false;
    }
    if let Some(inc) = &self.include
      && !inc.is_match(path)
    {
      return false;
    }
    if !self.include_contains.is_empty() && !self.include_contains.iter().any(|s| path.contains(s)) {
      return false;
    }
    true
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

/// 将 glob 表达式转换为 PathFilter（仅包含 include 规则）
pub fn path_glob_to_filter(glob: &str) -> Result<PathFilter, String> {
  // 使用 strict glob 模式：literal_separator(true)
  // 这意味着 * 不能匹配路径分隔符，必须使用 ** 才能跨目录匹配
  let glob = globset::GlobBuilder::new(glob)
    .literal_separator(true)
    .build()
    .map_err(|e| format!("无效路径模式: {}", e))?;

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
}
