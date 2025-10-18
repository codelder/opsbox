use std::{io, path::PathBuf, sync::Arc, time::Duration};

use async_compression::tokio::bufread::GzipDecoder;
use async_trait::async_trait;
use futures::StreamExt;
use log::{debug, warn};
use tokio::io::{AsyncRead, BufReader};
use tokio_util::compat::{FuturesAsyncReadCompatExt, TokioAsyncReadCompatExt};

use super::search::{SearchProcessor, SearchResult};
use opsbox_core::SqlitePool;

/// 条目元数据（目录相对路径或归档内路径）
#[derive(Clone, Debug)]
pub struct EntryMeta {
  pub path: String,
  pub size: Option<u64>,
  pub is_compressed: bool,
}

/// 统一的“条目流”抽象：每次产出 (EntryMeta, Reader)
#[async_trait]
pub trait EntryStream: Send {
  async fn next_entry(&mut self) -> io::Result<Option<(EntryMeta, Box<dyn AsyncRead + Send + Unpin>)>>;
}

/// 目录条目流（DFS 遍历）
pub struct FsEntryStream {
  stack: Vec<tokio::fs::ReadDir>,
  root: Option<PathBuf>,
}

impl FsEntryStream {
  /// 从根目录创建条目流
  pub async fn new(root: PathBuf) -> io::Result<Self> {
    let rd = tokio::fs::read_dir(&root).await?;
    Ok(Self {
      stack: vec![rd],
      root: Some(root),
    })
  }

  /// 直接从已存在的 ReadDir 创建（无根路径信息）
  pub fn from_read_dir(rd: tokio::fs::ReadDir) -> Self {
    Self {
      stack: vec![rd],
      root: None,
    }
  }
}

#[async_trait]
impl EntryStream for FsEntryStream {
  async fn next_entry(&mut self) -> io::Result<Option<(EntryMeta, Box<dyn AsyncRead + Send + Unpin>)>> {
    loop {
      let Some(current) = self.stack.last_mut() else {
        return Ok(None);
      };
      match current.next_entry().await {
        Ok(Some(entry)) => {
          let ft = match entry.file_type().await {
            Ok(t) => t,
            Err(_) => continue,
          };
          if ft.is_symlink() {
            continue;
          }
          if ft.is_dir() {
            if let Ok(sub) = tokio::fs::read_dir(entry.path()).await {
              self.stack.push(sub);
            }
            continue;
          }
          if !ft.is_file() {
            continue;
          }

          let path_abs = entry.path();
          let rel = if let Some(root) = &self.root {
            path_abs
              .strip_prefix(root)
              .unwrap_or(&path_abs)
              .to_string_lossy()
              .to_string()
          } else {
            path_abs.to_string_lossy().to_string()
          };
          let file = tokio::fs::File::open(&path_abs).await?;
          let reader = BufReader::new(file);
          let meta = EntryMeta {
            path: rel,
            size: None,
            is_compressed: false,
          };
          return Ok(Some((meta, Box::new(reader))));
        }
        Ok(None) => {
          self.stack.pop(); /* 回溯 */
        }
        Err(_) => {
          self.stack.pop(); /* 跳过该目录 */
        }
      }
    }
  }
}

/// tar.gz 条目流（基于 AsyncRead 输入）
pub struct TarEntryStream<R: AsyncRead + Send + Unpin + 'static> {
  entries: async_tar::Entries<tokio_util::compat::Compat<GzipDecoder<BufReader<R>>>>,
}

impl<R: AsyncRead + Send + Unpin + 'static> TarEntryStream<R> {
  pub async fn new(reader: R) -> io::Result<Self> {
    // gzip 解压 + 适配为 futures::io::AsyncRead
    let gz = GzipDecoder::new(BufReader::new(reader));
    let archive = async_tar::Archive::new(gz.compat());
    let entries = archive.entries()?; // 注意：entries 拥有 archive
    Ok(Self { entries })
  }
}

#[async_trait]
impl<R: AsyncRead + Send + Unpin + 'static> EntryStream for TarEntryStream<R> {
  async fn next_entry(&mut self) -> io::Result<Option<(EntryMeta, Box<dyn AsyncRead + Send + Unpin>)>> {
    match self.entries.next().await {
      Some(Ok(entry)) => {
        let path = entry
          .path()
          .ok()
          .map(|p| p.to_string_lossy().to_string())
          .unwrap_or_else(|| "<unknown>".into());
        let reader = entry.compat(); // 转为 tokio AsyncRead
        let meta = EntryMeta {
          path,
          size: None,
          is_compressed: false,
        };
        Ok(Some((meta, Box::new(reader))))
      }
      Some(Err(e)) => Err(e),
      None => Ok(None),
    }
  }
}

/// 统一条目流处理器：消费 EntryStream，调用 SearchProcessor 处理内容
pub struct EntryStreamProcessor {
  processor: Arc<SearchProcessor>,
  content_timeout: Duration,
  // 额外路径过滤器（可选），与用户查询中的 path: 规则做 AND
  extra_path_filter: Option<crate::query::PathFilter>,
}

impl EntryStreamProcessor {
  pub fn new(processor: Arc<SearchProcessor>) -> Self {
    Self {
      processor,
      content_timeout: Duration::from_secs(60),
      extra_path_filter: None,
    }
  }

  /// 设置额外路径过滤器（与用户 path: 规则做 AND）
  pub fn with_extra_path_filter(mut self, filter: crate::query::PathFilter) -> Self {
    self.extra_path_filter = Some(filter);
    self
  }

  #[allow(dead_code)]
  pub fn with_content_timeout(mut self, timeout: Duration) -> Self {
    self.content_timeout = timeout;
    self
  }

  /// 顺序处理条目（稳妥；后续可在确保 Reader 是 'static 时加入并发）
  pub async fn process_stream(
    &mut self,
    entries: &mut dyn EntryStream,
    tx: tokio::sync::mpsc::Sender<SearchResult>,
  ) -> Result<(), String> {
    loop {
      let Some((meta, mut reader)) = entries.next_entry().await.map_err(|e| e.to_string())? else {
        break;
      };

      // 路径过滤：优先应用额外过滤（若有），再应用用户查询的 path: 规则
      if !self
        .processor
        .should_process_path_with(&meta.path, self.extra_path_filter.as_ref())
      {
        debug!("路径不匹配，跳过: {}", &meta.path);
        continue;
      }

      // 带超时的内容处理
      match tokio::time::timeout(
        self.content_timeout,
        self.processor.process_content(meta.path.clone(), &mut reader),
      )
      .await
      {
        Ok(Ok(Some(result))) => {
          if self.processor.send_result(result, &tx).await.is_err() {
            // 接收方关闭
            warn!("下游接收已关闭，终止条目流处理");
            break;
          }
        }
        Ok(Ok(None)) => {}
        Ok(Err(e)) => {
          warn!("处理条目内容失败: {}", e);
        }
        Err(_) => {
          warn!("处理条目超时: {}", meta.path);
        }
      }
    }
    Ok(())
  }
}

/// 条目流工厂：根据 SourceConfig 构造 Box<dyn EntryStream>
pub struct EntryStreamFactory {
  db_pool: SqlitePool,
}

impl EntryStreamFactory {
  pub fn new(db_pool: SqlitePool) -> Self {
    Self { db_pool }
  }

  /// 从存储源配置创建条目流
  ///
  /// - Local: 返回 FsEntryStream（DFS 遍历）
  /// - S3(key): 读取指定对象并按 tar.gz 展开为条目流
  pub async fn create_stream(
    &self,
    source: crate::domain::config::SourceConfig,
  ) -> Result<Box<dyn EntryStream>, String> {
    match source {
      crate::domain::config::SourceConfig::Local { path, .. } => {
        let root = PathBuf::from(path);
        let stream = FsEntryStream::new(root).await.map_err(|e| e.to_string())?;
        Ok(Box::new(stream))
      }
      crate::domain::config::SourceConfig::S3 {
        profile,
        bucket: _bucket_opt,
        prefix: _prefix,
        pattern: _pattern,
        key,
      } => {
        // 仅支持 key 明确指向 tar.gz 对象的场景
        let key = key.ok_or_else(|| "S3 条目流暂仅支持指定单个对象 key".to_string())?;
        if !(key.ends_with(".tar.gz") || key.ends_with(".tgz")) {
          return Err("当前仅支持 S3 tar.gz 对象的条目流".to_string());
        }

        // 加载 Profile，获取 endpoint/bucket/AK/SK
        let profile_row = crate::repository::settings::load_s3_profile(&self.db_pool, &profile)
          .await
          .map_err(|e| format!("加载 S3 Profile 失败: {:?}", e))?
          .ok_or_else(|| format!("S3 Profile 不存在: {}", profile))?;

        // 构造读取器（复用旧的 S3 客户端工具）
        let reader = {
          use crate::utils::storage::{ReaderProvider as _, S3ReaderProvider, get_or_create_s3_client};
          // 先确保客户端创建（便于日志与错误提前暴露）
          let _ = get_or_create_s3_client(&profile_row.endpoint, &profile_row.access_key, &profile_row.secret_key)
            .map_err(|e| format!("创建 S3 客户端失败: {:?}", e))?;
          let provider = S3ReaderProvider::new(
            &profile_row.endpoint,
            &profile_row.access_key,
            &profile_row.secret_key,
            &profile_row.bucket,
            &key,
          );
          provider
            .open()
            .await
            .map_err(|e| format!("打开 S3 对象失败: {:?}", e))?
        };

        // 将对象作为 tar.gz 流展开
        let stream = TarEntryStream::new(reader).await.map_err(|e| e.to_string())?;
        Ok(Box::new(stream))
      }
      crate::domain::config::SourceConfig::Agent { .. } => {
        Err("Agent 来源暂不支持构造 EntryStream（建议走远程 SearchService）".to_string())
      }
    }
  }
}
