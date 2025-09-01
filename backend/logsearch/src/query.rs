use globset::{Glob, GlobSet, GlobSetBuilder};
use regex::Regex;
use thiserror::Error;

#[derive(Debug, Clone)]
pub enum Term {
  // Matches a simple substring
  Literal(String),
  // Matches an exact phrase (substring semantics)
  Phrase(String),
  // Matches a regex (Rust regex syntax)
  Regex(Regex),
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
  pub fn allowed(&self, path: &str) -> bool {
    if let Some(ex) = &self.exclude {
      if ex.is_match(path) {
        return false;
      }
    }
    if self.exclude_contains.iter().any(|s| path.contains(s)) {
      return false;
    }
    if let Some(inc) = &self.include {
      if !inc.is_match(path) {
        return false;
      }
    }
    if !self.include_contains.is_empty() {
      if !self.include_contains.iter().any(|s| path.contains(s)) {
        return false;
      }
    }
    true
  }
}

#[derive(Debug, Clone)]
pub enum Expr {
  And(Vec<Expr>),
  Or(Vec<Expr>),
  Not(Box<Expr>),
  Atom(usize), // index into QuerySpec.terms
}

#[derive(Debug, Clone, Default)]
pub struct QuerySpec {
  pub terms: Vec<Term>,
  pub expr: Option<Expr>,
  pub path_filter: PathFilter,
  pub highlights: Vec<String>, // strings to highlight in UI
}

impl QuerySpec {
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

  pub fn path_allowed(&self, path: &str) -> bool {
    self.path_filter.allowed(path)
  }

  pub fn parse_github_like(input: &str) -> Result<Self, ParseError> {
    let tokens = tokenize(input)?;

    // Extract path qualifiers first
    let mut includes_glob = GlobSetBuilder::new();
    let mut excludes_glob = GlobSetBuilder::new();
    let mut include_contains: Vec<String> = Vec::new();
    let mut exclude_contains: Vec<String> = Vec::new();
    let mut has_include_glob = false;
    let mut has_exclude_glob = false;

    let mut code_tokens: Vec<Token> = Vec::new();
    for t in tokens.into_iter() {
      match t.kind {
        TokenKind::QualifierPath { negative, pattern } => {
          if pattern.contains('*') || pattern.contains('?') || pattern.contains('[') {
            let pat = if pattern.starts_with('/') || pattern.starts_with("**/") {
              pattern
            } else {
              format!("**/{}", pattern)
            };
            if negative {
              has_exclude_glob = true;
              excludes_glob.add(Glob::new(&pat).map_err(|_| ParseError::InvalidPathPattern(pat.clone()))?);
            } else {
              has_include_glob = true;
              includes_glob.add(Glob::new(&pat).map_err(|_| ParseError::InvalidPathPattern(pat.clone()))?);
            }
          } else {
            if negative { exclude_contains.push(pattern); } else { include_contains.push(pattern); }
          }
        }
        _ => code_tokens.push(Token { kind: t.kind }),
      }
    }

    let mut parser = Parser { tokens: code_tokens, pos: 0 };
    let mut terms: Vec<Term> = Vec::new();
    let expr = parser.parse_expr(&mut terms)?;

    // Build highlights from positive atoms
    let mut highlights: Vec<String> = Vec::new();
    if let Some(ref e) = expr {
      let mut indices = Vec::new();
      collect_positive_atoms(e, false, &mut indices);
      indices.sort();
      indices.dedup();
      for &i in &indices {
        if let Some(s) = terms[i].display_text() { highlights.push(s); }
      }
    }

    let include = if has_include_glob { Some(includes_glob.build().map_err(|_| ParseError::InvalidPathPattern("<build>".into()))?) } else { None };
    let exclude = if has_exclude_glob { Some(excludes_glob.build().map_err(|_| ParseError::InvalidPathPattern("<build>".into()))?) } else { None };

    let path_filter = PathFilter { include, exclude, include_contains, exclude_contains };

    Ok(QuerySpec { terms, expr, path_filter, highlights })
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

#[derive(Debug, Error)]
pub enum ParseError {
  #[error("invalid regex: {0}")]
  InvalidRegex(String),
  #[error("invalid path pattern: {0}")]
  InvalidPathPattern(String),
  #[error("unexpected token")] 
  UnexpectedToken,
  #[error("unbalanced parentheses")] 
  UnbalancedParens,
}

#[derive(Debug, Clone)]
struct Token {
  kind: TokenKind,
}

#[derive(Debug, Clone)]
enum TokenKind {
  QualifierPath { negative: bool, pattern: String },
  Or,
  And,
  LParen,
  RParen,
  Minus,
  Term(Term),
}

struct Parser {
  tokens: Vec<Token>,
  pos: usize,
}

impl Parser {
  fn peek(&self) -> Option<&TokenKind> { self.tokens.get(self.pos).map(|t| &t.kind) }
  fn bump(&mut self) -> Option<TokenKind> { if self.pos < self.tokens.len() { let k = self.tokens[self.pos].kind.clone(); self.pos += 1; Some(k) } else { None } }

  fn parse_expr(&mut self, terms: &mut Vec<Term>) -> Result<Option<Expr>, ParseError> {
    if self.pos >= self.tokens.len() { return Ok(None); }
    let mut left = match self.parse_and(terms)? { Some(e) => e, None => return Ok(None) };
    while matches!(self.peek(), Some(TokenKind::Or)) {
      self.bump(); // consume OR
      let right = match self.parse_and(terms)? { Some(e) => e, None => return Err(ParseError::UnexpectedToken) };
      left = match left { Expr::Or(mut v) => { v.push(right); Expr::Or(v) }, _ => Expr::Or(vec![left, right]) };
    }
    Ok(Some(left))
  }

  fn parse_and(&mut self, terms: &mut Vec<Term>) -> Result<Option<Expr>, ParseError> {
    let mut factors: Vec<Expr> = Vec::new();
    if !self.can_start_pref() { return Ok(None); }
    factors.push(self.parse_pref(terms)?);
    loop {
      match self.peek() {
        Some(TokenKind::And) => {
          self.bump();
          if !self.can_start_pref() { return Err(ParseError::UnexpectedToken); }
          factors.push(self.parse_pref(terms)?);
        }
        Some(TokenKind::Minus) | Some(TokenKind::LParen) | Some(TokenKind::Term(_)) => {
          // implicit AND (adjacent)
          factors.push(self.parse_pref(terms)?);
        }
        _ => break,
      }
    }
    if factors.len() == 1 { Ok(Some(factors.remove(0))) } else { Ok(Some(Expr::And(factors))) }
  }

  fn can_start_pref(&self) -> bool {
    match self.peek() {
      Some(TokenKind::Minus) | Some(TokenKind::LParen) | Some(TokenKind::Term(_)) => true,
      _ => false,
    }
  }

  fn parse_pref(&mut self, terms: &mut Vec<Term>) -> Result<Expr, ParseError> {
    if matches!(self.peek(), Some(TokenKind::Minus)) {
      self.bump();
      // After unary '-', we expect an atom (either '(' expr ')' or a term)
      let e = self.parse_atom(terms)?;
      return Ok(Expr::Not(Box::new(e)));
    }
    self.parse_atom(terms)
  }

  fn parse_atom(&mut self, terms: &mut Vec<Term>) -> Result<Expr, ParseError> {
    match self.bump() {
      Some(TokenKind::LParen) => {
        let inner = self.parse_expr(terms)?;
        if !matches!(self.bump(), Some(TokenKind::RParen)) { return Err(ParseError::UnbalancedParens); }
        Ok(inner.unwrap_or(Expr::And(vec![])))
      }
      Some(TokenKind::Term(t)) => { let idx = terms.len(); terms.push(t); Ok(Expr::Atom(idx)) }
      _ => Err(ParseError::UnexpectedToken),
    }
  }
}

fn peek_char(chars: &[char], i: usize, n: usize) -> Option<char> {
  let idx = i + n;
  if idx < chars.len() { Some(chars[idx]) } else { None }
}

fn tokenize(input: &str) -> Result<Vec<Token>, ParseError> {
  let mut tokens: Vec<Token> = Vec::new();
  let chars: Vec<char> = input.chars().collect();
  let len = chars.len();
  let mut i = 0usize;

  while i < len {
    // skip whitespace
    if chars[i].is_whitespace() { i += 1; continue; }

    // parentheses
    if chars[i] == '(' { tokens.push(Token { kind: TokenKind::LParen }); i += 1; continue; }
    if chars[i] == ')' { tokens.push(Token { kind: TokenKind::RParen }); i += 1; continue; }

    // -path: qualifier or unary minus
    if chars[i] == '-' {
      let is_path = match (peek_char(&chars, i, 1), peek_char(&chars, i, 2), peek_char(&chars, i, 3), peek_char(&chars, i, 4), peek_char(&chars, i, 5)) {
        (Some('p'), Some('a'), Some('t'), Some('h'), Some(':')) => true,
        _ => false,
      };
      if is_path {
        i += 6; // '-' + 'path:'
        let mut pat = String::new();
        while let Some(c) = peek_char(&chars, i, 0) {
          if c.is_whitespace() { break; }
          pat.push(c);
          i += 1;
        }
        tokens.push(Token { kind: TokenKind::QualifierPath { negative: true, pattern: pat } });
        continue;
      } else {
        tokens.push(Token { kind: TokenKind::Minus });
        i += 1;
        continue;
      }
    }

    // path: qualifier
    if chars[i] == 'p' {
      let is_path = match (peek_char(&chars, i, 1), peek_char(&chars, i, 2), peek_char(&chars, i, 3), peek_char(&chars, i, 4)) {
        (Some('a'), Some('t'), Some('h'), Some(':')) => true,
        _ => false,
      };
      if is_path {
        i += 5; // 'path:'
        let mut pat = String::new();
        while let Some(c) = peek_char(&chars, i, 0) {
          if c.is_whitespace() { break; }
          pat.push(c);
          i += 1;
        }
        tokens.push(Token { kind: TokenKind::QualifierPath { negative: false, pattern: pat } });
        continue;
      }
    }

    // OR operator (uppercase OR) with boundary check
    if chars[i] == 'O' {
      if let Some('R') = peek_char(&chars, i, 1) {
        match peek_char(&chars, i, 2) {
          None | Some(' ') | Some('\t') | Some('\n') | Some('\r') | Some(')') => {
            tokens.push(Token { kind: TokenKind::Or });
            i += 2;
            continue;
          }
          _ => {}
        }
      }
    }
    // AND operator (uppercase AND) with boundary check
    if chars[i] == 'A' {
      if let (Some('N'), Some('D')) = (peek_char(&chars, i, 1), peek_char(&chars, i, 2)) {
        match peek_char(&chars, i, 3) {
          None | Some(' ') | Some('\t') | Some('\n') | Some('\r') | Some(')') => {
            tokens.push(Token { kind: TokenKind::And });
            i += 3;
            continue;
          }
          _ => {}
        }
      }
    }

    // Term/phrase/regex
    if chars[i] == '"' {
      // phrase
      i += 1; // skip opening quote
      let mut s = String::new();
      while let Some(c) = peek_char(&chars, i, 0) {
        i += 1;
        if c == '"' { break; }
        s.push(c);
      }
      tokens.push(Token { kind: TokenKind::Term(Term::Phrase(s)) });
      continue;
    }
    if chars[i] == '/' {
      // regex /.../
      i += 1; // skip leading '/'
      let mut s = String::new();
      let mut escaped = false;
      while let Some(c) = peek_char(&chars, i, 0) {
        i += 1;
        if escaped { s.push(c); escaped = false; continue; }
        if c == '\\' { escaped = true; continue; }
        if c == '/' { break; }
        s.push(c);
      }
      let re = Regex::new(&s).map_err(|e| ParseError::InvalidRegex(e.to_string()))?;
      tokens.push(Token { kind: TokenKind::Term(Term::Regex(re)) });
      continue;
    }

    // literal until whitespace or right paren
    let mut s = String::new();
    while let Some(c) = peek_char(&chars, i, 0) {
      if c.is_whitespace() || c == ')' || c == '(' { break; }
      s.push(c);
      i += 1;
    }
    if !s.is_empty() {
      tokens.push(Token { kind: TokenKind::Term(Term::Literal(s)) });
      continue;
    }

    // safety: advance one to avoid infinite loop if we hit unexpected char
    i += 1;
  }

  Ok(tokens)
}

fn parse_term<I: Iterator<Item = char>>(it: &mut std::iter::Peekable<I>) -> Result<Term, ParseError> {
  if let Some('"') = it.peek().copied() {
    // phrase
    it.next();
    let mut s = String::new();
    while let Some(c) = it.next() {
      if c == '"' { break; }
      s.push(c);
    }
    return Ok(Term::Phrase(s));
  }
  if let Some('/') = it.peek().copied() {
    // regex delimited by /.../
    it.next();
    let mut s = String::new();
    let mut escaped = false;
    while let Some(c) = it.next() {
      if escaped { s.push(c); escaped = false; continue; }
      if c == '\\' { escaped = true; continue; }
      if c == '/' { break; }
      s.push(c);
    }
    let re = Regex::new(&s).map_err(|e| ParseError::InvalidRegex(e.to_string()))?;
    return Ok(Term::Regex(re));
  }

  // word until whitespace
  let mut s = String::new();
  while let Some(&c) = it.peek() { if c.is_whitespace() { break; } s.push(c); it.next(); }
  Ok(Term::Literal(s))
}

#[cfg(test)]
mod tests {
  use super::*;

  fn idx_of_literal(spec: &QuerySpec, s: &str) -> Option<usize> {
    spec.terms.iter().position(|t| matches!(t, Term::Literal(x) if x == s))
  }

  #[test]
  fn tokenize_parens_minus_or_and_terms() {
    let input = "(foo OR \"bar baz\") -/ERR\\d+/";
    let toks = tokenize(input).expect("tokenize");
    assert!(matches!(toks[0].kind, TokenKind::LParen));
    assert!(matches!(toks[1].kind, TokenKind::Term(super::Term::Literal(ref s)) if s == "foo"));
    assert!(matches!(toks[2].kind, TokenKind::Or));
    assert!(matches!(toks[3].kind, TokenKind::Term(super::Term::Phrase(ref s)) if s == "bar baz"));
    assert!(matches!(toks[4].kind, TokenKind::RParen));
    assert!(matches!(toks[5].kind, TokenKind::Minus));
    // Can't easily compare regex content; just ensure it's a regex term
    assert!(matches!(toks[6].kind, TokenKind::Term(super::Term::Regex(_))));
  }

  #[test]
  fn tokenize_group_then_term() {
    let input = "(foo OR bar) baz";
    let toks = tokenize(input).expect("tokenize");
    assert_eq!(toks.len(), 6);
    assert!(matches!(toks[0].kind, TokenKind::LParen));
    assert!(matches!(toks[1].kind, TokenKind::Term(super::Term::Literal(ref s)) if s == "foo"));
    assert!(matches!(toks[2].kind, TokenKind::Or));
    assert!(matches!(toks[3].kind, TokenKind::Term(super::Term::Literal(ref s)) if s == "bar"));
    assert!(matches!(toks[4].kind, TokenKind::RParen));
    assert!(matches!(toks[5].kind, TokenKind::Term(super::Term::Literal(ref s)) if s == "baz"));
  }

  #[test]
  fn parse_precedence_and_structure() {
    let spec = QuerySpec::parse_github_like("foo OR bar -baz").expect("parse");
    // Expect terms = [foo, bar, baz]
    assert_eq!(spec.terms.len(), 3);
    assert!(matches!(spec.expr, Some(_)));
    // Check shape: Or( foo , And(bar, Not(baz)) )
    match spec.expr.unwrap() {
      Expr::Or(v) => {
        assert_eq!(v.len(), 2);
        match &v[0] { Expr::Atom(i) => assert_eq!(*i, 0), _ => panic!("left not atom") }
        match &v[1] {
          Expr::And(v2) => {
            assert_eq!(v2.len(), 2);
            match &v2[0] { Expr::Atom(i) => assert_eq!(*i, 1), _ => panic!("and[0]") }
            match &v2[1] { Expr::Not(inner) => match **inner { Expr::Atom(i) => assert_eq!(i, 2), _ => panic!("not atom") }, _ => panic!("and[1] not Not") }
          }
          _ => panic!("right not and"),
        }
      }
      _ => panic!("top not Or"),
    }
  }

  #[test]
  fn eval_file_boolean() {
    let spec = QuerySpec::parse_github_like("(foo OR bar) baz").expect("parse");
    // Map indices
    let i_foo = idx_of_literal(&spec, "foo").unwrap();
    let i_bar = idx_of_literal(&spec, "bar").unwrap();
    let i_baz = idx_of_literal(&spec, "baz").unwrap();

    let mut occurs = vec![false; spec.terms.len()];
    occurs[i_foo] = true;
    occurs[i_baz] = true;
    assert!(spec.eval_file(&occurs), "foo and baz present");

    let mut occurs2 = vec![false; spec.terms.len()];
    occurs2[i_bar] = true;
    assert!(!spec.eval_file(&occurs2), "only bar present (missing baz)");
  }

  #[test]
  fn path_filter_glob_and_contains() {
    let spec = QuerySpec::parse_github_like("path:logs/*.log -path:node_modules/ foo").expect("parse");
    assert!(spec.path_allowed("logs/app/app.log"));
    assert!(!spec.path_allowed("app/node_modules/x.js"));
    assert!(!spec.path_allowed("logs/app/readme.md")); // include glob requires *.log
    assert!(!spec.path_allowed("src/app.log")); // include glob requires logs/
  }

  #[test]
  fn highlights_positive_atoms_only() {
    let spec = QuerySpec::parse_github_like("(\"hello world\" OR foo) /ERR\\d+/").expect("parse");
    assert!(spec.highlights.contains(&"hello world".to_string()));
    assert!(spec.highlights.contains(&"foo".to_string()));
    // regex not added to highlights
    assert_eq!(spec.highlights.iter().filter(|s| s.starts_with("ERR")).count(), 0);
  }

  #[test]
  fn unbalanced_parens_error() {
    let err = QuerySpec::parse_github_like("foo OR (bar").unwrap_err();
    matches!(err, ParseError::UnbalancedParens);
  }

  #[test]
  fn or_must_be_uppercase() {
    let spec = QuerySpec::parse_github_like("foo or bar").expect("parse");
    // should be parsed as AND of three literals: foo, or, bar
    match spec.expr.unwrap() {
      Expr::And(v) => assert_eq!(v.len(), 3),
      other => panic!("expected And of 3 terms, got {:?}", other),
    }
  }

  #[test]
  fn or_boundary_not_split() {
    let spec = QuerySpec::parse_github_like("foo ORbar baz").expect("parse");
    // should be parsed as AND of three literals: foo, ORbar, baz
    assert_eq!(spec.terms.len(), 3);
    match spec.expr.unwrap() {
      Expr::And(v) => assert_eq!(v.len(), 3),
      other => panic!("expected And of 3 terms, got {:?}", other),
    }
  }

  #[test]
  fn deep_nested_parse_and_eval() {
    let q = "((foo OR bar) AND (baz OR (qux AND -zim))) OR -(alpha OR beta)";
    let spec = QuerySpec::parse_github_like(q).expect("parse");
    // helper to set occurs by literal names
    let mut occurs = vec![false; spec.terms.len()];
    let set = |occurs: &mut Vec<bool>, name: &str| {
      if let Some(i) = spec.terms.iter().position(|t| matches!(t, Term::Literal(s) if s == name)) {
        occurs[i] = true;
      }
    };

    // Case 1: left satisfied via foo + baz
    let mut oc1 = occurs.clone();
    set(&mut oc1, "foo"); set(&mut oc1, "baz");
    assert!(spec.eval_file(&oc1));

    // Case 2: left satisfied via qux AND -zim
    let mut oc2 = occurs.clone();
    set(&mut oc2, "qux");
    assert!(spec.eval_file(&oc2));

    // Case 3: right side negation disables when alpha present and left false
    let mut oc3 = occurs.clone();
    set(&mut oc3, "alpha");
    assert!(!spec.eval_file(&oc3));

    // Case 4: right side true when neither alpha nor beta present and left false
    let oc4 = occurs.clone();
    assert!(spec.eval_file(&oc4));
  }

  #[test]
  fn or_chain_with_and_precedence() {
    // a OR b OR (c AND d)
    let spec = QuerySpec::parse_github_like("a OR b OR (c d)").expect("parse");
    let idx = |name: &str| spec.terms.iter().position(|t| matches!(t, Term::Literal(s) if s == name)).unwrap();
    let mut oc = vec![false; spec.terms.len()];

    // c alone should not satisfy
    oc[idx("c")] = true; assert!(!spec.eval_file(&oc)); oc[idx("c")] = false;
    // c and d together should satisfy
    oc[idx("c")] = true; oc[idx("d")] = true; assert!(spec.eval_file(&oc)); oc = vec![false; spec.terms.len()];
    // b alone should satisfy
    oc[idx("b")] = true; assert!(spec.eval_file(&oc));
  }

  #[test]
  fn group_and_negation_inside() {
    // (a b) OR -(c OR d)
    let spec = QuerySpec::parse_github_like("(a b) OR -(c OR d)").expect("parse");
    let idx = |name: &str| spec.terms.iter().position(|t| matches!(t, Term::Literal(s) if s == name)).unwrap();

    let mut oc = vec![false; spec.terms.len()];
    // a & b -> true
    oc[idx("a")] = true; oc[idx("b")] = true; assert!(spec.eval_file(&oc));

    // only c -> false (right becomes false, left false)
    let mut oc2 = vec![false; spec.terms.len()];
    oc2[idx("c")] = true; assert!(!spec.eval_file(&oc2));

    // neither c nor d and left false -> right NOT(false) == true
    let oc3 = vec![false; spec.terms.len()];
    assert!(spec.eval_file(&oc3));
  }

  #[test]
  fn positive_indices_under_negation() {
    let spec = QuerySpec::parse_github_like("-(a OR b) c").expect("parse");
    let idxs = spec.positive_term_indices();
    // Only 'c' should be positive
    assert_eq!(idxs.len(), 1);
    let lit = match &spec.terms[idxs[0]] { Term::Literal(s) => s, _ => panic!("not literal") };
    assert_eq!(lit, "c");
  }

  #[test]
  fn trailing_or_is_error() {
    let err = QuerySpec::parse_github_like("foo OR ").unwrap_err();
    matches!(err, ParseError::UnexpectedToken | ParseError::UnbalancedParens);
  }
}

