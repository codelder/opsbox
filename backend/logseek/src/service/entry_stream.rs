use std::{io, path::PathBuf, sync::Arc, time::Duration};

use futures::{StreamExt, stream::FuturesUnordered};
use num_cpus;
use tokio::io::{AsyncRead, AsyncReadExt};
use tracing::{trace, warn};

use opsbox_core::SqlitePool;
use opsbox_core::fs::{EntrySource, EntryStream, PrefixedReader};
use opsbox_core::odfs::orl::EndpointType;
use opsbox_core::odfs::providers::{LocalOpsFS, S3OpsFS};
use opsbox_core::odfs::OrlManager;

use super::search::{SearchEvent, SearchProcessor};

// 统一读取并发度：使用 ENTRY_CONCURRENCY（范围 1-128）
// 对于 CPU 密集型任务（同机部署），根据 CPU 核心数动态调整，充分利用多核 CPU
fn entry_concurrency() -> usize {
  // 优先使用环境变量
  if let Ok(val) = std::env::var("ENTRY_CONCURRENCY")
    && let Ok(parsed) = val.parse::<usize>()
  {
    return parsed.clamp(1, 128);
  }

  // 默认值：根据 CPU 核心数动态计算
  // 对于 CPU 密集型任务（解压、搜索），建议设置为 CPU 核心数的 2-4 倍
  // 这样可以充分利用多核，同时避免过多的上下文切换
  let cpu_count = num_cpus::get();
  let default = (cpu_count * 2).clamp(8, 32); // 至少 8，最多 32，默认 2 倍核心数

  default.clamp(1, 128)
}

// 已删除 try_resolve_minio_local_path 函数
// 原因：MinIO 使用 Erasure Coding，数据被分片存储，无法直接通过 bucket/key 映射到文件系统路径
// 即使在同一台机器上，也必须通过 S3 API 访问，以确保数据一致性和安全性

/// 预读结果：小文件完整内容，或大文件的已读取部分
enum PreloadResult {
  /// 小文件：完整内容已读取
  Complete(Vec<u8>),
  /// 大文件：已读取部分内容（reader 已被部分消费）
  Partial(Vec<u8>),
}

/// 组合 Reader：先读取 prefix，然后读取 inner
/// 用于处理预读时已读取的部分内容
// 移除 ChainedReader，改用 opsbox_core::fs::PrefixedReader

/// 预读文件条目到内存
/// 返回：
/// - Complete(content): 文件完全读取（小文件）
/// - Partial(content): 文件太大，只读取了部分（reader 已被部分消费）
async fn preload_entry(reader: &mut (dyn AsyncRead + Send + Unpin), max_size: usize) -> io::Result<PreloadResult> {
  let mut buffer = Vec::with_capacity(64 * 1024); // 64KB 初始容量
  let mut temp = vec![0u8; 64 * 1024];

  loop {
    let n = reader.read(&mut temp).await?;
    if n == 0 {
      // EOF，文件完全读取
      return Ok(PreloadResult::Complete(buffer));
    }
    buffer.extend_from_slice(&temp[..n]);

    // 如果超过最大大小，返回已读取的部分
    if buffer.len() > max_size {
      return Ok(PreloadResult::Partial(buffer));
    }
  }
}

/// 统一条目流处理器：消费 EntryStream，调用 SearchProcessor 处理内容
pub struct EntryStreamProcessor {
  processor: Arc<SearchProcessor>,
  content_timeout: Duration,
  // 额外路径过滤器（可选），与用户查询中的 path: 规则做 AND
  extra_path_filter: Option<crate::query::PathFilter>,
  cancel_token: Option<tokio_util::sync::CancellationToken>,
  // 基础路径（可选）：如果设置，过滤时将先去除该前缀（用于支持相对路径 glob）
  base_path: Option<PathBuf>,
}

impl EntryStreamProcessor {
  pub fn new(processor: Arc<SearchProcessor>) -> Self {
    Self {
      processor,
      content_timeout: Duration::from_secs(60),
      extra_path_filter: None,
      cancel_token: None,
      base_path: None,
    }
  }

  /// 设置取消令牌
  pub fn with_cancel_token(mut self, token: tokio_util::sync::CancellationToken) -> Self {
    self.cancel_token = Some(token);
    self
  }

  /// 设置基础路径（用于相对路径过滤）
  pub fn with_base_path(mut self, base_path: impl Into<PathBuf>) -> Self {
    self.base_path = Some(base_path.into());
    self
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
      // 检查取消
      if tx.is_closed() {
        trace!("探测到下游通道已关闭，主动终止扫描任务");
        break;
      }

      if let Some(token) = &self.cancel_token
        && token.is_cancelled()
      {
        break;
      }

      // 如果并发达到上限，先等待一个任务完成
      if in_flight.len() >= max_conc {
        if let Some(handle) = in_flight.next().await {
          let _ = handle; // JoinHandle 本身就是 future，已经在 FuturesUnordered 中等待
        }
        continue;
      }

      // 拉取下一个条目
      let next = entries.next_entry().await.map_err(|e| e.to_string())?;
      let Some((meta, mut reader)) = next else {
        break;
      };

      // 路径过滤（仅在主循环进行，任务内无需再次判断）
      // 对于目录类型，使用 base_path 进行相对路径转换以支持相对 glob 匹配
      let path_str: String = if let Some(base) = &self.base_path {
        let path_obj = std::path::Path::new(&meta.path);
        if let Ok(p) = path_obj.strip_prefix(base) {
          // 可能得到形如 "./file.log" 的相对路径；对 strict glob（literal_separator=true）来说这会导致 "*.log" 不匹配。
          let mut out = std::path::PathBuf::new();
          let mut leading = true;
          for c in p.components() {
            match c {
              std::path::Component::CurDir if leading => continue,
              _ => {
                leading = false;
                out.push(c.as_os_str());
              }
            }
          }
          out.to_string_lossy().into_owned()
        } else if let (Ok(canon_path), Ok(canon_base)) =
          (std::fs::canonicalize(path_obj), std::fs::canonicalize(base))
        {
          // canonicalize 后如果能 strip_prefix，则必须使用相对路径进行匹配，
          // 否则在 strict glob（literal_separator=true）下，像 "*.log" 这类模式会因为包含分隔符而无法匹配绝对路径。
          if let Ok(p) = canon_path.strip_prefix(&canon_base) {
            let mut out = std::path::PathBuf::new();
            let mut leading = true;
            for c in p.components() {
              match c {
                std::path::Component::CurDir if leading => continue,
                _ => {
                  leading = false;
                  out.push(c.as_os_str());
                }
              }
            }
            out.to_string_lossy().into_owned()
          } else {
            path_obj.to_string_lossy().into_owned()
          }
        } else {
          path_obj.to_string_lossy().into_owned()
        }
      } else {
        std::path::Path::new(&meta.path).to_string_lossy().into_owned()
      };
      if !self
        .processor
        .should_process_path_with(&path_str, self.extra_path_filter.as_ref())
      {
        trace!(
          "路径不匹配，跳过: meta.path={} path_str_for_filter={} base_path={:?}",
          &meta.path,
          &path_str,
          self.base_path
        );
        continue;
      }

      if meta.is_compressed || meta.source == EntrySource::Tar || meta.source == EntrySource::TarGz {
        // tar.gz 等共享底层读取器的来源：必须保证串行读取，但可以预读小文件到内存后并发处理
        // 优化：对于文件（< 120MB），预读到内存后允许并发处理，充分利用多核 CPU
        const MAX_PRELOAD_SIZE: usize = 120 * 1024 * 1024; // 120MB

        // 尝试预读文件到内存
        match preload_entry(&mut reader, MAX_PRELOAD_SIZE).await {
          Ok(PreloadResult::Complete(content)) => {
            // 小文件完全读取，可以并发处理
            trace!(
              "归档条目预读成功（完整），允许并发处理: {} ({} bytes)",
              meta.path,
              content.len()
            );
            let proc_clone = processor.clone();
            let tx_clone = tx.clone();
            let path = meta.path.clone();
            let container_path = meta.container_path.clone();

            // 使用 spawn 创建任务，统一类型
            let handle = tokio::spawn(async move {
              let mut mem_reader = std::io::Cursor::new(content);
              match tokio::time::timeout(
                content_timeout,
                proc_clone.process_content(path.clone(), &mut mem_reader),
              )
              .await
              {
                Ok(Ok(Some(mut result))) => {
                  result.archive_path = container_path;
                  let _ = tx_clone.send(SearchEvent::Success(result)).await;
                }
                Ok(Ok(None)) => {}
                Ok(Err(e)) => {
                  warn!("处理预读条目内容失败: {}", e);
                  let error_msg = format!("内容处理失败: {}", e);
                  let _ = tx_clone
                    .send(SearchEvent::Error {
                      source: "条目流#preload".to_string(),
                      message: error_msg,
                      recoverable: true,
                    })
                    .await;
                }
                Err(_) => {
                  warn!("处理预读条目超时: {}", path);
                }
              }
            });
            in_flight.push(handle);
          }
          Ok(PreloadResult::Partial(prefix)) => {
            // 大文件：已读取部分内容，使用 PrefixedReader 组合已读取部分和剩余 reader
            trace!(
              "归档条目过大，使用流式处理: {} (已读取 {} bytes)",
              meta.path,
              prefix.len()
            );
            while let Some(handle) = in_flight.next().await {
              let _ = handle; // 等待所有并发任务完成
            }

            // 使用 PrefixedReader 组合已读取的部分和剩余的 reader
            let combined_reader = PrefixedReader::new(prefix, reader);
            let container_path = meta.container_path.clone();

            // 串行处理大文件（大文件必须串行，因为 reader 已被部分消费）
            match tokio::time::timeout(
              content_timeout,
              processor.process_content(meta.path.clone(), &mut Box::pin(combined_reader)),
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
                warn!("处理大文件条目内容失败: {}", e);
                let error_msg = format!("内容处理失败: {}", e);
                let _ = tx
                  .send(SearchEvent::Error {
                    source: "条目流#large".to_string(),
                    message: error_msg,
                    recoverable: true,
                  })
                  .await;
              }
              Err(_) => warn!("处理大文件条目超时: {}", meta.path),
            }
          }
          Err(e) => {
            // 预读失败（IO 错误）
            warn!("归档条目预读失败: {}: {}", meta.path, e);
            let error_msg = format!("预读失败: {}", e);
            let _ = tx
              .send(SearchEvent::Error {
                source: "条目流#preload-error".to_string(),
                message: error_msg,
                recoverable: true,
              })
              .await;
          }
        }
      } else {
        // 本地文件等独立 Reader：可以并发处理
        let proc_clone = processor.clone();
        let tx_clone = tx.clone();
        let path = meta.path.clone();
        let container_path = meta.container_path.clone();
        // 使用 spawn 创建任务，统一类型
        let handle = tokio::spawn(async move {
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
        in_flight.push(handle);
      }
    }

    // 等待所有在途任务完成
    while let Some(handle) = in_flight.next().await {
      let _ = handle; // JoinHandle 本身就是 future，已经在 FuturesUnordered 中等待
    }

    Ok(())
  }
}

/// 创建条目流（不含 Agent）
///
/// 根据 ORL 配置创建对应的条目流：
/// - Local: Dir/Files/Archive（自动探测 tar/tar.gz/gz/zip；zip 暂不支持）
/// - S3: Archive（自动探测；zip 暂不支持）
pub async fn create_entry_stream(
  db_pool: &SqlitePool,
  orl: &opsbox_core::odfs::orl::ORL,
) -> Result<Box<dyn EntryStream>, String> {
  // 构造临时的 OrlManager
  // 在生产架构中，OrlManager 最好是全局单例或注入的，但为了保持 LogSeek 独立性及 API 兼容，
  // 我们在这里临时组装它。
  let mut manager = OrlManager::new();

  match orl.endpoint_type() {
    Ok(EndpointType::Local) => {
      // 注册 Local Provider
      // LogSeek 默认 Local 是根目录
      manager.register("local".to_string(), Arc::new(LocalOpsFS::new(None)));
    }
    Ok(EndpointType::S3) => {
      let profile = orl.effective_id();
      // 加载 Profile
      let profile_row = crate::repository::s3::load_s3_profile(db_pool, &profile)
        .await
        .map_err(|e| format!("加载 S3 Profile 失败: {:?}", e))?
        .ok_or_else(|| format!("S3 Profile 不存在: {}", profile))?;

      // 构造 S3 客户端
      use crate::utils::storage::get_or_create_s3_client;
      let client = get_or_create_s3_client(
        &profile_row.endpoint,
        &profile_row.access_key,
        &profile_row.secret_key,
      )
      .map_err(|e| format!("创建 S3 客户端失败: {:?}", e))?;

      // 注册 S3 Provider
      let (bucket_name, _) = orl
        .path()
        .trim_start_matches('/')
        .split_once('/')
        .unwrap_or((orl.path().trim_start_matches('/'), ""));

      // 注意：这里需要解引用 Arc<Client> 并 clone，因为 S3OpsFS::new 期望 Client 结构体
      manager.register(
        format!("s3.{}", profile),
        Arc::new(S3OpsFS::new(client.as_ref().clone(), bucket_name)),
      );
    }
    // Agent 类型由 search_agent_source 处理，不应到达这里
    // 其他未知类型也不应出现（ORL 解析时已验证）
    _ => unreachable!(
      "create_entry_stream 仅处理 Local/S3 类型，Agent 应由 search_agent_source 处理"
    ),
  }

  // recursive 默认为 true
  manager
    .get_entry_stream(orl, true)
    .await
    .map_err(|e| format!("获取流失败: {}", e))
}

/// S3 单文件流（临时定义，建议移至 opsbox_core）


/// 构建本地来源条目流（单文件/目录/归档，支持 target 提示）
///
/// 根据 target 类型优先使用明确处理，否则自动检测路径类型


/// 通用条目流处理函数（支持基于回调的结果处理）
///
/// 提供统一的条目流处理方式，可被 Server 和 Agent 复用，避免重复实现核心处理逻辑。
/// 事件通过回调函数返回，调用方可灵活处理（发送到 channel、生成消息等）。
pub async fn process_entry_stream_with_callback<F>(
  stream: Box<dyn EntryStream>,
  processor: Arc<crate::service::search::SearchProcessor>,
  extra_path_filter: Option<crate::query::PathFilter>,
  cancel_token: Option<tokio_util::sync::CancellationToken>,
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
  if let Some(token) = cancel_token {
    stream_processor = stream_processor.with_cancel_token(token);
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
    use super::*;

    #[test]
    fn test_entry_concurrency_default() {
        let conc = entry_concurrency();
        assert!(conc >= 1);
        assert!(conc <= 128);
    }

    #[tokio::test]
    async fn test_preload_entry_small() {
        let content = b"hello world";
        let mut reader = &content[..];
        // max size larger than content
        let res = preload_entry(&mut reader, 100).await.expect("preload failed");
        match res {
            PreloadResult::Complete(c) => assert_eq!(c, content),
            PreloadResult::Partial(_) => panic!("should be complete"),
        }
    }

    #[tokio::test]
    async fn test_preload_entry_large() {
        // Create content slightly larger than our max check, but smaller than the chunk size (64KB)
        let content = vec![0u8; 100];
        let mut reader = &content[..];
        // max size smaller than content
        let res = preload_entry(&mut reader, 50).await.expect("preload failed");
        match res {
            PreloadResult::Partial(c) => {
                 // It reads in chunks of 64KB. So the first read will read all 100 bytes.
                 // Then buffer.len() is 100. 100 > 50. Returns Partial(100 bytes).
                 assert_eq!(c.len(), 100);
            }
            PreloadResult::Complete(_) => panic!("should be partial"),
        }
    }
}
