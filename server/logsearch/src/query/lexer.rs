use super::ParseError;
use std::iter::Peekable;

#[derive(Debug, Clone)]
pub struct Token {
  pub kind: TokenKind,
  pub span: (usize, usize), // 字符偏移范围：[start, end)
}

#[derive(Debug, Clone)]
pub enum TokenKind {
  QualifierPath { negative: bool, pattern: String },
  Or,
  And,
  LParen,
  RParen,
  Minus,
  Literal(String),
  Phrase(String),
  RegexBody(String),
}

fn is_boundary(next: Option<char>) -> bool {
  matches!(
    next,
    None | Some(' ') | Some('\t') | Some('\n') | Some('\r') | Some(')')
  )
}

fn eat_exact(it: &mut Peekable<std::str::Chars<'_>>, pos: &mut usize, s: &str) -> bool {
  let mut backup = it.clone();
  for ch in s.chars() {
    match backup.peek().copied() {
      Some(c) if c == ch => {
        backup.next();
      }
      _ => return false,
    }
  }
  *it = backup;
  *pos += s.chars().count();
  true
}

fn eat_keyword_with_boundary(it: &mut Peekable<std::str::Chars<'_>>, pos: &mut usize, kw: &str) -> bool {
  let mut backup = it.clone();
  for ch in kw.chars() {
    match backup.peek().copied() {
      Some(c) if c == ch => {
        backup.next();
      }
      _ => return false,
    }
  }
  if is_boundary(backup.peek().copied()) {
    *it = backup;
    *pos += kw.chars().count();
    true
  } else {
    false
  }
}

fn eat_ws(it: &mut Peekable<std::str::Chars<'_>>, pos: &mut usize) {
  while matches!(it.peek(), Some(c) if c.is_whitespace()) {
    it.next();
    *pos += 1;
  }
}

fn read_until_ws_paren(it: &mut Peekable<std::str::Chars<'_>>, pos: &mut usize) -> String {
  let mut s = String::new();
  while let Some(&c) = it.peek() {
    if c.is_whitespace() || c == '(' || c == ')' {
      break;
    }
    s.push(c);
    it.next();
    *pos += 1;
  }
  s
}

fn read_quoted(it: &mut Peekable<std::str::Chars<'_>>, pos: &mut usize) -> String {
  let mut s = String::new();
  while let Some(c) = it.next() {
    *pos += 1;
    if c == '"' {
      break;
    }
    s.push(c);
  }
  s
}

fn read_regex_body(it: &mut Peekable<std::str::Chars<'_>>, pos: &mut usize) -> String {
  // 保留反斜杠本身，仅在遇到 \/ 时让分隔符被转义
  let mut s = String::new();
  let mut escaped = false;
  while let Some(c) = it.next() {
    *pos += 1;
    if escaped {
      // 上一个字符是 '\\'，当前字符无论是什么都原样加入
      s.push(c);
      escaped = false;
      continue;
    }
    if c == '\\' {
      // 记录反斜杠本身，再标记转义状态
      s.push('\\');
      escaped = true;
      continue;
    }
    if c == '/' {
      // 非转义的分隔符，结束
      break;
    }
    s.push(c);
  }
  s
}

fn read_path_pattern(it: &mut Peekable<std::str::Chars<'_>>, pos: &mut usize) -> String {
  let mut s = String::new();
  while let Some(&c) = it.peek() {
    if c.is_whitespace() {
      break;
    }
    s.push(c);
    it.next();
    *pos += 1;
  }
  s
}

pub fn tokenize(input: &str) -> Result<Vec<Token>, ParseError> {
  let mut it = input.chars().peekable();
  let mut tokens = Vec::new();
  let mut pos: usize = 0;

  while it.peek().is_some() {
    eat_ws(&mut it, &mut pos);
    if it.peek().is_none() {
      break;
    }

    // 单字符括号
    if let Some('(') = it.peek().copied() {
      let start = pos;
      let _ = eat_exact(&mut it, &mut pos, "(");
      tokens.push(Token {
        kind: TokenKind::LParen,
        span: (start, pos),
      });
      continue;
    }
    if let Some(')') = it.peek().copied() {
      let start = pos;
      let _ = eat_exact(&mut it, &mut pos, ")");
      tokens.push(Token {
        kind: TokenKind::RParen,
        span: (start, pos),
      });
      continue;
    }

    // -path: 路径限定符或一元负号
    if let Some('-') = it.peek().copied() {
      let start = pos;
      if eat_exact(&mut it, &mut pos, "-path:") {
        let pat = read_path_pattern(&mut it, &mut pos);
        tokens.push(Token {
          kind: TokenKind::QualifierPath {
            negative: true,
            pattern: pat,
          },
          span: (start, pos),
        });
        continue;
      } else {
        let _ = eat_exact(&mut it, &mut pos, "-");
        tokens.push(Token {
          kind: TokenKind::Minus,
          span: (start, pos),
        });
        continue;
      }
    }

    // path: 路径限定符
    if let Some('p') = it.peek().copied() {
      let start = pos;
      if eat_exact(&mut it, &mut pos, "path:") {
        let pat = read_path_pattern(&mut it, &mut pos);
        tokens.push(Token {
          kind: TokenKind::QualifierPath {
            negative: false,
            pattern: pat,
          },
          span: (start, pos),
        });
        continue;
      }
    }

    // OR / AND（需大写）并进行边界判断
    if let Some('O') = it.peek().copied() {
      let start = pos;
      if eat_keyword_with_boundary(&mut it, &mut pos, "OR") {
        tokens.push(Token {
          kind: TokenKind::Or,
          span: (start, pos),
        });
        continue;
      }
    }
    if let Some('A') = it.peek().copied() {
      let start = pos;
      if eat_keyword_with_boundary(&mut it, &mut pos, "AND") {
        tokens.push(Token {
          kind: TokenKind::And,
          span: (start, pos),
        });
        continue;
      }
    }

    // 短语："..."
    if let Some('"') = it.peek().copied() {
      let start = pos;
      let _ = eat_exact(&mut it, &mut pos, "\"");
      let s = read_quoted(&mut it, &mut pos);
      tokens.push(Token {
        kind: TokenKind::Phrase(s),
        span: (start, pos),
      });
      continue;
    }

    // 正则：/.../
    if let Some('/') = it.peek().copied() {
      let start = pos;
      let _ = eat_exact(&mut it, &mut pos, "/");
      let body = read_regex_body(&mut it, &mut pos);
      tokens.push(Token {
        kind: TokenKind::RegexBody(body),
        span: (start, pos),
      });
      continue;
    }

    // 字面量
    let start = pos;
    let lit = read_until_ws_paren(&mut it, &mut pos);
    if !lit.is_empty() {
      tokens.push(Token {
        kind: TokenKind::Literal(lit),
        span: (start, pos),
      });
      continue;
    }

    it.next();
    pos += 1;
  }

  Ok(tokens)
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn tokenize_parens_minus_or_and_terms() {
    let input = "(foo OR \"bar baz\") -/ERR\\d+/";
    let toks = tokenize(input).expect("tokenize");
    assert!(matches!(toks[0].kind, TokenKind::LParen));
    assert!(matches!(toks[1].kind, TokenKind::Literal(ref s) if s == "foo"));
    assert!(matches!(toks[2].kind, TokenKind::Or));
    assert!(matches!(toks[3].kind, TokenKind::Phrase(ref s) if s == "bar baz"));
    assert!(matches!(toks[4].kind, TokenKind::RParen));
    assert!(matches!(toks[5].kind, TokenKind::Minus));
    assert!(matches!(toks[6].kind, TokenKind::RegexBody(_)));
  }

  #[test]
  fn tokenize_group_then_term() {
    let input = "(foo OR bar) baz";
    let toks = tokenize(input).expect("tokenize");
    assert_eq!(toks.len(), 6);
    assert!(matches!(toks[0].kind, TokenKind::LParen));
    assert!(matches!(toks[1].kind, TokenKind::Literal(ref s) if s == "foo"));
    assert!(matches!(toks[2].kind, TokenKind::Or));
    assert!(matches!(toks[3].kind, TokenKind::Literal(ref s) if s == "bar"));
    assert!(matches!(toks[4].kind, TokenKind::RParen));
    assert!(matches!(toks[5].kind, TokenKind::Literal(ref s) if s == "baz"));
  }
}
