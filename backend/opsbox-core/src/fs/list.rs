use serde::{Deserialize, Serialize};
use std::path::Path;
use tokio::fs;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiskItem {
  pub name: String,
  pub path: String,
  pub is_dir: bool,
  pub is_symlink: bool,
  pub size: Option<u64>,
  pub modified: Option<i64>,
  pub child_count: Option<u32>,
  pub hidden_child_count: Option<u32>,
  pub mime_type: Option<String>,
}

pub async fn list_directory<P: AsRef<Path>>(path: P) -> Result<Vec<DiskItem>, String> {
  let path = path.as_ref();
  let mut read_dir = fs::read_dir(path).await.map_err(|e| e.to_string())?;
  let mut items = Vec::new();

  while let Ok(Some(entry)) = read_dir.next_entry().await {
    let entry_path = entry.path();
    let entry_type = match entry.file_type().await {
      Ok(t) => t,
      Err(_) => continue,
    };

    let is_symlink = entry_type.is_symlink();
    let (is_dir, meta) = if is_symlink {
      match fs::metadata(&entry_path).await {
        Ok(m) => (m.is_dir(), m),
        Err(_) => {
          if let Ok(m) = entry.metadata().await {
            (m.is_dir(), m)
          } else {
            continue;
          }
        }
      }
    } else {
      match entry.metadata().await {
        Ok(m) => (m.is_dir(), m),
        Err(_) => continue,
      }
    };

    let name = entry.file_name().to_string_lossy().to_string();
    let size = if is_dir { None } else { Some(meta.len()) };
    let modified = meta
      .modified()
      .ok()
      .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
      .map(|d| d.as_secs() as i64);

    let (child_count, hidden_child_count) = if is_dir {
      if let Ok(mut d) = fs::read_dir(&entry_path).await {
        let mut count = 0;
        let mut hidden = 0;
        while let Ok(Some(e)) = d.next_entry().await {
          count += 1;
          if e.file_name().to_string_lossy().starts_with('.') {
            hidden += 1;
          }
        }
        (Some(count), Some(hidden))
      } else {
        (Some(0), Some(0))
      }
    } else {
      (None, None)
    };

    let mime_type = if is_dir {
      None
    } else if let Ok(mut f) = tokio::fs::File::open(&entry_path).await {
      use tokio::io::AsyncReadExt;
      let mut buf = [0u8; 1024];
      let n = f.read(&mut buf).await.unwrap_or(0);
      if n > 0 {
        infer::get(&buf[..n]).map(|m| m.mime_type().to_string())
      } else {
        None
      }
    } else {
      None
    };

    items.push(DiskItem {
      name,
      path: entry_path.to_string_lossy().to_string(),
      is_dir,
      is_symlink,
      size,
      modified,
      child_count,
      hidden_child_count,
      mime_type,
    });
  }

  // Sort: Dirs first, then files
  items.sort_by(|a, b| {
    if a.is_dir == b.is_dir {
      a.name.cmp(&b.name)
    } else if a.is_dir {
      std::cmp::Ordering::Less
    } else {
      std::cmp::Ordering::Greater
    }
  });

  Ok(items)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_disk_item_serialization() {
        let item = DiskItem {
            name: "test.log".to_string(),
            path: "/var/log/test.log".to_string(),
            is_dir: false,
            is_symlink: false,
            size: Some(1024),
            modified: Some(1234567890),
            child_count: None,
            hidden_child_count: None,
            mime_type: Some("text/plain".to_string()),
        };

        let json = serde_json::to_string(&item).unwrap();
        assert!(json.contains("test.log"));
        assert!(json.contains("1024"));

        let deserialized: DiskItem = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.name, "test.log");
        assert!(!deserialized.is_dir);
        assert_eq!(deserialized.size, Some(1024));
    }

    #[test]
    fn test_disk_item_directory() {
        let item = DiskItem {
            name: "logs".to_string(),
            path: "/var/logs".to_string(),
            is_dir: true,
            is_symlink: false,
            size: None,
            modified: Some(1234567890),
            child_count: Some(10),
            hidden_child_count: Some(2),
            mime_type: None,
        };

        assert!(item.is_dir);
        assert_eq!(item.child_count, Some(10));
        assert_eq!(item.hidden_child_count, Some(2));
        assert_eq!(item.size, None);
    }

    #[test]
    fn test_disk_item_symlink() {
        let item = DiskItem {
            name: "link".to_string(),
            path: "/tmp/link".to_string(),
            is_dir: false,
            is_symlink: true,
            size: Some(100),
            modified: None,
            child_count: None,
            hidden_child_count: None,
            mime_type: None,
        };

        assert!(item.is_symlink);
        assert!(!item.is_dir);
    }

    #[tokio::test]
    async fn test_list_directory_nonexistent() {
        let result = list_directory("/nonexistent/path/12345").await;
        assert!(result.is_err());
    }
}
