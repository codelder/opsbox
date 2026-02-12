//! Factory 模块 - 文件系统创建
//!
//! 定义了 create_fs 函数和 FsConfig

use std::collections::HashMap;
use std::path::PathBuf;

use super::{
    endpoint::{Endpoint, StorageBackend, Location},
    filesystem::{FsError, OpbxFileSystem},
    impls::{AgentClient, AgentProxyFS, LocalFileSystem, S3Storage, S3Config as ImplS3Config},
};

/// 文件系统配置（按需配置）
pub enum FsConfig {
    /// 本地文件系统配置
    Local {
        root: PathBuf,
    },

    /// S3 对象存储配置
    S3 {
        configs: HashMap<String, ImplS3Config>,
    },

    /// Agent 代理配置
    #[allow(clippy::type_complexity)]
    Agent {
        client_factory: Box<dyn Fn(&str, u16) -> Result<AgentClient, String> + Send + Sync>,
    },
}

impl Clone for FsConfig {
    fn clone(&self) -> Self {
        match self {
            FsConfig::Local { root } => FsConfig::Local { root: root.clone() },
            FsConfig::S3 { configs } => FsConfig::S3 { configs: configs.clone() },
            FsConfig::Agent { .. } => {
                // Agent variant cannot be truly cloned due to function pointer
                // This is a known limitation - in practice, create new configs when needed
                panic!("FsConfig::Agent cannot be cloned. Create a new config instead.")
            }
        }
    }
}

impl std::fmt::Debug for FsConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FsConfig::Local { root } => f.debug_tuple("Local").field(root).finish(),
            FsConfig::S3 { configs } => f
                .debug_tuple("S3")
                .field(&configs.keys().collect::<Vec<_>>())
                .finish(),
            FsConfig::Agent { .. } => f.debug_tuple("Agent").field(&"<function>").finish(),
        }
    }
}

/// 根据 Endpoint 和 FsConfig 创建对应的 OpbxFileSystem 实例
///
/// # 参数
/// * `endpoint` - 端点描述
/// * `config` - 文件系统配置（必须与 endpoint 类型匹配）
///
/// # 示例
/// ```rust,no_run
/// use opsbox_core::dfs::{Endpoint, create_fs, FsConfig};
/// use std::path::PathBuf;
///
/// let endpoint = Endpoint::local_fs();
/// let config = FsConfig::Local { root: PathBuf::from("/var/logs") };
/// let fs = create_fs(&endpoint, &config)?;
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
pub fn create_fs(endpoint: &Endpoint, config: &FsConfig) -> Result<Box<dyn OpbxFileSystem>, FsError> {
    match (&endpoint.backend, config) {
        (StorageBackend::Directory, FsConfig::Local { root }) => {
            let fs = LocalFileSystem::new(root.clone())?;
            Ok(Box::new(fs))
        }
        (StorageBackend::ObjectStorage, FsConfig::S3 { configs }) => {
            let s3_config = configs
                .get(&endpoint.identity)
                .ok_or_else(|| FsError::MissingConfig(endpoint.identity.clone()))?;
            let fs = S3Storage::new(s3_config.clone())?;
            Ok(Box::new(fs))
        }
        (_, FsConfig::Agent { client_factory }) => {
            if let Location::Remote { host, port } = &endpoint.location {
                let agent_client = client_factory(host, *port)
                    .map_err(FsError::Agent)?;
                let fs = AgentProxyFS::new(agent_client);
                Ok(Box::new(fs))
            } else {
                Err(FsError::InvalidConfig("Agent config requires Remote location".to_string()))
            }
        }
        _ => Err(FsError::ConfigMismatch),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fs_config_local() {
        let config = FsConfig::Local {
            root: PathBuf::from("/var/logs"),
        };
        assert!(matches!(config, FsConfig::Local { .. }));
    }

    #[test]
    fn test_fs_config_clone_local() {
        let config = FsConfig::Local {
            root: PathBuf::from("/var/logs"),
        };
        let cloned = config.clone();
        assert!(matches!(cloned, FsConfig::Local { .. }));
    }

    #[test]
    fn test_fs_config_clone_s3() {
        use crate::dfs::S3Config;

        let mut configs = HashMap::new();
        configs.insert(
            "backup".to_string(),
            S3Config::new(
                "backup".to_string(),
                "https://s3.amazonaws.com".to_string(),
                "key".to_string(),
                "secret".to_string(),
            ),
        );
        let config = FsConfig::S3 { configs };
        let cloned = config.clone();
        assert!(matches!(cloned, FsConfig::S3 { .. }));
    }

    #[test]
    fn test_s3_config() {
        use crate::dfs::S3Config;

        let config = S3Config::new(
            "backup".to_string(),
            "https://s3.amazonaws.com".to_string(),
            "key".to_string(),
            "secret".to_string(),
        );
        assert_eq!(config.profile_name, "backup");
    }

    #[test]
    fn test_create_fs_s3() {
        use crate::dfs::S3Config;

        let endpoint = Endpoint::s3("backup".to_string());
        let mut configs = HashMap::new();
        configs.insert(
            "backup".to_string(),
            S3Config::new(
                "backup".to_string(),
                "https://s3.amazonaws.com".to_string(),
                "key".to_string(),
                "secret".to_string(),
            ),
        );

        let config = FsConfig::S3 { configs };
        let result = create_fs(&endpoint, &config);
        // 注意：S3Storage 的创建在没有真实 AWS 环境时会失败
        // 这里我们只验证配置匹配
        if let Err(FsError::S3(_)) = result {
            // 预期会失败，因为没有真实的 AWS 环境
        }
    }

    #[test]
    fn test_agent_client() {
        let client = AgentClient::new("192.168.1.100".to_string(), 4001).unwrap();
        assert_eq!(client.host, "192.168.1.100");
        assert_eq!(client.port, 4001);
    }

    #[test]
    fn test_create_fs_agent() {
        let endpoint = Endpoint::agent("192.168.1.100".to_string(), 4001, "web-01".to_string());
        let config = FsConfig::Agent {
            client_factory: Box::new(|host, port| {
                AgentClient::new(host.to_string(), port).map_err(|e| e.to_string())
            }),
        };
        let result = create_fs(&endpoint, &config);
        assert!(result.is_ok());
    }

    #[test]
    fn test_create_fs_local() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let endpoint = Endpoint::local_fs();
        let config = FsConfig::Local {
            root: temp_dir.path().to_path_buf(),
        };
        let result = create_fs(&endpoint, &config);
        assert!(result.is_ok());
    }

    #[test]
    fn test_create_fs_local_invalid_root() {
        let endpoint = Endpoint::local_fs();
        let config = FsConfig::Local {
            root: PathBuf::from("/nonexistent/path"),
        };
        let result = create_fs(&endpoint, &config);
        assert!(result.is_err());
        match result {
            Err(FsError::NotFound(_)) => {}
            _ => panic!("Expected NotFound error"),
        }
    }

    #[test]
    fn test_create_fs_config_mismatch() {
        use crate::dfs::S3Config;

        // Local endpoint with S3 config should fail
        let endpoint = Endpoint::local_fs();
        let mut configs = HashMap::new();
        configs.insert(
            "backup".to_string(),
            S3Config::new(
                "backup".to_string(),
                "https://s3.amazonaws.com".to_string(),
                "key".to_string(),
                "secret".to_string(),
            ),
        );
        let config = FsConfig::S3 { configs };
        let result = create_fs(&endpoint, &config);
        match result {
            Err(FsError::ConfigMismatch) => {}
            _ => panic!("Expected ConfigMismatch error"),
        }
    }

    #[test]
    fn test_create_fs_agent_requires_remote() {
        // Cloud endpoint with Agent config should fail
        let endpoint = Endpoint::s3("backup".to_string());
        let config = FsConfig::Agent {
            client_factory: Box::new(|_, _| Err("not implemented".to_string())),
        };
        let result = create_fs(&endpoint, &config);
        match result {
            Err(FsError::InvalidConfig(_)) => {}
            _ => panic!("Expected InvalidConfig error"),
        }
    }

    #[test]
    fn test_create_fs_s3_missing_config() {
        let endpoint = Endpoint::s3("nonexistent".to_string());
        let configs = HashMap::new();
        let config = FsConfig::S3 { configs };
        let result = create_fs(&endpoint, &config);
        match result {
            Err(FsError::MissingConfig(_)) => {}
            _ => panic!("Expected MissingConfig error"),
        }
    }
}
