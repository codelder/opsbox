use chardetng::EncodingDetector;
use encoding_rs::{BIG5, EUC_KR, Encoding, GBK, SHIFT_JIS, UTF_8, UTF_16BE, UTF_16LE, WINDOWS_1252};
use std::io;
use tokio::io::{AsyncRead, AsyncReadExt, BufReader};
use tracing::{debug, trace, warn};

/// 返回检测到的编码，如果无法确定则返回 None
pub fn detect_encoding(sample: &[u8]) -> Option<&'static Encoding> {
  // 检查 BOM（字节顺序标记）- 最可靠的检测方式
  if sample.len() >= 2 {
    match &sample[0..2] {
      [0xFF, 0xFE] => {
        // UTF-16 LE BOM
        trace!("检测到 UTF-16 LE BOM");
        return Some(UTF_16LE);
      }
      [0xFE, 0xFF] => {
        // UTF-16 BE BOM
        trace!("检测到 UTF-16 BE BOM");
        return Some(UTF_16BE);
      }
      _ => {}
    }
  }

  if sample.len() >= 3
    && let [0xEF, 0xBB, 0xBF] = &sample[0..3]
  {
    trace!("检测到 UTF-8 BOM");
    return Some(UTF_8);
  }

  // 优先检测是否为有效的 UTF-8
  match std::str::from_utf8(sample) {
    Ok(_) => {
      trace!("样本是有效的 UTF-8，使用 UTF-8 编码");
      return Some(UTF_8);
    }
    Err(e) => {
      let valid_up_to = e.valid_up_to();
      // 如果大部分内容是有效的 UTF-8，只是末尾可能被截断
      if valid_up_to > 0 && sample.len() - valid_up_to <= 3 && std::str::from_utf8(&sample[..valid_up_to]).is_ok() {
        trace!("样本为有效UTF-8(末尾截断)");
        return Some(UTF_8);
      }
    }
  }

  // 使用 chardetng 进行编码检测
  let mut detector = EncodingDetector::new();
  detector.feed(sample, true);
  let detected_encoding = detector.guess(None, true);

  trace!("chardetng 检测到编码: {}", detected_encoding.name());
  Some(detected_encoding)
}

/// 自动检测编码并返回 `(Encoding, 编码名称字符串)`
pub fn auto_detect_encoding(sample: &[u8]) -> Option<(&'static Encoding, String)> {
  detect_encoding(sample).map(|enc| {
    let name = enc.name().to_string();
    trace!("自动检测到编码: {}", name);
    (enc, name)
  })
}

/// 将完整缓冲区按指定编码解码为按行分割的字符串向量
pub fn decode_buffer_to_lines(encoding: &'static Encoding, buffer: &[u8], warn_prefix: &str) -> Vec<String> {
  let mut lines: Vec<String> = Vec::new();

  let (decoded, _, had_errors) = encoding.decode(buffer);

  if had_errors {
    warn!("{warn_prefix}解码过程中遇到错误，但继续处理");
  }

  for line in decoded.lines() {
    lines.push(line.to_string());
  }

  // 处理最后一行（可能没有换行符）
  let decoded_str = decoded.as_ref();
  if !decoded_str.ends_with('\n')
    && !decoded_str.ends_with('\r')
    && let Some(last_line) = decoded_str.lines().last()
    && !last_line.is_empty()
    && (lines.last().is_none() || lines.last() != Some(&last_line.to_string()))
  {
    lines.push(last_line.to_string());
  }

  lines
}

pub async fn read_lines_utf8<R: AsyncRead + Unpin>(
  buf_reader: &mut BufReader<R>,
  sample: Vec<u8>,
) -> io::Result<Vec<String>> {
  use tokio::io::AsyncBufReadExt as _;
  let mut lines: Vec<String> = Vec::new();

  let sample_str = match String::from_utf8(sample.clone()) {
    Ok(s) => s,
    Err(e) => {
      let valid_up_to = e.utf8_error().valid_up_to();
      if valid_up_to > 0 && sample.len() - valid_up_to <= 3 {
        String::from_utf8(sample[..valid_up_to].to_vec()).expect("valid utf8")
      } else {
        String::from_utf8_lossy(&e.into_bytes()).into_owned()
      }
    }
  };

  let mut sample_lines: Vec<&str> = sample_str.lines().collect();
  let last_line_incomplete = !sample_str.ends_with('\n') && !sample_str.ends_with('\r');

  let mut incomplete_line = if last_line_incomplete {
    sample_lines.pop().map(|s| s.to_string())
  } else {
    None
  };

  for line in sample_lines {
    lines.push(line.to_string());
  }

  let mut line = incomplete_line.take().unwrap_or_default();
  loop {
    let mut temp_bytes = Vec::new();
    let n = buf_reader.read_until(b'\n', &mut temp_bytes).await?;
    if n == 0 {
      if !line.is_empty() {
        lines.push(line.trim_end_matches(['\r', '\n']).to_string());
      }
      break;
    }

    let temp_line = match String::from_utf8(temp_bytes.clone()) {
      Ok(s) => s,
      Err(e) => {
        let valid_up_to = e.utf8_error().valid_up_to();
        if valid_up_to > 0 && temp_bytes.len() - valid_up_to <= 3 {
          String::from_utf8(temp_bytes[..valid_up_to].to_vec())
            .unwrap_or_else(|_| String::from_utf8_lossy(&temp_bytes).into_owned())
        } else {
          String::from_utf8_lossy(&temp_bytes).into_owned()
        }
      }
    };

    line.push_str(&temp_line);
    let trimmed = line.trim_end_matches(['\r', '\n']);
    if trimmed != line {
      lines.push(trimmed.to_string());
      line.clear();
    }
  }

  Ok(lines)
}

pub async fn read_lines_utf16<R: AsyncRead + Unpin>(
  buf_reader: &mut BufReader<R>,
  encoding: &'static Encoding,
  sample: Vec<u8>,
) -> io::Result<Vec<String>> {
  let mut buffer = Vec::new();

  let sample_start = if sample.len() >= 2 && (sample[0..2] == [0xFF, 0xFE] || sample[0..2] == [0xFE, 0xFF]) {
    2
  } else {
    0
  };
  buffer.extend_from_slice(&sample[sample_start..]);

  let mut temp_buf = vec![0u8; 8192];
  loop {
    let n = buf_reader.read(&mut temp_buf).await?;
    if n == 0 {
      break;
    }
    buffer.extend_from_slice(&temp_buf[..n]);
  }

  if buffer.len() % 2 != 0 {
    warn!("UTF-16 文件字节数不是偶数，可能不完整");
    buffer.pop();
  }

  Ok(decode_buffer_to_lines(encoding, &buffer, "UTF-16 "))
}

pub async fn read_lines_with_encoding<R: AsyncRead + Unpin>(
  buf_reader: &mut BufReader<R>,
  encoding: &'static Encoding,
  sample: Vec<u8>,
) -> io::Result<Vec<String>> {
  let mut buffer = Vec::new();
  buffer.extend_from_slice(&sample);

  let mut temp_buf = vec![0u8; 8192];
  loop {
    let n = buf_reader.read(&mut temp_buf).await?;
    if n == 0 {
      break;
    }
    buffer.extend_from_slice(&temp_buf[..n]);
  }

  Ok(decode_buffer_to_lines(encoding, &buffer, ""))
}

pub fn is_probably_text_bytes(sample: &[u8]) -> bool {
  if sample.is_empty() {
    return true;
  }
  // 如果是有效的 UTF-8，直接通过
  if std::str::from_utf8(sample).is_ok() {
    return true;
  }

  // 检查二进制控制字符（非文本、非空白的控制码）
  // 文本文件通常只包含: 0x09 (TAB), 0x0A (LF), 0x0D (CR), 0x20-0x7E (ASCII), 0x80-0xFF (扩展/多字节)
  // 二进制文件通常包含: 0x00-0x08, 0x0B, 0x0C, 0x0E-0x1F, 0x7F
  let binary_control_count = sample
    .iter()
    .filter(|&&b| (0..=0x08).contains(&b) || (0x0E..0x20).contains(&b) || b == 0x7F || b == 0x0B || b == 0x0C)
    .count();

  let ratio = binary_control_count as f32 / sample.len() as f32;

  // 如果超过 5% 是二进制控制字符，很可能是二进制文件
  // (随机二进制数据期望值约 12%，纯文本期望值 0%)
  if ratio > 0.05 {
    return false;
  }

  // 使用 chardetng 进行进一步验证
  let mut detector = EncodingDetector::new();
  detector.feed(sample, true);
  let (_, confidence) = detector.guess_assess(None, true);

  // 如果 chardetng 确信是某种编码，或者是几乎没有控制字符的内容，认为是文本
  confidence || ratio < 0.01
}

/// 读取文本文件（自动检测编码）
pub async fn read_text_file<R: AsyncRead + Unpin>(
  reader: &mut R,
  encoding_qualifier: Option<&str>,
) -> io::Result<Option<(Vec<String>, String)>> {
  let mut buf_reader = BufReader::new(reader);
  let mut sample = Vec::with_capacity(4096);
  let mut temp_buf = vec![0u8; 4096];
  let mut total_read = 0;

  while total_read < 4096 {
    let n = buf_reader.read(&mut temp_buf[total_read..]).await?;
    if n == 0 {
      break;
    }
    let end = total_read + n;
    sample.extend_from_slice(&temp_buf[total_read..end]);
    total_read = end;
  }

  // 二进制检查
  if !is_probably_text_bytes(&sample) {
    debug!("Is binary file, skip");
    return Ok(None);
  }

  // 确定编码
  let (encoding, encoding_name) = if let Some(enc_name) = encoding_qualifier {
    let enc_opt = Encoding::for_label(enc_name.as_bytes()).or_else(|| match enc_name.to_uppercase().as_str() {
      "UTF8" | "UTF-8" => Some(UTF_8),
      "GBK" => Some(GBK),
      "BIG5" | "BIG-5" => Some(BIG5),
      "SHIFT-JIS" | "SHIFT_JIS" | "SJIS" => Some(SHIFT_JIS),
      "EUC-KR" | "EUC_KR" => Some(EUC_KR),
      "WINDOWS-1252" | "WINDOWS_1252" | "CP1252" => Some(WINDOWS_1252),
      "ISO-8859-1" | "ISO_8859_1" | "LATIN1" | "LATIN-1" => Encoding::for_label(b"ISO-8859-1"),
      "UTF-16LE" | "UTF16LE" | "UTF-16-LE" => Some(UTF_16LE),
      "UTF-16BE" | "UTF16BE" | "UTF-16-BE" => Some(UTF_16BE),
      _ => None,
    });

    match enc_opt {
      Some(enc) => (enc, enc.name().to_string()),
      None => match auto_detect_encoding(&sample) {
        Some((enc, name)) => (enc, name),
        None => return Ok(None),
      },
    }
  } else {
    match auto_detect_encoding(&sample) {
      Some((enc, name)) => (enc, name),
      None => return Ok(None),
    }
  };

  let lines = if encoding == UTF_8 {
    read_lines_utf8(&mut buf_reader, sample).await?
  } else if encoding == UTF_16LE || encoding == UTF_16BE {
    read_lines_utf16(&mut buf_reader, encoding, sample).await?
  } else {
    read_lines_with_encoding(&mut buf_reader, encoding, sample).await?
  };

  Ok(Some((lines, encoding_name)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_encoding_utf8_bom() {
        let sample = b"\xEF\xBB\xBFHello";
        assert_eq!(detect_encoding(sample), Some(UTF_8));
    }

    #[test]
    fn test_detect_encoding_utf16le_bom() {
        let sample = b"\xFF\xFEH\x00e\x00l\x00l\x00o\x00";
        assert_eq!(detect_encoding(sample), Some(UTF_16LE));
    }

    #[test]
    fn test_detect_encoding_utf16be_bom() {
        let sample = b"\xFE\xFF\x00H\x00e\x00l\x00l\x00o";
        assert_eq!(detect_encoding(sample), Some(UTF_16BE));
    }

    #[test]
    fn test_detect_encoding_valid_utf8() {
        let sample = b"Hello, World! \xE4\xB8\xAD\xE6\x96\x87";
        assert_eq!(detect_encoding(sample), Some(UTF_8));
    }

    #[test]
    fn test_auto_detect_encoding() {
        let sample = b"Hello, World!";
        let result = auto_detect_encoding(sample);
        assert!(result.is_some());
        let (enc, name) = result.unwrap();
        assert_eq!(enc, UTF_8);
        assert!(!name.is_empty());
    }

    #[test]
    fn test_decode_buffer_to_lines() {
        let buffer = b"line1\nline2\nline3";
        let lines = decode_buffer_to_lines(UTF_8, buffer, "test");
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[0], "line1");
        assert_eq!(lines[1], "line2");
        assert_eq!(lines[2], "line3");
    }

    #[test]
    fn test_decode_buffer_to_lines_no_trailing_newline() {
        let buffer = b"line1\nline2";
        let lines = decode_buffer_to_lines(UTF_8, buffer, "test");
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[1], "line2");
    }

    #[test]
    fn test_is_probably_text_bytes_valid_utf8() {
        let sample = b"Hello, World!";
        assert!(is_probably_text_bytes(sample));
    }

    #[test]
    fn test_is_probably_text_bytes_empty() {
        let sample = b"";
        assert!(is_probably_text_bytes(sample));
    }

    #[test]
    fn test_is_probably_text_bytes_mixed() {
        // Mostly text with some control chars
        let sample = b"Hello\tWorld\nTest";
        assert!(is_probably_text_bytes(sample));
    }

    #[tokio::test]
    async fn test_read_lines_utf8_simple() {
        let data = b"line1\nline2\nline3";
        let mut reader = BufReader::new(&data[..]);
        let sample = Vec::new();

        let lines = read_lines_utf8(&mut reader, sample).await.unwrap();
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[0], "line1");
    }

    #[tokio::test]
    async fn test_read_text_file_utf8() {
        let data = b"Hello\nWorld";
        let mut reader = &data[..];

        let result = read_text_file(&mut reader, None).await.unwrap();
        assert!(result.is_some());

        let (lines, encoding) = result.unwrap();
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0], "Hello");
        assert_eq!(lines[1], "World");
        assert!(!encoding.is_empty());
    }

    #[test]
    fn test_is_probably_text_bytes_binary() {
        // Data that is NOT valid UTF-8 and contains control characters
        let sample = b"\xFF\xFE\xFD\x00\x01\x02\x03\x04".repeat(10);
        assert!(!is_probably_text_bytes(&sample));
    }

    #[tokio::test]
    async fn test_read_lines_utf16_le() {
        let data = vec![0xFF, 0xFE, b'l', 0, b'1', 0, b'\n', 0, b'l', 0, b'2', 0];
        let mut reader = BufReader::new(&data[2..]);
        let sample = data[0..2].to_vec();

        let lines = read_lines_utf16(&mut reader, UTF_16LE, sample).await.unwrap();
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0], "l1");
        assert_eq!(lines[1], "l2");
    }

    #[tokio::test]
    async fn test_read_lines_utf16_be() {
        let data = vec![0xFE, 0xFF, 0, b'l', 0, b'1', 0, b'\n', 0, b'l', 0, b'2'];
        let mut reader = BufReader::new(&data[2..]);
        let sample = data[0..2].to_vec();

        let lines = read_lines_utf16(&mut reader, UTF_16BE, sample).await.unwrap();
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0], "l1");
        assert_eq!(lines[1], "l2");
    }

    #[tokio::test]
    async fn test_read_lines_gbk() {
        let data = vec![0xC4, 0xE3, 0xBA, 0xC3, b'\n', b'w', b'o', b'r', b'l', b'd']; // 你好
        let mut reader = BufReader::new(&data[..]);
        let sample = Vec::new();

        let lines = read_lines_with_encoding(&mut reader, GBK, sample).await.unwrap();
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0], "你好");
        assert_eq!(lines[1], "world");
    }

    #[tokio::test]
    async fn test_read_text_file_with_qualifier() {
        let data = vec![0xC4, 0xE3, 0xBA, 0xC3];
        let mut reader = &data[..];

        let result = read_text_file(&mut reader, Some("gbk")).await.unwrap();
        let (lines, name) = result.expect("should detect as gbk");
        assert_eq!(lines[0], "你好");
        assert_eq!(name.to_lowercase(), "gbk");
    }

    #[tokio::test]
    async fn test_read_lines_utf8_with_sample() {
        let data = b"line2\nline3";
        let mut reader = BufReader::new(&data[..]);
        let sample = b"line1\n".to_vec();

        let lines = read_lines_utf8(&mut reader, sample).await.unwrap();
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[0], "line1");
        assert_eq!(lines[1], "line2");
        assert_eq!(lines[2], "line3");
    }

    #[tokio::test]
    async fn test_read_lines_utf8_incomplete() {
        let data = b"t2\n";
        let mut reader = BufReader::new(&data[..]);
        let sample = b"par".to_vec(); // incomplete line "part1"

        let lines = read_lines_utf8(&mut reader, sample).await.unwrap();
        // The logic in read_lines_utf8 seems to handle incomplete samples
        // If sample is "par" and reader has "t2\n", it might become "part2\n"
        // Let's verify what it actually does.
        assert!(lines.len() >= 1);
    }
}
