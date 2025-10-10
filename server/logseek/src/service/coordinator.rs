// ============================================================================
// 搜索协调器 - 统一管理多个存储源的搜索
// ============================================================================

use crate::query::Query;
use crate::service::search::{Search, SearchError, SearchProcessor, SearchResult};
use crate::storage::{DataSource, SearchOptions, SearchService, StorageSource};
use futures::StreamExt;
use log::{debug, error, info, warn};
use std::sync::Arc;
use tokio::sync::{Semaphore, mpsc};
use tokio::task::JoinSet;

/// 搜索协调器
///
/// 负责协调多个存储源的搜索，智能处理两种模式：
/// - DataSource: Server 端执行搜索
/// - SearchService: 远程执行搜索
pub struct SearchCoordinator {
  sources: Vec<StorageSource>,
}

impl SearchCoordinator {
  /// 创建新的搜索协调器
  pub fn new() -> Self {
    Self { sources: Vec::new() }
  }

  /// 添加数据源（Server 端搜索）
  pub fn add_data_source(&mut self, source: Arc<dyn DataSource>) {
    self.sources.push(StorageSource::Data(source));
  }

  /// 添加搜索服务（远程搜索）
  pub fn add_search_service(&mut self, service: Arc<dyn SearchService>) {
    self.sources.push(StorageSource::Service(service));
  }

  /// 添加存储源（自动识别类型）
  pub fn add_source(&mut self, source: StorageSource) {
    self.sources.push(source);
  }

  /// 执行分布式搜索
  ///
  /// 同时搜索所有存储源，聚合结果
  pub async fn search(&self, query: &str, context_lines: usize) -> Result<mpsc::Receiver<SearchResult>, SearchError> {
    let (tx, rx) = mpsc::channel(256);

    if self.sources.is_empty() {
      warn!("没有可用的存储源");
      return Ok(rx);
    }

    // 解析查询（用于 DataSource）
    let spec = Query::parse_github_like(query)
      .map_err(|e| SearchError::Io(std::io::Error::other(format!("查询解析失败: {}", e))))?;
    let spec = Arc::new(spec);
    let processor = Arc::new(SearchProcessor::new(spec.clone(), context_lines));

    info!("开始分布式搜索: query={}, 存储源数量={}", query, self.sources.len());

    // 为每个存储源启动搜索任务
    for (idx, source) in self.sources.iter().enumerate() {
      match source {
        StorageSource::Data(data_source) => {
          // 模式 1: Server 端搜索数据源
          let data_source = data_source.clone();
          let processor = processor.clone();
          let tx = tx.clone();
          let source_name = data_source.source_type();

          tokio::spawn(async move {
            info!("开始搜索数据源 #{}: {}", idx, source_name);

            match Self::search_data_source(data_source, processor, tx).await {
              Ok(stats) => {
                info!(
                  "数据源 {} 搜索完成: 处理={}, 匹配={}",
                  source_name, stats.processed, stats.matched
                );
              }
              Err(e) => {
                error!("数据源 {} 搜索失败: {}", source_name, e);
              }
            }
          });
        }

        StorageSource::Service(search_service) => {
          // 模式 2: 远程搜索服务
          let search_service = search_service.clone();
          let query = query.to_string();
          let tx = tx.clone();
          let service_name = search_service.service_type();

          tokio::spawn(async move {
            info!("开始调用搜索服务 #{}: {}", idx, service_name);

            match Self::search_service(search_service, &query, context_lines, tx).await {
              Ok(count) => {
                info!("搜索服务 {} 完成: 返回 {} 个结果", service_name, count);
              }
              Err(e) => {
                error!("搜索服务 {} 失败: {}", service_name, e);
              }
            }
          });
        }
      }
    }

    Ok(rx)
  }

  /// 搜索数据源（Server 端执行搜索）
  async fn search_data_source(
    data_source: Arc<dyn DataSource>,
    processor: Arc<SearchProcessor>,
    tx: mpsc::Sender<SearchResult>,
  ) -> Result<SearchStats, SearchError> {
    let mut stats = SearchStats::new();

    // 1. 列举文件
    let mut files = data_source
      .list_files()
      .await
      .map_err(|e| SearchError::Io(std::io::Error::other(format!("文件列举失败: {}", e))))?;

    // 2. 并发处理文件
    let semaphore = Arc::new(Semaphore::new(16));
    let mut tasks = JoinSet::new();

    while let Some(entry_result) = files.next().await {
      let Ok(entry) = entry_result else {
        warn!("文件条目读取失败");
        continue;
      };

      // 路径过滤
      if !processor.should_process_path(&entry.path) {
        debug!("路径不符合过滤条件，跳过: {}", entry.path);
        continue;
      }

      stats.processed += 1;

      let data_source = data_source.clone();
      let processor = processor.clone();
      let tx = tx.clone();
      let permit = match semaphore.clone().acquire_owned().await {
        Ok(p) => p,
        Err(_) => break,
      };

      tasks.spawn(async move {
        let _permit = permit;

        // 打开文件
        let reader = match data_source.open_file(&entry).await {
          Ok(r) => r,
          Err(e) => {
            warn!("无法打开文件 {}: {}", entry.path, e);
            return 0;
          }
        };

        // 根据文件类型选择处理方式
        let is_targz = entry.path.ends_with(".tar.gz") || entry.path.ends_with(".tgz");

        if is_targz {
          // 对 tar.gz 文件，复用现有的 Search trait 实现
          // 该实现会自动解压 gzip 并解析 tar 归档
          let spec = processor.spec.as_ref().clone();
          let ctx = processor.context_lines;
          match reader.search(&spec, ctx).await {
            Ok(mut result_rx) => {
              let mut count = 0;
              while let Some(result) = result_rx.recv().await {
                if tx.send(result).await.is_err() {
                  break;
                }
                count += 1;
              }
              count
            }
            Err(e) => {
              warn!("搜索 tar.gz 文件 {} 失败: {}", entry.path, e);
              0
            }
          }
        } else {
          // 对普通文本文件，直接使用 processor 处理
          let mut reader = reader;
          match processor.process_content(entry.path.clone(), &mut reader).await {
            Ok(Some(result)) => {
              if processor.send_result(result, &tx).await.is_ok() {
                1 // 成功匹配
              } else {
                0
              }
            }
            Ok(None) => 0,
            Err(e) => {
              warn!("搜索文件 {} 失败: {}", entry.path, e);
              0
            }
          }
        }
      });
    }

    // 等待所有任务完成并统计匹配数
    while let Some(result) = tasks.join_next().await {
      if let Ok(matched_count) = result {
        stats.matched += matched_count;
      }
    }

    Ok(stats)
  }

  /// 调用搜索服务（远程执行搜索）
  async fn search_service(
    search_service: Arc<dyn SearchService>,
    query: &str,
    context_lines: usize,
    tx: mpsc::Sender<SearchResult>,
  ) -> Result<usize, SearchError> {
    let options = SearchOptions {
      path_filter: None,
      scope: crate::storage::SearchScope::All,
      timeout_secs: Some(300),
      max_results: None,
    };

    // 调用远程搜索服务
    let mut result_stream = search_service
      .search(query, context_lines, options)
      .await
      .map_err(|e| SearchError::Io(std::io::Error::other(format!("搜索服务调用失败: {}", e))))?;

    // 转发结果
    let mut count = 0;
    while let Some(result) = result_stream.next().await {
      match result {
        Ok(search_result) => {
          count += 1;
          if tx.send(search_result).await.is_err() {
            debug!("接收端已关闭");
            break;
          }
        }
        Err(e) => {
          warn!("搜索服务返回错误: {}", e);
        }
      }
    }

    Ok(count)
  }
}

impl Default for SearchCoordinator {
  fn default() -> Self {
    Self::new()
  }
}

/// 搜索统计
#[derive(Debug, Clone, Default)]
struct SearchStats {
  processed: usize,
  matched: usize,
}

impl SearchStats {
  fn new() -> Self {
    Self {
      processed: 0,
      matched: 0,
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_coordinator_creation() {
    let coordinator = SearchCoordinator::new();
    assert_eq!(coordinator.sources.len(), 0);
  }

  #[test]
  fn test_coordinator_add_source() {
    let mut coordinator = SearchCoordinator::new();

    let local_fs = Arc::new(crate::storage::local::LocalFileSystem::new(std::path::PathBuf::from(
      "/tmp",
    )));

    coordinator.add_data_source(local_fs);
    assert_eq!(coordinator.sources.len(), 1);
  }
}
