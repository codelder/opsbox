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
    #[allow(clippy::needless_range_loop)]
    for i in s..=e {
      use std::fmt::Write as _;
      let highlighted = highlight_with_mark(&all_lines[i], keywords);
      let _ = writeln!(&mut buf, "{:>6} | {}", i + 1, highlighted);
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
  /// 文件编码名称（如 "UTF-8"、"GBK"）
  pub encoding: Option<String>,
}

pub fn render_json_chunks(
  path: &str,
  ranges: Vec<(usize, usize)>,
  all_lines: Vec<String>,
  keywords: &[String],
  encoding: Option<String>,
) -> SearchJsonResult {
  let mut chunks: Vec<JsonChunk> = Vec::with_capacity(ranges.len());
  for (s, e) in ranges.into_iter() {
    let mut lines_vec: Vec<JsonLine> = Vec::with_capacity(e.saturating_sub(s) + 1);
    #[allow(clippy::needless_range_loop)]
    for i in s..=e {
      lines_vec.push(JsonLine {
        no: i + 1,
        text: all_lines[i].clone(),
      });
    }
    chunks.push(JsonChunk {
      range: (s + 1, e + 1), // 区间也使用从 1 开始的行号
      lines: lines_vec,
    });
  }

  SearchJsonResult {
    path: path.to_string(),
    keywords: keywords.to_vec(),
    chunks,
    encoding,
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_escape_html_basic() {
    assert_eq!(escape_html("hello"), "hello");
    assert_eq!(escape_html("<script>"), "&lt;script&gt;");
    assert_eq!(escape_html("&"), "&amp;");
    assert_eq!(escape_html("\"test\""), "&quot;test&quot;");
    assert_eq!(escape_html("'test'"), "&#39;test&#39;");
  }

  #[test]
  fn test_escape_html_mixed() {
    assert_eq!(
      escape_html("<div class=\"test\" data='value' & more>"),
      "&lt;div class=&quot;test&quot; data=&#39;value&#39; &amp; more&gt;"
    );
  }

  #[test]
  fn test_highlight_with_mark_no_keywords() {
    let result = highlight_with_mark("hello world", &[]);
    assert_eq!(result, "hello world");
  }

  #[test]
  fn test_highlight_with_mark_empty_keywords() {
    let result = highlight_with_mark("hello world", &["".to_string()]);
    assert_eq!(result, "hello world");
  }

  #[test]
  fn test_highlight_with_mark_single_keyword() {
    let result = highlight_with_mark("hello world", &["world".to_string()]);
    assert_eq!(result, "hello <mark>world</mark>");
  }

  #[test]
  fn test_highlight_with_mark_multiple_keywords() {
    let result = highlight_with_mark("hello world foo bar", &["world".to_string(), "foo".to_string()]);
    assert_eq!(result, "hello <mark>world</mark> <mark>foo</mark> bar");
  }

  #[test]
  fn test_highlight_with_mark_overlapping_keywords() {
    // 更长的关键词优先
    let result = highlight_with_mark("foobar", &["foo".to_string(), "foobar".to_string()]);
    assert_eq!(result, "<mark>foobar</mark>");
  }

  #[test]
  fn test_highlight_with_mark_repeated_keywords() {
    let result = highlight_with_mark("foo foo bar", &["foo".to_string()]);
    assert_eq!(result, "<mark>foo</mark> <mark>foo</mark> bar");
  }

  #[test]
  fn test_highlight_with_mark_html_escape() {
    let result = highlight_with_mark("<script>alert('xss')</script>", &["alert".to_string()]);
    assert_eq!(result, "&lt;script&gt;<mark>alert</mark>(&#39;xss&#39;)&lt;/script&gt;");
  }

  #[test]
  fn test_render_markdown_single_range() {
    let lines = vec!["line 1".to_string(), "line 2".to_string(), "line 3".to_string()];
    let result = render_markdown("test.log", vec![(0, 1)], lines, &["line".to_string()]);

    assert!(result.contains("test.log"));
    assert!(result.contains("<pre>"));
    assert!(result.contains("</pre>"));
    assert!(result.contains("<mark>line</mark> 1"));
    assert!(result.contains("<mark>line</mark> 2"));
  }

  #[test]
  fn test_render_markdown_multiple_ranges() {
    let lines = vec![
      "line 1".to_string(),
      "line 2".to_string(),
      "line 3".to_string(),
      "line 4".to_string(),
    ];
    let result = render_markdown("test.log", vec![(0, 1), (2, 3)], lines, &[]);

    // 应该在两个范围之间包含省略号
    assert!(result.contains("..."));
  }

  #[test]
  fn test_render_json_chunks_basic() {
    let lines = vec!["line 1".to_string(), "line 2".to_string()];
    let result = render_json_chunks("test.log", vec![(0, 1)], lines, &["test".to_string()], None);

    assert_eq!(result.path, "test.log");
    assert_eq!(result.keywords, vec!["test".to_string()]);
    assert_eq!(result.chunks.len(), 1);
    assert_eq!(result.chunks[0].range, (1, 2)); // 从 1 开始
    assert_eq!(result.chunks[0].lines.len(), 2);
    assert_eq!(result.chunks[0].lines[0].no, 1);
    assert_eq!(result.chunks[0].lines[0].text, "line 1");
  }

  #[test]
  fn test_render_json_chunks_multiple_ranges() {
    let lines = vec![
      "line 1".to_string(),
      "line 2".to_string(),
      "line 3".to_string(),
      "line 4".to_string(),
    ];
    let result = render_json_chunks("test.log", vec![(0, 1), (2, 3)], lines, &["line".to_string()], None);

    assert_eq!(result.chunks.len(), 2);
    assert_eq!(result.chunks[0].range, (1, 2));
    assert_eq!(result.chunks[1].range, (3, 4));
  }

  #[test]
  fn test_render_json_chunks_empty_ranges() {
    let lines = vec!["line 1".to_string()];
    let result = render_json_chunks("test.log", vec![], lines, &[], None);

    assert_eq!(result.chunks.len(), 0);
  }
}
