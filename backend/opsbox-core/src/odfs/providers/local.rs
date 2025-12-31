use crate::odfs::{OpsEntry, OpsFileSystem, OpsFileType, OpsMetadata, OpsPath, OpsRead};
use async_trait::async_trait;
use std::io;
use std::path::{Path, PathBuf};
use tokio::fs;

/// 本地文件系统提供者
///
/// 将 ORL 路径映射到本地文件系统
/// 例如: `orl://local/var/log/syslog` -> `/var/log/syslog`
pub struct LocalOpsFS {
  root: PathBuf,
}

impl LocalOpsFS {
  /// 创建新的本地文件系统提供者
  /// root: 根目录限制（可选，默认为 "/"）
  pub fn new(root: Option<PathBuf>) -> Self {
    Self {
      root: root.unwrap_or_else(|| PathBuf::from("/")),
    }
  }

  /// 将 OpsPath 转换为本地 PathBuf
  fn resolve_path(&self, path: &OpsPath) -> io::Result<PathBuf> {
    let p = path.as_str();
    let p = p.trim_start_matches('/');

    // 简单路径拼接，生产环境需注意路径遍历攻击防护
    let full_path = self.root.join(p);
    Ok(full_path)
  }
}

#[async_trait]
impl OpsFileSystem for LocalOpsFS {
  fn name(&self) -> &str {
    "LocalOpsFS"
  }

  async fn metadata(&self, path: &OpsPath) -> io::Result<OpsMetadata> {
    let fs_path = self.resolve_path(path)?;
    let metadata = fs::metadata(&fs_path).await?;

    let file_type = if metadata.is_dir() {
      OpsFileType::Directory
    } else if metadata.is_symlink() {
      OpsFileType::Symlink
    } else {
      OpsFileType::File
    };

    // TODO: 真正的 Magic Bytes 检测
    // 这里暂时使用扩展名推断，作为占位符
    let (mime_type, is_archive, compression) = if metadata.is_file() {
      detect_file_type(&fs_path).await
    } else {
      (None, false, None)
    };

    Ok(OpsMetadata {
      name: fs_path.file_name().unwrap_or_default().to_string_lossy().to_string(),
      file_type,
      size: metadata.len(),
      modified: metadata.modified().ok(),
      mode: 0o644, // TODO: cross-platform mode
      mime_type,
      compression,
      is_archive,
    })
  }

  async fn read_dir(&self, path: &OpsPath) -> io::Result<Vec<OpsEntry>> {
    let fs_path = self.resolve_path(path)?;
    let mut read_dir = fs::read_dir(fs_path).await?;
    let mut entries = Vec::new();

    while let Some(entry) = read_dir.next_entry().await? {
      let metadata = entry.metadata().await?;
      let name = entry.file_name().to_string_lossy().to_string();

      let file_type = if metadata.is_dir() {
        OpsFileType::Directory
      } else if metadata.is_symlink() {
        OpsFileType::Symlink
      } else {
        OpsFileType::File
      };

      // read_dir 不做深度检测
      let ops_metadata = OpsMetadata {
        name: name.clone(),
        file_type,
        size: metadata.len(),
        modified: metadata.modified().ok(),
        mode: 0,
        mime_type: None,
        compression: None,
        is_archive: false, // 列表时暂不检测，性能优先
      };

      entries.push(OpsEntry {
        name,
        path: path.join(&entry.file_name().to_string_lossy()).as_str().to_string(),
        metadata: ops_metadata,
      });
    }

    Ok(entries)
  }

  async fn open_read(&self, path: &OpsPath) -> io::Result<OpsRead> {
    let fs_path = self.resolve_path(path)?;
    let file = fs::File::open(fs_path).await?;
    Ok(Box::pin(file))
  }
}

// 简单的文件类型检测帮助函数
// 未来应该被替换为基于 magic bytes 的实现
async fn detect_file_type(path: &Path) -> (Option<String>, bool, Option<String>) {
  // 简单实现：仅基于扩展名，后续迭代增强
  if let Some(ext) = path.extension().and_then(|s| s.to_str()) {
    match ext {
      "gz" => (Some("application/gzip".to_string()), false, Some("gzip".to_string())),
      "tar" => (Some("application/x-tar".to_string()), true, None),
      "zip" => (Some("application/zip".to_string()), true, None),
      _ => (None, false, None),
    }
  } else {
    (None, false, None)
  }
}
