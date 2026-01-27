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

  // 2. 创建搜索处理器（支持用户指定的编码）
  let processor = Arc::new(SearchProcessor::new_with_encoding(
    spec.clone(),
    request.context_lines,
    request.encoding.clone(),
  ));

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

  // 4. 额外路径过滤
  let mut extra_filters = Vec::new();

  // 4.1 Base Filter (来自 ORL)
  if let Some(base) = &request.path_filter
      && let Ok(f) = logseek::query::path_glob_to_filter(base) {
         extra_filters.push(f);
      }

  // 4.2 User Filter (path_includes / path_excludes)
  if let Some(user_filter) = combine_filters(&request.path_includes, &request.path_excludes) {
      extra_filters.push(user_filter);
  }

  let filtered_paths = base_paths;

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
  for search_path in filtered_paths {
    if cancel_token.is_cancelled() {
      info!("搜索任务 {} 已被取消", task_id);
      return;
    }

    debug!("开始搜索路径: {}", search_path.display());

    let path_str = search_path.to_string_lossy().to_string();
    let target_hint = match &request.target {
      ConfigTarget::Files { .. } => {
        Some(ConfigTarget::Files {
          paths: vec![path_str.clone()],
        })
      }
      ConfigTarget::Dir { recursive, .. } => {
        Some(ConfigTarget::Dir {
          path: ".".to_string(),
          recursive: *recursive,
        })
      }
      ConfigTarget::Archive { path, .. } => {
        Some(ConfigTarget::Archive {
          path: path.clone(),
          entry: None,
        })
      }
    };

    // Create estream
    let mut estream: Box<dyn opsbox_core::fs::EntryStream> = match target_hint {
        Some(ConfigTarget::Dir { path, recursive }) => {
             let full_path = if path == "." {
                 search_path.clone()
             } else {
                 search_path.join(path)
             };
             match opsbox_core::fs::FsEntryStream::new(full_path, recursive).await {
                 Ok(s) => Box::new(s),
                 Err(e) => {
                     warn!("构建本地条目流失败 {}: {}", search_path.display(), e);
                     continue;
                 }
             }
        },
        Some(ConfigTarget::Files { paths }) => {
             Box::new(opsbox_core::fs::MultiFileEntryStream::new(paths))
        },
        Some(ConfigTarget::Archive { path, .. }) => {
             match tokio::fs::File::open(&path).await {
                 Ok(f) => {
                     match opsbox_core::fs::create_archive_stream_from_reader(f, Some(&path)).await {
                         Ok(s) => s,
                         Err(e) => {
                             warn!("打开归档流失败 {}: {}", path, e);
                             continue;
                         }
                     }
                 },
                 Err(e) => {
                     warn!("打开归档文件失败 {}: {}", path, e);
                     continue;
                 }
             }
        },
        None => {
             match opsbox_core::fs::FsEntryStream::new(std::path::PathBuf::from(&path_str), true).await {
                 Ok(s) => Box::new(s),
                 Err(e) => {
                     warn!("构建本地条目流失败 {}: {}", path_str, e);
                     continue;
                 }
             }
        }
    };

    let mut stream_processor = logseek::service::entry_stream::EntryStreamProcessor::new(processor.clone())
      .with_cancel_token(cancel_token.clone());

    if matches!(&request.target, ConfigTarget::Dir { .. }) {
      if search_path.is_file() {
          if let Some(parent) = search_path.parent() {
              stream_processor = stream_processor.with_base_path(parent.to_path_buf());
          } else {
              stream_processor = stream_processor.with_base_path(search_path.clone());
          }
      } else {
          stream_processor = stream_processor.with_base_path(search_path.clone());
      }
    } else if let Some(parent) = search_path.parent() {
      stream_processor = stream_processor.with_base_path(parent.to_path_buf());
    }

    for filter in &extra_filters {
      stream_processor = stream_processor.with_extra_path_filter(filter.clone());
    }

    if let Err(e) = stream_processor.process_stream(&mut *estream, tx.clone()).await {
      warn!("处理条目流失败 {}: {}", search_path.display(), e);
    }
  }

  let elapsed_ms = started_at.elapsed().as_millis() as u64;
  send_event!(SearchEvent::Complete {
    source: "agent:complete".to_string(),
    elapsed_ms,
  });

  info!("搜索完成: task_id={}", task_id);
}

fn combine_filters(
    includes: &[String],
    excludes: &[String],
) -> Option<logseek::query::PathFilter> {
    let mut final_filter = logseek::query::PathFilter::default();
    let mut has_filter = false;

    if !includes.is_empty() {
        let mut builder = globset::GlobSetBuilder::new();
        for p in includes {
             match globset::GlobBuilder::new(p).literal_separator(true).build() {
                Ok(g) => { builder.add(g); },
                Err(e) => warn!("无效的 path glob: {} ({})", p, e),
             }
        }
        if let Ok(set) = builder.build() {
            final_filter.include = Some(set);
            has_filter = true;
        }
    }

    if !excludes.is_empty() {
        let mut builder = globset::GlobSetBuilder::new();
        for p in excludes {
             match globset::GlobBuilder::new(p).literal_separator(true).build() {
                Ok(g) => { builder.add(g); },
                Err(e) => warn!("无效的 -path glob: {} ({})", p, e),
             }
        }
        if let Ok(set) = builder.build() {
            final_filter.exclude = Some(set);
            has_filter = true;
        }
    }

    if has_filter {
        Some(final_filter)
    } else {
        None
    }
}
