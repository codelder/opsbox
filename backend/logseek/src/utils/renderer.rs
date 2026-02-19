use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct JsonLine<'a> {
  pub no: usize,
  pub text: &'a str,
}

#[derive(Debug, Serialize)]
pub struct JsonChunk<'a> {
  pub range: (usize, usize),
  pub lines: Vec<JsonLine<'a>>,
}

#[derive(Debug, Serialize)]
pub struct SearchJsonResult<'a> {
  pub path: &'a str,
  pub keywords: &'a [crate::query::KeywordHighlight], // 带类型信息的关键词列表
  pub chunks: Vec<JsonChunk<'a>>,
  /// 文件编码名称（如 "UTF-8"、"GBK"）
  pub encoding: &'a Option<String>,
}

pub fn render_json_chunks<'a>(
  path: &'a str,
  ranges: Vec<(usize, usize)>,
  all_lines: &'a [String],
  highlights_with_type: &'a [crate::query::KeywordHighlight],
  encoding: &'a Option<String>,
) -> SearchJsonResult<'a> {
  let mut chunks: Vec<JsonChunk> = Vec::with_capacity(ranges.len());
  for (s, e) in ranges.into_iter() {
    let mut lines_vec: Vec<JsonLine> = Vec::with_capacity(e.saturating_sub(s) + 1);
    #[allow(clippy::needless_range_loop)]
    for i in s..=e {
      lines_vec.push(JsonLine {
        no: i + 1,
        text: &all_lines[i],
      });
    }
    chunks.push(JsonChunk {
      range: (s + 1, e + 1), // 区间也使用从 1 开始的行号
      lines: lines_vec,
    });
  }

  SearchJsonResult {
    path,
    keywords: highlights_with_type,
    chunks,
    encoding,
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_render_json_chunks_basic() {
    let lines = vec!["line 1".to_string(), "line 2".to_string()];
    let highlights = vec![crate::query::KeywordHighlight::Literal("test".to_string())];
    let result = render_json_chunks("test.log", vec![(0, 1)], &lines, &highlights, &None);

    assert_eq!(result.path, "test.log");
    assert_eq!(result.keywords.len(), 1);
    match &result.keywords[0] {
      crate::query::KeywordHighlight::Literal(s) => assert_eq!(s, "test"),
      _ => panic!("expected Literal"),
    }
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
    let highlights = vec![crate::query::KeywordHighlight::Literal("line".to_string())];
    let result = render_json_chunks("test.log", vec![(0, 1), (2, 3)], &lines, &highlights, &None);

    assert_eq!(result.chunks.len(), 2);
    assert_eq!(result.chunks[0].range, (1, 2));
    assert_eq!(result.chunks[1].range, (3, 4));
  }

  #[test]
  fn test_render_json_chunks_empty_ranges() {
    let lines = vec!["line 1".to_string()];
    let result = render_json_chunks("test.log", vec![], &lines, &[], &None);

    assert_eq!(result.chunks.len(), 0);
  }

  #[test]
  fn test_keyword_highlight_serialization() {
    use serde_json;
    let literal = crate::query::KeywordHighlight::Literal("error".to_string());
    let phrase = crate::query::KeywordHighlight::Phrase("Error".to_string());
    let regex = crate::query::KeywordHighlight::Regex("ERR\\d+".to_string());

    let literal_json = serde_json::to_string(&literal).unwrap();
    let phrase_json = serde_json::to_string(&phrase).unwrap();
    let regex_json = serde_json::to_string(&regex).unwrap();

    assert_eq!(literal_json, r#"{"type":"literal","text":"error"}"#);
    assert_eq!(phrase_json, r#"{"type":"phrase","text":"Error"}"#);
    assert_eq!(regex_json, r#"{"type":"regex","text":"ERR\\d+"}"#);
  }
}
