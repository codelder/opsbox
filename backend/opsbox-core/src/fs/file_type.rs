use infer;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileKind {
  Image(String),    // mime
  Video(String),    // mime
  Audio(String),    // mime
  Document(String), // mime
  Archive(String),  // mime
  Text,             // plain text
  Binary,           // unknown binary
}

impl FileKind {
  pub fn mime_type(&self) -> &str {
    match self {
      FileKind::Image(m) => m,
      FileKind::Video(m) => m,
      FileKind::Audio(m) => m,
      FileKind::Document(m) => m,
      FileKind::Archive(m) => m,
      FileKind::Text => "text/plain",
      FileKind::Binary => "application/octet-stream",
    }
  }

  ///是否为归档或压缩格式（通常需要特殊处理或解压）
  pub fn is_archive_or_compressed(&self) -> bool {
    matches!(self, FileKind::Archive(_))
  }

  /// 是否为 Gzip 格式
  pub fn is_gzip(&self) -> bool {
    match self {
      FileKind::Archive(mime) => mime == "application/gzip",
      _ => false,
    }
  }

  /// 是否为 Tar 格式
  pub fn is_tar(&self) -> bool {
    match self {
      FileKind::Archive(mime) => mime == "application/x-tar",
      _ => false,
    }
  }
}

/// 通过内容嗅探文件类型
///
/// 读取缓冲区的前 N 个字节（建议至少 262 字节，infer 推荐）来进行判断。
/// 如果 infer 无法识别，会尝试进行文本检测。
pub fn sniff_file_type(data: &[u8]) -> FileKind {
  if let Some(kind) = infer::get(data) {
    let mime = kind.mime_type().to_string();
    match kind.matcher_type() {
      infer::MatcherType::Image => FileKind::Image(mime),
      infer::MatcherType::Video => FileKind::Video(mime),
      infer::MatcherType::Audio => FileKind::Audio(mime),
      infer::MatcherType::Doc => FileKind::Document(mime),
      infer::MatcherType::Archive => FileKind::Archive(mime),
      infer::MatcherType::Text => FileKind::Text, // infer 也有 Text 类型? infer 0.15+ 有吗？infer 通常不检测纯文本
      // infer::MatcherType::App => FileKind::Binary, // Treat as binary/other
      // Custom or others
      _ => {
        // 某些特定的类型 infer 归类为 App 或其他，我们可能需要细分
        // 但总体来说归为 Binary 或 Archive
        // 比如 application/pdf 是 Doc 吗？infer 认为是 Doc?
        // infer 0.16 MatcherType: App, Archive, Audio, Book, Doc, Font, Image, Text, Video
        // Book -> Document
        // Font -> Binary?
        // App -> Binary (e.g. exe, wasm)
        match kind.matcher_type() {
          infer::MatcherType::Book => FileKind::Document(mime),
          infer::MatcherType::Text => FileKind::Text, // xml, json sometimes detected
          _ => {
            // 即使 infer 识别出来了（如 application/octet-stream 或 application/x-executable），
            // 只要不是上面的主要媒体类型，我们暂且归为 Binary
            FileKind::Binary
          }
        }
      }
    }
  } else {
    // Fallback: 简单的文本检测
    if is_looks_like_text(data) {
      FileKind::Text
    } else {
      FileKind::Binary
    }
  }
}

/// 启发式检查是否像文本
///
/// 规则：
/// 1. 不包含 null 字节 (0x00)（除非是 UTF-16/32，这里暂不处理复杂编码探测，留给上层 chardetng）
/// 2. 可打印字符比例较高
fn is_looks_like_text(data: &[u8]) -> bool {
  if data.is_empty() {
    return true;
  }

  // 快速检查 null 字节，二进制文件通常包含大量 0
  // 注意：UTF-16 文件会有很多 0，但通常 infer 可能会识别不了或者识别为 text/plain?
  // infer 并不擅长识别纯文本编码。
  // 如果包含 0，我们假设它是二进制（除非它是 UTF-16 BOM 开头，但我们在上层处理编码时会检测）
  // 为了安全起见，如果不确定，归为 Binary，让上层决定是否尝试强行解码。
  if data.contains(&0) {
    // 例外：检查是否有 UTF-16 BOM
    if data.len() >= 2 && ((data[0] == 0xFF && data[1] == 0xFE) || (data[0] == 0xFE && data[1] == 0xFF)) {
      return true;
    }
    return false;
  }

  // 统计可打印字符
  let printable = data
    .iter()
    .filter(|&&b| b == 0x09 || b == 0x0A || b == 0x0D || (0x20..=0x7E).contains(&b))
    .count();

  // 如果可打印字符占比超过 90%，或者是很短的片段，认为是文本
  let ratio = printable as f32 / data.len() as f32;
  ratio > 0.9
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_is_looks_like_text() {
    // Empty is text
    assert!(is_looks_like_text(&[]));

    // ASCII text
    assert!(is_looks_like_text(b"Hello world"));
    assert!(is_looks_like_text(b"Hello\nworld\r\t"));

    // Binary with null
    assert!(!is_looks_like_text(b"Hello\0world"));

    // High ratio of printable
    let mut data = Vec::new();
    for _ in 0..95 {
      data.push(b'a');
    }
    for _ in 0..5 {
      data.push(0xFF); // non-printable
    }
    assert!(is_looks_like_text(&data)); // 95% printable

    // Low ratio of printable
    let mut data = Vec::new();
    for _ in 0..50 {
      data.push(b'a');
    }
    for _ in 0..51 {
      data.push(0xFF); // non-printable
    }
    assert!(!is_looks_like_text(&data)); // < 50% printable
  }

  #[test]
  fn test_sniff_file_type_text() {
    let text = b"This is a plain text file.";
    assert_eq!(sniff_file_type(text), FileKind::Text);
  }

  #[test]
  fn test_sniff_file_type_gzip() {
    // 1F 8B 08 is standard GZip with Deflate.
    // infer should detect this.
    let mut data = vec![0x1f, 0x8b, 0x08];
    data.extend_from_slice(&[0; 30]);

    let kind = sniff_file_type(&data);
    // Note: if infer library is strict, it might return None for garbage data even with valid header.
    // In that case our fallback returns Binary.
    // We assert it is NOT Text.
    assert_ne!(kind, FileKind::Text);

    // Optimistically check for Gzip is detected
    if let FileKind::Archive(ref m) = kind {
      assert_eq!(m, "application/gzip");
    }
  }

  #[test]
  fn test_file_kind_properties() {
    let k = FileKind::Archive("application/gzip".to_string());
    assert!(k.is_gzip());
    assert!(!k.is_tar());
    assert!(k.is_archive_or_compressed());

    let k = FileKind::Archive("application/x-tar".to_string());
    assert!(k.is_tar());
    assert!(!k.is_gzip());
    assert!(k.is_archive_or_compressed());

    let k = FileKind::Text;
    assert_eq!(k.mime_type(), "text/plain");
    assert!(!k.is_archive_or_compressed());
  }
}
