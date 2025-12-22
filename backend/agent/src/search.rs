//! 搜索执行逻辑
//!
//! 处理搜索请求的执行流程

use crate::config::AgentConfig;
use crate::path::{get_available_subdirs, resolve_target_paths};
use logseek::{
  agent::AgentSearchRequest,
  domain::config::Target as ConfigTarget,
  query::Query,
  service::search::{SearchEvent, SearchProcessor},
};
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, warn};

/// 执行搜索
pub async fn execute_search(
  config: Arc<AgentConfig>,
  request: AgentSearchRequest,
  tx: mpsc::Sender<SearchEvent>,
  cancel_token: CancellationToken,
) {
  let task_id = request.task_id.clone();
  let started_at = std::time::Instant::now();

  // 辅助宏：发送事件并检查取消
  macro_rules! send_event {
    ($event:expr) => {
      if cancel_token.is_cancelled() {
        info!("搜索任务 {} 已被取消", task_id);
        return;
      }
      if tx.send($event).await.is_err() {
        info!("客户端已断开连接，停止搜索任务 {}", task_id);
        return;
      }
    };
  }

  // 1. 解析查询（第三层过滤：query 中的 path: 指令）
  let spec = match Query::parse_github_like(&request.query) {
    Ok(s) => Arc::new(s),
    Err(e) => {
      error!("查询解析失败: {}", e);
      send_event!(SearchEvent::Error {
        source: "agent-query-parse".to_string(),
        message: format!("查询解析失败: {}", e),
        recoverable: false,
      });
      return;
    }
  };

  // 2. 创建搜索处理器
  let processor = Arc::new(SearchProcessor::new(spec.clone(), request.context_lines));

  // 3. 第一层过滤：解析 Target 到实际路径
  let base_paths = match resolve_target_paths(&config, &request.target) {
    Ok(paths) => {
      info!("Target 解析成功: {:?}", paths);
      paths
    }
    Err(e) => {
      error!("Target 解析失败: {}", e);
      let available_dirs = get_available_subdirs(&config);
      let error_msg = if available_dirs.is_empty() {
        format!("Target 解析失败: {}。未找到可用的搜索目录。", e)
      } else {
        format!("Target 解析失败: {}。可用的子目录: {:?}", e, available_dirs)
      };
      send_event!(SearchEvent::Error {
        source: "agent-target".to_string(),
        message: error_msg,
        recoverable: false,
      });
      return;
    }
  };

  // 4. 额外路径过滤：将 path_filter 转为仅含 path: 的 Query，提取 PathFilter 作为"硬性 AND 限定"
  let extra_path_filter: Option<logseek::query::PathFilter> = if let Some(filter) = &request.path_filter {
    match logseek::query::path_glob_to_filter(filter) {
      Ok(f) => Some(f),
      Err(e) => {
        error!("路径过滤器解析失败: {}", e);
        send_event!(SearchEvent::Error {
          source: "agent-path-filter".to_string(),
          message: format!("路径过滤器解析失败: {}", e),
          recoverable: true,
        });
        return;
      }
    }
  } else {
    None
  };

  let filtered_paths = base_paths; // 与 LogSeek 对齐：仅以目录为起点，后置过滤

  if filtered_paths.is_empty() {
    warn!("没有找到匹配的搜索路径");
    send_event!(SearchEvent::Error {
      source: "agent-path".to_string(),
      message: "没有找到匹配的搜索路径".to_string(),
      recoverable: true,
    });
    return;
  }

  // 5. 执行搜索
  let mut all_processed = 0;
  let mut all_matched = 0;

  for search_path in filtered_paths {
    // 检查是否被取消
    if cancel_token.is_cancelled() {
      info!("搜索任务 {} 已被取消", task_id);
      return;
    }

    debug!("开始搜索路径: {}", search_path.display());

    // 统一由 logseek 提供的构造器创建本地来源条目流
    let path_str = search_path.to_string_lossy().to_string();
    // 根据 Target 类型传递完整信息，与 Server 端对齐
    let target_hint = match &request.target {
      ConfigTarget::Files { .. } => {
        // Files 类型：传递单个文件路径（已解析为绝对路径）
        // 注意：每个 search_path 已经是单个文件，所以传递单个路径
        Some(ConfigTarget::Files {
          paths: vec![path_str.clone()],
        })
      }
      ConfigTarget::Dir { recursive, .. } => {
        // Dir 类型：传递 recursive 标志，path 使用 "." 表示当前路径
        Some(ConfigTarget::Dir {
          path: ".".to_string(),
          recursive: *recursive,
        })
      }
      ConfigTarget::Archive { path } => {
        // Archive 类型：传递相对路径
        Some(ConfigTarget::Archive { path: path.clone() })
      }
    };
    let estream = match logseek::service::entry_stream::build_local_entry_stream(&path_str, target_hint).await {
      Ok(s) => s,
      Err(e) => {
        warn!("构建本地条目流失败 {}: {}", search_path.display(), e);
        continue;
      }
    };

    if let Err(e) = search_with_entry_stream(
      estream,
      processor.clone(),
      &task_id,
      &tx,
      &mut all_processed,
      &mut all_matched,
      extra_path_filter.clone(),
      &cancel_token,
    )
    .await
    {
      warn!("处理条目流失败 {}: {}", search_path.display(), e);
    }
  }

  // 发送完成事件
  let elapsed_ms = started_at.elapsed().as_millis() as u64;
  send_event!(SearchEvent::Complete {
    source: "agent:complete".to_string(),
    elapsed_ms,
  });

  info!(
    "搜索完成: task_id={}, processed={}, matched={}",
    task_id, all_processed, all_matched
  );
}

/// 通用条目流搜索辅助函数
/// 使用通用处理函数并自动处理消息发送
#[allow(clippy::too_many_arguments)]
/// 通用条目流搜索辅助函数
/// 使用通用处理函数并自动处理消息发送
#[allow(clippy::too_many_arguments)]
async fn search_with_entry_stream(
  mut stream: Box<dyn opsbox_core::fs::EntryStream>,
  processor: Arc<SearchProcessor>,
  task_id: &str,
  tx: &mpsc::Sender<SearchEvent>,
  all_processed: &mut usize,
  all_matched: &mut usize,
  extra_path_filter: Option<logseek::query::PathFilter>,
  cancel_token: &CancellationToken,
) -> Result<(), String> {
  loop {
    if cancel_token.is_cancelled() {
      info!("搜索任务 {} 已被取消", task_id);
      break;
    }

    match stream.next_entry().await {
      Ok(Some((meta, mut reader))) => {
        // 全局 Path 过滤（硬性 AND + Query Spec 过滤）
        // combined check: extra_path_filter AND spec.path_filter
        if !processor.should_process_path_with(&meta.path, extra_path_filter.as_ref()) {
          continue;
        }

        // 处理条目内容
        match processor.process_content(meta.path.clone(), &mut reader).await {
          Ok(Some(mut result)) => {
            // 设置 archive_path (如果来自归档)
            result.archive_path = meta.container_path;

            *all_processed += 1;
            *all_matched += 1;

            // Send with backpressure
            if tx.send(SearchEvent::Success(result)).await.is_err() {
              debug!("接收端已关闭，停止处理任务 {}", task_id);
              break;
            }
          }
          Ok(None) => {
            *all_processed += 1;
            // No match, do nothing
          }
          Err(e) => {
            *all_processed += 1;
            warn!("处理内容失败 {}: {}", meta.path, e);
            let err_event = SearchEvent::Error {
              source: "agent-process".to_string(),
              message: format!("处理失败: {}", e),
              recoverable: true,
            };
            if tx.send(err_event).await.is_err() {
              break;
            }
          }
        }
      }
      Ok(None) => break, // Stream 结束
      Err(e) => {
        warn!("读取条目流失败: {}", e);
        let err_event = SearchEvent::Error {
          source: "agent-stream".to_string(),
          message: format!("读取流失败: {}", e),
          recoverable: true,
        };
        if tx.send(err_event).await.is_err() {
          break;
        }
      }
    }
  }

  Ok(())
}
