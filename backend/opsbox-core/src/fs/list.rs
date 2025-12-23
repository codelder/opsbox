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

    items.push(DiskItem {
      name,
      path: entry_path.to_string_lossy().to_string(),
      is_dir,
      is_symlink,
      size,
      modified,
      child_count,
      hidden_child_count,
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
