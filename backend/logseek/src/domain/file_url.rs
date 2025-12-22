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
  pub server_addr: Option<String>,
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
  /// Regular file or directory (corresponds to old "dir")
  Dir,
  /// File inside an archive
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
      server_addr: None,
      endpoint_type,
      endpoint_id: endpoint_id.into(),
      target_type,
      path: path.into(),
      entry_path,
    }
  }

  pub fn with_server_addr(mut self, addr: impl Into<String>) -> Self {
    self.server_addr = Some(addr.into());
    self
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

    // Format: ls://[id]@[type][.server_addr]/[path]?entry=[entry_path]
    let mut url = Url::parse("ls://placeholder").map_err(|_| fmt::Error)?;

    // Set Path
    let mut final_path = if self.path.starts_with('/') {
      self.path.clone()
    } else {
      format!("/{}", self.path)
    };

    // Special handling for S3: ls://profile@s3/bucket/path
    if self.endpoint_type == EndpointType::S3 {
      if let Some((profile, bucket)) = self.endpoint_id.split_once(':') {
        url.set_username(profile).map_err(|_| fmt::Error)?;
        final_path = format!("/{}{}", bucket, final_path);
      } else {
        url.set_username(&self.endpoint_id).map_err(|_| fmt::Error)?;
      }
    } else if !self.endpoint_id.is_empty()
      && (self.endpoint_type != EndpointType::Local || self.endpoint_id != "localhost")
    {
      // For non-S3, id is just userinfo
      if let Some((user, pass)) = self.endpoint_id.split_once(':') {
        url.set_username(user).map_err(|_| fmt::Error)?;
        url.set_password(Some(pass)).map_err(|_| fmt::Error)?;
      } else {
        url.set_username(&self.endpoint_id).map_err(|_| fmt::Error)?;
      }
    }

    // Set Host (type[.server_addr]) and Port
    let mut host = endpoint_type_str.to_string();
    let mut port = None;

    if let Some(addr) = &self.server_addr {
      if let Some((h, p)) = addr.split_once(':') {
        if !h.is_empty() {
          host.push('.');
          host.push_str(h);
        }
        port = p.parse::<u16>().ok();
      } else {
        host.push('.');
        host.push_str(addr);
      }
    }

    url.set_host(Some(&host)).map_err(|_| fmt::Error)?;
    if let Some(p) = port {
      url.set_port(Some(p)).map_err(|_| fmt::Error)?;
    }

    url.set_path(&final_path);

    // Set Query
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

    // Parse id from userinfo (base)
    let mut endpoint_id = url.username().to_string();
    if let Some(pass) = url.password() {
      endpoint_id = format!("{}:{}", endpoint_id, pass);
    }

    // Parse type and server_addr from host
    let host = url.host_str().ok_or(FileUrlError::MissingField("host"))?;
    let (endpoint_type_str, mut server_addr) = if let Some((t, addr)) = host.split_once('.') {
      (t, Some(addr.to_string()))
    } else {
      (host, None)
    };

    // Append port to server_addr if present
    if let Some(port) = url.port() {
      if let Some(ref mut addr) = server_addr {
        addr.push_str(&format!(":{}", port));
      } else {
        server_addr = Some(format!(":{}", port));
      }
    }

    let endpoint_type = match endpoint_type_str {
      "local" => EndpointType::Local,
      "agent" => EndpointType::Agent,
      "s3" => EndpointType::S3,
      other => return Err(FileUrlError::InvalidEndpointType(other.to_string())),
    };

    if endpoint_type == EndpointType::Local && endpoint_id.is_empty() {
      endpoint_id = "localhost".to_string();
    }

    // Parse path and handle S3 specific bucket extraction
    let mut path = url.path().trim_start_matches('/').to_string();

    if endpoint_type == EndpointType::S3 {
      // In ls://profile@s3/bucket/path, the first segment of path is the bucket
      if let Some((bucket, rest)) = path.split_once('/') {
        endpoint_id = format!("{}:{}", endpoint_id, bucket);
        path = rest.to_string();
      } else if !path.is_empty() {
        // Only bucket is provided
        endpoint_id = format!("{}:{}", endpoint_id, path);
        path = String::new();
      }
    }

    // Parse entry_path
    let entry_path = url
      .query_pairs()
      .find(|(k, _)| k == "entry")
      .map(|(_, v)| v.to_string());

    // Infer target_type
    let target_type = if entry_path.is_some() {
      TargetType::Archive
    } else {
      TargetType::Dir
    };

    Ok(FileUrl {
      server_addr,
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

      let mut url = FileUrl::new(endpoint_type, endpoint_id, TargetType::Dir, final_path, None);
      if let Some(tuning) = crate::utils::tuning::get()
        && let Some(sid) = &tuning.server_id
      {
        url = url.with_server_addr(sid);
      }
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

      let mut url = FileUrl::new(endpoint_type, endpoint_id, TargetType::Dir, final_path, None);
      if let Some(tuning) = crate::utils::tuning::get()
        && let Some(sid) = &tuning.server_id
      {
        url = url.with_server_addr(sid);
      }
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

      let mut url = FileUrl::new(
        endpoint_type,
        endpoint_id,
        TargetType::Archive,
        archive_path,
        Some(normalize_path_segment(rel_path)),
      );
      if let Some(tuning) = crate::utils::tuning::get()
        && let Some(sid) = &tuning.server_id
      {
        url = url.with_server_addr(sid);
      }
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
  fn test_local_file() {
    let url = FileUrl::new(
      EndpointType::Local,
      "localhost",
      TargetType::Dir,
      "/var/log/syslog",
      None,
    );
    assert_eq!(url.to_string(), "ls://local/var/log/syslog");
  }

  #[test]
  fn test_agent_file() {
    let url = FileUrl::new(
      EndpointType::Agent,
      "web-01",
      TargetType::Dir,
      "app/logs/error.log",
      None,
    );
    assert_eq!(url.to_string(), "ls://web-01@agent/app/logs/error.log");
  }

  #[test]
  fn test_s3_archive() {
    let url = FileUrl::new(
      EndpointType::S3,
      "prod:logs",
      TargetType::Archive,
      "2023/data.tgz",
      Some("access.log".to_string()),
    );
    assert_eq!(url.to_string(), "ls://prod@s3/logs/2023/data.tgz?entry=access.log");
  }

  #[test]
  fn test_multi_cluster() {
    let url = FileUrl::new(EndpointType::Agent, "web-01", TargetType::Dir, "app.log", None).with_server_addr("hk-prod");
    assert_eq!(url.to_string(), "ls://web-01@agent.hk-prod/app.log");

    let url_with_port =
      FileUrl::new(EndpointType::Agent, "web-01", TargetType::Dir, "app.log", None).with_server_addr("hk-prod:4000");
    assert_eq!(url_with_port.to_string(), "ls://web-01@agent.hk-prod:4000/app.log");
  }

  #[test]
  fn test_parse_concise() {
    let s = "ls://web-01@agent.hk-prod:4000/var/log/syslog?entry=internal.log";
    let url = FileUrl::from_str(s).unwrap();
    assert_eq!(url.endpoint_id, "web-01");
    assert_eq!(url.endpoint_type, EndpointType::Agent);
    assert_eq!(url.server_addr.as_deref(), Some("hk-prod:4000"));
    assert_eq!(url.path, "var/log/syslog");
    assert_eq!(url.entry_path.as_deref(), Some("internal.log"));
    assert_eq!(url.target_type, TargetType::Archive);
  }

  #[test]
  fn test_parse_s3_with_colon() {
    let s = "ls://prod@s3/bucket/archive.tgz?entry=log";
    let url = FileUrl::from_str(s).unwrap();
    assert_eq!(url.endpoint_id, "prod:bucket");
    assert_eq!(url.endpoint_type, EndpointType::S3);
    assert_eq!(url.target_type, TargetType::Archive);
  }
}
