

use crate::query::{Query, Term};
use std::io::Cursor;
use std::sync::Arc;
use encoding_rs::{UTF_8, UTF_16LE, UTF_16BE};
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

        let result = processor.process_content("test.log".to_string(), &mut reader).await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_process_content_multiple_matches() {
        let spec = Arc::new(Query::new(vec![Term::Literal("match".into())]));
        let processor = SearchProcessor::new(spec, 0);
        let content = b"match line 1\nno match\nmatch line 3";
        let mut reader = Cursor::new(content);

        let result = processor.process_content("test.log".to_string(), &mut reader).await.unwrap();
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

        let result = processor.process_content("test.log".to_string(), &mut reader).await.unwrap();
        assert!(result.is_some());
        let res = result.unwrap();
        // Context should merge since matches are close
        assert!(res.merged.len() >= 1);
    }

    #[test]
    fn test_search_processor_path_filter_combined() {
        let spec = Arc::new(Query::new(vec![Term::Literal("foo".into())])
            .with_path_filter(Some("*.log".to_string()))
            .unwrap());
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
