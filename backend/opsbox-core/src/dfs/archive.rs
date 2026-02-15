//! Archive 模块 - 归档容器概念
//!
//! 定义了 ArchiveType 和 ArchiveContext

use super::filesystem::OpbxFileSystem;
use super::path::ResourcePath;
use super::resource::Resource;

/// 归档类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ArchiveType {
  /// TAR 归档
  Tar,
  /// GZIP 压缩的 TAR
  TarGz,
  /// .tgz 扩展名的 TAR+GZ
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

  /// 从文件头 magic bytes 检测归档类型（完全基于内容）
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
  pub fn from_magic_bytes(head: &[u8]) -> Self {
    // ZIP 检测 - magic bytes: 50 4B 03/05/07 04/06/08
    if head.len() >= 4 {
      let sig = &head[..4];
      if sig == [0x50, 0x4B, 0x03, 0x04] || sig == [0x50, 0x4B, 0x05, 0x06] || sig == [0x50, 0x4B, 0x07, 0x08] {
        return ArchiveType::Zip;
      }
    }

    // TAR 检测 - ustar 标记在偏移 257-262
    if head.len() >= 512 && &head[257..257 + 5] == b"ustar" {
      return ArchiveType::Tar;
    }

    // Gzip 检测 - magic bytes: 1F 8B
    if head.len() >= 2 {
      let sig = &head[..2];
      if sig == [0x1F, 0x8B] {
        // 尝试解压头部以检测内部是否为 Tar
        if let Some(inner_head) = Self::try_decompress_gzip_head(head)
          && inner_head.len() >= 512
          && &inner_head[257..262] == b"ustar"
        {
          return ArchiveType::TarGz;
        }
        return ArchiveType::Gz;
      }
    }

    ArchiveType::Unknown
  }

  /// 尝试解压 Gzip 头部以检测内部格式
  ///
  /// 返回解压后的数据（至少 512 字节以检测 Tar），失败返回 None
  fn try_decompress_gzip_head(compressed: &[u8]) -> Option<Vec<u8>> {
    use flate2::read::GzDecoder;
    use std::io::Read;

    // 增加解压限制以确保能获取完整的 tar header
    // tar header 是 512 字节，我们需要至少这么多数据来检测 ustar 标记
    const MAX_DECOMPRESS_SIZE: usize = 512 * 10; // 最多解压 5 个 tar block

    let decoder = GzDecoder::new(compressed);
    let mut buffer = Vec::with_capacity(512);

    // 读取尽可能多的数据（最多 MAX_DECOMPRESS_SIZE）
    match decoder.take(MAX_DECOMPRESS_SIZE as u64).read_to_end(&mut buffer) {
      Ok(n) if n >= 512 => Some(buffer),
      Ok(_) => {
        // 解压成功但数据不足 512 字节
        // 这可能是一个很小的 gzip 文件，但仍可能是 tar（只有很少数据）
        // 如果有任何数据，返回它让调用者决定
        if !buffer.is_empty() { Some(buffer) } else { None }
      }
      Err(_) => None, // 解压失败
    }
  }
}

/// 检测资源是否为归档文件（magic bytes 优先）
///
/// 行为与 Explorer `auto_detect_archive` 保持一致：
/// - 成功读取文件头：使用 magic bytes 检测
/// - 读取数据为空：回退到扩展名检测
/// - open_read 失败（可能是目录）：返回 None，不设置归档
pub async fn detect_archive_type(fs: &dyn OpbxFileSystem, resource: &Resource) -> Option<ArchiveType> {
  use tokio::io::AsyncReadExt;

  let path_str = resource.primary_path.to_string();

  let head_bytes = match fs.open_read(&resource.primary_path).await {
    Ok(mut reader) => {
      let mut buffer = vec![0u8; 2048];
      let n = reader.read(&mut buffer).await.unwrap_or(0);
      buffer.truncate(n);

      if buffer.is_empty() {
        return infer_archive_from_path(&path_str);
      }
      buffer
    }
    Err(_) => {
      // 与 Explorer 对齐：open_read 失败时不进行扩展名回退
      return None;
    }
  };

  let archive_type = ArchiveType::from_magic_bytes(&head_bytes);
  if archive_type != ArchiveType::Unknown {
    Some(archive_type)
  } else {
    None
  }
}

/// 从路径推断归档类型（扩展名回退）
///
/// 使用 `ArchiveType::from_extension`，并将 `.tgz` 归一化为 `TarGz`。
pub fn infer_archive_from_path(path: &str) -> Option<ArchiveType> {
  let lower = path.to_lowercase();

  // 复合扩展名优先，避免被 `.gz` 提前匹配。
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

/// 归档上下文
///
/// 表示资源位于归档文件内的上下文信息
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArchiveContext {
  /// 归档内的路径
  pub inner_path: ResourcePath,
  /// 归档类型
  pub archive_type: Option<ArchiveType>,
}

impl ArchiveContext {
  /// 创建新的归档上下文
  pub fn new(inner_path: ResourcePath, archive_type: Option<ArchiveType>) -> Self {
    Self {
      inner_path,
      archive_type,
    }
  }

  /// 从路径字符串创建归档上下文
  pub fn from_path_str(inner_path: &str, archive_type: Option<ArchiveType>) -> Self {
    Self {
      inner_path: ResourcePath::parse(inner_path),
      archive_type,
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::dfs::{DirEntry, Endpoint, FileMetadata, FsError, Resource};
  use async_trait::async_trait;
  use std::pin::Pin;

  #[test]
  fn test_archive_type_from_extension() {
    assert_eq!(ArchiveType::from_extension(".tar"), Some(ArchiveType::Tar));
    assert_eq!(ArchiveType::from_extension(".tar.gz"), Some(ArchiveType::TarGz));
    assert_eq!(ArchiveType::from_extension(".tgz"), Some(ArchiveType::Tgz));
    assert_eq!(ArchiveType::from_extension(".zip"), Some(ArchiveType::Zip));
    assert_eq!(ArchiveType::from_extension(".gz"), Some(ArchiveType::Gz));
    assert_eq!(ArchiveType::from_extension(".txt"), None);
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
  fn test_archive_type_from_magic_bytes_zip() {
    // ZIP local file header: 50 4B 03 04
    let zip_head = vec![0x50, 0x4B, 0x03, 0x04, 0x00, 0x00, 0x00, 0x00];
    assert_eq!(ArchiveType::from_magic_bytes(&zip_head), ArchiveType::Zip);

    // ZIP end of central directory: 50 4B 05 06
    let zip_eocd = vec![0x50, 0x4B, 0x05, 0x06];
    assert_eq!(ArchiveType::from_magic_bytes(&zip_eocd), ArchiveType::Zip);
  }

  #[test]
  fn test_archive_type_from_magic_bytes_tar() {
    // TAR has "ustar" at offset 257
    let mut tar_head = vec![0u8; 512];
    tar_head[257] = b'u';
    tar_head[258] = b's';
    tar_head[259] = b't';
    tar_head[260] = b'a';
    tar_head[261] = b'r';
    assert_eq!(ArchiveType::from_magic_bytes(&tar_head), ArchiveType::Tar);
  }

  #[test]
  fn test_archive_type_from_magic_bytes_gzip() {
    // Gzip magic bytes: 1F 8B
    // 这是一个纯 Gzip 文件（不包含 Tar）
    let gzip_head = vec![
      0x1F, 0x8B, 0x08, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x03, 0x00, 0x00, 0x00, 0x00, 0x00,
    ]; // 无效的 gzip 数据，但前两字节是 magic
    // 由于无法有效解压，应返回 Gz（这是尽力而为的检测结果）
    let result = ArchiveType::from_magic_bytes(&gzip_head);
    // 无法解压的情况下，我们仍然认为是 Gzip
    assert_eq!(result, ArchiveType::Gz);
  }

  #[test]
  fn test_archive_type_from_magic_bytes_targz() {
    // 创建一个真实的 TarGz 文件头
    // 1. 先创建一个有效的 Tar 头
    let mut tar_data = vec![0u8; 512];
    tar_data[257] = b'u';
    tar_data[258] = b's';
    tar_data[259] = b't';
    tar_data[260] = b'a';
    tar_data[261] = b'r';

    // 2. 压缩它（使用 flate2）
    use flate2::Compression;
    use flate2::write::GzEncoder;
    use std::io::Write;

    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(&tar_data).unwrap();
    let gz_data = encoder.finish().unwrap();

    // 3. 检测应该是 TarGz
    assert_eq!(ArchiveType::from_magic_bytes(&gz_data), ArchiveType::TarGz);
  }

  #[test]
  fn test_archive_type_from_magic_bytes_unknown() {
    // Random bytes that don't match any archive type
    let random_head = vec![0x00, 0x01, 0x02, 0x03];
    assert_eq!(ArchiveType::from_magic_bytes(&random_head), ArchiveType::Unknown);

    // Empty data
    let empty_head = vec![];
    assert_eq!(ArchiveType::from_magic_bytes(&empty_head), ArchiveType::Unknown);

    // Too short data
    let short_head = vec![0x50];
    assert_eq!(ArchiveType::from_magic_bytes(&short_head), ArchiveType::Unknown);
  }

  #[test]
  fn test_infer_archive_from_path_normalize_tgz() {
    assert_eq!(infer_archive_from_path("a.tgz"), Some(ArchiveType::TarGz));
    assert_eq!(infer_archive_from_path("a.tar.gz"), Some(ArchiveType::TarGz));
    assert_eq!(infer_archive_from_path("a.gz"), Some(ArchiveType::Gz));
    assert_eq!(infer_archive_from_path("a.log"), None);
  }

  #[tokio::test]
  async fn test_detect_archive_no_fallback_on_open_read_failure() {
    struct MockFailingFS;

    #[async_trait]
    impl OpbxFileSystem for MockFailingFS {
      async fn metadata(&self, _path: &ResourcePath) -> Result<FileMetadata, FsError> {
        unreachable!("not used")
      }

      async fn read_dir(&self, _path: &ResourcePath) -> Result<Vec<DirEntry>, FsError> {
        unreachable!("not used")
      }

      async fn open_read(
        &self,
        _path: &ResourcePath,
      ) -> Result<Pin<Box<dyn tokio::io::AsyncRead + Send + Unpin>>, FsError> {
        Err(FsError::NotFound("mock failure".to_string()))
      }
    }

    let resource = Resource {
      endpoint: Endpoint::local_fs(),
      primary_path: ResourcePath::parse("/tmp/archive.tar.gz"),
      archive_context: None,
      filter_glob: None,
    };

    let result = detect_archive_type(&MockFailingFS, &resource).await;
    assert!(result.is_none(), "open_read 失败时不应回退扩展名");
  }

  #[test]
  fn test_archive_context_new() {
    let inner_path = ResourcePath::parse("logs/app.log");
    let ctx = ArchiveContext::new(inner_path.clone(), Some(ArchiveType::Tar));
    assert_eq!(ctx.inner_path, inner_path);
    assert_eq!(ctx.archive_type, Some(ArchiveType::Tar));
  }

  #[test]
  fn test_archive_context_from_path_str() {
    let ctx = ArchiveContext::from_path_str("data/file.txt", Some(ArchiveType::Zip));
    assert_eq!(ctx.inner_path.to_string(), "data/file.txt");
    assert_eq!(ctx.archive_type, Some(ArchiveType::Zip));
  }
}
