//! Agent 文件系统连接器
//!
//! 将 EndpointConnector 适配到 AgentOpsFS。

use std::pin::Pin;
use std::sync::Arc;

use async_trait::async_trait;
use tokio::io::AsyncRead;

use opsbox_core::odfs::{providers::agent::AgentOpsFS, OpsFileSystem, OpsPath};
use opsbox_domain::resource::{
    EndpointConnector, ResourceMetadata, ResourcePath, DomainError,
};

/// Agent 端点连接器
///
/// 委托给 AgentOpsFS 实现。
pub struct AgentEndpointConnector {
    inner: Arc<AgentOpsFS>,
}

impl AgentEndpointConnector {
    /// 从 agent_id 和 base_url 创建新的连接器
    pub fn new(agent_id: String, base_url: String) -> Self {
        Self {
            inner: Arc::new(AgentOpsFS::new(agent_id, base_url)),
        }
    }

    /// 从现有的 AgentOpsFS 创建
    pub fn from_opsfs(fs: Arc<AgentOpsFS>) -> Self {
        Self {
            inner: fs,
        }
    }

    /// 获取内部文件系统引用
    pub fn inner(&self) -> &AgentOpsFS {
        &self.inner
    }
}

#[async_trait]
impl EndpointConnector for AgentEndpointConnector {
    /// 获取资源元数据
    async fn metadata(&self, path: &ResourcePath) -> Result<ResourceMetadata, DomainError> {
        let ops_path = OpsPath::new(path.as_str());
        let ops_meta = self.inner.as_ref().metadata(&ops_path).await
            .map_err(|e| DomainError::ResourceNotFound(format!("Failed to get metadata: {}", e)))?;

        Ok(convert_metadata(ops_meta))
    }

    /// 列出目录内容
    async fn list(&self, path: &ResourcePath) -> Result<Vec<ResourceMetadata>, DomainError> {
        let ops_path = OpsPath::new(path.as_str());
        let entries = self.inner.as_ref().read_dir(&ops_path).await
            .map_err(|e| DomainError::ResourceNotFound(format!("Failed to list directory: {}", e)))?;

        entries.into_iter()
            .map(|entry| Ok(convert_entry(entry)))
            .collect()
    }

    /// 读取文件内容
    async fn read(&self, path: &ResourcePath) -> Result<Pin<Box<dyn AsyncRead + Send + Unpin>>, DomainError> {
        let ops_path = OpsPath::new(path.as_str());
        let ops_read = self.inner.as_ref().open_read(&ops_path).await
            .map_err(|e| DomainError::ResourceNotFound(format!("Failed to open file: {}", e)))?;

        // OpsRead 已经是 Pin<Box<dyn AsyncRead + Send + Unpin>>，可以直接返回
        Ok(ops_read)
    }

    /// 检查资源是否存在
    async fn exists(&self, path: &ResourcePath) -> Result<bool, DomainError> {
        let ops_path = OpsPath::new(path.as_str());
        match self.inner.as_ref().metadata(&ops_path).await {
            Ok(_) => Ok(true),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(e) => Err(DomainError::ResourceNotFound(format!("Failed to check existence: {}", e))),
        }
    }
}

/// 转换 OpsMetadata 到 ResourceMetadata
fn convert_metadata(ops_meta: opsbox_core::odfs::types::OpsMetadata) -> ResourceMetadata {
    use opsbox_core::odfs::types::OpsFileType;

    let is_dir = matches!(ops_meta.file_type, OpsFileType::Directory);
    ResourceMetadata {
        name: ops_meta.name.clone(),
        size: ops_meta.size,
        is_dir,
        modified: ops_meta.modified.map(|t| t.duration_since(std::time::UNIX_EPOCH).ok().map(|d| d.as_secs() as i64).unwrap_or(0)),
        mime_type: ops_meta.mime_type.clone(),
        is_archive: ops_meta.is_archive,
        child_count: None,  // Agent 不直接提供子目录数量
    }
}

/// 转换 OpsEntry 到 ResourceMetadata
///
/// 对于 Agent search root，Agent 返回的是绝对路径（如 /Users/.../temp_explorer_...）。
/// 对于嵌套的相对路径，Agent 返回的是完整路径（如 logs/app.log）。
///
/// 规则：当 entry.path 与 entry.metadata.name 不同时，使用 entry.path
/// 以保留完整的路径信息。这与旧的 map_entry 逻辑一致。
fn convert_entry(entry: opsbox_core::odfs::types::OpsEntry) -> ResourceMetadata {
    use opsbox_core::odfs::types::OpsFileType;

    // 如果 path 与 metadata.name 不同，使用 path（保留完整路径）
    // 否则使用 metadata.name
    let name = if entry.path != entry.metadata.name {
        entry.path.clone()
    } else {
        entry.metadata.name.clone()
    };

    let is_dir = matches!(entry.metadata.file_type, OpsFileType::Directory);
    ResourceMetadata {
        name,
        size: entry.metadata.size,
        is_dir,
        modified: entry.metadata.modified.map(|t| t.duration_since(std::time::UNIX_EPOCH).ok().map(|d| d.as_secs() as i64).unwrap_or(0)),
        mime_type: entry.metadata.mime_type.clone(),
        is_archive: entry.metadata.is_archive,
        child_count: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_agent_connector_new() {
        let connector = AgentEndpointConnector::new(
            "test-agent".to_string(),
            "http://localhost:4001".to_string(),
        );
        // 验证创建成功
        assert_eq!(connector.inner().name(), "AgentOpsFS");
    }

    /// 测试 convert_entry 处理 Agent 返回的绝对路径（search root 场景）
    ///
    /// E2E 测试发现的问题：当 Agent 列出 search root 时，返回的是规范化的绝对路径
    /// 而不是相对路径。由于 path != metadata.name，应该使用 path 保留完整路径。
    #[test]
    fn test_convert_entry_with_absolute_path() {
        use opsbox_core::odfs::types::{OpsEntry, OpsMetadata, OpsFileType};

        // 模拟 Agent search root 返回的绝对路径
        // 例如：/Users/.../temp_explorer_1769963806167
        let absolute_path = "/Users/test/workspace/temp_explorer_12345";
        let display_name = "temp_explorer_12345";

        let entry = OpsEntry {
            name: display_name.to_string(),
            path: absolute_path.to_string(),
            metadata: OpsMetadata {
                name: display_name.to_string(),
                file_type: OpsFileType::Directory,
                size: 0,
                modified: None,
                mode: 0o755,
                mime_type: None,
                compression: None,
                is_archive: false,
            },
        };

        let result = super::convert_entry(entry);

        // 应该使用绝对路径，因为这是 Agent search root 返回的完整路径
        assert_eq!(result.name, absolute_path);
        assert_eq!(result.is_dir, true);
    }

    /// 测试 convert_entry 处理普通文件（相对路径）
    ///
    /// 对于 Agent 内的普通文件，path 和 name 相同，应该使用 name
    #[test]
    fn test_convert_entry_with_relative_path() {
        use opsbox_core::odfs::types::{OpsEntry, OpsMetadata, OpsFileType};

        // 模拟 Agent 内的普通文件
        let file_path = "logs/app.log";
        let entry = OpsEntry {
            name: file_path.to_string(),
            path: file_path.to_string(),
            metadata: OpsMetadata {
                name: "app.log".to_string(),  // name 可能只是文件名部分
                file_type: OpsFileType::File,
                size: 1024,
                modified: None,
                mode: 0o644,
                mime_type: Some("text/plain".to_string()),
                compression: None,
                is_archive: false,
            },
        };

        let result = super::convert_entry(entry);

        // 应该使用 path（相对路径）
        assert_eq!(result.name, file_path);
        assert_eq!(result.is_dir, false);
        assert_eq!(result.size, 1024);
    }

    /// 测试 convert_entry 处理嵌套目录
    ///
    /// Agent 返回的路径可能是相对路径，但包含多级目录
    #[test]
    fn test_convert_entry_with_nested_relative_path() {
        use opsbox_core::odfs::types::{OpsEntry, OpsMetadata, OpsFileType};

        let nested_path = "logs/2024-01/app.log";
        let entry = OpsEntry {
            name: "app.log".to_string(),
            path: nested_path.to_string(),
            metadata: OpsMetadata {
                name: "app.log".to_string(),
                file_type: OpsFileType::File,
                size: 512,
                modified: None,
                mode: 0o644,
                mime_type: Some("text/plain".to_string()),
                compression: None,
                is_archive: false,
            },
        };

        let result = super::convert_entry(entry);

        // 应该使用完整的相对路径
        assert_eq!(result.name, nested_path);
    }

    /// 测试 convert_entry 不误判归档内的相对路径为绝对路径
    ///
    /// 归档内的路径如 "archive_content/sub_dir/" 不以 / 开头
    /// 但这不应该是绝对路径，应该使用 path
    #[test]
    fn test_convert_entry_archive_path_not_absolute() {
        use opsbox_core::odfs::types::{OpsEntry, OpsMetadata, OpsFileType};

        let archive_relative_path = "archive_content/sub_dir/";
        let entry = OpsEntry {
            name: "sub_dir".to_string(),
            path: archive_relative_path.to_string(),
            metadata: OpsMetadata {
                name: "sub_dir".to_string(),
                file_type: OpsFileType::Directory,
                size: 0,
                modified: None,
                mode: 0o755,
                mime_type: None,
                compression: None,
                is_archive: false,
            },
        };

        let result = super::convert_entry(entry);

        // 应该使用 path（归档内相对路径），而不是误判为绝对路径
        assert_eq!(result.name, archive_relative_path);
    }
}
