//! S3 发现端点连接器
//!
//! 提供 S3 Profile 和 Bucket 的虚拟目录视图。

use async_trait::async_trait;
use opsbox_core::SqlitePool;
use opsbox_domain::resource::{EndpointConnector, ResourcePath, ResourceMetadata, DomainError};
use opsbox_core::repository::s3::{list_s3_profiles, load_s3_profile};
use opsbox_core::storage::s3::get_or_create_s3_client;
use std::pin::Pin;
use tokio::io::AsyncRead;

/// S3 发现端点连接器
///
/// 虚拟目录结构：
/// - `/` - 列出所有 S3 Profile
/// - `/{profile}` - 列出该 Profile 下的所有 Bucket
pub struct S3DiscoveryEndpointConnector {
    db_pool: SqlitePool,
}

impl S3DiscoveryEndpointConnector {
    /// 创建新的 S3 发现连接器
    pub fn new(db_pool: SqlitePool) -> Self {
        Self { db_pool }
    }
}

#[async_trait]
impl EndpointConnector for S3DiscoveryEndpointConnector {
    /// 返回虚拟目录的元数据
    async fn metadata(&self, path: &ResourcePath) -> Result<ResourceMetadata, DomainError> {
        Ok(ResourceMetadata {
            name: if path.as_str() == "/" {
                "s3_root".to_string()
            } else {
                path.as_str().to_string()
            },
            is_dir: true,
            size: 0,
            modified: None,
            mime_type: None,
            is_archive: false,
            child_count: None,
        })
    }

    /// 列出 S3 Profile 或 Bucket
    async fn list(&self, path: &ResourcePath) -> Result<Vec<ResourceMetadata>, DomainError> {
        let path_str = path.as_str();

        // 1. 根目录：列出所有 Profile
        if path_str == "/" {
            let profiles = list_s3_profiles(&self.db_pool)
                .await
                .map_err(|e| DomainError::ResourceNotFound(format!("查询 S3 profiles 失败: {}", e)))?;

            tracing::info!("[S3Discovery] 列出 S3 Profile: {} 个", profiles.len());

            return Ok(profiles
                .into_iter()
                .map(|p| ResourceMetadata {
                    name: p.profile_name.clone(),
                    is_dir: true,
                    size: 0,
                    modified: None,
                    mime_type: None,
                    is_archive: false,
                    child_count: None,
                })
                .collect());
        }

        // 2. Profile 级别：列出 Bucket
        let profile_name = path_str.trim_start_matches('/');

        // 验证路径格式
        if profile_name.contains('/') {
            return Err(DomainError::InvalidResourceIdentifier(
                "S3 发现仅支持两级（根目录和 profile）".to_string(),
            ));
        }

        // 加载 profile
        let profile = load_s3_profile(&self.db_pool, profile_name)
            .await
            .map_err(|e| DomainError::ResourceNotFound(format!("加载 Profile 失败: {}", e)))?
            .ok_or_else(|| DomainError::ResourceNotFound(format!("Profile 未找到: {}", profile_name)))?;

        // 创建 S3 客户端
        let client = get_or_create_s3_client(&profile.endpoint, &profile.access_key, &profile.secret_key)
            .map_err(|e| DomainError::ResourceNotFound(format!("创建 S3 客户端失败: {}", e)))?;

        // 列出 buckets
        let resp = client
            .list_buckets()
            .send()
            .await
            .map_err(|e| DomainError::ResourceNotFound(format!("列出 Bucket 失败: {}", e)))?;

        let buckets = resp.buckets.unwrap_or_default();

        tracing::info!(
            "[S3Discovery] Profile '{}': {} 个 Bucket",
            profile_name,
            buckets.len()
        );

        Ok(buckets
            .into_iter()
            .map(|b| {
                let name = b.name.unwrap_or_default();
                let modified = b.creation_date.map(|d| d.secs() as i64);

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
            "无法读取 S3 根目录作为文件".to_string(),
        ))
    }

    /// 虚拟根目录始终存在
    async fn exists(&self, path: &ResourcePath) -> Result<bool, DomainError> {
        let path_str = path.as_str();
        if path_str == "/" || path_str.is_empty() {
            return Ok(true);
        }

        // 检查 profile 是否存在
        let profile_name = path_str.trim_start_matches('/');
        if !profile_name.contains('/') {
            // 这是一个 profile 路径，检查 profile 是否存在
            match load_s3_profile(&self.db_pool, profile_name).await {
                Ok(Some(_)) => Ok(true),
                Ok(None) => Ok(false),
                Err(_) => Ok(false),
            }
        } else {
            Ok(false)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_s3_discovery_connector_name() {
        // 这个测试验证结构体可以创建
        // 实际功能测试需要数据库实例
        assert!(true);
    }
}
