pub mod parser;
mod lexer;

use globset::{GlobSet};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ParseError {
  #[error("invalid regex at {span:?}: {message}")]
  InvalidRegex { message: String, span: (usize, usize) },
  #[error("invalid path pattern at {span:?}: {pattern}")]
  InvalidPathPattern { pattern: String, span: Option<(usize, usize)> },
  #[error("unexpected token at {span:?}")]
  UnexpectedToken { span: (usize, usize) },
  #[error("unbalanced parentheses starting at {span:?}")]
  UnbalancedParens { span: (usize, usize) },
}

#[derive(Debug, Clone)]
pub enum Expr {
  And(Vec<Expr>),
  Or(Vec<Expr>),
  Not(Box<Expr>),
  Atom(usize), // index into Query.terms
}

#[derive(Debug, Clone)]
pub enum Term {
  // Matches a simple substring
  Literal(String),
  // Matches an exact phrase (substring semantics)
  Phrase(String),
  // Matches a regex (Rust regex syntax)
  Regex(regex::Regex),
}

impl Term {
  pub fn matches(&self, line: &str) -> bool {
    match self {
      Term::Literal(s) => line.contains(s),
      Term::Phrase(p) => line.contains(p),
      Term::Regex(r) => r.is_match(line),
    }
  }

  pub fn display_text(&self) -> Option<String> {
    match self {
      Term::Literal(s) => Some(s.clone()),
      Term::Phrase(p) => Some(p.clone()),
      Term::Regex(_) => None, // skip regex for highlighting to avoid confusion
    }
  }
}

#[derive(Debug, Clone, Default)]
pub struct PathFilter {
  include: Option<GlobSet>,
  exclude: Option<GlobSet>,
  // For simple contains without wildcards
  include_contains: Vec<String>,
  exclude_contains: Vec<String>,
}

impl PathFilter {
  pub fn is_allowed(&self, path: &str) -> bool {
    if let Some(ex) = &self.exclude {
      if ex.is_match(path) { return false; }
    }
    if self.exclude_contains.iter().any(|s| path.contains(s)) { return false; }
    if let Some(inc) = &self.include { if !inc.is_match(path) { return false; } }
    if !self.include_contains.is_empty() {
      if !self.include_contains.iter().any(|s| path.contains(s)) { return false; }
    }
    true
  }
}

#[derive(Debug, Clone, Default)]
pub struct Query {
  pub terms: Vec<Term>,
  pub expr: Option<Expr>,
  pub path_filter: PathFilter,
  pub highlights: Vec<String>, // strings to highlight in UI
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
    let highlights: Vec<String> = keywords.iter().filter(|s| !s.is_empty()).cloned().collect();
    Self { terms, expr, path_filter: PathFilter::default(), highlights }
  }

  pub fn parse_github_like(input: &str) -> Result<Self, ParseError> {
    parser::parse_github_like(input)
  }

  pub fn positive_term_indices(&self) -> Vec<usize> {
    let mut indices = Vec::new();
    if let Some(ref e) = self.expr { collect_positive_atoms(e, false, &mut indices); }
    indices.sort(); indices.dedup();
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
    Expr::Atom(i) => { if !neg { out.push(*i); } }
    Expr::Not(inner) => collect_positive_atoms(inner, !neg, out),
    Expr::And(v) | Expr::Or(v) => { for e in v { collect_positive_atoms(e, neg, out); } }
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

