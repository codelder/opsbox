use serde::{Deserialize, Serialize};

/// 目标集合（查什么），路径均相对 endpoint.root（Local）或 endpoint.subpath（Agent）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Target {
  /// 目录
  Dir {
    path: String,
    #[serde(default = "default_true")]
    recursive: bool,
  },
  /// 文件清单
  Files { paths: Vec<String> },
  /// 归档（自动探测 tar/tar.gz/gz/zip；zip 暂不支持）
  Archive {
    path: String,
    /// 归档内的条目路径（如果要读取归档内的特定文件）
    #[serde(skip_serializing_if = "Option::is_none")]
    entry: Option<String>,
  },
}

fn default_true() -> bool {
  true
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_target_dir_serialization() {
    let target = Target::Dir {
      path: "/var/log".to_string(),
      recursive: true,
    };

    let json = serde_json::to_string(&target).unwrap();
    assert!(json.contains("\"type\":\"dir\""));
    assert!(json.contains("/var/log"));
    assert!(json.contains("\"recursive\":true"));

    let deserialized: Target = serde_json::from_str(&json).unwrap();
    match deserialized {
      Target::Dir { path, recursive } => {
        assert_eq!(path, "/var/log");
        assert!(recursive);
      }
      _ => panic!("Expected Dir variant"),
    }
  }

  #[test]
  fn test_target_dir_default_recursive() {
    let json = r#"{"type":"dir","path":"/test"}"#;
    let target: Target = serde_json::from_str(json).unwrap();

    match target {
      Target::Dir { recursive, .. } => assert!(recursive), // Should default to true
      _ => panic!("Expected Dir variant"),
    }
  }

  #[test]
  fn test_target_files_serialization() {
    let target = Target::Files {
      paths: vec!["file1.log".to_string(), "file2.log".to_string()],
    };

    let json = serde_json::to_string(&target).unwrap();
    assert!(json.contains("\"type\":\"files\""));
    assert!(json.contains("file1.log"));

    let deserialized: Target = serde_json::from_str(&json).unwrap();
    match deserialized {
      Target::Files { paths } => {
        assert_eq!(paths.len(), 2);
        assert_eq!(paths[0], "file1.log");
      }
      _ => panic!("Expected Files variant"),
    }
  }

  #[test]
  fn test_target_archive_serialization() {
    let target = Target::Archive {
      path: "logs.tar.gz".to_string(),
      entry: Some("inner/file.log".to_string()),
    };

    let json = serde_json::to_string(&target).unwrap();
    assert!(json.contains("\"type\":\"archive\""));
    assert!(json.contains("logs.tar.gz"));
    assert!(json.contains("inner/file.log"));

    let deserialized: Target = serde_json::from_str(&json).unwrap();
    match deserialized {
      Target::Archive { path, entry } => {
        assert_eq!(path, "logs.tar.gz");
        assert_eq!(entry, Some("inner/file.log".to_string()));
      }
      _ => panic!("Expected Archive variant"),
    }
  }

  #[test]
  fn test_target_archive_no_entry() {
    let target = Target::Archive {
      path: "logs.tar.gz".to_string(),
      entry: None,
    };

    let json = serde_json::to_string(&target).unwrap();
    // entry should be skipped when None
    assert!(!json.contains("\"entry\""));
  }

  #[test]
  fn test_default_true_helper() {
    assert!(default_true());
  }
}
