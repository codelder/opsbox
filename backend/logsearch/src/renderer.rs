fn escape_html(input: &str) -> String {
  let mut escaped = String::with_capacity(input.len());
  for ch in input.chars() {
    match ch {
      '&' => escaped.push_str("&amp;"),
      '<' => escaped.push_str("&lt;"),
      '>' => escaped.push_str("&gt;"),
      '"' => escaped.push_str("&quot;"),
      '\'' => escaped.push_str("&#39;"),
      _ => escaped.push(ch),
    }
  }
  escaped
}

fn highlight_with_mark(input: &str, keywords: &[String]) -> String {
  // 过滤空关键词，避免死循环
  let non_empty: Vec<&str> = keywords.iter().map(|s| s.as_str()).filter(|s| !s.is_empty()).collect();
  if non_empty.is_empty() {
    return escape_html(input);
  }
  let mut out = String::with_capacity(input.len() + 16);
  let mut start_idx = 0usize;
  while start_idx < input.len() {
    let mut best_pos: Option<usize> = None;
    let mut best_kw: &str = "";

    for &kw in &non_empty {
      if let Some(pos_rel) = input[start_idx..].find(kw) {
        let pos_abs = start_idx + pos_rel;
        match best_pos {
          None => {
            best_pos = Some(pos_abs);
            best_kw = kw;
          }
          Some(bp) => {
            if pos_abs < bp || (pos_abs == bp && kw.len() > best_kw.len()) {
              best_pos = Some(pos_abs);
              best_kw = kw;
            }
          }
        }
      }
    }

    match best_pos {
      None => {
        out.push_str(&escape_html(&input[start_idx..]));
        break;
      }
      Some(pos) => {
        out.push_str(&escape_html(&input[start_idx..pos]));
        out.push_str("<mark>");
        let end = pos + best_kw.len();
        out.push_str(&escape_html(&input[pos..end]));
        out.push_str("</mark>");
        start_idx = end;
      }
    }
  }
  out
}

pub fn render_markdown(path: &str, ranges: Vec<(usize, usize)>, all_lines: Vec<String>, keywords: &[String]) -> String {
  let mut buf = String::new();
  buf.push_str(&format!("\n## 文件 s3://{}/{}::{}\n\n", "test", "codeler.tar.gz", path));
  buf.push_str("<pre>\n");
  for (chunk_idx, (s, e)) in ranges.iter().copied().enumerate() {
    for i in s..=e {
      use std::fmt::Write as _;
      let highlighted = highlight_with_mark(&all_lines[i], &keywords);
      let _ = write!(&mut buf, "{:>6} | {}\n", i + 1, highlighted);
    }
    if chunk_idx + 1 < ranges.len() {
      buf.push_str("       ...\n");
    }
  }
  buf.push_str("</pre>\n\n");
  buf
}

use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct JsonLine {
  pub no: usize,
  pub text: String,
}

#[derive(Debug, Serialize)]
pub struct JsonChunk {
  pub range: (usize, usize),
  pub lines: Vec<JsonLine>,
}

#[derive(Debug, Serialize)]
pub struct SearchJsonResult {
  pub path: String,
  pub keywords: Vec<String>,
  pub chunks: Vec<JsonChunk>,
}

pub fn render_json_chunks(
  path: &str,
  ranges: Vec<(usize, usize)>,
  all_lines: Vec<String>,
  keywords: &[String],
) -> SearchJsonResult {
  let mut chunks: Vec<JsonChunk> = Vec::with_capacity(ranges.len());
  for (s, e) in ranges.into_iter() {
    let mut lines_vec: Vec<JsonLine> = Vec::with_capacity(e.saturating_sub(s) + 1);
    for i in s..=e {
      lines_vec.push(JsonLine {
        no: i + 1,
        text: all_lines[i].clone(),
      });
    }
    chunks.push(JsonChunk {
      range: (s + 1, e + 1), // use 1-based line numbers for range as well
      lines: lines_vec,
    });
  }

  SearchJsonResult {
    path: path.to_string(),
    keywords: keywords.to_vec(),
    chunks,
  }
}
