

use crate::query::{Query, Term};
use std::io::Cursor;
use std::sync::Arc;
use encoding_rs::{UTF_8, UTF_16LE};
// Re-import symbols to ensure they are available
use super::SearchProcessor;
// Note: detect_encoding is private in parent.
// We will need parent to expose it or we can't test it directly here.
// Let's comment out detect_encoding tests if we can't change visibility easily,
// OR better yet, let's just make `detect_encoding` pub(super) in search.rs.
// For now, I will assume I will fix search.rs visibility.
use super::detect_encoding;


    #[test]
    fn test_search_processor_should_process_path() {
        let spec = Arc::new(Query::new(vec![Term::Literal("foo".into())])
            .with_path_filter(Some("!*.txt".to_string())).unwrap());
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

        let result = processor.process_content("test.log".to_string(), &mut reader).await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_process_content_literal_match() {
        let spec = Arc::new(Query::new(vec![Term::Literal("line 2".into())]));
        let processor = SearchProcessor::new(spec, 0);

        let content = b"line 1\nline 2\nline 3";
        let mut reader = Cursor::new(content);

        let result = processor.process_content("test.log".to_string(), &mut reader).await.unwrap();
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

        let result = processor.process_content("test.log".to_string(), &mut reader).await.unwrap();
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
            re
        }]));
        let processor = SearchProcessor::new(spec, 0);
        let content = b"line 1\nfoo\nline 2";
        let mut reader = Cursor::new(content);

        let result = processor.process_content("test.log".to_string(), &mut reader).await.unwrap();
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

    #[tokio::test]
    async fn test_grep_reader_blocking_gzip() {
        use flate2::write::GzEncoder;
        use flate2::Compression;
        use std::io::Write;

        // 创建一个压缩的 Gzip 文件内容
        let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(b"line 1\nmatch this gzip\nline 3").unwrap();
        let compressed_data = encoder.finish().unwrap();

        let tmp_file = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(tmp_file.path(), &compressed_data).unwrap();

        let query = Query::parse_github_like("match").unwrap();

        let result = SearchProcessor::grep_reader_blocking_gzip(
            tmp_file.path().to_str().unwrap(),
            &query,
            0,
            None
        ).expect("Search Gzip failed");

        assert!(result.is_some());
        let res = result.unwrap();
        assert!(res.lines.iter().any(|l| l.contains("match this gzip")));
        assert_eq!(res.merged, vec![(1, 1)]);
    }
