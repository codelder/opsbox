/// Unified File URL Identifier (Redesigned)
///
/// Scheme: `ls://<endpoint_type>/<endpoint_id>/<target_type>/<path>?<params>`
///
/// Examples:
/// - Local Dir: `ls://local/localhost/dir/var/log/nginx/access.log`
/// - Agent Dir: `ls://agent/web-01/dir/app/logs/error.log`
/// - S3 Archive: `ls://s3/prod:logs-bucket/archive/2023/10/data.tar.gz?entry=internal/service.log`
use std::fmt;
use std::str::FromStr;
use thiserror::Error;
use url::Url;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FileUrl {
  pub endpoint_type: EndpointType,
  pub endpoint_id: String,
  pub target_type: TargetType,
  pub path: String,
  pub entry_path: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EndpointType {
  Local,
  Agent,
  S3,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TargetType {
  Dir,
  Archive,
}

#[derive(Debug, Error)]
pub enum FileUrlError {
  #[error("Invalid URL format: {0}")]
  InvalidFormat(String),
  #[error("Unsupported scheme: {0}")]
  UnsupportedScheme(String),
  #[error("Invalid endpoint type: {0}")]
  InvalidEndpointType(String),
  #[error("Invalid target type: {0}")]
  InvalidTargetType(String),
  #[error("Missing required field: {0}")]
  MissingField(&'static str),
  #[error("URL parsing error: {0}")]
  ParseError(#[from] url::ParseError),
}

impl FileUrl {
  pub fn new(
    endpoint_type: EndpointType,
    endpoint_id: impl Into<String>,
    target_type: TargetType,
    path: impl Into<String>,
    entry_path: Option<String>,
  ) -> Self {
    Self {
      endpoint_type,
      endpoint_id: endpoint_id.into(),
      target_type,
      path: path.into(),
      entry_path,
    }
  }

  /// Check if it points to an entry inside an archive
  pub fn is_archive_entry(&self) -> bool {
    self.target_type == TargetType::Archive && self.entry_path.is_some()
  }

  /// Get a human-readable display name (usually the file name)
  pub fn display_name(&self) -> String {
    if let Some(entry) = &self.entry_path {
      entry.split('/').next_back().unwrap_or(entry).to_string()
    } else {
      self.path.split('/').next_back().unwrap_or(&self.path).to_string()
    }
  }
}

impl fmt::Display for FileUrl {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    let endpoint_type_str = match self.endpoint_type {
      EndpointType::Local => "local",
      EndpointType::Agent => "agent",
      EndpointType::S3 => "s3",
    };

    let target_type_str = match self.target_type {
      TargetType::Dir => "dir",
      TargetType::Archive => "archive",
    };

    // Construct the path part: /<endpoint_id>/<target_type>/<path>
    // Note: path should be percent-encoded if it contains special chars,
    // but here we assume the caller handles basic path safety or we rely on Url struct to encode.
    // To ensure correct encoding, we use the `url` crate to build the string.

    let mut url = Url::parse("ls://placeholder").map_err(|_| fmt::Error)?;
    url.set_host(Some(endpoint_type_str)).map_err(|_| fmt::Error)?;

    // We use path segments to ensure proper encoding
    let mut path_segments = vec![self.endpoint_id.as_str(), target_type_str];

    // Split the path into segments to avoid double encoding slashes if we just pushed the whole string
    // But wait, if we split by '/', we might break paths that actually contain encoded slashes?
    // For simplicity in this implementation, we assume `path` is a standard path string separated by '/'.
    for segment in self.path.split('/') {
      if !segment.is_empty() {
        path_segments.push(segment);
      }
    }

    url
      .path_segments_mut()
      .map_err(|_| fmt::Error)?
      .clear()
      .extend(path_segments);

    if let Some(entry) = &self.entry_path {
      url.query_pairs_mut().append_pair("entry", entry);
    }

    write!(f, "{}", url)
  }
}

impl FromStr for FileUrl {
  type Err = FileUrlError;

  fn from_str(s: &str) -> Result<Self, Self::Err> {
    let url = Url::parse(s)?;

    if url.scheme() != "ls" {
      return Err(FileUrlError::UnsupportedScheme(url.scheme().to_string()));
    }

    let endpoint_type = match url.host_str() {
      Some("local") => EndpointType::Local,
      Some("agent") => EndpointType::Agent,
      Some("s3") => EndpointType::S3,
      Some(other) => return Err(FileUrlError::InvalidEndpointType(other.to_string())),
      None => return Err(FileUrlError::MissingField("endpoint_type")),
    };

    let mut segments = url.path_segments().ok_or(FileUrlError::MissingField("path"))?;

    let endpoint_id = segments
      .next()
      .ok_or(FileUrlError::MissingField("endpoint_id"))?
      .to_string();
    let target_type_str = segments.next().ok_or(FileUrlError::MissingField("target_type"))?;

    let target_type = match target_type_str {
      "dir" => TargetType::Dir,
      "archive" => TargetType::Archive,
      other => return Err(FileUrlError::InvalidTargetType(other.to_string())),
    };

    // The rest of the segments form the path
    let path_parts: Vec<&str> = segments.collect();
    let path = path_parts.join("/"); // Reconstruct path
    // Note: paths are stored without leading slash to ensure consistency with Display

    let entry_path = url
      .query_pairs()
      .find(|(k, _)| k == "entry")
      .map(|(_, v)| v.to_string());

    Ok(FileUrl {
      endpoint_type,
      endpoint_id,
      target_type,
      path,
      entry_path,
    })
  }
}

/// Helper to join root and relative path
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

/// Entry source type (kept for compatibility with existing logic if needed, but mainly for mapping)
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum EntrySourceType {
  #[default]
  File,
  Tar,
  TarGz,
  Gz,
}

/// Build FileUrl from Source and relative path
pub fn build_file_url_for_result(source: &crate::domain::config::Source, rel_path: &str) -> Option<(FileUrl, String)> {
  build_file_url_for_result_with_source_type(source, rel_path, EntrySourceType::default())
}

pub fn build_file_url_for_result_with_source_type(
  source: &crate::domain::config::Source,
  rel_path: &str,
  _source_type: EntrySourceType,
) -> Option<(FileUrl, String)> {
  build_file_url_for_result_with_source_type_and_archive_path(source, rel_path, _source_type, None)
}

/// Build FileUrl from Source and relative path, with optional archive path override.
///
/// When the result is an archive entry, callers may provide `archive_path_override` as an absolute
/// archive file path (typically filled by Agent/Local side), so the resulting FileUrl becomes
/// unambiguous even with multiple roots.
pub fn build_file_url_for_result_with_archive_path(
  source: &crate::domain::config::Source,
  rel_path: &str,
  archive_path_override: Option<&str>,
) -> Option<(FileUrl, String)> {
  build_file_url_for_result_with_source_type_and_archive_path(
    source,
    rel_path,
    EntrySourceType::default(),
    archive_path_override,
  )
}

fn build_file_url_for_result_with_source_type_and_archive_path(
  source: &crate::domain::config::Source,
  rel_path: &str,
  _source_type: EntrySourceType,
  archive_path_override: Option<&str>,
) -> Option<(FileUrl, String)> {
  use crate::domain::config::{Endpoint, Target};

  let (endpoint_type, endpoint_id, base_path, _is_archive_target) = match &source.endpoint {
    Endpoint::Local { root } => (EndpointType::Local, "localhost".to_string(), root.clone(), false),
    Endpoint::Agent { agent_id, subpath } => (EndpointType::Agent, agent_id.clone(), subpath.clone(), false),
    Endpoint::S3 { profile, bucket } => {
      let id = format!("{}:{}", profile, bucket);
      (EndpointType::S3, id, String::new(), true)
    }
  };

  // Determine TargetType and full path
  match &source.target {
    Target::Dir { path, .. } => {
      let full_path = if path == "." {
        base_path
      } else {
        join_root_path(&base_path, path)
      };

      // For Dir target, the result is a file inside this dir
      // So path = full_path / rel_path
      let mut final_path = join_root_path(&full_path, rel_path);

      // For Local and Agent Dir, remove leading slash to ensure consistency
      // Display implementation splits by '/' and filters empty, which loses leading slash
      // So we store paths without leading slash to match what FromStr will parse
      if endpoint_type == EndpointType::Local || endpoint_type == EndpointType::Agent {
        final_path = normalize_path_segment(&final_path);
      }

      let url = FileUrl::new(endpoint_type, endpoint_id, TargetType::Dir, final_path, None);
      Some((url.clone(), url.to_string()))
    }
    Target::Files { .. } => {
      // For Files target, rel_path is likely the full path or relative to root?
      // In original logic: root/rel_path
      let mut final_path = join_root_path(&base_path, rel_path);

      // For Local and Agent Dir, remove leading slash to ensure consistency
      if endpoint_type == EndpointType::Local || endpoint_type == EndpointType::Agent {
        final_path = normalize_path_segment(&final_path);
      }

      let url = FileUrl::new(endpoint_type, endpoint_id, TargetType::Dir, final_path, None);
      Some((url.clone(), url.to_string()))
    }
    Target::Archive { path } => {
      // Archive target.
      // If Endpoint is S3, path is the object key.
      // If Endpoint is Local/Agent, path is relative to root/subpath.

      let mut archive_path = if let Some(override_path) = archive_path_override
        && endpoint_type != EndpointType::S3
      {
        override_path.to_string()
      } else if endpoint_type == EndpointType::S3 {
        path.clone()
      } else {
        join_root_path(&base_path, path)
      };

      // Remove leading slash to ensure consistency with FileUrl Display/FromStr behavior
      // Display splits by '/' and filters empty segments, which loses leading slash
      // So we store paths without leading slash to match what FromStr will parse
      if endpoint_type == EndpointType::Local || endpoint_type == EndpointType::Agent {
        archive_path = normalize_path_segment(&archive_path);
      }

      let url = FileUrl::new(
        endpoint_type,
        endpoint_id,
        TargetType::Archive,
        archive_path,
        Some(normalize_path_segment(rel_path)),
      );
      Some((url.clone(), url.to_string()))
    }
  }
}

fn normalize_path_segment(s: &str) -> String {
  let mut t = s;
  loop {
    if t.starts_with("./") {
      t = &t[2..];
      continue;
    }
    if t.starts_with('/') {
      t = &t[1..];
      continue;
    }
    break;
  }
  t.to_string()
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_local_dir() {
    let url = FileUrl::new(
      EndpointType::Local,
      "localhost",
      TargetType::Dir,
      "/var/log/nginx/access.log",
      None,
    );
    assert_eq!(url.to_string(), "ls://local/localhost/dir/var/log/nginx/access.log");
  }

  #[test]
  fn test_agent_dir() {
    let url = FileUrl::new(
      EndpointType::Agent,
      "web-01",
      TargetType::Dir,
      "/app/logs/error.log",
      None,
    );
    assert_eq!(url.to_string(), "ls://agent/web-01/dir/app/logs/error.log");
  }

  #[test]
  fn test_s3_archive() {
    let url = FileUrl::new(
      EndpointType::S3,
      "prod:logs-bucket",
      TargetType::Archive,
      "2023/10/data.tar.gz",
      Some("internal/service.log".to_string()),
    );
    // Note: URL encoding might affect the output string, e.g. ':' in host might be allowed or encoded?
    // In ls:// scheme, host is "s3". "prod:logs-bucket" is the first path segment.
    assert_eq!(
      url.to_string(),
      "ls://s3/prod:logs-bucket/archive/2023/10/data.tar.gz?entry=internal%2Fservice.log"
    );
  }

  #[test]
  fn test_parse_local() {
    let s = "ls://local/localhost/dir/var/log/syslog";
    let url = FileUrl::from_str(s).unwrap();
    assert_eq!(url.endpoint_type, EndpointType::Local);
    assert_eq!(url.endpoint_id, "localhost");
    assert_eq!(url.target_type, TargetType::Dir);
    assert_eq!(url.path, "var/log/syslog"); // Note: leading slash might be stripped by path_segments?
    // Url::parse("ls://local/localhost/dir/var/log/syslog")
    // path segments: ["localhost", "dir", "var", "log", "syslog"]
    // We take first 2 as id and type. Rest is path.
    // So path is "var/log/syslog".
    // If the original path was absolute "/var/log/syslog", we might want to preserve that?
    // In the implementation: path_parts.join("/") -> "var/log/syslog".
    // If we want absolute path, we might need to handle it.
    // However, for "local", usually we want absolute.
    // Let's adjust implementation to handle leading slash if needed or just accept that it's relative to "root" of the URL structure.
  }
}
