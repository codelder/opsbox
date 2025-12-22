use std::{path::PathBuf, sync::Arc, time::Duration};

use futures::{StreamExt, stream::FuturesUnordered};
use tracing::{trace, warn};

use opsbox_core::SqlitePool;
use opsbox_core::fs::{EntryStream, FsEntryStream, MultiFileEntryStream, create_archive_stream_from_reader};

use super::search::{SearchEvent, SearchProcessor};

// 统一读取并发度：使用 ENTRY_CONCURRENCY（范围 1-64，默认 8）
fn entry_concurrency() -> usize {
  std::env::var("ENTRY_CONCURRENCY")
    .ok()
    .and_then(|s| s.parse::<usize>().ok())
    .unwrap_or(8)
    .clamp(1, 64)
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

  /// 並发处理条目（有畊并发，默认并发度 8，可通过 ENTRY_CONCURRENCY 环境变量调整，范围 1-64）
  pub async fn process_stream(
    &mut self,
    entries: &mut dyn EntryStream,
    tx: tokio::sync::mpsc::Sender<SearchEvent>,
  ) -> Result<(), String> {
    let processor = self.processor.clone();
    let content_timeout = self.content_timeout;
    let mut in_flight: FuturesUnordered<_> = FuturesUnordered::new();
    let max_conc = entry_concurrency();

    loop {
      // 如果并发达到上限，先等待一个任务完成
      if in_flight.len() >= max_conc {
        let _ = in_flight.next().await; // 丢弃一个完成结果
        continue;
      }

      // 拉取下一个条目
      let next = entries.next_entry().await.map_err(|e| e.to_string())?;
      let Some((meta, mut reader)) = next else {
        break;
      };

      // 路径过滤（仅在主循环进行，任务内无需再次判断）
      if !self
        .processor
        .should_process_path_with(&meta.path, self.extra_path_filter.as_ref())
      {
        trace!("路径不匹配，跳过: {}", &meta.path);
        continue;
      }

      if meta.is_compressed {
        // tar.gz 等共享底层读取器的来源：必须保证串行处理，避免并发读取导致解码错乱
        while in_flight.next().await.is_some() {}
        let container_path = meta.container_path.clone();
        match tokio::time::timeout(
          content_timeout,
          processor.process_content(meta.path.clone(), &mut reader),
        )
        .await
        {
          Ok(Ok(Some(mut result))) => {
            result.archive_path = container_path;
            if tx.send(SearchEvent::Success(result)).await.is_err() {
              warn!("下游接收已关闭，终止条目流处理");
              break;
            }
          }
          Ok(Ok(None)) => {}
          Ok(Err(e)) => {
            warn!("处理条目内容失败: {}", e);
            let error_msg = format!("内容处理失败: {}", e);
            let _ = tx
              .send(SearchEvent::Error {
                source: "条目流#1".to_string(),
                message: error_msg,
                recoverable: true,
              })
              .await;
          }
          Err(_) => warn!("处理条目超时: {}", meta.path),
        }
      } else {
        // 本地文件等独立 Reader：可以并发处理
        let proc_clone = processor.clone();
        let tx_clone = tx.clone();
        let path = meta.path.clone();
        let container_path = meta.container_path.clone();
        in_flight.push(async move {
          match tokio::time::timeout(content_timeout, proc_clone.process_content(path.clone(), &mut reader)).await {
            Ok(Ok(Some(mut result))) => {
              result.archive_path = container_path;
              let _ = tx_clone.send(SearchEvent::Success(result)).await;
            }
            Ok(Ok(None)) => {}
            Ok(Err(e)) => {
              warn!("处理条目内容失败: {}", e);
              let error_msg = format!("内容处理失败: {}", e);
              let _ = tx_clone
                .send(SearchEvent::Error {
                  source: "条目流#2".to_string(),
                  message: error_msg,
                  recoverable: true,
                })
                .await;
            }
            Err(_) => {
              warn!("处理条目超时: {}", path);
            }
          }
        });
      }
    }

    // 等待所有在途任务完成
    while in_flight.next().await.is_some() {}

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

  /// 从来源配置创建条目流（不含 Agent）
  ///
  /// - Local: Dir/Files/Archive（自动探测 tar/tar.gz/gz/zip；zip 暂不支持）
  /// - S3: Archive（自动探测；zip 暂不支持）
  pub async fn create_stream(&self, source: crate::domain::config::Source) -> Result<Box<dyn EntryStream>, String> {
    use crate::domain::config::{Endpoint, Target};
    match (&source.endpoint, &source.target) {
      (Endpoint::Local { root }, Target::Dir { path, recursive }) => {
        let joined = if path == "." {
          root.clone()
        } else {
          format!("{}/{}", root, path)
        };
        // 使用 FsEntryStream 并尊重 recursive 标志
        let stream = FsEntryStream::new(PathBuf::from(joined), *recursive)
          .await
          .map_err(|e| format!("无法读取目录: {}", e))?;
        Ok(Box::new(stream))
      }
      (Endpoint::Local { root }, Target::Files { paths }) => {
        let files: Vec<String> = paths
          .iter()
          .map(|p| {
            if p.starts_with('/') {
              p.clone()
            } else {
              format!("{}/{}", root, p)
            }
          })
          .collect();
        Ok(Box::new(MultiFileEntryStream::new(files)))
      }
      (Endpoint::Local { root }, Target::Archive { path }) => {
        let full = if path.starts_with('/') {
          path.clone()
        } else {
          format!("{}/{}", root, path)
        };
        let file = tokio::fs::File::open(&full)
          .await
          .map_err(|e| format!("无法打开归档文件 {}: {}", full, e))?;
        create_archive_stream_from_reader(file, Some(&full)).await
      }
      (Endpoint::S3 { profile, bucket }, Target::Archive { path }) => {
        // 加载 Profile
        let profile_row = crate::repository::s3::load_s3_profile(&self.db_pool, profile)
          .await
          .map_err(|e| format!("加载 S3 Profile 失败: {:?}", e))?
          .ok_or_else(|| format!("S3 Profile 不存在: {}", profile))?;

        // 构造读取器
        let reader = {
          use crate::utils::storage::{ReaderProvider as _, S3ReaderProvider, get_or_create_s3_client};
          let _ = get_or_create_s3_client(&profile_row.endpoint, &profile_row.access_key, &profile_row.secret_key)
            .map_err(|e| format!("创建 S3 客户端失败: {:?}", e))?;

          let provider = S3ReaderProvider::new(
            &profile_row.endpoint,
            &profile_row.access_key,
            &profile_row.secret_key,
            bucket,
            path,
          );
          provider
            .open()
            .await
            .map_err(|e| format!("打开 S3 对象失败: {:?}", e))?
        };
        create_archive_stream_from_reader(reader, Some(path)).await
      }
      (Endpoint::S3 { .. }, _) => Err("S3 仅支持 archive 目标".to_string()),
      (Endpoint::Agent { .. }, _) => Err("Agent 来源请通过远程 SearchService 处理".to_string()),
    }
  }
}

/// 构建本地来源条目流（单文件/目录/归档，支持 target 提示）
///
/// 根据 target 类型优先使用明确处理，否则自动检测路径类型
pub async fn build_local_entry_stream(
  root_or_file: &str,
  target: Option<crate::domain::config::Target>,
) -> Result<Box<dyn EntryStream>, String> {
  use crate::domain::config::Target;

  // 根据 target 类型进行精确处理（与 Server 端对齐）
  if let Some(target) = target {
    match target {
      Target::Files { paths } => {
        // Files 类型：直接使用 MultiFileEntryStream，与 Server 端一致
        return Ok(Box::new(MultiFileEntryStream::new(paths)));
      }
      Target::Dir { path, recursive } => {
        // Dir 类型：直接使用 FsEntryStream，与 Server 端一致
        // path 为 "." 时使用 root_or_file 作为根目录
        let dir_path = if path == "." {
          PathBuf::from(root_or_file)
        } else {
          PathBuf::from(root_or_file).join(path)
        };
        let stream = FsEntryStream::new(dir_path, recursive)
          .await
          .map_err(|e| format!("无法读取目录 {}: {}", root_or_file, e))?;
        return Ok(Box::new(stream));
      }
      Target::Archive { path } => {
        // Archive 类型：处理归档文件
        // 如果 root_or_file 本身已经是归档文件（通过扩展名判断），直接使用它
        // 否则拼接 path（适用于 root_or_file 是目录的情况）
        let archive_path = {
          let root_path = PathBuf::from(root_or_file);
          let root_lower = root_or_file.to_lowercase();
          if root_lower.ends_with(".tar")
            || root_lower.ends_with(".tar.gz")
            || root_lower.ends_with(".tgz")
            || root_lower.ends_with(".gz")
          {
            // root_or_file 本身就是归档文件，直接使用
            root_path
          } else {
            // root_or_file 是目录，需要拼接 path
            root_path.join(path)
          }
        };
        let file = tokio::fs::File::open(&archive_path)
          .await
          .map_err(|e| format!("无法打开归档文件 {}: {}", archive_path.display(), e))?;
        return create_archive_stream_from_reader(file, Some(&archive_path.to_string_lossy())).await;
      }
    }
  }

  // 自动检测路径类型（当 target 为 None 时）
  // 先检查是否为归档文件（基于扩展名）
  let lower = root_or_file.to_lowercase();
  if lower.ends_with(".tar")
    || lower.ends_with(".tar.gz")
    || lower.ends_with(".tgz")
    || lower.ends_with(".gz")
    || lower.ends_with(".zip")
  {
    let file = tokio::fs::File::open(root_or_file)
      .await
      .map_err(|e| format!("无法打开归档文件 {}: {}", root_or_file, e))?;
    return create_archive_stream_from_reader(file, Some(root_or_file)).await;
  }

  // 通过 metadata 检测文件或目录
  match tokio::fs::metadata(root_or_file).await {
    Ok(meta) if meta.is_file() => Ok(Box::new(MultiFileEntryStream::new(vec![root_or_file.to_string()]))),
    Ok(meta) if meta.is_dir() => {
      // 默认递归（与 Server 端 Target::Dir 的默认行为一致）
      let stream = FsEntryStream::new(PathBuf::from(root_or_file), true)
        .await
        .map_err(|e| format!("无法读取目录 {}: {}", root_or_file, e))?;
      Ok(Box::new(stream))
    }
    Ok(_) => Err(format!("不支持的文件类型: {}", root_or_file)),
    Err(e) => Err(format!("无法访问路径 {}: {}", root_or_file, e)),
  }
}

/// 通用条目流处理函数（支持基于回调的结果处理）
///
/// 提供统一的条目流处理方式，可被 Server 和 Agent 复用，避免重复实现核心处理逻辑。
/// 事件通过回调函数返回，调用方可灵活处理（发送到 channel、生成消息等）。
pub async fn process_entry_stream_with_callback<F>(
  stream: Box<dyn EntryStream>,
  processor: Arc<crate::service::search::SearchProcessor>,
  extra_path_filter: Option<crate::query::PathFilter>,
  mut result_callback: F,
) -> Result<(usize, usize), String>
where
  F: FnMut(crate::service::search::SearchEvent) -> bool + Send,
{
  // 创建条目流处理器
  let mut stream_processor = EntryStreamProcessor::new(processor);
  if let Some(filter) = extra_path_filter {
    stream_processor = stream_processor.with_extra_path_filter(filter);
  }

  // 创建结果通道
  let (tx, mut rx) = tokio::sync::mpsc::channel(128);

  // 后台条目流处理任务
  let handle = tokio::spawn(async move {
    let mut stream = stream;
    let _ = stream_processor.process_stream(&mut *stream, tx).await;
  });

  let mut processed_count = 0;
  let mut matched_count = 0;

  // 收集结果并调用回调
  while let Some(result) = rx.recv().await {
    processed_count += 1;

    // 回调返回 false 则停止处理
    if !result_callback(result) {
      break;
    }

    matched_count += 1;
  }

  let _ = handle.await;
  Ok((processed_count, matched_count))
}

#[cfg(test)]
mod tests {
  // Tests are temporarily removed.
}
