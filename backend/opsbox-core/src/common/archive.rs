//! Archive 类型定义 - 跨层共享的归档类型和判型逻辑
//!
//! 这个模块提供归档类型的底层定义，不依赖任何特定上下文（如文件系统），
//! 可以被 fs、dfs 等模块安全地依赖。

/// 归档类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ArchiveType {
  /// TAR 归档
  Tar,
  /// GZIP 压缩的 TAR
  TarGz,
  /// .tgz 扩展名的 TAR+GZ（语义上等同于 TarGz）
  Tgz,
  /// ZIP 归档
  Zip,
  /// 单独的 GZIP 文件
  Gz,
  /// 未知类型
  Unknown,
}

impl ArchiveType {
  /// 从文件扩展名识别归档类型
  pub fn from_extension(ext: &str) -> Option<Self> {
    match ext.to_lowercase().as_str() {
      ".tar" => Some(ArchiveType::Tar),
      ".tar.gz" => Some(ArchiveType::TarGz),
      ".tgz" => Some(ArchiveType::Tgz),
      ".zip" => Some(ArchiveType::Zip),
      ".gz" => Some(ArchiveType::Gz),
      _ => None,
    }
  }

  /// 获取归档类型的扩展名
  pub fn extension(&self) -> &'static str {
    match self {
      ArchiveType::Tar => ".tar",
      ArchiveType::TarGz => ".tar.gz",
      ArchiveType::Tgz => ".tgz",
      ArchiveType::Zip => ".zip",
      ArchiveType::Gz => ".gz",
      ArchiveType::Unknown => "",
    }
  }

  /// 判断是否为 tar.gz 类型（需要内部 tar 处理）
  pub fn is_tar_gz(&self) -> bool {
    matches!(self, ArchiveType::TarGz | ArchiveType::Tgz)
  }

  /// 判断是否为 gzip 压缩类型（TarGz 或 Gz）
  pub fn is_gzip_compressed(&self) -> bool {
    matches!(self, ArchiveType::TarGz | ArchiveType::Tgz | ArchiveType::Gz)
  }
}

/// 从路径推断归档类型（扩展名回退）
///
/// 使用 `ArchiveType::from_extension`，并将 `.tgz` 归一化为 `TarGz`。
pub fn infer_archive_from_path(path: &str) -> Option<ArchiveType> {
  let lower = path.to_lowercase();

  // 复合扩展名优先，避免被 `.gz` 提前匹配
  if lower.ends_with(".tar.gz") {
    return Some(ArchiveType::TarGz);
  }

  if let Some(pos) = lower.rfind('.') {
    let ext = &lower[pos..];
    match ArchiveType::from_extension(ext) {
      Some(ArchiveType::Tgz) => Some(ArchiveType::TarGz),
      other => other,
    }
  } else {
    None
  }
}

/// 从文件头 magic bytes 检测归档类型（完全基于内容，无兜底）
///
/// # 参数
/// - `head`: 文件头部字节（需要足够长度以检测嵌套格式）
///
/// # Magic Bytes 参考
/// - ZIP: `50 4B 03 04` / `50 4B 05 06` / `50 4B 07 08` (偏移 0)
/// - TAR: `75 73 74 61 72` ("ustar") (偏移 257)
/// - Gzip: `1F 8B` (偏移 0)
///
/// # 嵌套格式检测
/// - TarGz: Gzip 压缩的 Tar（解压后检测 Tar ustar 标记）
pub fn detect_archive_type_from_head(head: &[u8]) -> ArchiveType {
  // ZIP 检测 - magic bytes: 50 4B 03/05/07 04/06/08
  if head.len() >= 4 {
    let sig = &head[..4];
    if sig == [0x50, 0x4B, 0x03, 0x04] || sig == [0x50, 0x4B, 0x05, 0x06] || sig == [0x50, 0x4B, 0x07, 0x08] {
      return ArchiveType::Zip;
    }
  }

  // TAR 检测 - ustar 标记在偏移 257-262
  if head.len() >= 512 && &head[257..262] == b"ustar" {
    return ArchiveType::Tar;
  }

  // Gzip 检测 - magic bytes: 1F 8B
  if head.len() >= 2 && head[0] == 0x1F && head[1] == 0x8B {
    // 尝试解压头部以检测内部是否为 Tar
    if let Some(inner_head) = try_decompress_gzip_head(head)
      && inner_head.len() >= 512
      && &inner_head[257..262] == b"ustar"
    {
      return ArchiveType::TarGz;
    }
    return ArchiveType::Gz;
  }

  ArchiveType::Unknown
}

/// 从文件头 magic bytes 检测归档类型（带 hint_name 兜底）
///
/// 与 `detect_archive_type_from_head` 的区别：
/// - 当 Gzip 解压失败时，使用 `hint_name` 后缀兜底判断是否为 TarGz
/// - 适用于文件名可信但内容检测不确定的场景
///
/// # 参数
/// - `head`: 文件头部字节
/// - `hint_name`: 可选的文件名提示（用于兜底判断）
///
/// # 示例
/// ```ignore
/// let head = vec![0x1F, 0x8B, ...];  // Gzip 头部
/// let t = detect_archive_type_with_hint(&head, Some("data.tar.gz"));
/// assert_eq!(t, ArchiveType::TarGz);
/// ```
pub fn detect_archive_type_with_hint(head: &[u8], hint_name: Option<&str>) -> ArchiveType {
  // ZIP 检测
  if head.len() >= 4 {
    let sig = &head[..4];
    if sig == [0x50, 0x4B, 0x03, 0x04] || sig == [0x50, 0x4B, 0x05, 0x06] || sig == [0x50, 0x4B, 0x07, 0x08] {
      return ArchiveType::Zip;
    }
  }

  // TAR 检测
  if head.len() >= 512 && &head[257..262] == b"ustar" {
    return ArchiveType::Tar;
  }

  // Gzip 检测 - 带 hint_name 兜底
  if head.len() >= 2 && head[0] == 0x1F && head[1] == 0x8B {
    // 尝试解压头部以检测内部是否为 Tar
    if let Some(inner_head) = try_decompress_gzip_head(head)
      && inner_head.len() >= 512
      && &inner_head[257..262] == b"ustar"
    {
      return ArchiveType::TarGz;
    }
    // 解压失败时，用 hint_name 兜底判断是否为 TarGz
    if is_tar_gz_hint(hint_name) {
      return ArchiveType::TarGz;
    }
    return ArchiveType::Gz;
  }

  ArchiveType::Unknown
}

/// 检测 Gzip 内部是否为 Tar（带 hint_name 兜底）
///
/// 这是 `detect_archive_type_with_hint` 的 Gzip 专用版本，
/// 适用于已经确定外层是 Gzip 的情况。
///
/// # 参数
/// - `head`: Gzip 文件头部字节
/// - `hint_name`: 可选的文件名提示
///
/// # 返回
/// - `true`: 内部是 Tar（或 hint_name 指示为 TarGz）
/// - `false`: 内部不是 Tar
pub fn detect_gzip_inner_is_tar(head: &[u8], hint_name: Option<&str>) -> bool {
  // 尝试解压头部并检测 tar header
  if let Some(inner_head) = try_decompress_gzip_head(head) {
    return inner_head.len() >= 512 && &inner_head[257..262] == b"ustar";
  }
  // 解压失败时，用 hint_name 兜底
  is_tar_gz_hint(hint_name)
}

/// 尝试解压 Gzip 头部
///
/// # 参数
/// - `compressed`: Gzip 压缩数据
///
/// # 返回
/// - `Some(Vec<u8>)`: 解压成功，返回解压后的数据
/// - `None`: 解压失败或数据不足
pub fn try_decompress_gzip_head(compressed: &[u8]) -> Option<Vec<u8>> {
  use flate2::read::GzDecoder;
  use std::io::Read;

  // tar header 是 512 字节，我们需要至少这么多数据来检测 ustar 标记
  const MAX_DECOMPRESS_SIZE: usize = 512 * 10;

  let decoder = GzDecoder::new(compressed);
  let mut buf = Vec::with_capacity(512);

  match decoder.take(MAX_DECOMPRESS_SIZE as u64).read_to_end(&mut buf) {
    Ok(_) if buf.len() >= 512 => Some(buf),
    _ => None,
  }
}

/// 根据 hint_name 判断是否为 TarGz
fn is_tar_gz_hint(hint_name: Option<&str>) -> bool {
  hint_name
    .map(|h| {
      let lower = h.to_lowercase();
      lower.ends_with(".tar.gz") || lower.ends_with(".tgz")
    })
    .unwrap_or(false)
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_archive_type_from_extension() {
    assert_eq!(ArchiveType::from_extension(".tar"), Some(ArchiveType::Tar));
    assert_eq!(ArchiveType::from_extension(".tar.gz"), Some(ArchiveType::TarGz));
    assert_eq!(ArchiveType::from_extension(".tgz"), Some(ArchiveType::Tgz));
    assert_eq!(ArchiveType::from_extension(".zip"), Some(ArchiveType::Zip));
    assert_eq!(ArchiveType::from_extension(".gz"), Some(ArchiveType::Gz));
    assert_eq!(ArchiveType::from_extension(".txt"), None);
    assert_eq!(ArchiveType::from_extension(".TAR"), Some(ArchiveType::Tar)); // case insensitive
  }

  #[test]
  fn test_archive_type_extension() {
    assert_eq!(ArchiveType::Tar.extension(), ".tar");
    assert_eq!(ArchiveType::TarGz.extension(), ".tar.gz");
    assert_eq!(ArchiveType::Tgz.extension(), ".tgz");
    assert_eq!(ArchiveType::Zip.extension(), ".zip");
    assert_eq!(ArchiveType::Gz.extension(), ".gz");
    assert_eq!(ArchiveType::Unknown.extension(), "");
  }

  #[test]
  fn test_archive_type_helpers() {
    assert!(ArchiveType::TarGz.is_tar_gz());
    assert!(ArchiveType::Tgz.is_tar_gz());
    assert!(!ArchiveType::Tar.is_tar_gz());
    assert!(!ArchiveType::Gz.is_tar_gz());

    assert!(ArchiveType::TarGz.is_gzip_compressed());
    assert!(ArchiveType::Tgz.is_gzip_compressed());
    assert!(ArchiveType::Gz.is_gzip_compressed());
    assert!(!ArchiveType::Tar.is_gzip_compressed());
    assert!(!ArchiveType::Zip.is_gzip_compressed());
  }

  #[test]
  fn test_infer_archive_from_path() {
    assert_eq!(infer_archive_from_path("file.tar"), Some(ArchiveType::Tar));
    assert_eq!(infer_archive_from_path("file.tar.gz"), Some(ArchiveType::TarGz));
    assert_eq!(infer_archive_from_path("file.tgz"), Some(ArchiveType::TarGz)); // normalized
    assert_eq!(infer_archive_from_path("file.gz"), Some(ArchiveType::Gz));
    assert_eq!(infer_archive_from_path("file.zip"), Some(ArchiveType::Zip));
    assert_eq!(infer_archive_from_path("file.txt"), None);
    assert_eq!(infer_archive_from_path("FILE.TAR.GZ"), Some(ArchiveType::TarGz)); // case insensitive
  }

  #[test]
  fn test_detect_archive_type_from_head_zip() {
    let zip_head = vec![0x50, 0x4B, 0x03, 0x04, 0x00, 0x00, 0x00, 0x00];
    assert_eq!(detect_archive_type_from_head(&zip_head), ArchiveType::Zip);

    let zip_eocd = vec![0x50, 0x4B, 0x05, 0x06];
    assert_eq!(detect_archive_type_from_head(&zip_eocd), ArchiveType::Zip);
  }

  #[test]
  fn test_detect_archive_type_from_head_tar() {
    let mut tar_head = vec![0u8; 512];
    tar_head[257] = b'u';
    tar_head[258] = b's';
    tar_head[259] = b't';
    tar_head[260] = b'a';
    tar_head[261] = b'r';
    assert_eq!(detect_archive_type_from_head(&tar_head), ArchiveType::Tar);
  }

  #[test]
  fn test_detect_archive_type_from_head_gzip() {
    // 无效的 gzip 数据，但前两字节是 magic
    let gzip_head = vec![0x1F, 0x8B, 0x00, 0x00];
    // 无法解压，应返回 Gz
    assert_eq!(detect_archive_type_from_head(&gzip_head), ArchiveType::Gz);
  }

  #[test]
  fn test_detect_archive_type_from_head_targz() {
    use flate2::Compression;
    use flate2::write::GzEncoder;
    use std::io::Write;

    // 创建有效的 TarGz 数据
    let mut tar_data = vec![0u8; 512];
    tar_data[257] = b'u';
    tar_data[258] = b's';
    tar_data[259] = b't';
    tar_data[260] = b'a';
    tar_data[261] = b'r';

    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(&tar_data).unwrap();
    let gz_data = encoder.finish().unwrap();

    assert_eq!(detect_archive_type_from_head(&gz_data), ArchiveType::TarGz);
  }

  #[test]
  fn test_detect_archive_type_with_hint_fallback() {
    // 无效的 gzip 数据
    let gzip_head = vec![0x1F, 0x8B, 0x00, 0x00];

    // 无 hint，返回 Gz
    assert_eq!(detect_archive_type_with_hint(&gzip_head, None), ArchiveType::Gz);

    // 有 tar.gz hint，返回 TarGz
    assert_eq!(
      detect_archive_type_with_hint(&gzip_head, Some("data.tar.gz")),
      ArchiveType::TarGz
    );
    assert_eq!(
      detect_archive_type_with_hint(&gzip_head, Some("data.TGZ")),
      ArchiveType::TarGz
    );

    // 有非 tar.gz hint，返回 Gz
    assert_eq!(
      detect_archive_type_with_hint(&gzip_head, Some("data.gz")),
      ArchiveType::Gz
    );
  }

  #[test]
  fn test_detect_gzip_inner_is_tar() {
    use flate2::Compression;
    use flate2::write::GzEncoder;
    use std::io::Write;

    // 创建包含 tar header 的 gzip 数据
    let mut tar_header = vec![0u8; 512];
    tar_header[257] = b'u';
    tar_header[258] = b's';
    tar_header[259] = b't';
    tar_header[260] = b'a';
    tar_header[261] = b'r';

    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(&tar_header).unwrap();
    let gz_data = encoder.finish().unwrap();

    // 应该检测到内部是 tar
    assert!(detect_gzip_inner_is_tar(&gz_data, None));
    assert!(detect_gzip_inner_is_tar(&gz_data, Some("data.gz")));

    // 创建不包含 tar header 的 gzip 数据
    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(b"plain text").unwrap();
    let plain_gz = encoder.finish().unwrap();

    // 应该检测到内部不是 tar
    assert!(!detect_gzip_inner_is_tar(&plain_gz, None));
    assert!(!detect_gzip_inner_is_tar(&plain_gz, Some("data.gz")));

    // 但如果 hint 说是 tar.gz，应该返回 true
    assert!(detect_gzip_inner_is_tar(&plain_gz, Some("data.tar.gz")));
  }

  #[test]
  fn test_detect_archive_type_unknown() {
    let random_head = vec![0x00, 0x01, 0x02, 0x03];
    assert_eq!(detect_archive_type_from_head(&random_head), ArchiveType::Unknown);
    assert_eq!(
      detect_archive_type_with_hint(&random_head, Some("file.txt")),
      ArchiveType::Unknown
    );

    let empty_head = vec![];
    assert_eq!(detect_archive_type_from_head(&empty_head), ArchiveType::Unknown);

    let short_head = vec![0x50];
    assert_eq!(detect_archive_type_from_head(&short_head), ArchiveType::Unknown);
  }
}
