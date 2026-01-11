use crate::agent::{AgentClient, models::AgentListResponse};
use crate::odfs::{OpsEntry, OpsFileSystem, OpsFileType, OpsMetadata, OpsPath, OpsRead};
use async_trait::async_trait;
use futures::TryStreamExt;
use percent_encoding::{utf8_percent_encode, NON_ALPHANUMERIC};
use std::io;
use tokio_util::io::StreamReader;

pub struct AgentOpsFS {
  client: AgentClient,
}

impl AgentOpsFS {
  pub fn new(agent_id: impl Into<String>, base_url: impl Into<String>) -> Self {
    Self {
      client: AgentClient::new(
        agent_id.into(),
        base_url.into(),
        Some(std::time::Duration::from_secs(30)),
      ),
    }
  }
}

#[async_trait]
impl OpsFileSystem for AgentOpsFS {
  fn name(&self) -> &str {
    "AgentOpsFS"
  }

  async fn metadata(&self, path: &OpsPath) -> io::Result<OpsMetadata> {
    // Agent doesn't have a direct metadata endpoint commonly exposed,
    // usually we use list_files.
    // However, list_files on a FILE usually returns that file's info?
    // Let's assume listing the PARENT and finding the item is safer,
    // OR just listing the path itself.

    // Agent list_files on parent directory approach
    // If path is a file, Agent might return array with 1 item?
    // If path is a directory, it returns contents.

    // Limitation: We might not distinguish "Directory itself" vs "Contents of Directory" easily
    // unless the items list matches the name or is empty?
    // Actually Agent `list_files` lists CONTENTS of a directory.
    // If passed a file path, does it list the file?

    // If we assume standard unix ls behavior:
    // ls file -> lists file.
    // ls dir -> lists contents.

    // But we need Metadata of the TARGET, not contents.
    // If we can't distinguish, we might need a `stat` endpoint.
    // For now, we stub metadata as "Directory" if we can list it?
    // Or we fail?

    // Workaround: Use list_files on PARENT directory and find the item.
    let p = std::path::Path::new(path.as_str());
    let parent = p.parent().unwrap_or(std::path::Path::new("/"));
    let name_to_find = p.file_name().unwrap_or_default().to_string_lossy();

    if path.as_str() == "/" || path.as_str().is_empty() {
        return Ok(OpsMetadata {
            name: "/".to_string(),
            file_type: OpsFileType::Directory,
            size: 0,
            modified: None,
            mode: 0755,
            mime_type: None,
            compression: None,
            is_archive: false,
        });
    }

    let parent_str = parent.to_string_lossy();
    let encoded_parent = utf8_percent_encode(&parent_str, NON_ALPHANUMERIC).to_string();
    let list_url = format!("/api/v1/list_files?path={}", encoded_parent);
    let list_resp: AgentListResponse = self.client.get(&list_url).await
        .map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))?;

    if let Some(item) = list_resp.items.into_iter().find(|i| i.name == name_to_find) {
         Ok(OpsMetadata {
            name: item.name,
            file_type: if item.is_dir {
                OpsFileType::Directory
            } else if item.is_symlink {
                OpsFileType::Symlink
            } else {
                OpsFileType::File
            },
            size: item.size.unwrap_or(0),
            modified: item.modified.map(|t| std::time::UNIX_EPOCH + std::time::Duration::from_secs(t as u64)),
            mode: 0,
            mime_type: item.mime_type,
            compression: None,
            is_archive: false, // Could check extension
        })
    } else {
        Err(io::Error::new(io::ErrorKind::NotFound, "Path not found"))
    }
  }

  async fn read_dir(&self, path: &OpsPath) -> io::Result<Vec<OpsEntry>> {
    let resp: AgentListResponse = self.client.get_with_query("/api/v1/list_files", &[("path", path.as_str())]).await
         .map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))?;

    let entries = resp.items.into_iter().map(|item| {
        OpsEntry {
            name: item.name.clone(),
            path: item.path.clone(), // Agent returns absolute path?
            metadata: OpsMetadata {
                name: item.name,
                file_type: if item.is_dir { OpsFileType::Directory } else { OpsFileType::File }, // Simplify
                size: item.size.unwrap_or(0),
            modified: item.modified.map(|t| std::time::UNIX_EPOCH + std::time::Duration::from_secs(t as u64)),
                mode: 0,
                mime_type: item.mime_type,
                compression: None,
                is_archive: false,
            }
        }
    }).collect();

    Ok(entries)
  }

  async fn open_read(&self, path: &OpsPath) -> io::Result<OpsRead> {
      let resp = self.client.get_raw_with_query("/api/v1/file_raw", &[("path", path.as_str())]).await
          .map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))?;

      let stream = resp.bytes_stream().map_err(std::io::Error::other);
      let reader = StreamReader::new(stream);

      Ok(Box::pin(reader))
  }
}
