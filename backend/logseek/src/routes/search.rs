//! 搜索路由
//!
//! 处理 /search.ndjson 端点，实现多存储源并行搜索

use crate::api::{LogSeekApiError, models::SearchBody};
use crate::service::search::SearchEvent;
use crate::service::search_executor::{SearchExecutor, SearchExecutorConfig};
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

#[derive(serde::Serialize)]
#[serde(tag = "type", rename_all = "lowercase")]
enum SearchResponse<'a> {
  Result {
    data: crate::utils::renderer::SearchJsonResult<'a>,
  },
  Error {
    source: String,
    message: String,
    recoverable: bool,
  },
  Complete {
    source: String,
    elapsed_ms: u64,
  },
}

/// 构建 NDJSON HTTP 响应（包含 X-Logseek-SID 头）
/// 将 SearchEvent 流转换为 NDJSON 字节流
fn convert_to_ndjson_stream(
  mut rx: mpsc::Receiver<SearchEvent>,
  highlights: Vec<crate::query::KeywordHighlight>,
) -> impl Stream<Item = Result<Bytes, std::io::Error>> {
  async_stream::stream! {
    let mut event_count = 0;

    while let Some(event) = rx.recv().await {
      event_count += 1;
      tracing::trace!("[Search Route] 收到事件 #{}: {:?}", event_count,
        match &event {
          SearchEvent::Success(res) => format!("Success(path={}, lines={})", res.path, res.lines.len()),
          SearchEvent::Error { source, message, .. } => format!("Error(source={}, msg={})", source, message),
          SearchEvent::Complete { source, elapsed_ms } => format!("Complete(source={}, elapsed={}ms)", source, elapsed_ms),
        }
      );

      let json_vec = match event {
        SearchEvent::Success(res) => {
          let json_obj = crate::utils::renderer::render_json_chunks(
            &res.path,
            res.merged.clone(),
            &res.lines, // Pass ref
            &highlights,
            &res.encoding, // Pass ref
          );
          serde_json::to_vec(&SearchResponse::Result { data: json_obj }).ok()
        }
        SearchEvent::Error { source, message, recoverable } => {
          tracing::debug!("[Search Route] 错误事件: source={}, msg={}", source, message);
          serde_json::to_vec(&SearchResponse::Error { source, message, recoverable }).ok()
        }
        SearchEvent::Complete { source, elapsed_ms } => {
          tracing::debug!("[Search Route] 完成事件: source={}, elapsed={}ms", source, elapsed_ms);
          serde_json::to_vec(&SearchResponse::Complete { source, elapsed_ms }).ok()
        }
      };

      if let Some(mut bytes) = json_vec {
          bytes.push(b'\n');
          yield Ok(Bytes::from(bytes));
        } else {
          tracing::warn!("[Search Route] 序列化失败，跳过事件");
        }
    }

    tracing::info!("[Search Route] SearchEvent 流结束，共处理 {} 个事件", event_count);
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
    .header(
      CONTENT_TYPE,
      HeaderValue::from_static("application/x-ndjson; charset=utf-8"),
    )
    .header("X-Logseek-SID", sid_header)
    .body(Body::from_stream(stream))
    .map_err(|e| {
      LogSeekApiError::Service(crate::service::ServiceError::ProcessingError(format!(
        "构建 HTTP 响应失败: {}",
        e
      )))
    })
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
  let cancel_token = tokio_util::sync::CancellationToken::new();
  let token_for_drop = cancel_token.clone();

  let (result_rx, sid) = executor.search(&body.q, ctx, Some(cancel_token)).await?;

  let query = crate::query::Query::parse_github_like(&body.q).unwrap_or_default();
  let highlights = query.highlights.clone();

  let inner_stream = convert_to_ndjson_stream(result_rx, highlights);
  let stream = async_stream::stream! {
    // 使用变量持有 Guard，确保 stream 被 drop 时调用 cancel
    let _guard = token_for_drop.drop_guard();
    for await item in inner_stream {
      yield item;
    }
  };

  build_ndjson_response(stream, sid)
}

/// 清理搜索会话缓存
pub async fn delete_search_session(
  axum::extract::Path(sid): axum::extract::Path<String>,
) -> Result<HttpResponse<Body>, LogSeekApiError> {
  tracing::info!("[Search] 清理会话缓存: sid={}", sid);
  crate::repository::cache::cache().remove_sid(&sid).await;

  HttpResponse::builder().status(200).body(Body::empty()).map_err(|e| {
    LogSeekApiError::Service(crate::service::ServiceError::ProcessingError(format!(
      "构建响应失败: {}",
      e
    )))
  })
}
