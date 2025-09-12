use super::lexer::{Token, TokenKind, tokenize};
use super::{Expr, ParseError, PathFilter, Query, Term};
use globset::{Glob, GlobSetBuilder};
// 标准 regex 引擎直接通过路径使用：regex::Regex

pub fn parse_github_like(input: &str) -> Result<Query, ParseError> {
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
            excludes_glob.add(Glob::new(&pat).map_err(|_| ParseError::InvalidPathPattern {
              pattern: pat.clone(),
              span: Some(t.span),
            })?);
          } else {
            has_include_glob = true;
            includes_glob.add(Glob::new(&pat).map_err(|_| ParseError::InvalidPathPattern {
              pattern: pat.clone(),
              span: Some(t.span),
            })?);
          }
        } else {
          if negative {
            exclude_contains.push(pattern);
          } else {
            include_contains.push(pattern);
          }
        }
      }
      _ => code_tokens.push(t),
    }
  }

  let mut parser = Parser {
    tokens: code_tokens,
    pos: 0,
  };
  let mut terms: Vec<Term> = Vec::new();
  let expr = parser.parse_expr(&mut terms)?;

  // Build highlights from positive atoms
  let mut highlights: Vec<String> = Vec::new();
  if let Some(ref e) = expr {
    let mut indices = Vec::new();
    super::collect_positive_atoms(e, false, &mut indices);
    indices.sort();
    indices.dedup();
    for &i in &indices {
      if let Some(s) = terms[i].display_text() {
        highlights.push(s);
      }
    }
  }

  let include = if has_include_glob {
    Some(includes_glob.build().map_err(|_| ParseError::InvalidPathPattern {
      pattern: "<build>".into(),
      span: None,
    })?)
  } else {
    None
  };
  let exclude = if has_exclude_glob {
    Some(excludes_glob.build().map_err(|_| ParseError::InvalidPathPattern {
      pattern: "<build>".into(),
      span: None,
    })?)
  } else {
    None
  };

  let path_filter = PathFilter {
    include,
    exclude,
    include_contains,
    exclude_contains,
  };

  Ok(Query {
    terms,
    expr,
    path_filter,
    highlights,
  })
}

struct Parser {
  tokens: Vec<Token>,
  pos: usize,
}

impl Parser {
  fn peek_kind(&self) -> Option<&TokenKind> {
    self.tokens.get(self.pos).map(|t| &t.kind)
  }
  fn peek_span(&self) -> Option<(usize, usize)> {
    self.tokens.get(self.pos).map(|t| t.span)
  }
  fn bump(&mut self) -> Option<Token> {
    if self.pos < self.tokens.len() {
      let t = self.tokens[self.pos].clone();
      self.pos += 1;
      Some(t)
    } else {
      None
    }
  }

  fn parse_expr(&mut self, terms: &mut Vec<Term>) -> Result<Option<Expr>, ParseError> {
    if self.pos >= self.tokens.len() {
      return Ok(None);
    }
    let mut left = match self.parse_and(terms)? {
      Some(e) => e,
      None => return Ok(None),
    };
    while matches!(self.peek_kind(), Some(TokenKind::Or)) {
      let _ = self.bump(); // consume OR
      let right = match self.parse_and(terms)? {
        Some(e) => e,
        None => {
          return Err(ParseError::UnexpectedToken {
            span: self.peek_span().unwrap_or((0, 0)),
          });
        }
      };
      left = match left {
        Expr::Or(mut v) => {
          v.push(right);
          Expr::Or(v)
        }
        _ => Expr::Or(vec![left, right]),
      };
    }
    Ok(Some(left))
  }

  fn parse_and(&mut self, terms: &mut Vec<Term>) -> Result<Option<Expr>, ParseError> {
    let mut factors: Vec<Expr> = Vec::new();
    if !self.can_start_pref() {
      return Ok(None);
    }
    factors.push(self.parse_pref(terms)?);
    loop {
      match self.peek_kind() {
        Some(TokenKind::And) => {
          let _ = self.bump();
          if !self.can_start_pref() {
            return Err(ParseError::UnexpectedToken {
              span: self.peek_span().unwrap_or((0, 0)),
            });
          }
          factors.push(self.parse_pref(terms)?);
        }
        Some(TokenKind::Minus)
        | Some(TokenKind::LParen)
        | Some(TokenKind::Literal(_))
        | Some(TokenKind::Phrase(_))
        | Some(TokenKind::RegexBody(_)) => {
          // implicit AND (adjacent)
          factors.push(self.parse_pref(terms)?);
        }
        _ => break,
      }
    }
    if factors.len() == 1 {
      Ok(Some(factors.remove(0)))
    } else {
      Ok(Some(Expr::And(factors)))
    }
  }

  fn can_start_pref(&self) -> bool {
    match self.peek_kind() {
      Some(TokenKind::Minus)
      | Some(TokenKind::LParen)
      | Some(TokenKind::Literal(_))
      | Some(TokenKind::Phrase(_))
      | Some(TokenKind::RegexBody(_)) => true,
      _ => false,
    }
  }

  fn parse_pref(&mut self, terms: &mut Vec<Term>) -> Result<Expr, ParseError> {
    if matches!(self.peek_kind(), Some(TokenKind::Minus)) {
      let _ = self.bump();
      let e = self.parse_atom(terms)?;
      return Ok(Expr::Not(Box::new(e)));
    }
    self.parse_atom(terms)
  }

  fn parse_atom(&mut self, terms: &mut Vec<Term>) -> Result<Expr, ParseError> {
    match self.bump() {
      Some(Token {
        kind: TokenKind::LParen,
        span,
      }) => {
        let inner = self.parse_expr(terms)?;
        match self.bump() {
          Some(Token {
            kind: TokenKind::RParen,
            ..
          }) => Ok(inner.unwrap_or(Expr::And(vec![]))),
          _ => Err(ParseError::UnbalancedParens { span }),
        }
      }
      Some(Token {
        kind: TokenKind::Literal(s),
        ..
      }) => {
        let idx = terms.len();
        terms.push(Term::Literal(s));
        Ok(Expr::Atom(idx))
      }
      Some(Token {
        kind: TokenKind::Phrase(s),
        ..
      }) => {
        let idx = terms.len();
        terms.push(Term::Phrase(s));
        Ok(Expr::Atom(idx))
      }
      Some(Token {
        kind: TokenKind::RegexBody(body),
        span,
      }) => {
        // 根据是否包含 look-around 切换正则引擎
        let has_lookaround = body.contains("(?=") || body.contains("(?!") || body.contains("(?<=") || body.contains("(?<!");
        let term = if has_lookaround {
          match fancy_regex::Regex::new(&body) {
            Ok(r) => Term::RegexFancy(r),
            Err(e) => {
              return Err(ParseError::InvalidRegex { message: e.to_string(), span });
            }
          }
        } else {
          match regex::Regex::new(&body) {
            Ok(r) => Term::RegexStd(r),
            Err(e) => {
              return Err(ParseError::InvalidRegex { message: e.to_string(), span });
            }
          }
        };
        let idx = terms.len();
        terms.push(term);
        Ok(Expr::Atom(idx))
      }
      Some(Token { span, .. }) => Err(ParseError::UnexpectedToken { span }),
      None => Err(ParseError::UnexpectedToken { span: (0, 0) }),
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  fn idx_of_literal(spec: &Query, s: &str) -> Option<usize> {
    spec.terms.iter().position(|t| matches!(t, Term::Literal(x) if x == s))
  }

  #[test]
  fn parse_precedence_and_structure() {
    let spec = parse_github_like("foo OR bar -baz").expect("parse");
    assert_eq!(spec.terms.len(), 3);
    assert!(matches!(spec.expr, Some(_)));
    match spec.expr.unwrap() {
      Expr::Or(v) => {
        assert_eq!(v.len(), 2);
        match &v[0] {
          Expr::Atom(i) => assert_eq!(*i, 0),
          _ => panic!("left not atom"),
        }
        match &v[1] {
          Expr::And(v2) => {
            assert_eq!(v2.len(), 2);
            match &v2[0] {
              Expr::Atom(i) => assert_eq!(*i, 1),
              _ => panic!("and[0]"),
            }
            match &v2[1] {
              Expr::Not(inner) => match **inner {
                Expr::Atom(i) => assert_eq!(i, 2),
                _ => panic!("not atom"),
              },
              _ => panic!("and[1] not Not"),
            }
          }
          _ => panic!("right not and"),
        }
      }
      _ => panic!("top not Or"),
    }
  }

  #[test]
  fn eval_file_boolean() {
    let spec = parse_github_like("(foo OR bar) baz").expect("parse");
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
    let spec = parse_github_like("path:logs/*.log -path:node_modules/ foo").expect("parse");
    assert!(spec.path_filter.is_allowed("logs/app/app.log"));
    assert!(!spec.path_filter.is_allowed("app/node_modules/x.js"));
    assert!(!spec.path_filter.is_allowed("logs/app/readme.md"));
    assert!(!spec.path_filter.is_allowed("src/app.log"));
  }

  #[test]
  fn highlights_positive_atoms_only() {
    let spec = parse_github_like("(\"hello world\" OR foo) /ERR\\d+/").expect("parse");
    assert!(spec.highlights.contains(&"hello world".to_string()));
    assert!(spec.highlights.contains(&"foo".to_string()));
    assert_eq!(spec.highlights.iter().filter(|s| s.starts_with("ERR")).count(), 0);
  }

  #[test]
  fn unbalanced_parens_error() {
    let err = parse_github_like("foo OR (bar").unwrap_err();
    matches!(err, ParseError::UnbalancedParens { .. });
  }

  #[test]
  fn or_must_be_uppercase() {
    let spec = parse_github_like("foo or bar").expect("parse");
    match spec.expr.unwrap() {
      Expr::And(v) => assert_eq!(v.len(), 3),
      other => panic!("期望 And 表达式包含 3 个关键字，实际为 {:?}", other),
    }
  }

  #[test]
  fn or_boundary_not_split() {
    let spec = parse_github_like("foo ORbar baz").expect("parse");
    assert_eq!(spec.terms.len(), 3);
    match spec.expr.unwrap() {
      Expr::And(v) => assert_eq!(v.len(), 3),
      other => panic!("期望 And 表达式包含 3 个关键字，实际为 {:?}", other),
    }
  }

  #[test]
  fn deep_nested_parse_and_eval() {
    let q = "((foo OR bar) AND (baz OR (qux AND -zim))) OR -(alpha OR beta)";
    let spec = parse_github_like(q).expect("parse");
    let occurs = vec![false; spec.terms.len()];
    let set = |occurs: &mut Vec<bool>, name: &str| {
      if let Some(i) = spec
        .terms
        .iter()
        .position(|t| matches!(t, Term::Literal(s) if s == name))
      {
        occurs[i] = true;
      }
    };
    let mut oc1 = occurs.clone();
    set(&mut oc1, "foo");
    set(&mut oc1, "baz");
    assert!(spec.eval_file(&oc1));
    let mut oc2 = occurs.clone();
    set(&mut oc2, "qux");
    assert!(spec.eval_file(&oc2));
    let mut oc3 = occurs.clone();
    set(&mut oc3, "alpha");
    assert!(!spec.eval_file(&oc3));
    let oc4 = occurs.clone();
    assert!(spec.eval_file(&oc4));
  }

  #[test]
  fn or_chain_with_and_precedence() {
    let spec = parse_github_like("a OR b OR (c d)").expect("parse");
    let idx = |name: &str| {
      spec
        .terms
        .iter()
        .position(|t| matches!(t, Term::Literal(s) if s == name))
        .unwrap()
    };
    let mut oc = vec![false; spec.terms.len()];
    oc[idx("c")] = true;
    assert!(!spec.eval_file(&oc));
    oc[idx("c")] = false;
    oc[idx("c")] = true;
    oc[idx("d")] = true;
    assert!(spec.eval_file(&oc));
    oc = vec![false; spec.terms.len()];
    oc[idx("b")] = true;
    assert!(spec.eval_file(&oc));
  }

  #[test]
  fn group_and_negation_inside() {
    let spec = parse_github_like("(a b) OR -(c OR d)").expect("parse");
    let idx = |name: &str| {
      spec
        .terms
        .iter()
        .position(|t| matches!(t, Term::Literal(s) if s == name))
        .unwrap()
    };

    let mut oc = vec![false; spec.terms.len()];
    oc[idx("a")] = true;
    oc[idx("b")] = true;
    assert!(spec.eval_file(&oc));

    let mut oc2 = vec![false; spec.terms.len()];
    oc2[idx("c")] = true;
    assert!(!spec.eval_file(&oc2));

    let oc3 = vec![false; spec.terms.len()];
    assert!(spec.eval_file(&oc3));
  }

  #[test]
  fn positive_indices_under_negation() {
    let spec = parse_github_like("-(a OR b) c").expect("parse");
    let idxes = spec.positive_term_indices();
    assert_eq!(idxes.len(), 1);
    let lit = match &spec.terms[idxes[0]] {
      Term::Literal(s) => s,
      _ => panic!("not literal"),
    };
    assert_eq!(lit, "c");
  }

  #[test]
  fn trailing_or_is_error() {
    let err = parse_github_like("foo OR ").unwrap_err();
    matches!(
      err,
      ParseError::UnexpectedToken { .. } | ParseError::UnbalancedParens { .. }
    );
  }

  #[test]
  fn invalid_regex_unclosed_group() {
    let err = parse_github_like("/(foo").unwrap_err();
    matches!(err, ParseError::InvalidRegex { .. });
  }

  #[test]
  fn unknown_qualifier_is_literal() {
    let spec = parse_github_like("repo:core foo").expect("parse");
    assert!(
      spec
        .terms
        .iter()
        .any(|t| matches!(t, Term::Literal(s) if s == "repo:core"))
    );
    assert!(spec.terms.iter().any(|t| matches!(t, Term::Literal(s) if s == "foo")));
  }

  #[test]
  fn path_qualifier_requires_no_whitespace() {
    let a = parse_github_like("path:logs/*.log foo").expect("parse a");
    let b = parse_github_like("path :logs/*.log foo").expect("parse b");
    // With proper qualifier, only logs/*.log under logs/ should pass
    assert!(a.path_filter.is_allowed("logs/app/app.log"));
    assert!(!a.path_filter.is_allowed("src/app.log"));
    // With whitespace after 'path', qualifier should not be recognized; all paths allowed
    assert!(b.path_filter.is_allowed("logs/app/app.log"));
    assert!(b.path_filter.is_allowed("src/app.log"));
  }

  #[test]
  fn path_qualifier_is_case_sensitive() {
    let spec = parse_github_like("PATH:logs/*.log foo").expect("parse");
    // Uppercase PATH should be treated as literal; no path restriction
    assert!(spec.path_filter.is_allowed("logs/x.log"));
    assert!(spec.path_filter.is_allowed("src/x.log"));
  }

  #[test]
  fn negative_path_excludes_vendor() {
    let spec = parse_github_like("-path:vendor/ foo").expect("parse");
    assert!(!spec.path_filter.is_allowed("lib/vendor/a.js"));
    assert!(spec.path_filter.is_allowed("lib/src/a.js"));
  }

  #[test]
  fn trailing_minus_is_error() {
    let err = parse_github_like("foo -").unwrap_err();
    matches!(err, ParseError::UnexpectedToken { .. });
  }

  #[test]
  fn span_invalid_regex() {
    // "/(foo" => token span should cover from index 0 to end (5)
    match parse_github_like("/(foo").unwrap_err() {
      ParseError::InvalidRegex { span, .. } => assert_eq!(span, (0, 5)),
      e => panic!("unexpected error: {:?}", e),
    }
  }

  #[test]
  fn span_unbalanced_parens() {
    // "foo OR (bar" => '(' at index 7
    match parse_github_like("foo OR (bar").unwrap_err() {
      ParseError::UnbalancedParens { span } => assert_eq!(span, (7, 8)),
      e => panic!("unexpected error: {:?}", e),
    }
  }

  #[test]
  fn span_unexpected_token_after_and() {
    // "foo AND )" => unexpected token is ')' at index 8
    match parse_github_like("foo AND )").unwrap_err() {
      ParseError::UnexpectedToken { span } => assert_eq!(span, (8, 9)),
      e => panic!("unexpected error: {:?}", e),
    }
  }

  #[test]
  fn span_invalid_path_pattern_from_qualifier() {
    // "path:a[" => invalid glob; qualifier token spans 0..7
    match parse_github_like("path:a[").unwrap_err() {
      ParseError::InvalidPathPattern { span, .. } => assert_eq!(span, Some((0, 7))),
      e => panic!("unexpected error: {:?}", e),
    }
  }
}
