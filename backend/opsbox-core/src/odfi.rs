/// Unified ODFI Identifier (OpsBox Distributed File Interface)
///
/// Scheme: `odfi://[id]@[type][.server_addr]/[path]?entry=[entry_path]`
///
/// Examples:
/// - Local Dir: `odfi://local/var/log/nginx/access.log`
/// - Agent Dir: `odfi://web-01@agent/app/logs/error.log`
/// - S3 Archive: `odfi://prod@s3/logs/2023/10/data.tar.gz?entry=internal/service.log`
use std::fmt;
use std::str::FromStr;
use thiserror::Error;
use url::Url;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Odfi {
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
  /// Regular file or directory
  Dir,
  /// File inside an archive
  Archive,
}

#[derive(Debug, Error)]
pub enum OdfiError {
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

impl Odfi {
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

impl fmt::Display for Odfi {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    let endpoint_type_str = match self.endpoint_type {
      EndpointType::Local => "local",
      EndpointType::Agent => "agent",
      EndpointType::S3 => "s3",
    };

    // Use odfi:// protocol
    let mut url = Url::parse("odfi://placeholder").map_err(|_| fmt::Error)?;

    // Set Path
    let mut final_path = if self.path.starts_with('/') {
      self.path.clone()
    } else {
      format!("/{}", self.path)
    };

    // Special handling for S3: odfi://profile@s3/bucket/path
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

impl FromStr for Odfi {
  type Err = OdfiError;

  fn from_str(s: &str) -> Result<Self, Self::Err> {
    let url = Url::parse(s)?;

    // Only accept odfi scheme
    if url.scheme() != "odfi" {
      return Err(OdfiError::UnsupportedScheme(url.scheme().to_string()));
    }

    // Parse id from userinfo
    let mut endpoint_id = url.username().to_string();
    if let Some(pass) = url.password() {
      endpoint_id = format!("{}:{}", endpoint_id, pass);
    }

    // Parse type and server_addr from host
    let host = url.host_str().ok_or(OdfiError::MissingField("host"))?;
    let (endpoint_type_str, mut server_addr): (&str, Option<String>) = if let Some((t, addr)) = host.split_once('.') {
      (t, Some(addr.to_string()))
    } else {
      (host, None)
    };

    // Append port
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
      other => return Err(OdfiError::InvalidEndpointType(other.to_string())),
    };

    if endpoint_type == EndpointType::Local && endpoint_id.is_empty() {
      endpoint_id = "localhost".to_string();
    }

    // Parse path and handle S3 bucket
    let path_encoded = url.path();
    let path_encoded = if let Some(p) = path_encoded.strip_prefix('/') {
      p
    } else {
      path_encoded
    };
    let mut path = percent_encoding::percent_decode_str(path_encoded)
      .decode_utf8_lossy()
      .into_owned();

    if endpoint_type == EndpointType::S3 {
      if let Some((bucket, rest)) = path.split_once('/') {
        endpoint_id = format!("{}:{}", endpoint_id, bucket);
        path = rest.to_string();
      } else if !path.is_empty() {
        endpoint_id = format!("{}:{}", endpoint_id, path);
        path = String::new();
      }
    }

    // Parse entry_path
    let entry_path = url
      .query_pairs()
      .find(|(k, _)| k == "entry")
      .map(|(_, v)| v.to_string());

    let target_type = if entry_path.is_some() {
      TargetType::Archive
    } else {
      TargetType::Dir
    };

    Ok(Odfi {
      server_addr,
      endpoint_type,
      endpoint_id,
      target_type,
      path,
      entry_path,
    })
  }
}

pub fn normalize_path_segment(s: &str) -> String {
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

pub fn join_root_path(root: &str, rel: &str) -> String {
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

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_local_file() {
    let url = Odfi::new(
      EndpointType::Local,
      "localhost",
      TargetType::Dir,
      "var/log/syslog",
      None,
    );
    assert_eq!(url.to_string(), "odfi://local/var/log/syslog");
  }

  #[test]
  fn test_agent_file() {
    let url = Odfi::new(
      EndpointType::Agent,
      "web-01",
      TargetType::Dir,
      "app/logs/error.log",
      None,
    );
    assert_eq!(url.to_string(), "odfi://web-01@agent/app/logs/error.log");
  }

  #[test]
  fn test_s3_archive() {
    let url = Odfi::new(
      EndpointType::S3,
      "prod:logs",
      TargetType::Archive,
      "2023/data.tgz",
      Some("access.log".to_string()),
    );
    assert_eq!(url.to_string(), "odfi://prod@s3/logs/2023/data.tgz?entry=access.log");
  }

  #[test]
  fn test_parse_concise() {
    let s = "odfi://web-01@agent.hk-prod:4000/var/log/syslog?entry=internal.log";
    let url = Odfi::from_str(s).unwrap();
    assert_eq!(url.endpoint_id, "web-01");
    assert_eq!(url.endpoint_type, EndpointType::Agent);
    assert_eq!(url.server_addr.as_deref(), Some("hk-prod:4000"));
    assert_eq!(url.path, "var/log/syslog");
    assert_eq!(url.entry_path.as_deref(), Some("internal.log"));
    assert_eq!(url.target_type, TargetType::Archive);
  }

  #[test]
  fn test_parse_encoded_path() {
    let s = "odfi://local/path%20with%20spaces/file.log";
    let url = Odfi::from_str(s).unwrap();
    assert_eq!(url.endpoint_type, EndpointType::Local);
    assert_eq!(url.path, "path with spaces/file.log");
  }

  #[test]
  fn test_parse_root_path() {
    // Tests that double slash is parsed as root path '/'
    let s = "odfi://agent-01@agent//";
    let url = Odfi::from_str(s).unwrap();
    assert_eq!(url.endpoint_id, "agent-01");
    assert_eq!(url.endpoint_type, EndpointType::Agent);
    assert_eq!(url.path, "/"); // Was "" before fix
  }

  #[test]
  fn test_parse_empty_path() {
    // Tests that single slash (after host) is parsed as empty path
    let s = "odfi://agent-01@agent/";
    let url = Odfi::from_str(s).unwrap();
    assert_eq!(url.endpoint_id, "agent-01");
    assert_eq!(url.endpoint_type, EndpointType::Agent);
    assert_eq!(url.path, "");
  }
}
