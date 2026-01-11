use crate::odfs::{OpsEntry, OpsFileSystem, OpsFileType, OpsMetadata, OpsPath, OpsRead};
use async_trait::async_trait;
use std::io;
use std::path::PathBuf;
use std::sync::Arc;
use tempfile::NamedTempFile;
use tokio::fs::File;
use tokio::io::AsyncReadExt;
use tokio::io::BufReader;
use tokio_stream::StreamExt;
use tokio_tar::Archive;

/// Tar 归档文件系统 Overlay
pub struct TarOpsFS {
  _temp_file: Option<Arc<NamedTempFile>>,
  path: PathBuf,
}

impl TarOpsFS {
  pub async fn new(path: PathBuf, temp_file: Option<NamedTempFile>) -> io::Result<Self> {
    let _temp_file = temp_file.map(Arc::new);
    Ok(Self { _temp_file, path })
  }

  // New reader for streaming
  async fn new_archive(&self) -> io::Result<Archive<BufReader<File>>> {
    let file = File::open(&self.path).await?;
    let reader = BufReader::new(file);
    Ok(Archive::new(reader))
  }
}

#[async_trait]
impl OpsFileSystem for TarOpsFS {
  fn name(&self) -> &str {
    "TarOpsFS"
  }

  async fn metadata(&self, path: &OpsPath) -> io::Result<OpsMetadata> {
    // Scanning tar for metadata is expensive but necessary without index
    let mut archive = self.new_archive().await?;
    let mut entries = archive.entries().map_err(io::Error::other)?;

    let target = path.as_str().trim_start_matches('/');

    while let Some(entry) = entries.next().await {
      let entry = entry.map_err(io::Error::other)?;
      let path_cow = entry.path().map_err(io::Error::other)?;
      let entry_path = path_cow.to_string_lossy();
      let entry_path_str = entry_path.trim_end_matches('/');

      if entry_path_str == target {
        let size = entry.header().size().unwrap_or(0);
        let is_dir = entry.header().entry_type().is_dir();

        return Ok(OpsMetadata {
          name: target.split('/').next_back().unwrap_or("unknown").to_string(),
          file_type: if is_dir {
            OpsFileType::Directory
          } else {
            OpsFileType::File
          },
          size,
          modified: None,
          mode: 0,
          mime_type: None,
          compression: None,
          is_archive: false,
        });
      }
    }

    Err(io::Error::new(io::ErrorKind::NotFound, "Entry not found"))
  }

  async fn read_dir(&self, path: &OpsPath) -> io::Result<Vec<OpsEntry>> {
    let mut archive = self.new_archive().await?;
    let mut entries = archive.entries().map_err(io::Error::other)?;

    let dir_path = path.as_str().trim_start_matches('/');
    let mut result = Vec::new();
    let mut seen = std::collections::HashSet::new();

    let prefix = if dir_path.is_empty() {
      "".to_string()
    } else {
      format!("{}/", dir_path)
    };

    while let Some(entry) = entries.next().await {
      let entry = entry.map_err(io::Error::other)?;
      let path_cow = entry.path().map_err(io::Error::other)?;
      let name = path_cow.to_string_lossy().to_string();

      if name.starts_with(&prefix) {
        let relative = &name[prefix.len()..];

        if relative.is_empty() {
          continue;
        }

        let (component, rest) = match relative.split_once('/') {
          Some((c, r)) => (c, Some(r)),
          None => (relative, None),
        };

        // Directory component
        if rest.is_some() || entry.header().entry_type().is_dir() {
          if seen.insert(component.to_string()) {
            result.push(OpsEntry {
              name: component.to_string(),
              path: if dir_path.is_empty() {
                component.to_string()
              } else {
                format!("{}/{}", dir_path, component)
              },
              metadata: OpsMetadata {
                name: component.to_string(),
                file_type: OpsFileType::Directory,
                size: 0,
                modified: None,
                mode: 0,
                mime_type: None,
                compression: None,
                is_archive: false,
              },
            });
          }
        } else {
          // File component
          result.push(OpsEntry {
            name: component.to_string(),
            path: name.clone(),
            metadata: OpsMetadata {
              name: component.to_string(),
              file_type: OpsFileType::File,
              size: entry.header().size().unwrap_or(0),
              modified: None,
              mode: 0,
              mime_type: None,
              compression: None,
              is_archive: false,
            },
          });
        }
      }
    }

    Ok(result)
  }

  async fn open_read(&self, path: &OpsPath) -> io::Result<OpsRead> {
    // We have to scan to find the entry, then read it.
    // Problem: tokio-tar Entry consumes the archive stream.
    // We can't return the Entry directly easily if it borrows from the local function variable `archive`.
    // We need an "Owned Entry Reader" or read to memory.

    // Similar strategy to Zip: Read into memory for now.
    // Tar entries are sequential, so we must scan.

    let mut archive = self.new_archive().await?;
    let mut entries = archive.entries().map_err(io::Error::other)?;
    let target = path.as_str().trim_start_matches('/');

    while let Some(entry) = entries.next().await {
      let mut entry = entry.map_err(io::Error::other)?;
      let path_cow = entry.path().map_err(io::Error::other)?;

      if path_cow.to_string_lossy() == target {
        let mut buf = Vec::new();
        entry.read_to_end(&mut buf).await?;
        return Ok(Box::pin(std::io::Cursor::new(buf)));
      }
    }

    Err(io::Error::new(io::ErrorKind::NotFound, "Entry not found"))
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::odfs::OpsPath;
  use tempfile::NamedTempFile;

  async fn create_test_tar() -> NamedTempFile {
    let file = NamedTempFile::new().unwrap();
    {
      let mut builder = tar::Builder::new(file.as_file());

      // Add a file
      let mut header = tar::Header::new_gnu();
      header.set_path("test.txt").unwrap();
      header.set_size(13);
      header.set_cksum();
      builder.append_data(&mut header, "test.txt", b"hello content".as_slice()).unwrap();

      // Add a directory entry (optional but good for test_read_dir)
      let mut dir_header = tar::Header::new_gnu();
      dir_header.set_path("logs/").unwrap();
      dir_header.set_entry_type(tar::EntryType::Directory);
      dir_header.set_size(0);
      dir_header.set_cksum();
      builder.append_data(&mut dir_header, "logs/", b"".as_slice()).unwrap();

      // Add file in directory
      let mut log_header = tar::Header::new_gnu();
      log_header.set_path("logs/app.log").unwrap();
      log_header.set_size(8);
      log_header.set_cksum();
      builder.append_data(&mut log_header, "logs/app.log", b"log data".as_slice()).unwrap();

      builder.finish().unwrap();
    }
    file
  }

  #[tokio::test]
  async fn test_tar_opsfs_metadata() {
    let tar_file = create_test_tar().await;
    let fs = TarOpsFS::new(tar_file.path().to_path_buf(), None).await.unwrap();

    let meta = fs.metadata(&OpsPath::new("test.txt")).await.unwrap();
    assert_eq!(meta.name, "test.txt");
    assert_eq!(meta.size, 13);
    assert!(matches!(meta.file_type, OpsFileType::File));

    let meta_dir = fs.metadata(&OpsPath::new("logs")).await.unwrap();
    assert!(matches!(meta_dir.file_type, OpsFileType::Directory));
  }

  #[tokio::test]
  async fn test_tar_opsfs_read_dir() {
    let tar_file = create_test_tar().await;
    let fs = TarOpsFS::new(tar_file.path().to_path_buf(), None).await.unwrap();

    // Root
    let entries = fs.read_dir(&OpsPath::new("")).await.unwrap();
    assert!(entries.iter().any(|e| e.name == "test.txt"));
    assert!(entries.iter().any(|e| e.name == "logs"));

    // Subdir
    let entries = fs.read_dir(&OpsPath::new("logs")).await.unwrap();
    assert!(entries.iter().any(|e| e.name == "app.log"));
  }

  #[tokio::test]
  async fn test_tar_opsfs_open_read() {
    let tar_file = create_test_tar().await;
    let fs = TarOpsFS::new(tar_file.path().to_path_buf(), None).await.unwrap();

    let mut reader = fs.open_read(&OpsPath::new("test.txt")).await.unwrap();
    let mut content = String::new();
    reader.read_to_string(&mut content).await.unwrap();
    assert_eq!(content, "hello content");
  }

  #[tokio::test]
  async fn test_tar_opsfs_metadata_not_found() {
    let tar_file = create_test_tar().await;
    let fs = TarOpsFS::new(tar_file.path().to_path_buf(), None).await.unwrap();

    let result = fs.metadata(&OpsPath::new("missing.txt")).await;
    assert!(result.is_err());
    assert_eq!(result.unwrap_err().kind(), io::ErrorKind::NotFound);
  }
}
