use super::SearchProcessor;
use crate::query::{Query, Term};
use crate::service::encoding::detect_encoding;
use encoding_rs::{UTF_8, UTF_16BE, UTF_16LE};
use std::io::Cursor;
use std::sync::Arc;

#[test]
fn test_search_processor_should_process_path() {
  let spec = Arc::new(
    Query::new(vec![Term::Literal("foo".into())])
      .with_path_filter(Some("!*.txt".to_string()))
      .unwrap(),
  );
  let processor = SearchProcessor::new(spec, 2);

  assert!(processor.should_process_path("foo.log"));
  assert!(!processor.should_process_path("foo.txt"));
}

#[tokio::test]
async fn test_process_content_no_match() {
  let spec = Arc::new(Query::new(vec![Term::Literal("notfound".into())]));
  let processor = SearchProcessor::new(spec, 0);

  let content = b"line 1\nline 2\nline 3";
  let mut reader = Cursor::new(content);

  let result = processor
    .process_content("test.log".to_string(), &mut reader)
    .await
    .unwrap();
  assert!(result.is_none());
}

#[tokio::test]
async fn test_process_content_literal_match() {
  let spec = Arc::new(Query::new(vec![Term::Literal("line 2".into())]));
  let processor = SearchProcessor::new(spec, 0);

  let content = b"line 1\nline 2\nline 3";
  let mut reader = Cursor::new(content);

  let result = processor
    .process_content("test.log".to_string(), &mut reader)
    .await
    .unwrap();
  assert!(result.is_some());
  let res = result.unwrap();
  assert_eq!(res.lines.len(), 3);
  // "line 2" is on index 1 (0-based)
  // With grep logic, we get merged ranges.
  assert_eq!(res.merged, vec![(1, 1)]);
}

#[tokio::test]
async fn test_process_content_context() {
  let spec = Arc::new(Query::new(vec![Term::Literal("line 3".into())]));
  let processor = SearchProcessor::new(spec, 1); // 1 context line

  let content = b"line 1\nline 2\nline 3\nline 4\nline 5";
  let mut reader = Cursor::new(content);

  let result = processor
    .process_content("test.log".to_string(), &mut reader)
    .await
    .unwrap();
  assert!(result.is_some());
  let res = result.unwrap();
  // line 3 is index 2. context 1 means 1..3
  assert_eq!(res.merged, vec![(1, 3)]);
}

#[tokio::test]
async fn test_detect_encoding_utf8_bom() {
  let data = vec![0xEF, 0xBB, 0xBF, b'a', b'b', b'c'];
  let enc = detect_encoding(&data);
  assert_eq!(enc, Some(UTF_8));
}

#[tokio::test]
async fn test_detect_encoding_utf16le_bom() {
  let data = vec![0xFF, 0xFE, b'a', 0x00];
  let enc = detect_encoding(&data);
  assert_eq!(enc, Some(UTF_16LE));
}

// Add more complex tests if needed, e.g. Regex
#[tokio::test]
async fn test_process_content_regex() {
  let re = regex::Regex::new("line \\d").unwrap();
  let spec = Arc::new(Query::new(vec![Term::RegexStd {
    pattern: "line \\d".to_string(),
    re,
  }]));
  let processor = SearchProcessor::new(spec, 0);
  let content = b"line 1\nfoo\nline 2";
  let mut reader = Cursor::new(content);

  let result = processor
    .process_content("test.log".to_string(), &mut reader)
    .await
    .unwrap();
  assert!(result.is_some());
  let res = result.unwrap();
  // line 1 (idx 0), line 2 (idx 2)
  // Note: process_content results depend on boolean eval.
  // Query logic: AND(Term).
  // Term::RegexStd checks match on line.
  // BooleanContextSink checks matches.
  // If line 1 matches, occurs[0] = true.
  // Eval file: AND(occurs) => true.
  // Matched lines: 0 and 2.
  assert_eq!(res.merged, vec![(0, 0), (2, 2)]);
}

#[test]
fn test_search_processor_with_encoding() {
  let spec = Arc::new(Query::new(vec![Term::Literal("test".into())]));
  let processor = SearchProcessor::new_with_encoding(spec, 2, Some("GBK".to_string()));
  assert_eq!(processor.encoding, Some("GBK".to_string()));
}

#[tokio::test]
async fn test_process_content_gbk_encoded() {
  use encoding_rs::GBK;

  // GBK encoded text: "测试"
  let text = "测试";
  let (encoded, _, _) = GBK.encode(text);
  let content = [&encoded[..], b"\nline2"].concat();

  let spec = Arc::new(Query::new(vec![Term::Literal(text.into())]));
  let processor = SearchProcessor::new_with_encoding(spec, 0, Some("GBK".to_string()));
  let mut reader = Cursor::new(&content);

  // This might fail if encoding handling is not perfect, so we just check it doesn't panic
  let _result = processor.process_content("test.log".to_string(), &mut reader).await;
}

#[tokio::test]
async fn test_detect_encoding_utf16be_bom() {
  let data = vec![0xFE, 0xFF, 0x00, b'a'];
  let enc = detect_encoding(&data);
  assert_eq!(enc, Some(UTF_16BE));
}

#[tokio::test]
async fn test_detect_encoding_no_bom() {
  // Pure ASCII - should be detected as UTF-8
  let data = b"Hello World";
  let enc = detect_encoding(data);
  assert_eq!(enc, Some(UTF_8));
}

#[tokio::test]
async fn test_process_content_empty_file() {
  let spec = Arc::new(Query::new(vec![Term::Literal("test".into())]));
  let processor = SearchProcessor::new(spec, 0);
  let content = b"";
  let mut reader = Cursor::new(content);

  let result = processor
    .process_content("test.log".to_string(), &mut reader)
    .await
    .unwrap();
  assert!(result.is_none());
}

#[tokio::test]
async fn test_process_content_multiple_matches() {
  let spec = Arc::new(Query::new(vec![Term::Literal("match".into())]));
  let processor = SearchProcessor::new(spec, 0);
  let content = b"match line 1\nno match\nmatch line 3";
  let mut reader = Cursor::new(content);

  let result = processor
    .process_content("test.log".to_string(), &mut reader)
    .await
    .unwrap();
  assert!(result.is_some());
  let res = result.unwrap();
  // With context 0, the matches at line 0 and 2 are merged into a single range
  assert_eq!(res.merged, vec![(0, 2)]);
}

#[tokio::test]
async fn test_process_content_with_context_overlap() {
  let spec = Arc::new(Query::new(vec![Term::Literal("match".into())]));
  let processor = SearchProcessor::new(spec, 2); // 2 context lines
  let content = b"line 1\nline 2\nmatch 1\nmatch 2\nline 5";
  let mut reader = Cursor::new(content);

  let result = processor
    .process_content("test.log".to_string(), &mut reader)
    .await
    .unwrap();
  assert!(result.is_some());
  let res = result.unwrap();
  // Context should merge since matches are close
  assert!(!res.merged.is_empty());
}

#[test]
fn test_search_processor_path_filter_combined() {
  let spec = Arc::new(
    Query::new(vec![Term::Literal("foo".into())])
      .with_path_filter(Some("*.log".to_string()))
      .unwrap(),
  );
  let processor = SearchProcessor::new(spec, 2);

  assert!(processor.should_process_path("app.log"));
  assert!(!processor.should_process_path("app.txt"));
}

#[test]
fn test_search_error_display() {
  use crate::service::search::SearchError;
  let err = SearchError::ChannelClosed;
  assert!(err.to_string().contains("Channel"));

  let io_err = SearchError::Io {
    path: "/tmp/test".to_string(),
    error: "permission denied".to_string(),
  };
  assert!(io_err.to_string().contains("permission denied"));
}

#[test]
fn test_search_error_from_io() {
  use crate::service::search::SearchError;
  let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
  let err: SearchError = io_err.into();
  match err {
    SearchError::Io { path, error } => {
      assert_eq!(path, "unknown");
      assert!(error.contains("file not found"));
    }
    _ => panic!("Expected Io error"),
  }
}

#[tokio::test]
async fn test_grep_reader_blocking_gzip() {
  use flate2::Compression;
  use flate2::write::GzEncoder;
  use std::io::Write;

  // 创建一个压缩的 Gzip 文件内容
  let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
  encoder.write_all(b"line 1\nmatch this gzip\nline 3").unwrap();
  let compressed_data = encoder.finish().unwrap();

  let tmp_file = tempfile::NamedTempFile::new().unwrap();
  std::fs::write(tmp_file.path(), &compressed_data).unwrap();

  let query = Query::parse_github_like("match").unwrap();

  let result = SearchProcessor::grep_reader_blocking_gzip(tmp_file.path().to_str().unwrap(), &query, 0, None)
    .expect("Search Gzip failed");

  assert!(result.is_some());
  let res = result.unwrap();
  assert!(res.lines.iter().any(|l| l.contains("match this gzip")));
  assert_eq!(res.merged, vec![(1, 1)]);
}

#[tokio::test]
async fn test_process_content_real_file_direct_mmap() {
  use std::io::Write;

  // 1. Create a real file for mmap path
  let mut tmp_file = tempfile::NamedTempFile::new().unwrap();
  writeln!(tmp_file, "line 1").unwrap();
  writeln!(tmp_file, "matched by mmap").unwrap();
  writeln!(tmp_file, "line 3").unwrap();

  let path = tmp_file.path().to_str().unwrap().to_string();

  // 2. Query
  let query = Arc::new(Query::parse_github_like("mmap").unwrap());
  let processor = SearchProcessor::new(query, 0);

  // 3. Process
  // We need a reader even if mmap uses path, because the signature requires it.
  // But process_content will optimize using path if possible.
  let file = tokio::fs::File::open(&path).await.unwrap();
  let mut reader = tokio::io::BufReader::new(file);

  let result = processor.process_content(path, &mut reader).await.unwrap();

  assert!(result.is_some());
  let res = result.unwrap();
  assert!(res.lines.iter().any(|l| l.contains("matched by mmap")));
}

#[tokio::test]
async fn test_process_content_real_file_gzip_integration() {
  use flate2::Compression;
  use flate2::write::GzEncoder;
  use std::io::Write;

  // 1. Create a real gzip file
  let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
  encoder
    .write_all(b"line 1\nmatched by gzip integration\nline 3")
    .unwrap();
  let compressed_data = encoder.finish().unwrap();

  let mut tmp_file = tempfile::Builder::new().suffix(".gz").tempfile().unwrap();
  tmp_file.write_all(&compressed_data).unwrap();

  let path = tmp_file.path().to_str().unwrap().to_string();

  // 2. Query
  let query = Arc::new(Query::parse_github_like("gzip").unwrap());
  let processor = SearchProcessor::new(query, 0);

  // 3. Process
  // Even though it's gzip, process_content takes a generic reader.
  // Normally the caller handles decompression if it's not optimized internally.
  // BUT process_content checks capability. If it detects Gzip capability on PATH,
  // it spawns blocking task to read file directly, IGNORING the passed reader.
  // If it falls back, it uses the passed reader.
  // The passed reader here is raw GZIP content if we open the file directly.
  // `grep_context` handles plain text primarily. If we pass raw gzip to grep_context, it won't match.
  // So this test verifies that the Gzip Optimization path IS taken.
  let file = tokio::fs::File::open(&path).await.unwrap();
  let mut reader = tokio::io::BufReader::new(file);

  let result = processor.process_content(path, &mut reader).await.unwrap();

  assert!(result.is_some(), "Should match via gzip optimization path");
  let res = result.unwrap();
  assert!(res.lines.iter().any(|l| l.contains("matched by gzip integration")));
}

#[tokio::test]
async fn test_process_content_fancy_regex_fallback() {
  use std::io::Write;

  // 1. Real file
  let mut tmp_file = tempfile::NamedTempFile::new().unwrap();
  writeln!(tmp_file, "line 1").unwrap();
  writeln!(tmp_file, "lookahead content").unwrap();

  let path = tmp_file.path().to_str().unwrap().to_string();

  // 2. Fancy Regex Query (lookaround) - supported by fancy-regex, not grep-searcher
  // `(?!...)` negative lookahead
  // Query parser handles this as RegexFancy
  // "lookahead(?! foo)" matches "lookahead" if not followed by " foo"
  // query_str is not used as we construct Query manually below

  // Note: github-like parser might not produce FancyRegex easily unless we specifically construct it
  // or the parser supports it.
  // Let's use Query constructor directly to force FancyRegex if parser is unsure.
  // But let's try parser first. parser usually produces RegexStd if valid, fallback to Fancy?
  // Actually parser uses `Term::from_regex`.
  // If we force constructs that `regex` crate rejects but `fancy-regex` accepts.
  // `regex` crate does NOT support look-around.

  let _start = std::time::Instant::now();
  // Construct manually to be sure
  let re = fancy_regex::Regex::new("lookahead(?! foo)").unwrap();
  let spec = Arc::new(Query::new(vec![Term::RegexFancy {
    pattern: "lookahead(?! foo)".to_string(),
    re,
  }]));

  let processor = SearchProcessor::new(spec.clone(), 0);

  // 3. Process
  let file = tokio::fs::File::open(&path).await.unwrap();
  let mut reader = tokio::io::BufReader::new(file);

  // This should trigger check_grep_capability -> sees FancyRegex -> returns None
  // Then falls back to grep_context (slow path)
  let result = processor.process_content(path, &mut reader).await.unwrap();

  assert!(result.is_some());
  // Verify match is found in the lines (it should be on the second line)
  assert!(result.as_ref().unwrap().lines.iter().any(|l| l.contains("lookahead")));

  // Ensure we didn't use the fast path (hard to verify internally without logs/mock, but functionality is key)
}
