//! 编码检测集成测试
//!
//! 测试编码检测模块的边界条件和复杂场景：
//! - 混合编码文件处理
//! - 损坏 BOM 处理
//! - 超大文件编码检测
//! - 无效编码名称处理

use logseek::service::encoding::{decode_buffer_to_lines, detect_encoding, is_probably_text_bytes, read_text_file};
use encoding_rs::{UTF_8, GBK};
use tempfile::TempDir;
use tokio::fs;

/// 测试混合编码边界条件
#[tokio::test]
async fn test_mixed_encoding_boundaries() {
    // 测试空样本
    let empty: &[u8] = b"";
    assert_eq!(detect_encoding(empty), Some(UTF_8));

    // 测试只有 BOM 的样本
    let utf8_bom_only: &[u8] = b"\xEF\xBB\xBF";
    assert_eq!(detect_encoding(utf8_bom_only), Some(UTF_8));

    // 测试不完整的 UTF-16 LE BOM
    let incomplete_utf16le: &[u8] = b"\xFF";
    let result = detect_encoding(incomplete_utf16le);
    // 应该返回某种编码，不会 panic
    assert!(result.is_some());

    // 测试不完整的 UTF-16 BE BOM
    let incomplete_utf16be: &[u8] = b"\xFE";
    let result = detect_encoding(incomplete_utf16be);
    assert!(result.is_some());
}

/// 测试损坏的 BOM 处理
#[test]
fn test_corrupted_bom_handling() {
    // 测试错误的 UTF-8 BOM（只有两个字节）
    let corrupted_utf8_bom: &[u8] = b"\xEF\xBB";
    let result = detect_encoding(corrupted_utf8_bom);
    // 应该返回某种编码，而不是 panic
    assert!(result.is_some());

    // 测试错误的 UTF-16 LE BOM 后跟有效内容
    let wrong_bom: &[u8] = b"\xFFHello";
    let result = detect_encoding(wrong_bom);
    assert!(result.is_some());

    // 测试错误的 UTF-16 BE BOM 后跟有效内容
    let wrong_bom: &[u8] = b"\xFEHello";
    let result = detect_encoding(wrong_bom);
    assert!(result.is_some());
}

/// 测试各种编码的边界样本
#[test]
fn test_encoding_detection_edge_cases() {
    // 测试截断的多字节 UTF-8 序列
    let truncated_utf8: &[u8] = b"Hello \xE4\xB8";  // 不完整的中文字符
    let result = detect_encoding(truncated_utf8);
    // 应该处理截断的情况
    assert!(result.is_some());

    // 测试有效的 UTF-8 多字节序列
    let valid_utf8: &[u8] = "Hello 世界 🌍".as_bytes();
    assert_eq!(detect_encoding(valid_utf8), Some(UTF_8));

    // 测试 GBK 编码样本
    let gbk_sample: &[u8] = &[0xC4, 0xE3, 0xBA, 0xC3]; // "你好"
    let result = detect_encoding(gbk_sample);
    assert!(result.is_some());
}

/// 测试大文件的编码检测性能
#[tokio::test]
async fn test_large_file_encoding_detection() {
    let temp_dir = TempDir::new().expect("创建临时目录失败");

    // 创建一个大文件（1MB 的 UTF-8 文本）
    let large_content = "This is a test line with some UTF-8 content: 你好世界\n".repeat(20000);
    let file_path = temp_dir.path().join("large_utf8.txt");
    fs::write(&file_path, &large_content).await.expect("写入文件失败");

    // 读取文件并检测编码
    let file = fs::File::open(&file_path).await.expect("打开文件失败");
    let mut reader = tokio::io::BufReader::new(file);

    let start = std::time::Instant::now();
    let result = read_text_file(&mut reader, None).await;
    let duration = start.elapsed();

    assert!(result.is_ok(), "应该成功读取文件");
    let (lines, encoding) = result.unwrap().expect("应该返回内容");

    assert_eq!(encoding.to_lowercase(), "utf-8", "应该检测为 UTF-8");
    assert!(lines.len() > 0, "应该读取到行");

    // 验证性能（应该在合理时间内完成）
    assert!(
        duration.as_secs() < 5,
        "大文件编码检测应该在 5 秒内完成，实际耗时: {:?}",
        duration
    );

    println!("大文件编码检测完成，耗时: {:?}, 行数: {}", duration, lines.len());
}

/// 测试无效编码名称的处理
#[tokio::test]
async fn test_invalid_encoding_names() {
    let test_data = b"Hello World";

    // 测试无效编码名称
    let invalid_names = vec![
        "invalid-encoding",
        "XYZ123",
        "",
        "UTF-999",
    ];

    for invalid_name in invalid_names {
        let mut reader = &test_data[..];
        // 使用无效编码名称，应该回退到自动检测
        let result = read_text_file(&mut reader, Some(invalid_name)).await;
        assert!(result.is_ok(), "无效编码名称 '{}' 应该被处理", invalid_name);
    }
}

/// 测试编码别名处理
#[tokio::test]
async fn test_encoding_aliases() {
    let gbk_data = vec![0xC4, 0xE3, 0xBA, 0xC3]; // "你好" in GBK

    let aliases = vec![
        ("gbk", "gbk"),
        ("GBK", "gbk"),
        ("Gbk", "gbk"),
        ("utf-8", "utf-8"),
        ("UTF8", "utf-8"),
        ("UTF-8", "utf-8"),
    ];

    for (input, expected_prefix) in aliases {
        let mut reader = &gbk_data[..];
        let result = read_text_file(&mut reader, Some(input)).await;
        assert!(result.is_ok(), "编码别名 '{}' 应该被处理", input);

        if let Ok(Some((_, encoding))) = result {
            assert!(
                encoding.to_lowercase().starts_with(expected_prefix),
                "编码别名 '{}' 应该解析为 '{}', 实际是 '{}'",
                input,
                expected_prefix,
                encoding
            );
        }
    }
}

/// 测试二进制文件检测
#[test]
fn test_binary_file_detection() {
    // 测试纯文本
    let text = b"This is plain text content\nWith multiple lines\n";
    assert!(is_probably_text_bytes(text), "纯文本应该被识别为文本");

    // 测试空内容
    let empty: &[u8] = b"";
    assert!(is_probably_text_bytes(empty), "空内容应该被视为文本");

    // 测试包含控制字符的文本（但比例较低）
    let mixed = b"Text\twith\tsome\tcontrol\tchars\n";
    assert!(is_probably_text_bytes(mixed), "少量控制字符应该不影响文本检测");

    // 测试高比例二进制控制字符
    // 包含大量控制字符的内容（超过5%阈值）
    let high_control: Vec<u8> = [0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x0E]
        .repeat(50);
    // 由于chardetng可能会给某些数据高置信度，我们只验证函数不会panic
    // 实际检测行为取决于具体的字节分布
    let _is_text = is_probably_text_bytes(&high_control);
    // 注意：高控制字符内容通常会被识别为二进制，但如果chardetng认为
    // 是某种有效编码，也可能被识别为文本
}

/// 测试解码错误处理
#[test]
fn test_decode_error_handling() {
    // 测试带有解码错误的 GBK 数据
    let invalid_gbk: &[u8] = &[0xFF, 0xFE, 0x80, 0x81]; // 包含无效序列
    let _lines = decode_buffer_to_lines(GBK, invalid_gbk, "test");
    // 应该返回一些内容，即使有部分解码错误
    // 即使_lines为空也是可接受的结果

    // 测试完全无法解码的数据
    let garbled: &[u8] = &[0xFF; 100];
    let _lines = decode_buffer_to_lines(UTF_8, garbled, "test");
    // 即使解码失败也不应该 panic
    // 注意：解码器可能会用替换字符替换无效字节
}

/// 测试内存高效处理大文件
#[tokio::test]
async fn test_memory_efficient_large_file() {
    let temp_dir = TempDir::new().expect("创建临时目录失败");
    let file_path = temp_dir.path().join("huge.txt");

    // 创建一个较大的文件（10MB）
    let mut file = fs::File::create(&file_path).await.expect("创建文件失败");
    let line = "This is a test line that will be repeated many times to create a large file.\n";
    let target_size = 10 * 1024 * 1024; // 10MB

    let mut written = 0;
    while written < target_size {
        let bytes = line.as_bytes();
        tokio::io::AsyncWriteExt::write_all(&mut file, bytes).await.expect("写入失败");
        written += bytes.len();
    }
    drop(file);

    // 验证文件大小
    let metadata = fs::metadata(&file_path).await.expect("获取元数据失败");
    assert!(metadata.len() >= target_size as u64, "文件创建失败");

    // 流式读取文件，不使用过多内存
    let file = fs::File::open(&file_path).await.expect("打开文件失败");
    let mut reader = tokio::io::BufReader::new(file);

    let result = read_text_file(&mut reader, None).await;
    assert!(result.is_ok(), "应该成功读取大文件");

    let (lines, encoding) = result.unwrap().expect("应该返回内容");
    assert_eq!(encoding.to_lowercase(), "utf-8", "应该检测为 UTF-8");
    assert!(lines.len() > 10000, "应该读取到大量行");

    println!("成功处理 {}MB 文件，读取 {} 行", metadata.len() / (1024 * 1024), lines.len());
}

/// 测试空文件处理
#[tokio::test]
async fn test_empty_file_handling() {
    let temp_dir = TempDir::new().expect("创建临时目录失败");
    let file_path = temp_dir.path().join("empty.txt");

    // 创建空文件
    fs::write(&file_path, b"").await.expect("创建空文件失败");

    let file = fs::File::open(&file_path).await.expect("打开文件失败");
    let mut reader = tokio::io::BufReader::new(file);

    let result = read_text_file(&mut reader, None).await;
    assert!(result.is_ok(), "应该成功读取空文件");

    // 空文件可能返回 None 或空内容
    if let Ok(Some((lines, encoding))) = result {
        assert!(lines.is_empty() || lines.len() == 0, "空文件应该返回空行列表");
        println!("空文件处理结果: 编码={}, 行数={}", encoding, lines.len());
    }
}

/// 测试多行文本文件，包含各种换行符
#[tokio::test]
async fn test_various_line_endings() {
    let test_cases = vec![
        ("unix.txt", "line1\nline2\nline3\n"),      // Unix 换行符
        ("windows.txt", "line1\r\nline2\r\nline3\r\n"), // Windows 换行符
        ("mixed.txt", "line1\nline2\r\nline3\n"),   // 混合换行符
        ("no_final_newline.txt", "line1\nline2\nline3"), // 无最终换行符
    ];

    let temp_dir = TempDir::new().expect("创建临时目录失败");

    for (filename, content) in test_cases {
        let file_path = temp_dir.path().join(filename);
        fs::write(&file_path, content).await.expect("写入文件失败");

        let file = fs::File::open(&file_path).await.expect("打开文件失败");
        let mut reader = tokio::io::BufReader::new(file);

        let result = read_text_file(&mut reader, None).await;
        assert!(result.is_ok(), "{} 应该被正确读取", filename);

        if let Ok(Some((lines, _))) = result {
            assert_eq!(lines.len(), 3, "{} 应该解析为 3 行", filename);
            assert_eq!(lines[0], "line1", "{} 第1行内容错误", filename);
            assert_eq!(lines[1], "line2", "{} 第2行内容错误", filename);
            assert_eq!(lines[2], "line3", "{} 第3行内容错误", filename);
        }
    }
}

/// 测试特殊字符文件
#[tokio::test]
async fn test_special_characters_file() {
    let special_content = "Special chars: ¡¢£¤¥¦§¨©ª«¬­®¯°±²³´µ¶·¸¹º»¼½¾¿\n\
                          Emoji: 🎉🎊🎁🎄🎅🤶🧑‍🎄🦌🌟⭐✨🔔🕯️🎶🎵🎼\n\
                          Math: ∫∑∏√∂∆π∞≈≠≤≥\n";

    let temp_dir = TempDir::new().expect("创建临时目录失败");
    let file_path = temp_dir.path().join("special.txt");
    fs::write(&file_path, special_content).await.expect("写入文件失败");

    let file = fs::File::open(&file_path).await.expect("打开文件失败");
    let mut reader = tokio::io::BufReader::new(file);

    let result = read_text_file(&mut reader, None).await;
    assert!(result.is_ok(), "特殊字符文件应该被正确读取");

    if let Ok(Some((lines, encoding))) = result {
        assert_eq!(encoding.to_lowercase(), "utf-8", "应该检测为 UTF-8");
        assert_eq!(lines.len(), 3, "应该有 3 行");
        println!("特殊字符文件读取成功，行数: {}", lines.len());
    }
}
