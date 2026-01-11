use std::time::SystemTime;

/// 文件类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpsFileType {
  File,
  Directory,
  Symlink,
  Unknown,
}

/// 资源元数据
#[derive(Debug, Clone)]
pub struct OpsMetadata {
  /// 文件名
  pub name: String,
  /// 文件类型
  pub file_type: OpsFileType,
  /// 文件大小（字节）
  pub size: u64,
  /// 修改时间
  pub modified: Option<SystemTime>,
  /// 文件模式（Unix permission bits）
  pub mode: u32,

  /// 内容类型（MIME type），通过魔数或服务端信息推断
  pub mime_type: Option<String>,

  /// 压缩算法（如果有），例如 "gzip", "xz", "zstd"
  /// 如果为 None，表示未压缩（或已经是解压后的视图）
  pub compression: Option<String>,

  /// 是否为归档文件（如 tar, zip）
  /// 如果为 true，该文件可能被挂载为 OpsFileSystem
  pub is_archive: bool,
}

impl OpsMetadata {
  pub fn is_dir(&self) -> bool {
    self.file_type == OpsFileType::Directory
  }

  pub fn is_file(&self) -> bool {
    self.file_type == OpsFileType::File
  }
}

/// 目录条目
#[derive(Debug, Clone)]
pub struct OpsEntry {
  /// 条目路径（相对于父目录）
  pub name: String,
  /// 完整 ORL 路径（可选，便于上层使用）
  pub path: String,
  /// 元数据
  pub metadata: OpsMetadata,
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_metadata_helpers() {
    let mut meta = OpsMetadata {
        name: "test".to_string(),
        file_type: OpsFileType::File,
        size: 0,
        modified: None,
        mode: 0,
        mime_type: None,
        compression: None,
        is_archive: false,
    };

    assert!(meta.is_file());
    assert!(!meta.is_dir());

    meta.file_type = OpsFileType::Directory;
    assert!(!meta.is_file());
    assert!(meta.is_dir());
  }
}
