//! Explorer Service 模块
//!
//! 提供资源浏览和下载功能，支持本地文件系统、Agent 远程文件和 S3 存储。

mod lister;

// 重新导出 lister 的公共类型
pub use lister::{ListerConfig, LocalEntry, ResourceLister};

use crate::domain::{ResourceItem, ResourceType};
use opsbox_core::SqlitePool;
use opsbox_core::dfs::{
  endpoint::{Location, StorageBackend},
  filesystem::{DirEntry, MemoryReader, OpbxFileSystem},
  impls::{LocalFileSystem, S3Config, S3Storage},
  orl_parser::OrlParser,
  path::ResourcePath,
  resource::Resource,
};
use opsbox_core::fs::normalize_archive_entry_path;
use opsbox_core::repository::s3::load_s3_profile;
use percent_encoding::{AsciiSet, CONTROLS, utf8_percent_encode};

// Discovery filesystems - 仅在 agent-manager feature 启用时可用
#[cfg(feature = "agent-manager")]
use crate::fs::agent_discovery::AgentDiscoveryFileSystem;
use crate::fs::s3_discovery::S3DiscoveryFileSystem;

// Agent 相关导入 - 仅在 agent-manager feature 启用时可用
#[cfg(feature = "agent-manager")]
use agent_manager::AgentManager;
#[cfg(feature = "agent-manager")]
use opsbox_core::dfs::impls::{AgentClient, AgentProxyFS};

use std::collections::HashMap;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;

// 用于归档文件系统的临时文件处理
use tempfile;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt};

// ORL 的 entry 查询值编码集合：
// 保留 '/' 以兼容前端整串 ORL 编码流程，避免在 view 链路出现 %252F。
const ORL_ENTRY_ENCODE_SET: &AsciiSet = &CONTROLS
  .add(b' ')
  .add(b'"')
  .add(b'#')
  .add(b'&')
  .add(b'=')
  .add(b'%')
  .add(b'?')
  .add(b'+');

// ZIP 远端下载优先走内存缓冲，超过阈值再回退临时文件。
const ZIP_IN_MEMORY_THRESHOLD_BYTES: usize = 800 * 1024 * 1024;

enum ZipArchiveSource {
  InMemory(Vec<u8>),
  TempFile {
    path: PathBuf,
    file: tempfile::NamedTempFile,
  },
}

/// Explorer Service - 使用 DFS 模块进行文件系统操作
pub struct ExplorerService {
  db_pool: SqlitePool,
  #[cfg(feature = "agent-manager")]
  agent_manager: Option<Arc<AgentManager>>,
  s3_configs_cache: Arc<tokio::sync::RwLock<HashMap<String, S3Config>>>,
}

impl ExplorerService {
  pub fn new(db_pool: SqlitePool) -> Self {
    Self {
      db_pool,
      #[cfg(feature = "agent-manager")]
      agent_manager: None,
      s3_configs_cache: Arc::new(tokio::sync::RwLock::new(HashMap::new())),
    }
  }

  #[cfg(feature = "agent-manager")]
  pub fn with_agent_manager(mut self, manager: Arc<AgentManager>) -> Self {
    self.agent_manager = Some(manager);
    self
  }

  /// 列出指定路径下的资源
  pub async fn list(&self, orl: &str) -> Result<Vec<ResourceItem>, String> {
    // 解析 ORL 字符串为 Resource
    let resource = OrlParser::parse(orl).map_err(|e| format!("Failed to parse ORL: {}", e))?;

    // 自动检测归档类型
    let resource = self.auto_detect_archive(resource).await?;

    // 对于归档资源，使用专门的归档处理逻辑
    if let Some(ctx) = &resource.archive_context {
      return self.list_archive(&resource, ctx).await;
    }

    // 特殊处理：S3 profile 根路径（列出 buckets）
    let path_str = resource.primary_path.to_string();
    let is_s3_root =
      resource.endpoint.backend == StorageBackend::ObjectStorage && (path_str == "/" || path_str.is_empty());

    if is_s3_root {
      // 使用 S3DiscoveryFileSystem 列出该 profile 的 buckets
      let profile_name = &resource.endpoint.identity;
      let discovery_path = ResourcePath::parse(&format!("/{}", profile_name));
      let fs = S3DiscoveryFileSystem::new(self.db_pool.clone());
      let entries = fs
        .read_dir(&discovery_path)
        .await
        .map_err(|e| format!("Failed to list S3 buckets: {}", e))?;
      return Ok(entries.into_iter().map(|e| self.map_entry(e, &resource)).collect());
    }

    // 创建适当的文件系统
    let fs = self.create_fs_for_resource(&resource).await?;

    // 读取目录
    let entries = fs
      .read_dir(&resource.primary_path)
      .await
      .map_err(|e| format!("Failed to read directory: {}", e))?;

    // 转换为 ResourceItem
    Ok(entries.into_iter().map(|e| self.map_entry(e, &resource)).collect())
  }

  /// 下载资源
  pub async fn download(
    &self,
    orl: &str,
  ) -> Result<(String, Option<u64>, Pin<Box<dyn AsyncRead + Send + Unpin>>), String> {
    // 解析 ORL 字符串为 Resource
    let resource = OrlParser::parse(orl).map_err(|e| format!("Failed to parse ORL: {}", e))?;

    // 自动检测归档类型（对于下载也需要检测）
    let resource = self.auto_detect_archive(resource).await?;

    // 对于归档资源，使用专门的归档处理逻辑
    // 但如果 inner_path 是根路径（用户想下载整个归档文件），则走普通文件下载
    if let Some(ctx) = &resource.archive_context {
      // 检查 inner_path 是否是根路径（segments 为空或路径为 "/"）
      let is_root_path = ctx.inner_path.segments().is_empty()
        || ctx.inner_path.to_string() == "/";

      if !is_root_path {
        return self.download_archive_entry(&resource, ctx).await;
      }
      // 否则继续走普通文件下载逻辑，下载整个归档文件
    }

    // 创建适当的文件系统
    let fs = self.create_fs_for_resource(&resource).await?;

    // 获取元数据
    let meta = fs
      .metadata(&resource.primary_path)
      .await
      .map_err(|e| format!("Failed to get metadata: {}", e))?;

    // 打开文件
    let dfs_reader = fs
      .open_read(&resource.primary_path)
      .await
      .map_err(|e| format!("Failed to open file: {}", e))?;

    // 获取文件名
    let name = resource
      .primary_path
      .segments()
      .last()
      .cloned()
      .unwrap_or_else(|| "download".to_string());

    // DFS 现在直接返回 tokio::io::AsyncRead，无需适配器
    Ok((name, Some(meta.size), dfs_reader))
  }

  /// 列出归档内的资源
  ///
  /// 参考 ODFS 的实现方式：
  /// - 本地文件：直接使用文件路径，无需复制
  /// - 内存数据源（S3）：流式复制到临时文件
  /// - 远程文件（Agent）：流式复制到临时文件
  /// - 无大小限制，使用流式处理
  async fn list_archive(
    &self,
    resource: &Resource,
    ctx: &opsbox_core::dfs::archive::ArchiveContext,
  ) -> Result<Vec<ResourceItem>, String> {
    use opsbox_core::dfs::impls::ArchiveFileSystem;

    // 获取归档类型
    let archive_type = ctx
      .archive_type
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
        let temp_file_result = tokio::task::spawn_blocking(tempfile::NamedTempFile::new)
          .await
          .map_err(|e| format!("Failed to spawn blocking task: {}", e))?;
        let temp_file = temp_file_result.map_err(|e| format!("Failed to create temp file: {}", e))?;
        let temp_path = temp_file.path().to_path_buf();

        let base_fs = self.create_fs_for_resource(resource).await?;
        let mut reader = base_fs
          .open_read(&resource.primary_path)
          .await
          .map_err(|e| format!("Failed to open archive file: {}", e))?;

        let mut dst = tokio::fs::File::from_std(
          temp_file
            .as_file()
            .try_clone()
            .map_err(|e| format!("Failed to clone temp file: {}", e))?,
        );

        // DFS 现在统一使用 tokio::io::AsyncRead，直接使用 tokio::io::copy
        tokio::io::copy(&mut reader, &mut dst)
          .await
          .map_err(|e| format!("Failed to copy archive data: {}", e))?;

        dst
          .flush()
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
    let local_fs =
      LocalFileSystem::new(archive_dir.to_path_buf()).map_err(|e| format!("Failed to create local FS: {}", e))?;

    let archive_fs = if let Some(tf) = temp_file {
      ArchiveFileSystem::with_temp_file(local_fs, archive_type, archive_path, tf)
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
          "无法解析归档文件：文件可能损坏或使用了不兼容的格式。建议：1) 使用 'tar -tzf 文件名.tar.gz' 验证文件完整性 2) 尝试使用 'gunzip -c 文件名.tar.gz | tar tf -' 重新打包".to_string()
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
  /// 实现方式：
  /// - 本地文件：直接使用文件路径，通过 ArchiveFileSystem 读取
  /// - 远程文件（S3/Agent）：
  ///   - Tar/TarGz/Gz：流式提取目标 entry
  ///   - Zip：小文件内存提取，超阈值回退到临时文件（Zip 读取需要 Seek）
  async fn download_archive_entry(
    &self,
    resource: &Resource,
    ctx: &opsbox_core::dfs::archive::ArchiveContext,
  ) -> Result<(String, Option<u64>, Pin<Box<dyn AsyncRead + Send + Unpin>>), String> {
    // 获取归档类型
    let archive_type = ctx
      .archive_type
      .ok_or_else(|| "Archive type not specified".to_string())?;

    // 根据资源类型选择处理方式
    match resource.endpoint.location {
      opsbox_core::dfs::endpoint::Location::Local => {
        // 本地文件：直接使用原始文件路径，无需流式处理
        use opsbox_core::dfs::impls::ArchiveFileSystem;

        let file_path = resource.primary_path.to_string();
        let archive_path = std::path::PathBuf::from(file_path);

        // 获取归档文件的父目录作为 LocalFileSystem 的根
        let archive_dir = archive_path
          .parent()
          .ok_or_else(|| "Failed to get archive parent directory".to_string())?;

        // 创建归档文件系统
        let local_fs =
          LocalFileSystem::new(archive_dir.to_path_buf()).map_err(|e| format!("Failed to create local FS: {}", e))?;

        let archive_fs = ArchiveFileSystem::with_path(local_fs, archive_type, archive_path);

        // 使用归档内路径获取元数据和打开文件
        let meta = archive_fs.metadata(&ctx.inner_path).await.map_err(|e| {
          let error_str = e.to_string();
          if error_str.contains("numeric field did not have utf-8") {
            "无法解析归档文件：文件可能损坏或使用了不兼容的格式".to_string()
          } else {
            format!("Failed to get metadata: {}", error_str)
          }
        })?;

        let dfs_reader = archive_fs.open_read(&ctx.inner_path).await.map_err(|e| {
          let error_str = e.to_string();
          if error_str.contains("numeric field did not have utf-8") {
            "无法读取归档内文件：文件可能损坏".to_string()
          } else {
            format!("Failed to open file: {}", error_str)
          }
        })?;

        // 获取文件名
        let name = ctx
          .inner_path
          .segments()
          .last()
          .cloned()
          .unwrap_or_else(|| "download".to_string());

        Ok((name, Some(meta.size), dfs_reader))
      }
      _ => {
        // 远程文件或内存数据源：优先走流式提取；
        // ZIP 因需要 Seek，回退到临时文件路径。
        // 获取远程 reader
        let base_fs = self.create_fs_for_resource(resource).await?;
        let reader = base_fs
          .open_read(&resource.primary_path)
          .await
          .map_err(|e| format!("Failed to open archive file: {}", e))?;

        // 归档文件名（用于类型推断提示）
        let path_str = resource.primary_path.to_string();

        let (size, entry_reader) = self
          .download_archive_entry_from_reader(
            reader,
            Some(&path_str),
            archive_type,
            &ctx.inner_path,
          )
          .await?;

        // 获取文件名
        let name = ctx
          .inner_path
          .segments()
          .last()
          .cloned()
          .unwrap_or_else(|| "download".to_string());

        Ok((name, size, entry_reader))
      }
    }
  }

  async fn download_archive_entry_from_reader(
    &self,
    reader: Pin<Box<dyn AsyncRead + Send + Unpin>>,
    hint_name: Option<&str>,
    archive_type: opsbox_core::dfs::archive::ArchiveType,
    inner_path: &ResourcePath,
  ) -> Result<(Option<u64>, Pin<Box<dyn AsyncRead + Send + Unpin>>), String> {
    use opsbox_core::dfs::archive::ArchiveType;

    if archive_type == ArchiveType::Zip {
      let source = self
        .spool_zip_archive(reader, ZIP_IN_MEMORY_THRESHOLD_BYTES)
        .await?;
      return match source {
        ZipArchiveSource::InMemory(data) => self.open_zip_entry_from_memory(data, inner_path).await,
        ZipArchiveSource::TempFile { path, file } => self.open_zip_entry_from_temp_file(path, file, inner_path).await,
      };
    }

    use opsbox_core::fs::extract_archive_entry;
    let (meta, entry_reader) = extract_archive_entry(
      reader,
      hint_name,
      archive_type,
      &inner_path.to_string(),
    )
    .await
    .map_err(|e| format!("Failed to extract archive entry: {}", e))?;

    Ok((meta.size, Pin::from(entry_reader)))
  }

  async fn spool_zip_archive(
    &self,
    mut reader: Pin<Box<dyn AsyncRead + Send + Unpin>>,
    threshold_bytes: usize,
  ) -> Result<ZipArchiveSource, String> {
    let mut in_memory = Vec::with_capacity(64 * 1024);
    let mut chunk = [0u8; 64 * 1024];

    loop {
      let n = reader
        .read(&mut chunk)
        .await
        .map_err(|e| format!("Failed to read ZIP archive data: {}", e))?;
      if n == 0 {
        break;
      }

      if in_memory.len() + n > threshold_bytes {
        let temp_file_result = tokio::task::spawn_blocking(tempfile::NamedTempFile::new)
          .await
          .map_err(|e| format!("Failed to spawn blocking task: {}", e))?;
        let temp_file = temp_file_result.map_err(|e| format!("Failed to create temp file: {}", e))?;
        let temp_path = temp_file.path().to_path_buf();

        let mut dst = tokio::fs::File::from_std(
          temp_file
            .as_file()
            .try_clone()
            .map_err(|e| format!("Failed to clone temp file: {}", e))?,
        );

        if !in_memory.is_empty() {
          dst
            .write_all(&in_memory)
            .await
            .map_err(|e| format!("Failed to write buffered ZIP data to temp file: {}", e))?;

          // 释放内存缓冲，避免在后续 copy 期间占用峰值内存
          in_memory.clear();
          in_memory.shrink_to_fit();
        }

        dst
          .write_all(&chunk[..n])
          .await
          .map_err(|e| format!("Failed to write ZIP chunk to temp file: {}", e))?;

        tokio::io::copy(&mut reader, &mut dst)
          .await
          .map_err(|e| format!("Failed to copy ZIP archive data: {}", e))?;

        dst
          .flush()
          .await
          .map_err(|e| format!("Failed to flush temp file: {}", e))?;

        return Ok(ZipArchiveSource::TempFile {
          path: temp_path,
          file: temp_file,
        });
      }

      in_memory.extend_from_slice(&chunk[..n]);
    }

    Ok(ZipArchiveSource::InMemory(in_memory))
  }

  async fn open_zip_entry_from_memory(
    &self,
    archive_data: Vec<u8>,
    inner_path: &ResourcePath,
  ) -> Result<(Option<u64>, Pin<Box<dyn AsyncRead + Send + Unpin>>), String> {
    use async_zip::base::read::mem::ZipFileReader;
    use futures_util::io::AsyncReadExt as FuturesAsyncReadExt;

    let zip_reader = ZipFileReader::new(archive_data)
      .await
      .map_err(|e| format!("Failed to parse ZIP archive in memory: {}", e))?;

    let target = normalize_archive_entry_path(&inner_path.to_string());
    let entry_index = zip_reader
      .file()
      .entries()
      .iter()
      .position(|entry| {
        entry
          .filename()
          .as_str()
          .ok()
          .map(normalize_archive_entry_path)
          .as_deref()
          == Some(target.as_str())
      })
      .ok_or_else(|| format!("Entry '{}' not found in ZIP archive", inner_path))?;

    let size = zip_reader
      .file()
      .entries()
      .get(entry_index)
      .map(|entry| entry.uncompressed_size());

    let mut entry_reader = zip_reader
      .reader_with_entry(entry_index)
      .await
      .map_err(|e| format!("Failed to create ZIP entry reader from memory: {}", e))?;
    let mut entry_data = Vec::new();
    FuturesAsyncReadExt::read_to_end(&mut entry_reader, &mut entry_data)
      .await
      .map_err(|e| format!("Failed to read ZIP archive entry from memory: {}", e))?;

    let tokio_reader: Pin<Box<dyn AsyncRead + Send + Unpin>> = Box::pin(MemoryReader::new(entry_data));
    Ok((size, tokio_reader))
  }

  async fn open_zip_entry_from_temp_file(
    &self,
    temp_path: PathBuf,
    temp_file: tempfile::NamedTempFile,
    inner_path: &ResourcePath,
  ) -> Result<(Option<u64>, Pin<Box<dyn AsyncRead + Send + Unpin>>), String> {
    use opsbox_core::dfs::archive::ArchiveType;
    use opsbox_core::dfs::impls::ArchiveFileSystem;

    let archive_dir = temp_path
      .parent()
      .ok_or_else(|| "Failed to get archive parent directory".to_string())?;

    let local_fs = LocalFileSystem::new(archive_dir.to_path_buf()).map_err(|e| format!("Failed to create local FS: {}", e))?;
    let archive_fs = ArchiveFileSystem::with_temp_file(local_fs, ArchiveType::Zip, temp_path, temp_file);

    let meta = archive_fs.metadata(inner_path).await.map_err(|e| {
      let error_str = e.to_string();
      format!("Failed to get metadata from ZIP archive: {}", error_str)
    })?;
    let dfs_reader = archive_fs.open_read(inner_path).await.map_err(|e| {
      let error_str = e.to_string();
      format!("Failed to open ZIP archive entry: {}", error_str)
    })?;

    Ok((Some(meta.size), dfs_reader))
  }

  /// 自动检测归档类型（基于文件内容 magic bytes）
  async fn auto_detect_archive(&self, mut resource: Resource) -> Result<Resource, String> {
    use opsbox_core::dfs::archive::{ArchiveContext, detect_archive_type};

    // 如果已经是归档类型，直接返回
    if resource.archive_context.is_some() {
      return Ok(resource);
    }

    // 特殊处理：S3 根路径和 discovery endpoints 不需要检测
    let path_str = resource.primary_path.to_string();
    let is_discovery = matches!(resource.endpoint.identity.as_str(), "agent.root" | "s3.root");
    let is_s3_root =
      resource.endpoint.backend == StorageBackend::ObjectStorage && (path_str == "/" || path_str.is_empty());

    if is_discovery || is_s3_root {
      return Ok(resource);
    }

    // 创建临时文件系统读取文件头
    let fs = self.create_fs_for_resource(&resource).await?;

    if let Some(archive_type) = detect_archive_type(fs.as_ref(), &resource).await {
      resource.archive_context = Some(ArchiveContext::new(
        ResourcePath::parse("/"), // 归档内路径默认为根
        Some(archive_type),
      ));
    }

    Ok(resource)
  }

  /// 为资源创建适当的文件系统
  async fn create_fs_for_resource(&self, resource: &Resource) -> Result<Box<dyn OpbxFileSystem>, String> {
    // 检查是否是 discovery endpoints
    match resource.endpoint.identity.as_str() {
      #[cfg(feature = "agent-manager")]
      "agent.root" => {
        let manager = self
          .agent_manager
          .as_ref()
          .ok_or_else(|| "AgentManager not configured".to_string())?;
        let fs = AgentDiscoveryFileSystem::new(manager.clone());
        return Ok(Box::new(fs) as Box<dyn OpbxFileSystem>);
      }
      "s3.root" => {
        let fs = S3DiscoveryFileSystem::new(self.db_pool.clone());
        return Ok(Box::new(fs) as Box<dyn OpbxFileSystem>);
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

            let fs = LocalFileSystem::new(root).map_err(|e| format!("Failed to create local FS: {}", e))?;
            Ok(Box::new(fs) as Box<dyn OpbxFileSystem>)
          }
          #[cfg(feature = "agent-manager")]
          Location::Remote { host, port } => {
            // 对于 Agent endpoint，需要从 AgentManager 查询实际的 host 和 port
            let (actual_host, actual_port) = if let Some(manager) = &self.agent_manager {
              // 查询 Agent 的实际监听端口和主机
              let agent_info = manager.get_agent(&resource.endpoint.identity).await;
              let queried_port = agent_info.as_ref().and_then(|agent| {
                agent
                  .tags
                  .iter()
                  .find(|t| t.key == "listen_port")
                  .and_then(|t| t.value.parse::<u16>().ok())
              });
              let queried_host = agent_info
                .as_ref()
                .and_then(|agent| agent.tags.iter().find(|t| t.key == "host").map(|t| t.value.clone()));

              (
                queried_host.unwrap_or_else(|| host.clone()),
                queried_port.unwrap_or(*port),
              )
            } else {
              (host.clone(), *port)
            };

            let client = AgentClient::new(actual_host, actual_port)
              .map_err(|e| format!("Failed to create Agent client: {}", e))?;
            let fs = AgentProxyFS::new(client);
            Ok(Box::new(fs) as Box<dyn OpbxFileSystem>)
          }
          #[cfg(not(feature = "agent-manager"))]
          Location::Remote { .. } => {
            Err("Agent remote location not supported (agent-manager feature disabled)".to_string())
          }
          Location::Cloud => Err("Cloud location not supported for Directory backend".to_string()),
        }
      }
      StorageBackend::ObjectStorage => {
        // S3 对象存储
        let mut s3_config = self.get_s3_config(&resource.endpoint.identity).await?;

        // 使用统一的 bucket 提取方法
        let (bucket, _) = resource.extract_s3_bucket_and_key();
        s3_config.bucket = Some(bucket);

        let fs = S3Storage::new_async(s3_config)
          .await
          .map_err(|e| format!("Failed to create S3 FS: {}", e))?;
        Ok(Box::new(fs) as Box<dyn OpbxFileSystem>)
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
    #[allow(unused_variables)]
    let path = if parent_resource.endpoint.identity == "agent.root" {
      // 从名称中提取 agent ID
      let agent_id = if entry.name.contains(" (") {
        // 格式: "agent-name (agent-id)"
        entry
          .name
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
        .next_back()
        .and_then(|s| std::path::Path::new(s).file_stem())
        .and_then(|s| s.to_str())
        .unwrap_or("");

      let entry_path = if parent_resource
        .archive_context
        .as_ref()
        .and_then(|ctx| ctx.archive_type)
        == Some(opsbox_core::dfs::archive::ArchiveType::Gz)
        && entry_path_str.starts_with("/.tmp")
        && !correct_name.is_empty()
      {
        format!("/{}", correct_name)
      } else {
        entry_path_str.clone()
      };

      // 仅编码会破坏查询串结构的保留字符，保留 '/' 以避免前端链路出现双重编码。
      let encoded_entry = utf8_percent_encode(&entry_path, ORL_ENTRY_ENCODE_SET).to_string();
      format!("{}?entry={}", base, encoded_entry)
    } else {
      // 标准目录遍历
      if entry_path_str.starts_with('/') {
        // 如果条目已经提供绝对路径
        let auth = self.resource_endpoint_orl(parent_resource);

        // 构建路径部分，确保以 / 开头
        let path_suffix = entry
          .path
          .segments()
          .iter()
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
      Some(ctx) if matches!(ctx.archive_type, Some(opsbox_core::dfs::archive::ArchiveType::Gz)) => {
        // 检查是否是临时文件名模式
        if entry.name.starts_with(".tmp") {
          // 从原始归档路径中提取正确的文件名
          parent_resource
            .primary_path
            .to_string()
            .split('/')
            .next_back()
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
      let rtype = if entry.metadata.is_symlink {
        if entry.metadata.is_dir {
          ResourceType::LinkDir
        } else {
          ResourceType::LinkFile
        }
      } else if entry.metadata.is_dir {
        ResourceType::Dir
      } else {
        ResourceType::File
      };
      let children = if entry.metadata.is_dir { Some(true) } else { None };
      (rtype, path, children)
    };

    let mime_type = if !entry.metadata.is_dir {
      Self::guess_mime_type(&display_name)
    } else {
      None
    };

    ResourceItem {
      name: display_name,
      path: resource_path,
      r#type: resource_type,
      size: Some(entry.metadata.size),
      modified: entry
        .metadata
        .modified
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64),
      has_children,
      child_count: None,
      hidden_child_count: None,
      mime_type,
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

  /// 根据文件扩展名推断 MIME 类型
  fn guess_mime_type(name: &str) -> Option<String> {
    ResourceLister::guess_mime_type(name)
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use opsbox_test_common::archive_utils::{ArchiveFormat, ArchiveGenerator};
  use tokio::io::AsyncReadExt;

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

    // 简单测试：验证错误处理
    let service = ExplorerService::new(pool);
    let result = service.download("orl://local/nonexistent.txt").await;
    assert!(result.is_err(), "Non-existent file should fail");
  }

  #[test]
  fn test_archive_entry_path_encodes_query_delimiters_but_keeps_slash() {
    // 验证归档条目会编码查询分隔符，避免破坏 ORL 查询串；
    // 同时保留 '/'，兼容前端整串 ORL 编码链路。

    let base = "orl://local/tmp/test.gz";
    let entry_path = "/home/user/file&name=1.txt";

    // 模拟后端构造路径的逻辑（按 ORL entry 规则编码）
    let result = format!(
      "{}?entry={}",
      base,
      utf8_percent_encode(entry_path, ORL_ENTRY_ENCODE_SET)
    );

    // 验证：会编码 '&' 和 '='，且保留 '/'
    assert_eq!(result, "orl://local/tmp/test.gz?entry=/home/user/file%26name%3D1.txt");

    // 验证：后端解析 ORL 时能正确恢复原始 entry 路径
    let parsed = opsbox_core::dfs::OrlParser::parse(&result).expect("ORL should parse");
    let parsed_entry = parsed
      .archive_context
      .as_ref()
      .map(|ctx| ctx.inner_path.to_string())
      .unwrap_or_default();
    assert_eq!(parsed_entry, entry_path);
  }

  #[tokio::test]
  async fn test_download_archive_entry_from_reader_tar_gz_streaming() {
    let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
    let service = ExplorerService::new(pool);

    let mut archive_gen = ArchiveGenerator::new().unwrap();
    let archive_path = archive_gen.create_tar_gz_archive("remote.tar.gz").await.unwrap();
    let archive_bytes = tokio::fs::read(&archive_path).await.unwrap();

    let reader: Pin<Box<dyn AsyncRead + Send + Unpin>> = Box::pin(std::io::Cursor::new(archive_bytes));
    let inner_path = ResourcePath::parse("/logs/app1.log");

    let (size, mut entry_reader) = service
      .download_archive_entry_from_reader(
        reader,
        Some("remote.tar.gz"),
        opsbox_core::dfs::archive::ArchiveType::TarGz,
        &inner_path,
      )
      .await
      .unwrap();

    assert!(size.unwrap_or(0) > 0);

    let mut content = String::new();
    entry_reader.read_to_string(&mut content).await.unwrap();
    assert!(content.contains("Test error in tar"));
  }

  #[tokio::test]
  async fn test_spool_zip_archive_uses_memory_for_small_archive() {
    let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
    let service = ExplorerService::new(pool);

    let mut archive_gen = ArchiveGenerator::new().unwrap();
    let archive_path = archive_gen.create_zip_archive("small.zip").await.unwrap();
    let archive_bytes = tokio::fs::read(&archive_path).await.unwrap();

    let reader: Pin<Box<dyn AsyncRead + Send + Unpin>> = Box::pin(std::io::Cursor::new(archive_bytes.clone()));
    let source = service
      .spool_zip_archive(reader, ZIP_IN_MEMORY_THRESHOLD_BYTES)
      .await
      .unwrap();

    match source {
      ZipArchiveSource::InMemory(data) => assert_eq!(data.len(), archive_bytes.len()),
      ZipArchiveSource::TempFile { .. } => panic!("Small ZIP should use in-memory buffering"),
    }
  }

  #[tokio::test]
  async fn test_spool_zip_archive_falls_back_to_temp_file_for_large_archive() {
    let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
    let service = ExplorerService::new(pool);

    // 用较小阈值验证回退逻辑，避免测试构造超大数据。
    let test_threshold = 64 * 1024;
    let mut archive_gen = ArchiveGenerator::new().unwrap();
    let files = vec![("logs/large.log".to_string(), "x".repeat(test_threshold + 1024))];
    let archive_path = archive_gen
      .create_custom_archive("large.zip", files, ArchiveFormat::Zip)
      .await
      .unwrap();
    let archive_bytes = tokio::fs::read(&archive_path).await.unwrap();

    let reader: Pin<Box<dyn AsyncRead + Send + Unpin>> = Box::pin(std::io::Cursor::new(archive_bytes));
    let source = service
      .spool_zip_archive(reader, test_threshold)
      .await
      .unwrap();

    match source {
      ZipArchiveSource::TempFile { path, file: _ } => assert!(path.exists(), "Temp file path should exist"),
      ZipArchiveSource::InMemory(_) => panic!("Large ZIP should fall back to temp file"),
    }
  }

  #[tokio::test]
  async fn test_download_archive_entry_from_reader_zip() {
    let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
    let service = ExplorerService::new(pool);

    let mut archive_gen = ArchiveGenerator::new().unwrap();
    let archive_path = archive_gen.create_zip_archive("remote.zip").await.unwrap();
    let archive_bytes = tokio::fs::read(&archive_path).await.unwrap();

    let reader: Pin<Box<dyn AsyncRead + Send + Unpin>> = Box::pin(std::io::Cursor::new(archive_bytes));
    let inner_path = ResourcePath::parse("/logs/app1.log");

    let (size, mut entry_reader) = service
      .download_archive_entry_from_reader(
        reader,
        Some("remote.zip"),
        opsbox_core::dfs::archive::ArchiveType::Zip,
        &inner_path,
      )
      .await
      .unwrap();

    assert!(size.unwrap_or(0) > 0);

    let mut content = String::new();
    entry_reader.read_to_string(&mut content).await.unwrap();
    assert!(content.contains("Test error in zip"));
  }
}
