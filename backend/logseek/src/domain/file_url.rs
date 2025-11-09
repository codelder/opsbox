/// 统一文件 URL 标识符
///
/// 支持多种存储源的文件标识：
/// - 本地文件系统
/// - S3 兼容对象存储
/// - Tar/Tar.gz 压缩包内文件
/// - 远程 Agent 节点文件
use std::fmt;
use std::str::FromStr;
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum FileUrl {
  /// 本地文件系统
  /// 格式: `file:///path/to/file.log`
  Local { path: String },

  /// S3 兼容对象存储
  /// 格式: `s3://bucket/path/to/object` (使用默认配置)
  ///       `s3://profile:bucket/path/to/object` (使用指定配置)
  S3 {
    profile: Option<String>, // 配置名称（None 表示使用默认配置）
    bucket: String,
    key: String,
  },

  /// Tar 压缩包内的文件
  /// 格式: `tar+<base>:<entry>` 或 `tar.gz+<base>:<entry>`
  /// 例如: `tar+s3://bucket/archive.tar.gz:home/logs/app.log`
  TarEntry {
    /// 压缩格式 (tar 或 tar.gz)
    compression: TarCompression,
    /// 基础文件 URL（可以是 S3、本地等）
    base: Box<FileUrl>,
    /// tar 包内路径
    entry_path: String,
  },

  /// 本地目录内的文件（用于标识“来源根目录 + 相对路径”的场景）
  /// 格式: `dir+<base>:<entry>`，其中 base 通常为 file:///root
  DirEntry { base: Box<FileUrl>, entry_path: String },

  /// 远程 Agent 节点文件
  /// 格式: `agent://agent-id/path/to/file`
  Agent { agent_id: String, path: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TarCompression {
  /// 未压缩的 tar
  Tar,
  /// gzip 压缩的 tar.gz
  Gzip,
}

#[derive(Debug, Error)]
pub enum FileUrlError {
  #[error("无效的 URL 格式: {0}")]
  InvalidFormat(String),

  #[error("不支持的协议: {0}")]
  UnsupportedScheme(String),

  #[error("缺少必需字段: {0}")]
  MissingField(&'static str),

  #[error("嵌套层级过深（最多支持 1 层 tar 嵌套）")]
  TooManyNestingLevels,

  #[error("URL 编码错误: {0}")]
  EncodingError(String),
}

impl FileUrl {
  /// 创建本地文件 URL
  pub fn local(path: impl Into<String>) -> Self {
    Self::Local { path: path.into() }
  }

  /// 创建 S3 对象 URL（使用默认配置）
  pub fn s3(bucket: impl Into<String>, key: impl Into<String>) -> Self {
    Self::S3 {
      profile: None,
      bucket: bucket.into(),
      key: key.into(),
    }
  }

  /// 创建 S3 对象 URL（指定配置名称）
  pub fn s3_with_profile(profile: impl Into<String>, bucket: impl Into<String>, key: impl Into<String>) -> Self {
    Self::S3 {
      profile: Some(profile.into()),
      bucket: bucket.into(),
      key: key.into(),
    }
  }

  /// 创建 tar 包内文件 URL
  pub fn tar_entry(
    compression: TarCompression,
    base: FileUrl,
    entry_path: impl Into<String>,
  ) -> Result<Self, FileUrlError> {
    // 防止多层嵌套（tar 套 tar）
    if matches!(base, FileUrl::TarEntry { .. }) {
      return Err(FileUrlError::TooManyNestingLevels);
    }

    Ok(Self::TarEntry {
      compression,
      base: Box::new(base),
      entry_path: entry_path.into(),
    })
  }

  /// 创建目录内文件 URL（本地目录来源）
  pub fn dir_entry(base: FileUrl, entry_path: impl Into<String>) -> Result<Self, FileUrlError> {
    if matches!(base, FileUrl::TarEntry { .. } | FileUrl::DirEntry { .. }) {
      return Err(FileUrlError::TooManyNestingLevels);
    }
    Ok(Self::DirEntry {
      base: Box::new(base),
      entry_path: entry_path.into(),
    })
  }

  /// 创建 Agent 文件 URL
  pub fn agent(agent_id: impl Into<String>, path: impl Into<String>) -> Self {
    Self::Agent {
      agent_id: agent_id.into(),
      path: path.into(),
    }
  }

  /// 获取文件类型描述
  pub fn file_type(&self) -> &'static str {
    match self {
      Self::Local { .. } => "local",
      Self::S3 { .. } => "s3",
      Self::TarEntry { .. } => "tar-entry",
      Self::DirEntry { .. } => "dir-entry",
      Self::Agent { .. } => "agent",
    }
  }

  /// 判断是否为归档文件（tar/tar.gz）
  pub fn is_archive(&self) -> bool {
    matches!(self, Self::TarEntry { .. })
  }

  /// 获取人类可读的简短描述
  pub fn display_name(&self) -> String {
    match self {
      Self::Local { path } => path.split('/').next_back().unwrap_or(path).to_string(),
      Self::S3 { key, .. } => key.split('/').next_back().unwrap_or(key).to_string(),
      Self::TarEntry { entry_path, .. } => entry_path.split('/').next_back().unwrap_or(entry_path).to_string(),
      Self::DirEntry { entry_path, .. } => entry_path.split('/').next_back().unwrap_or(entry_path).to_string(),
      Self::Agent { path, .. } => path.split('/').next_back().unwrap_or(path).to_string(),
    }
  }
}

impl fmt::Display for FileUrl {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    match self {
      Self::Local { path } => {
        write!(f, "file://{}", path)
      }
      Self::S3 { profile, bucket, key } => {
        if let Some(p) = profile {
          write!(f, "s3://{}:{}/{}", p, bucket, key)
        } else {
          write!(f, "s3://{}/{}", bucket, key)
        }
      }
      Self::TarEntry {
        compression,
        base,
        entry_path,
      } => {
        let scheme = match compression {
          TarCompression::Tar => "tar",
          TarCompression::Gzip => "tar.gz",
        };
        write!(f, "{}+{}:{}", scheme, base, entry_path)
      }
      Self::DirEntry { base, entry_path } => {
        write!(f, "dir+{}:{}", base, entry_path)
      }
      Self::Agent { agent_id, path } => {
        if path.starts_with('/') {
          write!(f, "agent://{}{}", agent_id, path)
        } else {
          write!(f, "agent://{}/{}", agent_id, path)
        }
      }
    }
  }
}

/// 拼接本地 root 与相对路径，避免出现多余的 '/'
fn join_root_path(root: &str, rel: &str) -> String {
  if rel.is_empty() {
    return root.to_string();
  }
  if rel.starts_with('/') {
    return rel.to_string();
  }
  if root.ends_with('/') {
    format!("{}{}", root, rel)
  } else {
    format!("{}/{}", root, rel)
  }
}

/// 根据来源配置和相对路径构造 FileUrl 及其字符串 ID
pub fn build_file_url_for_result(source: &crate::domain::config::Source, rel_path: &str) -> Option<(FileUrl, String)> {
  use crate::domain::config::{Endpoint, Target};
  match (&source.endpoint, &source.target) {
    (Endpoint::S3 { profile, bucket }, Target::Archive { path }) => {
      // S3 归档：以真实对象 key 作为 base；内部统一按归档视图暴露
      let base = FileUrl::s3_with_profile(profile, bucket, path);
      // 内部压缩类型对显示无关紧要（前端以 archive+ 展示），保持兼容用 Gzip
      match FileUrl::tar_entry(TarCompression::Gzip, base, rel_path) {
        Ok(url) => {
          let id = url.to_string();
          Some((url, id))
        }
        Err(_) => None,
      }
    }
    (Endpoint::Local { root }, Target::Dir { path, .. }) => {
      // 以实际扫描根作为 base：root/path
      let real_base = if path == "." {
        root.clone()
      } else {
        join_root_path(root, path)
      };
      let base = FileUrl::local(real_base);
      match FileUrl::dir_entry(base, rel_path) {
        Ok(url) => {
          let id = url.to_string();
          Some((url, id))
        }
        Err(_) => None,
      }
    }
    (Endpoint::Local { root }, Target::Archive { path }) => {
      // 本地归档：以真实归档文件路径作为 base；内部统一按归档视图暴露
      let full = join_root_path(root, path);
      let base = FileUrl::local(full);
      // 内部压缩类型对显示无关紧要（前端以 archive+ 展示），保持兼容用 Gzip
      match FileUrl::tar_entry(TarCompression::Gzip, base, rel_path) {
        Ok(url) => {
          let id = url.to_string();
          Some((url, id))
        }
        Err(_) => None,
      }
    }
    (Endpoint::Local { root }, Target::Files { .. }) => {
      // 单文件也可以按 dir_entry 表示
      let base = FileUrl::local(root);
      match FileUrl::dir_entry(base, rel_path) {
        Ok(url) => {
          let id = url.to_string();
          Some((url, id))
        }
        Err(_) => None,
      }
    }
    (Endpoint::Local { root }, Target::All) => {
      let base = FileUrl::local(root);
      match FileUrl::dir_entry(base, rel_path) {
        Ok(url) => {
          let id = url.to_string();
          Some((url, id))
        }
        Err(_) => None,
      }
    }
    (Endpoint::Agent { agent_id, root }, Target::Archive { path }) => {
      // Agent 归档：以 agent://<id>/<root/path> 作为 base；内部统一按归档视图暴露
      let full = join_root_path(root, path);
      let base = FileUrl::agent(agent_id, full);
      match FileUrl::tar_entry(TarCompression::Gzip, base, rel_path) {
        Ok(url) => {
          let id = url.to_string();
          Some((url, id))
        }
        Err(_) => None,
      }
    }
    (Endpoint::Agent { agent_id, root }, Target::Dir { path, .. }) => {
      // Agent 目录：以 agent://<id>/<root/path or root> 作为 base，再附加 entry 相对路径
      let real_base = if path == "." {
        root.clone()
      } else {
        join_root_path(root, path)
      };
      let base = FileUrl::agent(agent_id, real_base);
      match FileUrl::dir_entry(base, rel_path) {
        Ok(url) => {
          let id = url.to_string();
          Some((url, id))
        }
        Err(_) => None,
      }
    }
    (Endpoint::Agent { agent_id, root }, Target::Files { .. }) => {
      // 单文件集合：以 agent://<id>/<root> 作为 base，entry 为相对路径
      let base = FileUrl::agent(agent_id, root);
      match FileUrl::dir_entry(base, rel_path) {
        Ok(url) => {
          let id = url.to_string();
          Some((url, id))
        }
        Err(_) => None,
      }
    }
    (Endpoint::Agent { agent_id, root }, Target::All) => {
      // 全部：以 agent://<id>/<root> 作为 base，entry 为相对路径
      let base = FileUrl::agent(agent_id, root);
      match FileUrl::dir_entry(base, rel_path) {
        Ok(url) => {
          let id = url.to_string();
          Some((url, id))
        }
        Err(_) => None,
      }
    }
    _ => None,
  }
}

impl FromStr for FileUrl {
  type Err = FileUrlError;

  fn from_str(s: &str) -> Result<Self, Self::Err> {
    // 处理 tar+<base>:<entry> 或 tar.gz+<base>:<entry> 格式
    // 需要先检查 tar.gz+，因为 tar+ 会匹配 tar.gz+ 的子串
    if let Some(after_scheme) = s.strip_prefix("tar.gz+") {
      // "tar.gz+".len() == 7
      // 使用 rsplitn 从右边分割，避免基础 URL 中的 : 干扰
      let mut parts: Vec<&str> = after_scheme.rsplitn(2, ':').collect();
      parts.reverse(); // rsplitn 返回反向顺序，需要反转
      if parts.len() != 2 {
        return Err(FileUrlError::InvalidFormat(
          "tar.gz URL 必须包含 ':' 分隔基础 URL 和条目路径".into(),
        ));
      }

      let base = Self::from_str(parts[0])?;
      let entry_path = parts[1].to_string();

      return Self::tar_entry(TarCompression::Gzip, base, entry_path);
    }

    if let Some(after_scheme) = s.strip_prefix("tar+") {
      // "tar+".len() == 4
      // 使用 rsplitn 从右边分割，避免基础 URL 中的 : 干扰
      let mut parts: Vec<&str> = after_scheme.rsplitn(2, ':').collect();
      parts.reverse(); // rsplitn 返回反向顺序，需要反转
      if parts.len() != 2 {
        return Err(FileUrlError::InvalidFormat(
          "tar URL 必须包含 ':' 分隔基础 URL 和条目路径".into(),
        ));
      }

      let base = Self::from_str(parts[0])?;
      let entry_path = parts[1].to_string();

      return Self::tar_entry(TarCompression::Tar, base, entry_path);
    }

    if let Some(after_scheme) = s.strip_prefix("dir+") {
      // "dir+".len() == 4
      let mut parts: Vec<&str> = after_scheme.rsplitn(2, ':').collect();
      parts.reverse();
      if parts.len() != 2 {
        return Err(FileUrlError::InvalidFormat(
          "dir URL 必须包含 ':' 分隔基础 URL 和条目路径".into(),
        ));
      }
      let base = Self::from_str(parts[0])?;
      let entry_path = parts[1].to_string();
      return Self::dir_entry(base, entry_path);
    }

    // 处理标准 URL 格式
    if let Some(scheme_end) = s.find("://") {
      let scheme = &s[..scheme_end];
      let after_scheme = &s[scheme_end + 3..];

      match scheme {
        "file" => Ok(Self::Local {
          path: after_scheme.to_string(),
        }),
        "s3" => {
          let parts: Vec<&str> = after_scheme.splitn(2, '/').collect();
          if parts.len() != 2 {
            return Err(FileUrlError::InvalidFormat(
              "s3 URL 格式应为 s3://bucket/key 或 s3://profile:bucket/key".into(),
            ));
          }

          // 检查是否包含 profile (格式: profile:bucket)
          let (profile, bucket) = if let Some(colon_idx) = parts[0].find(':') {
            let profile_name = parts[0][..colon_idx].to_string();
            let bucket_name = parts[0][colon_idx + 1..].to_string();
            (Some(profile_name), bucket_name)
          } else {
            (None, parts[0].to_string())
          };

          Ok(Self::S3 {
            profile,
            bucket,
            key: parts[1].to_string(),
          })
        }
        "agent" => {
          // 处理 agent://agent-id/path 格式
          // agent-id 可以是简单标识符或 host:port，但不支持 http:// 前缀

          // 首先检查是否包含协议前缀
          if after_scheme.starts_with("http://") || after_scheme.starts_with("https://") {
            return Err(FileUrlError::InvalidFormat(
              "agent URL 不支持协议前缀，请使用 agent://host:port/path 格式".into(),
            ));
          }

          let parts: Vec<&str> = after_scheme.splitn(2, '/').collect();
          if parts.is_empty() {
            return Err(FileUrlError::InvalidFormat(
              "agent URL 格式应为 agent://agent-id/path".into(),
            ));
          }

          let agent_id = parts[0].to_string();
          let path = if parts.len() == 2 {
            parts[1].to_string()
          } else {
            "/".to_string()
          };

          Ok(Self::Agent { agent_id, path })
        }
        _ => Err(FileUrlError::UnsupportedScheme(scheme.to_string())),
      }
    } else {
      Err(FileUrlError::InvalidFormat(format!(
        "缺少 URL scheme（应包含 ://）: {}",
        s
      )))
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_local_file() {
    let url = FileUrl::local("/var/log/app.log");
    assert_eq!(url.to_string(), "file:///var/log/app.log");
    assert_eq!(url.file_type(), "local");
    assert_eq!(url.display_name(), "app.log");
  }

  #[test]
  fn test_s3_object() {
    let url = FileUrl::s3("my-bucket", "logs/2024/app.log");
    assert_eq!(url.to_string(), "s3://my-bucket/logs/2024/app.log");
    assert_eq!(url.file_type(), "s3");
    assert_eq!(url.display_name(), "app.log");
  }

  #[test]
  fn test_tar_gz_s3() {
    let base = FileUrl::s3("backupdr", "archive.tar.gz");
    let url = FileUrl::tar_entry(TarCompression::Gzip, base, "home/logs/app.log").unwrap();
    assert_eq!(url.to_string(), "tar.gz+s3://backupdr/archive.tar.gz:home/logs/app.log");
    assert!(url.is_archive());
  }

  #[test]
  fn test_tar_local() {
    let base = FileUrl::local("/data/backup.tar");
    let url = FileUrl::tar_entry(TarCompression::Tar, base, "var/log/nginx.log").unwrap();
    assert_eq!(url.to_string(), "tar+file:///data/backup.tar:var/log/nginx.log");
  }

  #[test]
  fn test_tar_gz_agent() {
    let base = FileUrl::agent("agent-localhost", "/backup/archive.tar.gz");
    let url = FileUrl::tar_entry(TarCompression::Gzip, base, "logs/app.log").unwrap();
    assert_eq!(
      url.to_string(),
      "tar.gz+agent://agent-localhost/backup/archive.tar.gz:logs/app.log"
    );
  }

  #[test]
  fn test_agent_file() {
    let url = FileUrl::agent("prod-server-01", "/var/log/app.log");
    assert_eq!(url.to_string(), "agent://prod-server-01/var/log/app.log");
    assert_eq!(url.file_type(), "agent");
  }

  #[test]
  fn test_agent_file_with_host_port() {
    let url = FileUrl::agent("192.168.50.146:4001", "logs/app.log");
    assert_eq!(url.to_string(), "agent://192.168.50.146:4001/logs/app.log");
    assert_eq!(url.file_type(), "agent");

    // 测试解析
    let parsed: FileUrl = "agent://192.168.50.146:4001/logs/app.log".parse().unwrap();
    match parsed {
      FileUrl::Agent { agent_id, path } => {
        assert_eq!(agent_id, "192.168.50.146:4001");
        assert_eq!(path, "logs/app.log");
      }
      _ => panic!("Expected Agent URL"),
    }
  }

  #[test]
  fn test_agent_file_with_standard_id() {
    let url = FileUrl::agent("agent-localhost", "logs/app.log");
    assert_eq!(url.to_string(), "agent://agent-localhost/logs/app.log");
    assert_eq!(url.file_type(), "agent");

    // 测试解析
    let parsed: FileUrl = "agent://agent-localhost/logs/app.log".parse().unwrap();
    match parsed {
      FileUrl::Agent { agent_id, path } => {
        assert_eq!(agent_id, "agent-localhost");
        assert_eq!(path, "logs/app.log");
      }
      _ => panic!("Expected Agent URL"),
    }
  }

  #[test]
  fn test_build_file_url_for_agent_dir() {
    use crate::domain::config::{Endpoint, Source, Target};
    let source = Source {
      endpoint: Endpoint::Agent {
        agent_id: "agent-a".into(),
        root: "/data".into(),
      },
      target: Target::Dir {
        path: "apps".into(),
        recursive: true,
      },
      filter_glob: None,
      display_name: None,
    };
    let (url, id) = build_file_url_for_result(&source, "logs/app.log").unwrap();
    assert_eq!(id, "dir+agent://agent-a/data/apps:logs/app.log");
    match url {
      FileUrl::DirEntry { base, entry_path } => {
        assert_eq!(entry_path, "logs/app.log");
        match *base {
          FileUrl::Agent { agent_id, path } => {
            assert_eq!(agent_id, "agent-a");
            assert_eq!(path, "/data/apps");
          }
          _ => panic!("expected agent base"),
        }
      }
      _ => panic!("expected dir-entry"),
    }
  }

  #[test]
  fn test_agent_file_rejects_http_prefix() {
    // 测试 http:// 前缀被拒绝
    assert!(
      "agent://http://192.168.50.146:4001/logs/app.log"
        .parse::<FileUrl>()
        .is_err()
    );
    assert!(
      "agent://https://192.168.50.146:4001/logs/app.log"
        .parse::<FileUrl>()
        .is_err()
    );

    // 测试其他格式仍然有效
    assert!("agent://192.168.50.146:4001/logs/app.log".parse::<FileUrl>().is_ok());
    assert!("agent://agent-localhost/logs/app.log".parse::<FileUrl>().is_ok());
  }

  #[test]
  fn test_parse_s3() {
    let url: FileUrl = "s3://my-bucket/path/to/file.log".parse().unwrap();
    match url {
      FileUrl::S3 { profile, bucket, key } => {
        assert_eq!(profile, None);
        assert_eq!(bucket, "my-bucket");
        assert_eq!(key, "path/to/file.log");
      }
      _ => panic!("Expected S3 URL"),
    }
  }

  #[test]
  fn test_s3_with_profile() {
    let url = FileUrl::s3_with_profile("prod", "my-bucket", "logs/app.log");
    assert_eq!(url.to_string(), "s3://prod:my-bucket/logs/app.log");

    // 测试解析
    let parsed: FileUrl = "s3://dev:bucket/key".parse().unwrap();
    match parsed {
      FileUrl::S3 { profile, bucket, key } => {
        assert_eq!(profile, Some("dev".to_string()));
        assert_eq!(bucket, "bucket");
        assert_eq!(key, "key");
      }
      _ => panic!("Expected S3 URL with profile"),
    }
  }

  #[test]
  fn test_parse_tar_gz_s3() {
    let url: FileUrl = "tar.gz+s3://bucket/archive.tar.gz:logs/app.log".parse().unwrap();
    match url {
      FileUrl::TarEntry {
        compression,
        base,
        entry_path,
      } => {
        assert_eq!(compression, TarCompression::Gzip);
        assert_eq!(entry_path, "logs/app.log");
        match *base {
          FileUrl::S3 { profile, bucket, key } => {
            assert_eq!(profile, None);
            assert_eq!(bucket, "bucket");
            assert_eq!(key, "archive.tar.gz");
          }
          _ => panic!("Expected S3 base"),
        }
      }
      _ => panic!("Expected TarEntry URL"),
    }
  }

  #[test]
  fn test_nested_tar_rejected() {
    let base1 = FileUrl::s3("bucket", "archive1.tar.gz");
    let base2 = FileUrl::tar_entry(TarCompression::Gzip, base1, "inner.tar.gz").unwrap();
    let result = FileUrl::tar_entry(TarCompression::Gzip, base2, "file.log");
    assert!(result.is_err());
  }

  #[test]
  fn test_roundtrip() {
    let urls = vec![
      "file:///var/log/app.log",
      "s3://my-bucket/logs/app.log",
      "tar.gz+s3://bucket/archive.tar.gz:home/logs/app.log",
      "agent://server-01/var/log/app.log",
      "tar.gz+agent://agent-localhost/backup/archive.tar.gz:logs/app.log",
    ];

    for url_str in urls {
      let url: FileUrl = url_str.parse().unwrap();
      assert_eq!(url.to_string(), url_str);
    }
  }
}
