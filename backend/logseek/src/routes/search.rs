//! 搜索路由
//!
//! 处理 /search.ndjson 端点，实现多存储源并行搜索

use crate::api::{LogSeekApiError, models::SearchBody};
use crate::service::search::SearchEvent;
use crate::service::search_executor::{SearchExecutor, SearchExecutorConfig};
use crate::utils::renderer::render_json_chunks;
use axum::{
  body::Body,
  extract::{Json, State},
  http::{HeaderValue, Response as HttpResponse, header::CONTENT_TYPE},
};
use bytes::Bytes;
use futures::Stream;
use opsbox_core::SqlitePool;
use tokio::sync::mpsc;

use super::helpers::{s3_max_concurrency, stream_channel_capacity};

// ============================================================================
// 搜索（多存储源并行搜索）
// ============================================================================

/// 序列化事件为 NDJSON 字节
fn serialize_event(value: &serde_json::Value) -> Option<Bytes> {
  match serde_json::to_vec(value) {
    Ok(mut v) => {
      v.push(b'\n');
      Some(Bytes::from(v))
    }
    Err(e) => {
      tracing::warn!("[Search] 序列化失败: {}", e);
      None
    }
  }
}

/// 将 SearchEvent 流转换为 NDJSON 字节流
fn convert_to_ndjson_stream(
  mut rx: mpsc::Receiver<SearchEvent>,
  highlights: Vec<String>,
) -> impl Stream<Item = Result<Bytes, std::io::Error>> {
  async_stream::stream! {
    let mut event_count = 0;
    
    while let Some(event) = rx.recv().await {
      event_count += 1;
      tracing::debug!("[Search Route] 收到事件 #{}: {:?}", event_count, 
        match &event {
          SearchEvent::Success(res) => format!("Success(path={}, lines={})", res.path, res.lines.len()),
          SearchEvent::Error { source, message, .. } => format!("Error(source={}, msg={})", source, message),
          SearchEvent::Complete { source, elapsed_ms } => format!("Complete(source={}, elapsed={}ms)", source, elapsed_ms),
        }
      );
      
      let json_value = match event {
        SearchEvent::Success(res) => {
          let json_obj = render_json_chunks(
            &res.path,
            res.merged.clone(),
            res.lines.clone(),
            &highlights,
            res.encoding.clone(),
          );
          Some(serde_json::json!({"type": "result", "data": json_obj}))
        }
        SearchEvent::Error { source, message, recoverable } => {
          tracing::debug!("[Search Route] 错误事件: source={}, msg={}", source, message);
          serde_json::to_value(SearchEvent::Error { source, message, recoverable }).ok()
        }
        SearchEvent::Complete { source, elapsed_ms } => {
          tracing::debug!("[Search Route] 完成事件: source={}, elapsed={}ms", source, elapsed_ms);
          serde_json::to_value(SearchEvent::Complete { source, elapsed_ms }).ok()
        }
      };
      
      if let Some(value) = json_value
        && let Some(bytes) = serialize_event(&value) {
          yield Ok(bytes);
        } else {
          tracing::warn!("[Search Route] 序列化失败，跳过事件");
        }
    }
    
    tracing::debug!("[Search Route] SearchEvent 流结束，共处理 {} 个事件", event_count);
  }
}

/// 构建 NDJSON HTTP 响应（包含 X-Logseek-SID 头）
fn build_ndjson_response(
  stream: impl Stream<Item = Result<Bytes, std::io::Error>> + Send + 'static,
  sid: String,
) -> Result<HttpResponse<Body>, LogSeekApiError> {
  let sid_header = HeaderValue::from_str(&sid).unwrap_or_else(|_| HeaderValue::from_static(""));
  HttpResponse::builder()
    .status(200)
    .header(CONTENT_TYPE, HeaderValue::from_static("application/x-ndjson; charset=utf-8"))
    .header("X-Logseek-SID", sid_header)
    .body(Body::from_stream(stream))
    .map_err(|e| LogSeekApiError::Service(
      crate::service::ServiceError::ProcessingError(format!("构建 HTTP 响应失败: {}", e))
    ))
}

/// 搜索处理函数（多存储源并行搜索）
pub async fn stream_search(
  State(pool): State<SqlitePool>,
  Json(body): Json<SearchBody>,
) -> Result<HttpResponse<Body>, LogSeekApiError> {
  tracing::info!("[Search] 开始搜索: q={}", body.q);

  let ctx = body.context.unwrap_or(3);
  let config = SearchExecutorConfig {
    io_max_concurrency: s3_max_concurrency(),
    stream_channel_capacity: stream_channel_capacity(),
  };
  
  let executor = SearchExecutor::new(pool, config);
  let (result_rx, sid) = executor.search(&body.q, ctx).await?;
  
  let highlights = crate::query::Query::parse_github_like(&body.q)
    .map(|spec| spec.highlights)
    .unwrap_or_default();
  
  let stream = convert_to_ndjson_stream(result_rx, highlights);
  build_ndjson_response(stream, sid)
}
