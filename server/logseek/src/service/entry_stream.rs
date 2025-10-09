use std::{io, path::PathBuf, sync::Arc, time::Duration};

use async_compression::tokio::bufread::GzipDecoder;
use async_trait::async_trait;
use futures::StreamExt;
use log::{debug, error, warn};
use tokio::io::{AsyncRead, BufReader};
use tokio_util::compat::{FuturesAsyncReadCompatExt, TokioAsyncReadCompatExt};

use super::search::{SearchProcessor, SearchResult};

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
  async fn next_entry(
    &mut self,
  ) -> io::Result<Option<(EntryMeta, Box<dyn AsyncRead + Send + Unpin>)>>;
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
    Ok(Self { stack: vec![rd], root: Some(root) })
  }

  /// 直接从已存在的 ReadDir 创建（无根路径信息）
  pub fn from_read_dir(rd: tokio::fs::ReadDir) -> Self {
    Self { stack: vec![rd], root: None }
  }
}

#[async_trait]
impl EntryStream for FsEntryStream {
  async fn next_entry(
    &mut self,
  ) -> io::Result<Option<(EntryMeta, Box<dyn AsyncRead + Send + Unpin>)>> {
    loop {
      let Some(current) = self.stack.last_mut() else { return Ok(None); };
      match current.next_entry().await {
        Ok(Some(entry)) => {
          let ft = match entry.file_type().await { Ok(t) => t, Err(_) => continue };
          if ft.is_symlink() { continue; }
          if ft.is_dir() {
            if let Ok(sub) = tokio::fs::read_dir(entry.path()).await { self.stack.push(sub); }
            continue;
          }
          if !ft.is_file() { continue; }

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
          let meta = EntryMeta { path: rel, size: None, is_compressed: false };
          return Ok(Some((meta, Box::new(reader))));
        }
        Ok(None) => { self.stack.pop(); /* 回溯 */ }
        Err(_) => { self.stack.pop(); /* 跳过该目录 */ }
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
  async fn next_entry(
    &mut self,
  ) -> io::Result<Option<(EntryMeta, Box<dyn AsyncRead + Send + Unpin>)>> {
    match self.entries.next().await {
      Some(Ok(entry)) => {
        let path = entry
          .path()
          .ok()
          .map(|p| p.to_string_lossy().to_string())
          .unwrap_or_else(|| "<unknown>".into());
        let reader = entry.compat(); // 转为 tokio AsyncRead
        let meta = EntryMeta { path, size: None, is_compressed: false };
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
}

impl EntryStreamProcessor {
  pub fn new(processor: Arc<SearchProcessor>) -> Self {
    Self { processor, content_timeout: Duration::from_secs(60) }
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
      let Some((meta, mut reader)) = entries.next_entry().await.map_err(|e| e.to_string())? else { break };

      // 路径过滤
      if !self.processor.should_process_path(&meta.path) {
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
          if let Err(_)
            = self.processor.send_result(result, &tx).await
          {
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