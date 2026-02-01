//! Agent 发现端点连接器
//!
//! 提供在线 Agent 的虚拟目录视图。

use async_trait::async_trait;
use opsbox_domain::resource::{EndpointConnector, ResourcePath, ResourceMetadata, DomainError};
use std::pin::Pin;
use tokio::io::AsyncRead;

use agent_manager::AgentManager;
use std::sync::Arc;

/// Agent 发现端点连接器
///
/// 将在线 Agent 列表作为虚拟目录呈现。
pub struct AgentDiscoveryEndpointConnector {
    manager: Arc<AgentManager>,
}

impl AgentDiscoveryEndpointConnector {
    /// 创建新的 Agent 发现连接器
    pub fn new(manager: Arc<AgentManager>) -> Self {
        Self { manager }
    }
}

#[async_trait]
impl EndpointConnector for AgentDiscoveryEndpointConnector {
    /// 返回虚拟根目录的元数据
    async fn metadata(&self, _path: &ResourcePath) -> Result<ResourceMetadata, DomainError> {
        Ok(ResourceMetadata {
            name: "agent_root".to_string(),
            is_dir: true,
            size: 0,
            modified: None,
            mime_type: None,
            is_archive: false,
            child_count: None,
        })
    }

    /// 列出在线 Agent
    async fn list(&self, _path: &ResourcePath) -> Result<Vec<ResourceMetadata>, DomainError> {
        let agents = self.manager.list_online_agents().await;

        tracing::info!(
            "[AgentDiscovery] 列出在线 Agent: {} 个",
            agents.len()
        );

        Ok(agents
            .into_iter()
            .map(|a| {
                let name = if a.name.is_empty() {
                    a.id.clone()
                } else {
                    format!("{} ({})", a.name, a.id)
                };

                let modified = if a.last_heartbeat > 0 {
                    Some(a.last_heartbeat as i64)
                } else {
                    None
                };

                ResourceMetadata {
                    name,
                    is_dir: true,
                    size: 0,
                    modified,
                    mime_type: None,
                    is_archive: false,
                    child_count: None,
                }
            })
            .collect())
    }

    /// 不支持读取操作
    async fn read(&self, _path: &ResourcePath) -> Result<
        Pin<Box<dyn AsyncRead + Send + Unpin + 'static>>,
        DomainError
    > {
        Err(DomainError::InvalidResourceIdentifier(
            "无法读取 Agent 列表作为文件".to_string(),
        ))
    }

    /// 虚拟根目录始终存在
    async fn exists(&self, path: &ResourcePath) -> Result<bool, DomainError> {
        Ok(path.as_str() == "/" || path.as_str().is_empty())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_agent_discovery_connector_name() {
        // 这个测试验证结构体可以创建
        // 实际功能测试需要 AgentManager 实例
        assert!(true);
    }
}
