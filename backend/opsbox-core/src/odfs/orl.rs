use std::fmt;
use std::str::FromStr;
use thiserror::Error;
use url::Url;

/// 资源端点类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EndpointType {
  Local,
  Agent,
  S3,
}

/// 目标资源类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TargetType {
  /// 普通文件或目录
  Dir,
  /// 归档文件内的条目
  Archive,
}

/// OpsBox 资源定位符 (ORL)
///
/// 结构对齐 ODFI 设计: `orl://[id]@[type][.server_addr]/[path]?entry=[entry_path]`
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ORL {
  pub server_addr: Option<String>,
  pub endpoint_type: EndpointType,
  pub endpoint_id: String,
  pub target_type: TargetType,
  pub path: String,
  pub entry_path: Option<String>,
}

#[derive(Debug, Error)]
pub enum OrlError {
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

impl ORL {
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

  /// Get a human-readable display name (usually the file name)
  pub fn display_name(&self) -> String {
    if let Some(entry) = &self.entry_path {
      entry.split('/').next_back().unwrap_or(entry).to_string()
    } else {
      self.path.split('/').next_back().unwrap_or(&self.path).to_string()
    }
  }
}

impl fmt::Display for ORL {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    let endpoint_type_str = match self.endpoint_type {
      EndpointType::Local => "local",
      EndpointType::Agent => "agent",
      EndpointType::S3 => "s3",
    };

    let mut url = Url::parse("orl://placeholder").map_err(|_| fmt::Error)?;

    // Set Path
    let mut final_path = if self.path.starts_with('/') {
      self.path.clone()
    } else {
      format!("/{}", self.path)
    };

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
      if let Some((user, pass)) = self.endpoint_id.split_once(':') {
        url.set_username(user).map_err(|_| fmt::Error)?;
        url.set_password(Some(pass)).map_err(|_| fmt::Error)?;
      } else {
        url.set_username(&self.endpoint_id).map_err(|_| fmt::Error)?;
      }
    }

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

    if let Some(entry) = &self.entry_path {
      url.query_pairs_mut().append_pair("entry", entry);
    }

    write!(f, "{}", url)
  }
}

impl FromStr for ORL {
  type Err = OrlError;

  fn from_str(s: &str) -> Result<Self, Self::Err> {
    let url = Url::parse(s)?;
    if url.scheme() != "orl" {
      return Err(OrlError::UnsupportedScheme(url.scheme().to_string()));
    }

    // Parse id from userinfo
    let mut endpoint_id = url.username().to_string();
    if let Some(pass) = url.password() {
      endpoint_id = format!("{}:{}", endpoint_id, pass);
    }

    let host = url.host_str().ok_or(OrlError::MissingField("host"))?;
    let (endpoint_type_str, mut server_addr): (&str, Option<String>) = if let Some((t, addr)) = host.split_once('.') {
      (t, Some(addr.to_string()))
    } else {
      (host, None)
    };

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
      other => return Err(OrlError::InvalidEndpointType(other.to_string())),
    };

    if endpoint_type == EndpointType::Local && endpoint_id.is_empty() {
      endpoint_id = "localhost".to_string();
    }

    let path_encoded = url.path().trim_start_matches('/');
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

    let entry_path = url
      .query_pairs()
      .find(|(k, _)| k == "entry")
      .map(|(_, v)| v.to_string());

    let target_type = if entry_path.is_some() {
      TargetType::Archive
    } else {
      TargetType::Dir
    };

    Ok(ORL {
      server_addr,
      endpoint_type,
      endpoint_id,
      target_type,
      path,
      entry_path,
    })
  }
}

/// 抽象路径封装
///
/// `OpsPath` 始终表示**当前文件系统层级**下的相对路径。
///
/// 在归档处理（Archive Overlay）场景中，它通过递归分层工作：
/// 假设访问 `orl://local/data.zip?entry=inner/file.log`
///
/// 1. **第一层 (底层 FS)**:
///    Router 看到宿主文件 `data.zip`，调用底层 `open_read(OpsPath("data.zip"))`。
///
/// 2. **第二层 (Overlay FS)**:
///    Router 识别出 ZIP 格式，挂载 `ZipOpsFS`。
///    `ZipOpsFS` 接收到的路径是归档内的相对路径：`OpsPath("inner/file.log")`。
///    它不需要关心 `data.zip` 这个前缀，因为它的根就是这个归档文件。
///
/// 保持为 String wrapper，方便 Trait 使用
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpsPath(String);

impl OpsPath {
  pub fn new(path: impl Into<String>) -> Self {
    Self(path.into())
  }

  pub fn as_str(&self) -> &str {
    &self.0
  }

  pub fn join(&self, other: &str) -> Self {
    let other = other.trim_start_matches('/');
    if self.0.ends_with('/') {
      Self(format!("{}{}", self.0, other))
    } else {
      Self(format!("{}/{}", self.0, other))
    }
  }
}

impl AsRef<str> for OpsPath {
  fn as_ref(&self) -> &str {
    &self.0
  }
}

impl fmt::Display for OpsPath {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(f, "{}", self.0)
  }
}
