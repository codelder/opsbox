use crate::odfs::{OpsEntry, OpsFileSystem, OpsFileType, OpsMetadata, OpsPath, OpsRead};
use async_trait::async_trait;
use async_zip::tokio::read::seek::ZipFileReader;
use futures_lite::io::AsyncReadExt;
use std::io;
use std::path::PathBuf;
use std::sync::Arc;
use tempfile::NamedTempFile;
use tokio::fs::File;
use tokio::io::BufReader;

/// ZIP 归档文件系统 Overlay
pub struct ZipOpsFS {
  // 保持对 temp_file 的引用以防止被删除 (RAII)
  _temp_file: Option<Arc<NamedTempFile>>,
  path: PathBuf,
}

impl ZipOpsFS {
  pub async fn new(path: PathBuf, temp_file: Option<NamedTempFile>) -> io::Result<Self> {
    let _temp_file = temp_file.map(Arc::new);
    Ok(Self { _temp_file, path })
  }

  // Helper to get a fresh reader owning its own file handle
  async fn new_reader(&self) -> io::Result<ZipFileReader<BufReader<File>>> {
    let file = File::open(&self.path).await?;
    let reader = BufReader::new(file);
    ZipFileReader::with_tokio(reader)
      .await
      .map_err(|e| io::Error::other(e.to_string()))
  }
}

#[async_trait]
impl OpsFileSystem for ZipOpsFS {
  fn name(&self) -> &str {
    "ZipOpsFS"
  }

  async fn metadata(&self, path: &OpsPath) -> io::Result<OpsMetadata> {
    let reader = self.new_reader().await?;
    let items = reader.file().entries();
    let target_name = path.as_str().trim_start_matches('/');

    // 1. Exact match (file)
    if let Some(index) = items
      .iter()
      .position(|e| e.filename().as_str().unwrap_or("") == target_name)
    {
      let entry = &items[index];
      return Ok(OpsMetadata {
        name: entry.filename().as_str().unwrap_or("unknown").to_string(),
        file_type: if entry.dir().unwrap_or(false) {
          OpsFileType::Directory
        } else {
          OpsFileType::File
        },
        size: entry.uncompressed_size() as u64,
        modified: None,
        mode: 0,
        mime_type: None,
        compression: None,
        is_archive: false,
      });
    }

    // 2. Directory match (Simulated)
    // Check if any entry starts with "target_name/"
    let prefix = format!("{}/", target_name);
    if items
      .iter()
      .any(|e| e.filename().as_str().unwrap_or("").starts_with(&prefix))
    {
      return Ok(OpsMetadata {
        name: target_name.split('/').next_back().unwrap_or(target_name).to_string(),
        file_type: OpsFileType::Directory,
        size: 0,
        modified: None,
        mode: 0,
        mime_type: None,
        compression: None,
        is_archive: false,
      });
    }

    Err(io::Error::new(
      io::ErrorKind::NotFound,
      format!("Entry not found: {}", target_name),
    ))
  }

  async fn read_dir(&self, path: &OpsPath) -> io::Result<Vec<OpsEntry>> {
    let reader = self.new_reader().await?;
    let items = reader.file().entries();
    let dir_path = path.as_str().trim_start_matches('/');

    // Logic to simulate directory listing from flat zip paths
    // If dir_path is empty (root), look for entries with no '/' or just one section
    // If dir_path is "foo", look for "foo/bar" but not "foo/bar/baz"

    let mut entries = Vec::new();
    let mut seen_dirs = std::collections::HashSet::new();

    for entry in items {
      let name = entry
        .filename()
        .as_str()
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "Invalid filename"))?;

      // Filter entries belonging to this directory
      if name.starts_with(dir_path) {
        let relative = if dir_path.is_empty() {
          name
        } else if name.len() > dir_path.len() + 1 {
          // +1 for slash
          &name[dir_path.len() + 1..]
        } else {
          continue; // exact match or shorter
        };

        if relative.is_empty() {
          continue;
        } // Listing itself

        let (component, rest) = match relative.split_once('/') {
          Some((c, r)) => (c, Some(r)),
          None => (relative, None),
        };

        if rest.is_some() {
          // It's a subdirectory
          if seen_dirs.insert(component.to_string()) {
            entries.push(OpsEntry {
              name: component.to_string(),
              path: format!("{}/{}", dir_path, component), // Construct full path
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
          // It's a file
          entries.push(OpsEntry {
            name: component.to_string(),
            path: name.to_string(),
            metadata: OpsMetadata {
              name: component.to_string(),
              file_type: OpsFileType::File,
              size: entry.uncompressed_size(),
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

    Ok(entries)
  }

  async fn open_read(&self, path: &OpsPath) -> io::Result<OpsRead> {
    let mut reader = self.new_reader().await?;
    let name = path.as_str().trim_start_matches('/');

    if let Some(index) = reader
      .file()
      .entries()
      .iter()
      .position(|e| e.filename().as_str().unwrap_or("") == name)
    {
      // We need a reader that owns the underlying resources because OpsRead is 'static (Box<...>)
      // async_zip's reader_with_entry borrows 'reader'.
      // WE CANNOT return a borrow of local 'reader'.

      // To support this, we would typically need a self-referential struct or read the whole content.
      // Alternative: `async_zip` 0.0.17+ might support splitting?
      // If not, for valid RAII streaming without unsafe, we might have to:
      // 1. Read entire entry into memory (Cursor<Vec<u8>>) - safe but memory heavy.
      // 2. Modify architecture to allow borrowing (complex).

      // Given "Download & Cache" implies we have local disk, maybe we extract the *single file* to temp?
      // Or just read into memory for now if files are log files (usually text).
      // Let's implement Memory Buffering for now as a safe MVP.

      let mut entry_reader = reader
        .reader_with_entry(index)
        .await
        .map_err(|e| io::Error::other(e.to_string()))?;

      let mut buf = Vec::new();
      entry_reader
        .read_to_end(&mut buf)
        .await
        .map_err(|e| io::Error::other(e.to_string()))?;

      Ok(Box::pin(std::io::Cursor::new(buf)))
    } else {
      Err(io::Error::new(io::ErrorKind::NotFound, "Entry not found"))
    }
  }
}
