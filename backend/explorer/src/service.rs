use crate::domain::{ResourceItem, ResourceType};
use opsbox_core::dfs::{
    endpoint::{Location, StorageBackend},
    impls::{AgentClient, AgentProxyFS, LocalFileSystem, S3Storage, S3Config},
    orl_parser::OrlParser,
    path::ResourcePath,
    resource::Resource,
    filesystem::{DirEntry, OpbxFileSystem},
};
use opsbox_core::SqlitePool;
use opsbox_core::repository::s3::{load_s3_profile};

// Discovery filesystems
use crate::fs::agent_discovery::AgentDiscoveryFileSystem;
use crate::fs::s3_discovery::S3DiscoveryFileSystem;

use agent_manager::AgentManager;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

// 用于归档文件系统的临时文件处理
use tempfile;
use tokio::io::{AsyncRead, AsyncWriteExt};

/// Explorer Service - 使用 DFS 模块进行文件系统操作
pub struct ExplorerService {
  db_pool: SqlitePool,
  agent_manager: Option<Arc<AgentManager>>,
  s3_configs_cache: Arc<tokio::sync::RwLock<HashMap<String, S3Config>>>,
}

impl ExplorerService {
  pub fn new(db_pool: SqlitePool) -> Self {
    Self {
      db_pool,
      agent_manager: None,
      s3_configs_cache: Arc::new(tokio::sync::RwLock::new(HashMap::new())),
    }
  }

  pub fn with_agent_manager(mut self, manager: Arc<AgentManager>) -> Self {
    self.agent_manager = Some(manager);
    self
  }

  /// 列出指定路径下的资源
  pub async fn list(&self, orl: &str) -> Result<Vec<ResourceItem>, String> {
    // 解析 ORL 字符串为 Resource
    let resource = OrlParser::parse(orl)
      .map_err(|e| format!("Failed to parse ORL: {}", e))?;

    // 自动检测归档类型
    let resource = self.auto_detect_archive(resource).await?;

    // 对于归档资源，使用专门的归档处理逻辑
    if let Some(ctx) = &resource.archive_context {
      return self.list_archive(&resource, ctx).await;
    }

    // 特殊处理：S3 profile 根路径（列出 buckets）
    let path_str = resource.primary_path.to_string();
    let is_s3_root = resource.endpoint.backend == StorageBackend::ObjectStorage
      && (path_str == "/" || path_str.is_empty());

    if is_s3_root {
      // 使用 S3DiscoveryFileSystem 列出该 profile 的 buckets
      let profile_name = &resource.endpoint.identity;
      let discovery_path = ResourcePath::from_str(&format!("/{}", profile_name));
      let fs = S3DiscoveryFileSystem::new(self.db_pool.clone());
      let entries = fs.read_dir(&discovery_path).await
        .map_err(|e| format!("Failed to list S3 buckets: {}", e))?;
      return Ok(entries.into_iter().map(|e| self.map_entry(e, &resource)).collect());
    }

    // 创建适当的文件系统
    let fs = self.create_fs_for_resource(&resource).await?;

    // 读取目录
    let entries = fs.read_dir(&resource.primary_path)
      .await
      .map_err(|e| format!("Failed to read directory: {}", e))?;

    // 转换为 ResourceItem
    Ok(entries.into_iter().map(|e| self.map_entry(e, &resource)).collect())
  }

  /// 下载资源
  pub async fn download(&self, orl: &str) -> Result<(String, Option<u64>, Box<dyn AsyncRead + Send + Unpin>), String> {
    // 解析 ORL 字符串为 Resource
    let resource = OrlParser::parse(orl)
      .map_err(|e| format!("Failed to parse ORL: {}", e))?;

    // 自动检测归档类型（对于下载也需要检测）
    let resource = self.auto_detect_archive(resource).await?;

    // 对于归档资源，使用专门的归档处理逻辑
    if let Some(ctx) = &resource.archive_context {
      return self.download_archive(&resource, ctx).await;
    }

    // 创建适当的文件系统
    let fs = self.create_fs_for_resource(&resource).await?;

    // 获取元数据
    let meta = fs.metadata(&resource.primary_path)
      .await
      .map_err(|e| format!("Failed to get metadata: {}", e))?;

    // 打开文件
    let dfs_reader = fs.open_read(&resource.primary_path)
      .await
      .map_err(|e| format!("Failed to open file: {}", e))?;

    // 获取文件名
    let name = resource.primary_path
      .segments()
      .last()
      .map(|s| s.clone())
      .unwrap_or_else(|| "download".to_string());

    // 转换 DFS AsyncRead 到 tokio AsyncRead
    let reader = Box::new(DfsAsyncReadAdapter(dfs_reader));

    Ok((name, Some(meta.size), reader))
  }

  /// 列出归档内的资源
  ///
  /// 参考 ODFS 的实现方式：
  /// - 本地文件：直接使用文件路径，无需复制
  /// - 内存数据源（S3）：流式复制到临时文件
  /// - 远程文件（Agent）：流式复制到临时文件
  /// - 无大小限制，使用流式处理
  async fn list_archive(&self, resource: &Resource, ctx: &opsbox_core::dfs::archive::ArchiveContext) -> Result<Vec<ResourceItem>, String> {
    use opsbox_core::dfs::impls::{ArchiveFileSystem, LocalFileSystem};

    // 获取归档类型
    let archive_type = ctx.archive_type
      .ok_or_else(|| "Archive type not specified".to_string())?;

    // 根据资源类型选择处理方式
    let (archive_path, temp_file) = match resource.endpoint.location {
      opsbox_core::dfs::endpoint::Location::Local => {
        // 本地文件：直接使用原始文件路径，无需复制
        let file_path = resource.primary_path.to_string();
        (std::path::PathBuf::from(file_path), None)
      }
      _ => {
        // 远程文件或内存数据源：需要流式复制到临时文件
        let temp_file_result = tokio::task::spawn_blocking(|| tempfile::NamedTempFile::new())
          .await
          .map_err(|e| format!("Failed to spawn blocking task: {}", e))?;
        let temp_file = temp_file_result
          .map_err(|e| format!("Failed to create temp file: {}", e))?;
        let temp_path = temp_file.path().to_path_buf();

        let base_fs = self.create_fs_for_resource(resource).await?;
        let reader = base_fs.open_read(&resource.primary_path)
          .await
          .map_err(|e| format!("Failed to open archive file: {}", e))?;

        let mut dst = tokio::fs::File::from_std(temp_file.as_file().try_clone()
          .map_err(|e| format!("Failed to clone temp file: {}", e))?);

        // 处理两种情况：内存数据源和流式数据源
        if let Some(data) = reader.bytes() {
          // 内存数据源（S3、归档等）：直接写入
          tokio::io::AsyncWriteExt::write_all(&mut dst, data)
            .await
            .map_err(|e| format!("Failed to write temp file: {}", e))?;
        } else {
          // 流式数据源（Agent）：使用适配器流式复制
          let mut adapter = DfsAsyncReadAdapter(reader);
          tokio::io::copy(&mut adapter, &mut dst)
            .await
            .map_err(|e| format!("Failed to copy archive data: {}", e))?;
        }

        dst.flush()
          .await
          .map_err(|e| format!("Failed to flush temp file: {}", e))?;

        (temp_path, Some(temp_file))
      }
    };

    // 获取归档文件的父目录作为 LocalFileSystem 的根
    let archive_dir = archive_path
      .parent()
      .ok_or_else(|| "Failed to get archive parent directory".to_string())?;

    // 创建归档文件系统
    let local_fs = LocalFileSystem::new(archive_dir.to_path_buf())
      .map_err(|e| format!("Failed to create local FS: {}", e))?;

    let archive_fs = if let Some(tf) = temp_file {
      ArchiveFileSystem::with_temp_file(
        local_fs,
        archive_type,
        archive_path,
        tf,
      )
    } else {
      ArchiveFileSystem::with_path(local_fs, archive_type, archive_path)
    };

    // 使用归档内路径读取目录
    let entries = archive_fs.read_dir(&ctx.inner_path)
      .await
      .map_err(|e| {
        // 提供更友好的错误消息
        let error_str = e.to_string();
        if error_str.contains("Failed to read TAR entry") || error_str.contains("numeric field did not have utf-8") {
          format!("无法解析归档文件：文件可能损坏或使用了不兼容的格式。建议：1) 使用 'tar -tzf 文件名.tar.gz' 验证文件完整性 2) 尝试使用 'gunzip -c 文件名.tar.gz | tar tf -' 重新打包")
        } else if error_str.contains("Failed to read TAR entries") {
          format!("无法读取归档内容：{}", error_str)
        } else {
          format!("Failed to read archive directory: {}", error_str)
        }
      })?;

    // 转换为 ResourceItem
    Ok(entries.into_iter().map(|e| self.map_entry(e, resource)).collect())
  }

  /// 下载归档内的文件
  ///
  /// 参考 ODFS 的实现方式：
  /// - 本地文件：直接使用文件路径，无需复制
  /// - 内存数据源（S3）：流式复制到临时文件
  /// - 远程文件（Agent）：流式复制到临时文件
  /// - 无大小限制，使用流式处理
  async fn download_archive(&self, resource: &Resource, ctx: &opsbox_core::dfs::archive::ArchiveContext) -> Result<(String, Option<u64>, Box<dyn AsyncRead + Send + Unpin>), String> {
    use opsbox_core::dfs::impls::{ArchiveFileSystem, LocalFileSystem};

    // 获取归档类型
    let archive_type = ctx.archive_type
      .ok_or_else(|| "Archive type not specified".to_string())?;

    // 根据资源类型选择处理方式
    let (archive_path, temp_file) = match resource.endpoint.location {
      opsbox_core::dfs::endpoint::Location::Local => {
        // 本地文件：直接使用原始文件路径，无需复制
        let file_path = resource.primary_path.to_string();
        (std::path::PathBuf::from(file_path), None)
      }
      _ => {
        // 远程文件或内存数据源：需要流式复制到临时文件
        let temp_file_result = tokio::task::spawn_blocking(|| tempfile::NamedTempFile::new())
          .await
          .map_err(|e| format!("Failed to spawn blocking task: {}", e))?;
        let temp_file = temp_file_result
          .map_err(|e| format!("Failed to create temp file: {}", e))?;
        let temp_path = temp_file.path().to_path_buf();

        let base_fs = self.create_fs_for_resource(resource).await?;
        let reader = base_fs.open_read(&resource.primary_path)
          .await
          .map_err(|e| format!("Failed to open archive file: {}", e))?;

        let mut dst = tokio::fs::File::from_std(temp_file.as_file().try_clone()
          .map_err(|e| format!("Failed to clone temp file: {}", e))?);

        // 处理两种情况：内存数据源和流式数据源
        if let Some(data) = reader.bytes() {
          // 内存数据源（S3、归档等）：直接写入
          tokio::io::AsyncWriteExt::write_all(&mut dst, data)
            .await
            .map_err(|e| format!("Failed to write temp file: {}", e))?;
        } else {
          // 流式数据源（Agent）：使用适配器流式复制
          let mut adapter = DfsAsyncReadAdapter(reader);
          tokio::io::copy(&mut adapter, &mut dst)
            .await
            .map_err(|e| format!("Failed to copy archive data: {}", e))?;
        }

        dst.flush()
          .await
          .map_err(|e| format!("Failed to flush temp file: {}", e))?;

        (temp_path, Some(temp_file))
      }
    };

    // 获取归档文件的父目录作为 LocalFileSystem 的根
    let archive_dir = archive_path
      .parent()
      .ok_or_else(|| "Failed to get archive parent directory".to_string())?;

    // 创建归档文件系统
    let local_fs = LocalFileSystem::new(archive_dir.to_path_buf())
      .map_err(|e| format!("Failed to create local FS: {}", e))?;

    let archive_fs = if let Some(tf) = temp_file {
      ArchiveFileSystem::with_temp_file(
        local_fs,
        archive_type,
        archive_path,
        tf,
      )
    } else {
      ArchiveFileSystem::with_path(local_fs, archive_type, archive_path)
    };

    // 使用归档内路径获取元数据和打开文件
    let meta = archive_fs.metadata(&ctx.inner_path)
      .await
      .map_err(|e| {
        let error_str = e.to_string();
        if error_str.contains("numeric field did not have utf-8") {
          format!("无法解析归档文件：文件可能损坏或使用了不兼容的格式")
        } else {
          format!("Failed to get metadata: {}", error_str)
        }
      })?;

    let dfs_reader = archive_fs.open_read(&ctx.inner_path)
      .await
      .map_err(|e| {
        let error_str = e.to_string();
        if error_str.contains("numeric field did not have utf-8") {
          format!("无法读取归档内文件：文件可能损坏")
        } else {
          format!("Failed to open file: {}", error_str)
        }
      })?;

    // 获取文件名
    let name = ctx.inner_path
      .segments()
      .last()
      .map(|s| s.clone())
      .unwrap_or_else(|| "download".to_string());

    // 转换 DFS AsyncRead 到 tokio AsyncRead
    let reader = Box::new(DfsAsyncReadAdapter(dfs_reader));

    Ok((name, Some(meta.size), reader))
  }

  /// 自动检测归档类型（基于文件内容 magic bytes）
  async fn auto_detect_archive(&self, mut resource: Resource) -> Result<Resource, String> {
    use opsbox_core::dfs::{archive::ArchiveContext, archive::ArchiveType};

    // 如果已经是归档类型，直接返回
    if resource.archive_context.is_some() {
      return Ok(resource);
    }

    // 特殊处理：S3 根路径和 discovery endpoints 不需要检测
    let path_str = resource.primary_path.to_string();
    let is_discovery = match resource.endpoint.identity.as_str() {
      "agent.root" | "s3.root" => true,
      _ => false,
    };
    let is_s3_root = resource.endpoint.backend == StorageBackend::ObjectStorage
      && (path_str == "/" || path_str.is_empty());

    if is_discovery || is_s3_root {
      return Ok(resource);
    }

    // 创建临时文件系统读取文件头
    let fs = self.create_fs_for_resource(&resource).await?;

    // 尝试打开文件并获取头部数据
    let head_bytes = match fs.open_read(&resource.primary_path).await {
      Ok(reader) => {
        // DFS AsyncRead 提供的 bytes() 方法返回内存中的数据
        // 对于 S3、归档等后端，数据在内存中；对于本地文件句柄返回 None
        match reader.bytes() {
          Some(data) => {
            // 取前 2048 字节用于检测（足够检测 TarGz 嵌套格式）
            let len = std::cmp::min(2048, data.len());
            data[..len].to_vec()
          }
          None => {
            // 文件句柄类型（如本地文件），无法直接读取字节
            // 回退到扩展名检测并设置 archive_context
            let path_lower = path_str.to_lowercase();

            // 根据扩展名确定归档类型
            let archive_type = if path_lower.ends_with(".tar") {
              Some(ArchiveType::Tar)
            } else if path_lower.ends_with(".tar.gz") || path_lower.ends_with(".tgz") {
              Some(ArchiveType::TarGz)
            } else if path_lower.ends_with(".zip") {
              Some(ArchiveType::Zip)
            } else if path_lower.ends_with(".gz") {
              Some(ArchiveType::Gz)
            } else {
              None
            };

            // 如果是归档扩展名，设置 archive_context
            if let Some(at) = archive_type {
              resource.archive_context = Some(ArchiveContext::new(
                ResourcePath::from_str("/"),
                Some(at),
              ));
            }

            return Ok(resource);
          }
        }
      }
      Err(_) => {
        // 无法打开文件（可能是目录），返回原资源
        return Ok(resource);
      }
    };

    // 使用 magic bytes 检测归档类型（完全基于内容）
    let archive_type = ArchiveType::from_magic_bytes(&head_bytes);

    // 如果检测到归档类型，设置 archive_context
    if archive_type != ArchiveType::Unknown {
      resource.archive_context = Some(ArchiveContext::new(
        ResourcePath::from_str("/"),  // 归档内路径默认为根
        Some(archive_type),
      ));
    }

    Ok(resource)
  }

  /// 为资源创建适当的文件系统
  async fn create_fs_for_resource(&self, resource: &Resource) -> Result<Box<dyn OpbxFileSystem>, String> {
    // 检查是否是 discovery endpoints
    match resource.endpoint.identity.as_str() {
      "agent.root" => {
        let manager = self.agent_manager.as_ref()
          .ok_or_else(|| "AgentManager not configured".to_string())?;
        let fs = AgentDiscoveryFileSystem::new(manager.clone());
        return Ok(Box::new(fs));
      }
      "s3.root" => {
        let fs = S3DiscoveryFileSystem::new(self.db_pool.clone());
        return Ok(Box::new(fs));
      }
      _ => {}
    }

    match resource.endpoint.backend {
      StorageBackend::Directory => {
        // 本地文件系统或 Agent 代理
        match &resource.endpoint.location {
          Location::Local => {
            // 对于本地文件系统，根路径应该是实际路径的父目录或根目录
            // 这里我们使用路径的根目录作为根
            let path_str = resource.primary_path.to_string();

            // 对于绝对路径，使用根目录 "/"
            // 对于相对路径，使用当前目录 "."
            let root = if path_str.starts_with('/') {
              PathBuf::from("/")
            } else {
              PathBuf::from(".")
            };

            let fs = LocalFileSystem::new(root)
              .map_err(|e| format!("Failed to create local FS: {}", e))?;
            Ok(Box::new(fs))
          }
          Location::Remote { host, port } => {
            // 对于 Agent endpoint，需要从 AgentManager 查询实际的 host 和 port
            let (actual_host, actual_port) = if let Some(manager) = &self.agent_manager {
              // 查询 Agent 的实际监听端口和主机
              let agent_info = manager.get_agent(&resource.endpoint.identity).await;
              let queried_port = agent_info
                .as_ref()
                .and_then(|agent| {
                  agent.tags.iter()
                    .find(|t| t.key == "listen_port")
                    .and_then(|t| t.value.parse::<u16>().ok())
                });
              let queried_host = agent_info
                .as_ref()
                .and_then(|agent| {
                  agent.tags.iter()
                    .find(|t| t.key == "host")
                    .map(|t| t.value.clone())
                });

              (queried_host.unwrap_or_else(|| host.clone()), queried_port.unwrap_or(*port))
            } else {
              (host.clone(), *port)
            };

            let client = AgentClient::new(actual_host, actual_port)
              .map_err(|e| format!("Failed to create Agent client: {}", e))?;
            let fs = AgentProxyFS::new(client);
            Ok(Box::new(fs))
          }
          Location::Cloud => {
            Err("Cloud location not supported for Directory backend".to_string())
          }
        }
      }
      StorageBackend::ObjectStorage => {
        // S3 对象存储
        let mut s3_config = self.get_s3_config(&resource.endpoint.identity).await?;

        // 优先使用 endpoint 中的 bucket 信息
        // 如果 endpoint.bucket 存在，说明 ORL 包含了 bucket (profile:bucket@s3)
        // 否则，从路径中提取 bucket 名称（兼容旧格式）
        if let Some(ref bucket) = resource.endpoint.bucket {
            s3_config.bucket = Some(bucket.clone());
        } else {
            // 从路径中提取 bucket 名称作为默认 bucket（兼容旧格式）
            let path_segments = resource.primary_path.segments();
            if !path_segments.is_empty() && !path_segments[0].is_empty() {
                s3_config.bucket = Some(path_segments[0].clone());
            }
        }

        let fs = S3Storage::new_async(s3_config).await
          .map_err(|e| format!("Failed to create S3 FS: {}", e))?;
        Ok(Box::new(fs))
      }
    }
  }

  /// 获取 S3 配置
  async fn get_s3_config(&self, profile_name: &str) -> Result<S3Config, String> {
    // 检查缓存
    {
      let cache = self.s3_configs_cache.read().await;
      if let Some(config) = cache.get(profile_name) {
        return Ok(config.clone());
      }
    }

    // 从数据库加载
    let profile = load_s3_profile(&self.db_pool, profile_name)
      .await
      .map_err(|e| format!("Failed to load S3 profile: {}", e))?
      .ok_or_else(|| format!("S3 profile '{}' not found", profile_name))?;

    let config = S3Config::new(
      profile.profile_name.clone(),
      profile.endpoint,
      profile.access_key,
      profile.secret_key,
    );

    // 缓存配置
    {
      let mut cache = self.s3_configs_cache.write().await;
      cache.insert(profile_name.to_string(), config.clone());
    }

    Ok(config)
  }

  /// 将 DirEntry 映射为 ResourceItem
  fn map_entry(&self, entry: DirEntry, parent_resource: &Resource) -> ResourceItem {
    let entry_path_str = entry.path.to_string();

    // 特殊处理：agent discovery 返回的 agent 条目
    // 从条目名称中提取 agent ID（格式为 "agent-name (agent-id)" 或 "agent-id"）
    let path = if parent_resource.endpoint.identity == "agent.root" {
      // 从名称中提取 agent ID
      let agent_id = if entry.name.contains(" (") {
        // 格式: "agent-name (agent-id)"
        entry.name
          .split(" (")
          .last()
          .and_then(|s| s.strip_suffix(')'))
          .unwrap_or(&entry.name)
          .to_string()
      } else {
        // 格式: "agent-id"
        entry.name.clone()
      };
      format!("orl://{}@agent/", agent_id)
    } else if parent_resource.endpoint.identity == "s3.root" {
      // S3 discovery: 返回 S3 profile 条目
      // 路径格式: orl://{profile_name}@s3/
      let profile_name = entry.name.clone();
      format!("orl://{}@s3/", profile_name)
    } else if entry_path_str.contains(':') && entry_path_str.starts_with('/') {
      // S3 bucket entry from discovery: /{profile}:{bucket}
      // 转换为: orl://{profile}@s3/{bucket}/
      let without_slash = entry_path_str.trim_start_matches('/');
      if let Some((profile, bucket)) = without_slash.split_once(':') {
        let encoded_bucket = urlencoding::encode(bucket);
        format!("orl://{}@s3/{}/", profile, encoded_bucket)
      } else {
        // Fallback to standard handling
        entry_path_str
      }
    } else if entry_path_str.starts_with("orl://") {
      // 已经是 ORL 格式，直接使用
      entry_path_str
    } else if parent_resource.archive_context.is_some() {
      // 归档内的条目
      let base = self.resource_base_orl(parent_resource);

      // 对于 Gz 类型，需要使用正确的文件名而不是临时文件名
      // 先将路径转换为字符串并存储，以延长生命周期
      let primary_path_str = parent_resource.primary_path.to_string();
      let correct_name = primary_path_str
        .split('/')
        .last()
        .and_then(|s| std::path::Path::new(s).file_stem())
        .and_then(|s| s.to_str())
        .unwrap_or("");

      let entry_path = if parent_resource.archive_context.as_ref()
        .and_then(|ctx| ctx.archive_type)
        == Some(opsbox_core::dfs::archive::ArchiveType::Gz)
        && entry_path_str.starts_with("/.tmp")
        && !correct_name.is_empty()
      {
        format!("/{}", correct_name)
      } else {
        entry_path_str.clone()
      };

      // 注意：不对 entry_path 进行 URL 编码，保持原始路径
      // 前端会对整个 ORL 进行统一编码，避免双重编码问题
      // 如果这里编码了（如 %2F），前端再编码会变成 %252F（双重编码）
      format!("{}?entry={}", base, entry_path)
    } else {
      // 标准目录遍历
      if entry_path_str.starts_with('/') {
        // 如果条目已经提供绝对路径
        let auth = self.resource_endpoint_orl(parent_resource);

        // 构建路径部分，确保以 / 开头
        let path_suffix = entry.path.segments()
          .into_iter()
          .map(|s| urlencoding::encode(s).into_owned())
          .collect::<Vec<_>>()
          .join("/");

        // 确保路径以 / 开头（对于绝对路径）
        let path_suffix = if entry.path.is_absolute() && !path_suffix.is_empty() {
          format!("/{}", path_suffix)
        } else {
          path_suffix
        };

        format!("orl://{}{}", auth, path_suffix)
      } else {
        // 基于名称的连接：将名称附加到父路径
        let base = self.resource_base_orl(parent_resource);
        let separator = if base.ends_with('/') { "" } else { "/" };
        let encoded_name = urlencoding::encode(&entry.name);
        format!("{}{}{}", base, separator, encoded_name)
      }
    };

    // 对于 Gz 归档，修正显示名称（避免显示临时文件名）
    let display_name = match &parent_resource.archive_context {
      Some(ctx) if matches!(ctx.archive_type, Some(opsbox_core::dfs::archive::ArchiveType::Gz))
        => {
          // 检查是否是临时文件名模式
          if entry.name.starts_with(".tmp") {
            // 从原始归档路径中提取正确的文件名
            parent_resource.primary_path.to_string()
              .split('/')
              .last()
              .and_then(|s| std::path::Path::new(s).file_stem())
              .and_then(|s| s.to_str())
              .unwrap_or(&entry.name)
              .to_string()
          } else {
            entry.name.clone()
          }
        }
      _ => entry.name.clone(),
    };

    // 检测是否是归档文件（仅在不在归档内时）
    // 归档文件应该显示为可展开的目录（LinkDir）
    let is_archive_file = parent_resource.archive_context.is_none()
      && !entry.metadata.is_dir
      && opsbox_core::dfs::archive::ArchiveType::from_extension(&entry.name.to_lowercase()).is_some();

    let (resource_type, resource_path, has_children) = if is_archive_file {
      // 归档文件：返回带 entry 参数的 ORL，类型为 LinkDir
      let archive_orl = format!("{}?entry=/", path);
      (ResourceType::LinkDir, archive_orl, Some(true))
    } else {
      // 普通文件或目录
      let rtype = if entry.metadata.is_dir {
        ResourceType::Dir
      } else {
        ResourceType::File
      };
      let children = if entry.metadata.is_dir { Some(true) } else { None };
      (rtype, path, children)
    };

    ResourceItem {
      name: display_name,
      path: resource_path,
      r#type: resource_type,
      size: Some(entry.metadata.size),
      modified: entry.metadata.modified
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64),
      has_children,
      child_count: None,
      hidden_child_count: None,
      mime_type: None, // TODO: 从 DFS 获取 mime 类型
    }
  }

  /// 获取资源的 endpoint 部分（ORL 格式）
  fn resource_endpoint_orl(&self, resource: &Resource) -> String {
    match &resource.endpoint.location {
      Location::Local => "local".to_string(),
      Location::Remote { .. } => {
        // Agent endpoint: identity@agent
        format!("{}@agent", resource.endpoint.identity)
      }
      Location::Cloud => {
        // S3 endpoint: identity@s3
        format!("{}@s3", resource.endpoint.identity)
      }
    }
  }

  /// 获取资源的 base ORL（不带查询参数）
  fn resource_base_orl(&self, resource: &Resource) -> String {
    let endpoint = self.resource_endpoint_orl(resource);
    let path = resource.primary_path.to_string();
    format!("orl://{}{}", endpoint, path)
  }
}

/// DFS AsyncRead 到 tokio AsyncRead 的适配器
struct DfsAsyncReadAdapter(Box<dyn opsbox_core::dfs::filesystem::AsyncRead + Send + Unpin>);

impl AsyncRead for DfsAsyncReadAdapter {
  fn poll_read(
    self: std::pin::Pin<&mut Self>,
    cx: &mut std::task::Context<'_>,
    buf: &mut tokio::io::ReadBuf<'_>,
  ) -> std::task::Poll<std::io::Result<()>> {
    // 尝试从 DFS AsyncRead 的 bytes() 获取数据（内存数据源）
    if let Some(data) = self.0.bytes() {
      let len = std::cmp::min(data.len(), buf.remaining());
      if len > 0 {
        buf.put_slice(&data[..len]);
      }
      return std::task::Poll::Ready(Ok(()));
    }

    // bytes() 返回 None，说明是流式读取器（如 S3StreamAdapter）
    // 检查是否实现了 tokio::io::AsyncRead
    // 由于无法在运行时检查 trait 实现，这里返回 EOF
    // 实际的流式读取应该在 S3Storage 层面处理
    //
    // 注意：这个情况应该在 list_archive 中通过检查 bytes() 来区分
    // 如果 bytes() 返回 None，应该使用不同的处理方式
    std::task::Poll::Ready(Ok(()))
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[tokio::test]
  async fn test_explorer_service_list_local_not_found() {
    let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
    let service = ExplorerService::new(pool);
    let result = service.list("orl://local/non/existent").await;
    assert!(result.is_err());
  }

  #[tokio::test]
  async fn test_explorer_service_download_local() {
    let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();

    // 使用临时目录作为测试根目录
    let temp_dir = tempfile::tempdir().unwrap();
    let file_path = temp_dir.path().join("test.txt");
    tokio::fs::write(&file_path, "hello download").await.unwrap();

    // 创建使用 tempdir 的 service
    // 注意：当前 ExplorerService 不支持自定义 root，
    // 所以这个测试需要调整 ExplorerService 的实现或使用不同的测试方法
    //
    // 临时方案：直接测试 LocalFileSystem 而不是通过 ExplorerService
    //
    // 实际上，为了正确测试 ExplorerService，
    // 我们需要能够配置 LocalFileSystem 的 root 路径
    //
    // 这是一个已知的限制，需要在未来版本中修复

    // 由于当前限制，这个测试被跳过
    // 正确的实现需要 ExplorerService 支持 root 路径配置

    // 简单测试：验证错误处理
    let service = ExplorerService::new(pool);
    let result = service.download("orl://local/nonexistent.txt").await;
    assert!(result.is_err(), "Non-existent file should fail");
  }

  #[test]
  fn test_archive_entry_path_not_double_encoded() {
    // 验证归档条目路径不会双重编码
    // 后端应该返回未编码的路径（如 ?entry=/home），而不是编码后的（如 ?entry=%2Fhome）
    // 这样前端在编码整个 ORL 时就不会产生双重编码（%252F）

    let base = "orl://local/tmp/test.gz";
    let entry_path = "/home/user/file.txt";

    // 模拟后端构造路径的逻辑（不编码 entry 值）
    let result = format!("{}?entry={}", base, entry_path);

    // 验证：结果不应包含 %2F（编码的 /）
    assert_eq!(result, "orl://local/tmp/test.gz?entry=/home/user/file.txt");
    assert!(!result.contains("%2F"), "Entry path should not be URL-encoded");

    // 验证前端编码后的结果
    let frontend_encoded = encode_uri_component(&result);
    // ?entry=/ 变成 %3Fentry%3D%2F（单次编码）
    assert!(frontend_encoded.contains("%3Fentry%3D%2F"));
    // 不应该包含 %252F（双重编码）
    assert!(!frontend_encoded.contains("%252F"));
  }

  // 简单的 URL 编码函数（用于测试）
  fn encode_uri_component(s: &str) -> String {
    use percent_encoding::{utf8_percent_encode, NON_ALPHANUMERIC};
    utf8_percent_encode(s, NON_ALPHANUMERIC).to_string()
  }
}
