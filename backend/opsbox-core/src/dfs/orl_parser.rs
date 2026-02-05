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
            return Err(OrlParseError::InvalidFormat(
                "ORL must start with 'orl://'".to_string(),
            ));
        }

        // 移除协议前缀
        let rest = &orl[6..]; // 跳过 "orl://"

        // 分离 endpoint 和 path+query
        let (endpoint_str, path_and_query) = rest.split_once('/').ok_or_else(|| {
            OrlParseError::InvalidFormat("Missing path after endpoint".to_string())
        })?;

        // 解析 endpoint
        let endpoint = Self::parse_endpoint(endpoint_str)?;

        // 分离 path 和 query
        let (path_str, query_str) = path_and_query
            .split_once('?')
            .unwrap_or((path_and_query, ""));

        // 解析归档上下文（从主路径推断归档类型）
        let archive_context = Self::parse_archive_context(query_str, path_str)?;

        // 构建 path
        let path = format!("/{path_str}");

        Ok(Resource::new(endpoint, path.into(), archive_context))
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
        let (identity, type_str) = s.rsplit_once('@').ok_or_else(|| {
            OrlParseError::InvalidFormat("Endpoint must be in format 'identity@type'".to_string())
        })?;

        match type_str {
            "agent" => Self::parse_agent_endpoint(identity),
            "s3" => Self::parse_s3_endpoint(identity),
            _ => Err(OrlParseError::UnknownEndpointType(type_str.to_string())),
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
            let (host, port_str) = host_port.split_once(':').ok_or_else(|| {
                OrlParseError::InvalidAgentFormat("Expected 'host:port' format".to_string())
            })?;
            let port = port_str.parse::<u16>().map_err(|_| {
                OrlParseError::InvalidAgentFormat(format!("Invalid port number: {port_str}"))
            })?;
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
    /// - profile@s3
    /// - profile:bucket@s3
    fn parse_s3_endpoint(s: &str) -> Result<Endpoint, OrlParseError> {
        // 如果有 bucket，我们仍然使用 profile 作为 identity
        // bucket 信息需要在实际访问时处理
        let identity = if let Some((profile, _bucket)) = s.split_once(':') {
            profile.to_string()
        } else {
            s.to_string()
        };
        Ok(Endpoint::s3(identity))
    }

    /// 解析归档上下文
    ///
    /// 支持 entry 参数指定归档内路径
    /// archive_type 从主路径推断
    fn parse_archive_context(query: &str, main_path: &str) -> Result<Option<ArchiveContext>, OrlParseError> {
        if query.is_empty() {
            return Ok(None);
        }

        let params = Self::parse_query_string(query);

        if let Some(inner_path) = params.get("entry") {
            // 从主路径推断归档类型
            let archive_type = Self::infer_archive_type_from_path(main_path);

            Ok(Some(ArchiveContext::from_path_str(inner_path, archive_type)))
        } else {
            Ok(None)
        }
    }

    /// 解析查询字符串
    fn parse_query_string(query: &str) -> HashMap<String, String> {
        let mut params = HashMap::new();
        for pair in query.split('&') {
            if let Some((key, value)) = pair.split_once('=') {
                params.insert(key.to_string(), value.to_string());
            }
        }
        params
    }

    /// 从路径推断归档类型
    fn infer_archive_type_from_path(path: &str) -> Option<ArchiveType> {
        // 检查路径中的归档扩展名
        if let Some(pos) = path.rfind('.') {
            let ext = &path[pos..];
            ArchiveType::from_extension(ext)
        } else {
            None
        }
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
        assert!(matches!(resource.endpoint.location, Location::Remote { host, port } if host == "192.168.1.100" && port == 4001));
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
}
