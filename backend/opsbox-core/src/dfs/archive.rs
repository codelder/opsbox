//! Archive 模块 - 归档容器概念
//!
//! 定义了 ArchiveContext 和异步检测函数。
//! ArchiveType 已移动到 common::archive 模块。

use super::filesystem::OpbxFileSystem;
use super::path::ResourcePath;
use super::resource::Resource;

// 重新导出 common::archive 的类型，保持向后兼容
pub use crate::common::archive::{
  ArchiveType, detect_archive_type_from_head, detect_archive_type_with_hint, detect_gzip_inner_is_tar,
  infer_archive_from_path, try_decompress_gzip_head,
};

/// 检测资源是否为归档文件（magic bytes 优先）
///
/// 行为与 Explorer `auto_detect_archive` 保持一致：
/// - 成功读取文件头：使用 magic bytes 检测
/// - 读取数据为空：返回 None（空文件不设置归档类型）
/// - open_read 失败（可能是目录）：返回 None，不设置归档
pub async fn detect_archive_type(fs: &dyn OpbxFileSystem, resource: &Resource) -> Option<ArchiveType> {
  use tokio::io::AsyncReadExt;

  let head_bytes = match fs.open_read(&resource.primary_path).await {
    Ok(mut reader) => {
      let mut buffer = vec![0u8; 2048];
      let n = match reader.read(&mut buffer).await {
        Ok(n) => n,
        Err(_) => return None,
      };
      buffer.truncate(n);

      if buffer.is_empty() {
        return None;
      }
      buffer
    }
    Err(_) => {
      // 与 Explorer 对齐：open_read 失败时不进行扩展名回退
      return None;
    }
  };

  let archive_type = detect_archive_type_from_head(&head_bytes);
  if archive_type != ArchiveType::Unknown {
    Some(archive_type)
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

  // 注意：ArchiveType::from_extension, extension 等基础测试已在 common::archive 模块中
  // 这里只测试 dfs 层特有的功能

  #[test]
  fn test_detect_archive_type_from_head_zip() {
    // ZIP local file header: 50 4B 03 04
    let zip_head = vec![0x50, 0x4B, 0x03, 0x04, 0x00, 0x00, 0x00, 0x00];
    assert_eq!(detect_archive_type_from_head(&zip_head), ArchiveType::Zip);

    // ZIP end of central directory: 50 4B 05 06
    let zip_eocd = vec![0x50, 0x4B, 0x05, 0x06];
    assert_eq!(detect_archive_type_from_head(&zip_eocd), ArchiveType::Zip);
  }

  #[test]
  fn test_detect_archive_type_from_head_tar() {
    // TAR has "ustar" at offset 257
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
    // Gzip magic bytes: 1F 8B（无效的 gzip 数据）
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
  fn test_detect_archive_type_from_head_unknown() {
    let random_head = vec![0x00, 0x01, 0x02, 0x03];
    assert_eq!(detect_archive_type_from_head(&random_head), ArchiveType::Unknown);

    let empty_head = vec![];
    assert_eq!(detect_archive_type_from_head(&empty_head), ArchiveType::Unknown);

    let short_head = vec![0x50];
    assert_eq!(detect_archive_type_from_head(&short_head), ArchiveType::Unknown);
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

    // 有非 tar.gz hint，返回 Gz
    assert_eq!(
      detect_archive_type_with_hint(&gzip_head, Some("data.gz")),
      ArchiveType::Gz
    );
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

  #[tokio::test]
  async fn test_detect_archive_none_for_empty_content() {
    struct MockEmptyFS;

    #[async_trait]
    impl OpbxFileSystem for MockEmptyFS {
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
        Ok(Box::pin(tokio::io::empty()))
      }
    }

    let resource = Resource {
      endpoint: Endpoint::local_fs(),
      primary_path: ResourcePath::parse("/tmp/empty.tar.gz"),
      archive_context: None,
      filter_glob: None,
    };

    let result = detect_archive_type(&MockEmptyFS, &resource).await;
    assert!(result.is_none(), "空文件不应通过扩展名被标记为归档");
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
