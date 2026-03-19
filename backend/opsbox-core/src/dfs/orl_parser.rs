//! ORL Parser 模块 - ORL 字符串解析器
//!
//! 将 ORL (OpsBox Resource Locator) 字符串解析为 Resource 对象
//!
//! # ORL 格式
//!
//! ## 基本格式
//! ```text
//! orl://<endpoint>/<path>?<query>
//! ```
//!
//! ## Endpoint 类型
//!
//! ### 本地文件系统
//! ```text
//! orl://local/var/log/app.log
//! ```
//!
//! ### Agent 代理
//! ```text
//! orl://web-01@agent/var/log/app.log
//! orl://web-01@192.168.1.100:4001/var/log/app.log
//! ```
//!
//! ### S3 对象存储
//! ```text
//! orl://backup@s3/bucket/path/to/file
//! orl://backupdr:my-bucket@s3/path/to/file
//! ```
//!
//! ## 归档内文件
//! ```text
//! orl://local/data/archive.tar?entry=inner/file.txt
//! orl://web-01@agent/logs/backup.zip?entry=2024/01/app.log
//! ```

use std::collections::HashMap;

use percent_encoding::{NON_ALPHANUMERIC, percent_decode_str, utf8_percent_encode};

use super::{
  archive::{ArchiveContext, ArchiveType},
  endpoint::Endpoint,
  resource::Resource,
};

/// ORL 解析错误
#[derive(Debug, Clone, thiserror::Error)]
pub enum OrlParseError {
  #[error("Invalid ORL format: {0}")]
  InvalidFormat(String),

  #[error("Unknown endpoint type: {0}")]
  UnknownEndpointType(String),

  #[error("Missing endpoint identity")]
  MissingIdentity,

  #[error("Missing path")]
  MissingPath,

  #[error("Invalid Agent endpoint format: {0}")]
  InvalidAgentFormat(String),

  #[error("Invalid S3 endpoint format: {0}")]
  InvalidS3Format(String),
}

/// ORL Parser
pub struct OrlParser;

impl OrlParser {
  /// 解析 ORL 字符串为 Resource
  ///
  /// # 示例
  /// ```rust
  /// use opsbox_core::dfs::OrlParser;
  ///
  /// // 本地文件
  /// let resource = OrlParser::parse("orl://local/var/log/app.log").unwrap();
  ///
  /// // Agent 代理
  /// let resource = OrlParser::parse("orl://web-01@agent/var/log/app.log").unwrap();
  ///
  /// // S3 对象
  /// let resource = OrlParser::parse("orl://backup@s3/bucket/path/to/file").unwrap();
  ///
  /// // 归档内文件
  /// let resource = OrlParser::parse("orl://local/data/archive.tar?entry=inner/file.txt").unwrap();
  /// ```
  pub fn parse(orl: &str) -> Result<Resource, OrlParseError> {
    // 检查协议前缀
    if !orl.starts_with("orl://") {
      return Err(OrlParseError::InvalidFormat("ORL must start with 'orl://'".to_string()));
    }

    // 移除协议前缀
    let rest = &orl[6..]; // 跳过 "orl://"

    // 分离 endpoint 和 path+query
    let (endpoint_str, path_and_query) = rest
      .split_once('/')
      .ok_or_else(|| OrlParseError::InvalidFormat("Missing path after endpoint".to_string()))?;

    // 解析 endpoint
    let endpoint = Self::parse_endpoint(endpoint_str)?;

    // 分离 path 和 query
    let (path_str, query_str) = path_and_query.split_once('?').unwrap_or((path_and_query, ""));
    let decoded_path = percent_decode_str(path_str).decode_utf8_lossy().to_string();

    // 解析查询参数（归档上下文和 glob 过滤）
    let (archive_context, filter_glob) = Self::parse_query_params(query_str, &decoded_path)?;

    // 构建 path - 对于 Windows 绝对路径（如 C:/...），不添加前导斜杠
    let path = if decoded_path.len() >= 2
      && decoded_path.chars().nth(1) == Some(':')
      && decoded_path
        .chars()
        .next()
        .map(|c| c.is_ascii_alphabetic())
        .unwrap_or(false)
    {
      // Windows 绝对路径 (C:/...)
      decoded_path
    } else {
      // Unix 路径或其他路径，添加前导斜杠
      format!("/{decoded_path}")
    };

    let mut resource = Resource::new(endpoint, path.into(), archive_context);
    resource.filter_glob = filter_glob;
    Ok(resource)
  }

  /// 解析 endpoint 部分
  fn parse_endpoint(s: &str) -> Result<Endpoint, OrlParseError> {
    if s == "local" {
      return Ok(Endpoint::local_fs());
    }

    // 特殊处理：agent discovery (orl://agent/)
    if s == "agent" {
      return Ok(Endpoint::agent_discovery());
    }

    // 特殊处理：S3 discovery (orl://s3/)
    if s == "s3" {
      return Ok(Endpoint::s3_discovery());
    }

    // 检查是否有 @ 符号
    let (identity, type_str) = s
      .rsplit_once('@')
      .ok_or_else(|| OrlParseError::InvalidFormat("Endpoint must be in format 'identity@type'".to_string()))?;

    match type_str {
      "agent" => Self::parse_agent_endpoint(identity),
      "s3" => Self::parse_s3_endpoint(identity),
      _ => {
        if let Some(addr) = type_str.strip_prefix("agent.") {
          // orl://identity@agent.host:port/path 格式
          if let Some((host, port_str)) = addr.split_once(':') {
            let port = port_str
              .parse::<u16>()
              .map_err(|_| OrlParseError::InvalidAgentFormat(format!("Invalid port number: {port_str}")))?;
            Ok(Endpoint::agent(host.to_string(), port, identity.to_string()))
          } else {
            // 只有 host，使用默认端口
            Ok(Endpoint::agent(addr.to_string(), 4001, identity.to_string()))
          }
        } else {
          Err(OrlParseError::UnknownEndpointType(type_str.to_string()))
        }
      }
    }
  }

  /// 解析 Agent endpoint
  ///
  /// 支持格式:
  /// - name@agent
  /// - name@host:port@agent
  fn parse_agent_endpoint(s: &str) -> Result<Endpoint, OrlParseError> {
    // 检查是否包含端口号
    if let Some((name, host_port)) = s.rsplit_once('@') {
      // name@host:port@agent 格式
      let (host, port_str) = host_port
        .split_once(':')
        .ok_or_else(|| OrlParseError::InvalidAgentFormat("Expected 'host:port' format".to_string()))?;
      let port = port_str
        .parse::<u16>()
        .map_err(|_| OrlParseError::InvalidAgentFormat(format!("Invalid port number: {port_str}")))?;
      Ok(Endpoint::agent(host.to_string(), port, name.to_string()))
    } else {
      // name@agent 格式 - 使用默认端口
      Ok(Endpoint::agent(
        s.to_string(),
        4001, // 默认端口
        s.to_string(),
      ))
    }
  }

  /// 解析 S3 endpoint
  ///
  /// 支持格式:
  /// - profile (无 bucket)
  /// - profile:bucket (带 bucket)
  fn parse_s3_endpoint(s: &str) -> Result<Endpoint, OrlParseError> {
    // 检查是否包含 bucket 信息 (profile:bucket)
    if let Some((profile, bucket)) = s.split_once(':') {
      Ok(Endpoint::s3_with_bucket(profile.to_string(), bucket.to_string()))
    } else {
      Ok(Endpoint::s3(s.to_string()))
    }
  }

  /// 解析查询参数
  ///
  /// 从查询字符串中提取 archive context（entry 参数）和 glob 过滤器
  fn parse_query_params(
    query: &str,
    main_path: &str,
  ) -> Result<(Option<ArchiveContext>, Option<String>), OrlParseError> {
    if query.is_empty() {
      return Ok((None, None));
    }

    let params = Self::parse_query_string(query);

    // 提取归档上下文
    let archive_context = if let Some(inner_path) = params.get("entry") {
      let archive_type = Self::infer_archive_type_from_path(main_path);
      Some(ArchiveContext::from_path_str(inner_path, archive_type))
    } else {
      None
    };

    // 提取 glob 过滤器
    let filter_glob = params.get("glob").cloned();

    Ok((archive_context, filter_glob))
  }

  /// 解析查询字符串
  fn parse_query_string(query: &str) -> HashMap<String, String> {
    let mut params = HashMap::new();
    for pair in query.split('&') {
      if let Some((key, value)) = pair.split_once('=') {
        // 解码 URL 编码的值
        let decoded_value = percent_decode_str(value).decode_utf8_lossy().to_string();
        params.insert(key.to_string(), decoded_value);
      }
    }
    params
  }

  /// 从路径推断归档类型
  ///
  /// 检查顺序：先检查复合扩展名（如 .tar.gz），再检查简单扩展名（如 .gz）
  fn infer_archive_type_from_path(path: &str) -> Option<ArchiveType> {
    let path_lower = path.to_lowercase();

    // 先检查复合扩展名（必须先检查，否则会被简单扩展名匹配）
    if path_lower.ends_with(".tar.gz") || path_lower.ends_with(".tgz") {
      return Some(ArchiveType::TarGz);
    }

    // 再检查简单扩展名 - 使用 rfind 查找最后一个点
    if let Some(pos) = path_lower.rfind('.') {
      let ext = &path_lower[pos..];
      ArchiveType::from_extension(ext)
    } else {
      None
    }
  }
}

/// 构建 ORL 字符串
///
/// 从 Endpoint 和路径构建 ORL 字符串，用于 API 响应序列化
///
/// # 示例
/// ```rust
/// use opsbox_core::dfs::{Endpoint, build_orl, ResourcePath};
///
/// // 本地文件
/// let endpoint = Endpoint::local_fs();
/// let path = ResourcePath::parse("/var/log/app.log");
/// let orl = build_orl(&endpoint, &path, None, None);
/// assert_eq!(orl, "orl://local/var/log/app.log");
///
/// // Agent 代理
/// let endpoint = Endpoint::agent("192.168.1.100".to_string(), 4001, "web-01".to_string());
/// let path = ResourcePath::parse("/var/log/app.log");
/// let orl = build_orl(&endpoint, &path, None, None);
/// assert_eq!(orl, "orl://web-01@agent.192.168.1.100:4001/var/log/app.log");
///
/// // 归档内文件
/// let endpoint = Endpoint::local_fs();
/// let path = ResourcePath::parse("/data/archive.tar");
/// let orl = build_orl(&endpoint, &path, Some("inner/file.txt"), None);
/// assert_eq!(orl, "orl://local/data/archive.tar?entry=inner%2Ffile%2Etxt");
///
/// // 带 glob 过滤
/// let endpoint = Endpoint::local_fs();
/// let path = ResourcePath::parse("/var/log");
/// let orl = build_orl(&endpoint, &path, None, Some("*.log"));
/// assert_eq!(orl, "orl://local/var/log?glob=%2A%2Elog");
/// ```
pub fn build_orl(
  endpoint: &Endpoint,
  path: &crate::dfs::ResourcePath,
  entry: Option<&str>,
  glob: Option<&str>,
) -> String {
  let endpoint_str = build_endpoint_string(endpoint);
  let path_str = path.to_string();

  // 移除路径前导斜杠（ORL 格式中路径紧跟 endpoint 后）
  let path_without_leading_slash = path_str.trim_start_matches('/');

  // 构建查询参数
  let mut query_parts = Vec::new();
  if let Some(entry_path) = entry {
    let encoded = utf8_percent_encode(entry_path, NON_ALPHANUMERIC).to_string();
    query_parts.push(format!("entry={encoded}"));
  }
  if let Some(glob_pattern) = glob {
    let encoded = utf8_percent_encode(glob_pattern, NON_ALPHANUMERIC).to_string();
    query_parts.push(format!("glob={encoded}"));
  }

  if query_parts.is_empty() {
    format!("orl://{endpoint_str}/{path_without_leading_slash}")
  } else {
    format!(
      "orl://{endpoint_str}/{path_without_leading_slash}?{}",
      query_parts.join("&")
    )
  }
}

/// 构建 endpoint 字符串
fn build_endpoint_string(endpoint: &Endpoint) -> String {
  use crate::dfs::Location;

  match &endpoint.location {
    Location::Local => "local".to_string(),
    Location::Remote { host, port } => {
      // Agent endpoint: identity@agent.host:port
      format!("{}@agent.{}:{}", endpoint.identity, host, port)
    }
    Location::Cloud => {
      // S3 endpoint: profile[:bucket]@s3
      if let Some(bucket) = &endpoint.bucket {
        format!("{}:{}@s3", endpoint.identity, bucket)
      } else {
        format!("{}@s3", endpoint.identity)
      }
    }
  }
}

/// 从 Resource 构建 ORL 字符串
///
/// # 示例
/// ```rust
/// use opsbox_core::dfs::{OrlParser, build_orl_from_resource};
///
/// let resource = OrlParser::parse("orl://local/var/log/app.log").unwrap();
/// let orl = build_orl_from_resource(&resource);
/// assert_eq!(orl, "orl://local/var/log/app.log");
/// ```
pub fn build_orl_from_resource(resource: &Resource) -> String {
  let entry = resource.archive_context.as_ref().map(|ctx| ctx.inner_path.to_string());
  build_orl(
    &resource.endpoint,
    &resource.primary_path,
    entry.as_deref(),
    resource.filter_glob.as_deref(),
  )
}

/// 将本地文件系统路径转换为 ORL 字符串
///
/// 此函数处理跨平台路径问题：
/// - Unix: `/var/log/app.log` → `orl://local/var/log/app.log`
/// - Windows: `C:\Users\test\file.log` → `orl://local/C:/Users/test/file.log`
///
/// # 参数
/// - `path`: 本地文件系统路径
/// - `entry`: 可选的归档内路径
/// - `glob`: 可选的 glob 过滤模式
///
/// # 示例
/// ```rust
/// use opsbox_core::dfs::local_path_to_orl;
/// use std::path::Path;
///
/// // Unix 路径
/// #[cfg(unix)]
/// {
///   let orl = local_path_to_orl("/var/log/app.log", None, None);
///   assert_eq!(orl, "orl://local/var/log/app.log");
/// }
///
/// // Windows 路径
/// #[cfg(windows)]
/// {
///   let orl = local_path_to_orl(r"C:\Users\test\file.log", None, None);
///   assert_eq!(orl, "orl://local/C:/Users/test/file.log");
/// }
/// ```
pub fn local_path_to_orl<P: AsRef<std::path::Path>>(
  path: P,
  entry: Option<&str>,
  glob: Option<&str>,
) -> String {
  let path = path.as_ref();
  let path_str = path.to_string_lossy();

  // 将所有反斜杠替换为正斜杠（Windows 兼容）
  let normalized_path = path_str.replace('\\', "/");

  // 构建查询参数
  let mut query_parts = Vec::new();
  if let Some(entry_path) = entry {
    let encoded = utf8_percent_encode(entry_path, NON_ALPHANUMERIC).to_string();
    query_parts.push(format!("entry={encoded}"));
  }
  if let Some(glob_pattern) = glob {
    let encoded = utf8_percent_encode(glob_pattern, NON_ALPHANUMERIC).to_string();
    query_parts.push(format!("glob={encoded}"));
  }

  // 构建 ORL
  if query_parts.is_empty() {
    format!("orl://local/{}", normalized_path.trim_start_matches('/'))
  } else {
    format!(
      "orl://local/{}?{}",
      normalized_path.trim_start_matches('/'),
      query_parts.join("&")
    )
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::dfs::Location;

  #[test]
  fn test_parse_local_file() {
    let resource = OrlParser::parse("orl://local/var/log/app.log").unwrap();
    assert!(matches!(resource.endpoint.location, Location::Local));
    assert_eq!(resource.primary_path.to_string(), "/var/log/app.log");
    assert!(!resource.is_archive());
  }

  #[test]
  fn test_parse_agent_simple() {
    let resource = OrlParser::parse("orl://web-01@agent/var/log/app.log").unwrap();
    assert!(matches!(resource.endpoint.location, Location::Remote { .. }));
    assert_eq!(resource.endpoint.identity, "web-01");
    assert_eq!(resource.primary_path.to_string(), "/var/log/app.log");
  }

  #[test]
  fn test_parse_agent_with_host() {
    let resource = OrlParser::parse("orl://web-01@192.168.1.100:4001@agent/var/log/app.log").unwrap();
    assert!(
      matches!(resource.endpoint.location, Location::Remote { host, port } if host == "192.168.1.100" && port == 4001)
    );
    assert_eq!(resource.endpoint.identity, "web-01");
  }

  #[test]
  fn test_parse_s3_simple() {
    let resource = OrlParser::parse("orl://backup@s3/bucket/path/to/file").unwrap();
    assert!(matches!(resource.endpoint.location, Location::Cloud));
    assert_eq!(resource.endpoint.identity, "backup");
    assert_eq!(resource.primary_path.to_string(), "/bucket/path/to/file");
  }

  #[test]
  fn test_parse_s3_with_bucket() {
    let resource = OrlParser::parse("orl://backup:my-bucket@s3/path/to/file").unwrap();
    assert!(matches!(resource.endpoint.location, Location::Cloud));
    assert_eq!(resource.endpoint.identity, "backup");
    assert_eq!(resource.primary_path.to_string(), "/path/to/file");
  }

  #[test]
  fn test_parse_archive_entry() {
    let resource = OrlParser::parse("orl://local/data/archive.tar?entry=inner/file.txt").unwrap();
    assert!(resource.is_archive());
    let ctx = resource.archive_context.as_ref().unwrap();
    assert_eq!(ctx.inner_path.to_string(), "inner/file.txt");
    assert_eq!(ctx.archive_type, Some(ArchiveType::Tar));
  }

  #[test]
  fn test_parse_archive_zip() {
    let resource = OrlParser::parse("orl://local/data/logs.zip?entry=2024/01/app.log").unwrap();
    assert!(resource.is_archive());
    let ctx = resource.archive_context.as_ref().unwrap();
    assert_eq!(ctx.inner_path.to_string(), "2024/01/app.log");
    assert_eq!(ctx.archive_type, Some(ArchiveType::Zip));
  }

  #[test]
  fn test_parse_missing_protocol() {
    let result = OrlParser::parse("local/var/log/app.log");
    assert!(result.is_err());
  }

  #[test]
  fn test_parse_missing_path() {
    let result = OrlParser::parse("orl://local");
    assert!(result.is_err());
  }

  #[test]
  fn test_parse_unknown_endpoint_type() {
    let result = OrlParser::parse("orl://unknown/type/path");
    assert!(result.is_err());
  }

  #[test]
  fn test_parse_invalid_agent_port() {
    let result = OrlParser::parse("orl://web-01@192.168.1.100:abc@agent/path");
    assert!(result.is_err());
  }

  #[test]
  fn test_parse_empty_entry() {
    let resource = OrlParser::parse("orl://local/data/archive.tar?entry=").unwrap();
    assert!(resource.is_archive());
    let ctx = resource.archive_context.as_ref().unwrap();
    assert_eq!(ctx.inner_path.to_string(), "");
  }

  #[test]
  fn test_parse_tar_gz_archive() {
    // 测试 .tar.gz 文件被正确识别为 TarGz 类型
    let resource = OrlParser::parse("orl://local/data/logs.tar.gz?entry=inner/file.txt").unwrap();
    assert!(resource.is_archive());
    let ctx = resource.archive_context.as_ref().unwrap();
    assert_eq!(ctx.inner_path.to_string(), "inner/file.txt");
    assert_eq!(ctx.archive_type, Some(ArchiveType::TarGz));
  }

  #[test]
  fn test_parse_tgz_archive() {
    // 测试 .tgz 文件被正确识别为 TarGz 类型
    // 注意：.tgz 是 .tar.gz 的简写，功能上完全相同，因此统一返回 TarGz
    let resource = OrlParser::parse("orl://local/data/logs.tgz?entry=app.log").unwrap();
    assert!(resource.is_archive());
    let ctx = resource.archive_context.as_ref().unwrap();
    assert_eq!(ctx.inner_path.to_string(), "app.log");
    // .tgz 和 .tar.gz 都映射到 TarGz
    assert_eq!(ctx.archive_type, Some(ArchiveType::TarGz));
  }

  #[test]
  fn test_parse_gz_file_not_tar() {
    // 测试纯 .gz 文件（不是 tar.gz）被正确识别为 Gz 类型
    let resource = OrlParser::parse("orl://local/data/app.log.gz?entry=").unwrap();
    assert!(resource.is_archive());
    let ctx = resource.archive_context.as_ref().unwrap();
    assert_eq!(ctx.archive_type, Some(ArchiveType::Gz));
  }

  #[test]
  fn test_parse_complex_tar_gz_path() {
    // 测试复杂路径中的 .tar.gz 文件
    let resource = OrlParser::parse(
      "orl://local/var/log/archives/MYAPP_20_APPLOG_2025-08-18.tar.gz?entry=/home/appadm/logs/app.log",
    )
    .unwrap();
    assert!(resource.is_archive());
    let ctx = resource.archive_context.as_ref().unwrap();
    assert_eq!(ctx.inner_path.to_string(), "/home/appadm/logs/app.log");
    assert_eq!(ctx.archive_type, Some(ArchiveType::TarGz));
  }

  #[test]
  fn test_parse_s3_with_bucket_in_endpoint() {
    // 测试 profile:bucket@s3 格式正确解析 bucket
    let resource = OrlParser::parse("orl://default:backupdr@s3/mybucket").unwrap();
    assert_eq!(resource.endpoint.identity, "default");
    assert_eq!(resource.endpoint.bucket, Some("backupdr".to_string()));
    assert_eq!(resource.primary_path.to_string(), "/mybucket");
  }

  #[test]
  fn test_parse_s3_without_bucket_in_endpoint() {
    // 测试 profile@s3 格式（兼容旧格式）
    let resource = OrlParser::parse("orl://default@s3/backupdr/path").unwrap();
    assert_eq!(resource.endpoint.identity, "default");
    assert!(resource.endpoint.bucket.is_none());
    assert_eq!(resource.primary_path.to_string(), "/backupdr/path");
  }

  #[test]
  fn test_parse_agent_windows_path_with_encoded_drive_letter() {
    let resource = OrlParser::parse("orl://windows-agent@agent/D%3A/workspace/project").unwrap();
    assert_eq!(resource.endpoint.identity, "windows-agent");
    assert_eq!(resource.primary_path.to_string(), "D:/workspace/project");
  }
}
