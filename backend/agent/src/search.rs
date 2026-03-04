//! 搜索执行逻辑
//!
//! 处理搜索请求的执行流程

use crate::config::AgentConfig;
use crate::path::{get_available_subdirs, resolve_target_paths};
use logseek::{
  agent::AgentSearchRequest,
  domain::config::Target as ConfigTarget,
  query::Query,
  service::search::SearchEvent,
  service::search_runner::{self, SearchRunnerConfig},
};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use tracing::{error, info, warn};

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

  // 1. 解析查询（提前解析，避免重复解析，同时确保错误能正确上报）
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

  // 2. 解析 Target 到实际路径（保留安全边界：白名单校验）
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

  if base_paths.is_empty() {
    warn!("没有找到匹配的搜索路径");
    send_event!(SearchEvent::Error {
      source: "agent-path".to_string(),
      message: "没有找到匹配的搜索路径".to_string(),
      recoverable: true,
    });
    return;
  }

  // 3. 构建路径过滤器
  let extra_filters = search_runner::build_path_filters(
    request.path_filter.as_deref(),
    &request.path_includes,
    &request.path_excludes,
  );

  // 4. 构建搜索配置（共享查询解析结果，避免重复解析）
  let runner_config = SearchRunnerConfig::new(&request.query)
    .with_context_lines(request.context_lines)
    .with_encoding_opt(request.encoding.clone())
    .with_extra_filters(extra_filters)
    .with_query_spec(spec); // 复用已解析的 Query

  // 5. 执行搜索
  for search_path in &base_paths {
    if cancel_token.is_cancelled() {
      info!("搜索任务 {} 已被取消", task_id);
      break;
    }

    if !search_path.exists() {
      warn!("搜索路径不存在: {}", search_path.display());
      continue;
    }

    info!("开始搜索路径: {}", search_path.display());

    let path_str = search_path.to_string_lossy().to_string();

    // 5.1 创建 EntryStream
    // 注意：resolve_target_paths 已经返回解析后的目标路径
    // - 对于 Dir 目标：search_path 已经是完整的目录路径，不需要再拼接 path
    // - 对于 Files 目标：base_paths 已经是解析后的文件列表
    // - 对于 Archive 目标：search_path 是归档文件的完整路径
    let mut estream: Box<dyn opsbox_core::fs::EntryStream> = match &request.target {
      ConfigTarget::Dir { recursive, .. } => {
        // search_path 已经是 resolve_target_paths 解析后的完整目录路径
        match opsbox_core::fs::FsEntryStream::new(search_path.clone(), *recursive).await {
          Ok(s) => Box::new(s),
          Err(e) => {
            warn!("构建本地条目流失败 {}: {}", search_path.display(), e);
            continue;
          }
        }
      }
      ConfigTarget::Files { .. } => {
        // 对于 Files 目标，base_paths 已经是解析后的文件列表
        // 每个 search_path 是一个文件，创建单文件流
        Box::new(opsbox_core::fs::MultiFileEntryStream::new(vec![path_str.clone()]))
      }
      ConfigTarget::Archive { .. } => match tokio::fs::File::open(search_path).await {
        Ok(f) => match opsbox_core::fs::create_archive_stream_from_reader(f, Some(&path_str)).await {
          Ok(s) => s,
          Err(e) => {
            warn!("打开归档流失败 {}: {}", search_path.display(), e);
            continue;
          }
        },
        Err(e) => {
          warn!("打开归档文件失败 {}: {}", search_path.display(), e);
          continue;
        }
      },
    };

    // 5.2 确定 base_path（仅对目录/文件目标，归档不需要）
    let base_path = match &request.target {
      ConfigTarget::Dir { .. } => {
        if search_path.is_file() {
          search_path.parent().map(PathBuf::from)
        } else {
          Some(search_path.clone())
        }
      }
      ConfigTarget::Files { .. } => search_path.parent().map(PathBuf::from),
      ConfigTarget::Archive { .. } => None,
    };

    // 5.3 执行搜索（复用配置）
    let path_runner_config = runner_config.clone().with_base_path_opt(base_path);

    if let Err(e) = search_runner::run_search(
      estream.as_mut(),
      path_runner_config,
      tx.clone(),
      Some(Arc::new(cancel_token.clone())),
      "Agent",
    )
    .await
    {
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
