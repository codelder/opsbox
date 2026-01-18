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
use crate::repository::cache::{cache as simple_cache, new_sid};
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

  let sid = new_sid();

  // 解析查询以获取 highlights（用于前端高亮显示）
  let highlights = crate::query::Query::parse_github_like(&body.q)
    .map(|spec| spec.highlights.clone())
    .unwrap_or_default();

  // 执行搜索
  let result_rx = executor.search(&body.q, sid.clone(), ctx, Some(cancel_token)).await?;

  // 缓存搜索关键词，用于后续文件查看高亮
  simple_cache().put_keywords(&sid, highlights.clone()).await;

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

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::mpsc;
    use futures::StreamExt;
    use axum::http::StatusCode;

    #[tokio::test]
    async fn test_convert_to_ndjson_stream() {
        let (tx, rx) = mpsc::channel(10);
        let highlights = vec![];

        let mut stream = Box::pin(convert_to_ndjson_stream(rx, highlights));

        // 模拟完成事件
        tx.send(SearchEvent::Complete { source: "test-source".into(), elapsed_ms: 123 }).await.unwrap();

        if let Some(res) = stream.next().await {
            let item = res.unwrap();
            let json: serde_json::Value = serde_json::from_slice(&item).unwrap();
            assert_eq!(json["type"], "complete");
            assert_eq!(json["source"], "test-source");
            assert_eq!(json["elapsed_ms"], 123);
        } else {
            panic!("Expected item from stream");
        }

        // 模拟错误事件
        tx.send(SearchEvent::Error {
            source: "err-source".into(),
            message: "error message".into(),
            recoverable: true
        }).await.unwrap();

        if let Some(res) = stream.next().await {
            let item = res.unwrap();
            let json: serde_json::Value = serde_json::from_slice(&item).unwrap();
            assert_eq!(json["type"], "error");
            assert_eq!(json["message"], "error message");
        }
    }

    #[tokio::test]
    async fn test_delete_search_session() {
        let sid = "test-sid-delete".to_string();
        let resp = delete_search_session(axum::extract::Path(sid)).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_convert_to_ndjson_stream_success() {
        let (tx, rx) = mpsc::channel(10);
        let highlights = vec![];

        let mut stream = Box::pin(convert_to_ndjson_stream(rx, highlights));

        // 模拟成功事件
        // 注意：SearchResult 的 new 是私有的或在当前 crate 可见，
        // 这里我们直接构造结构体（如果字段是 pub 的话）或者检查可见性。
        // 根据之前的查看，SearchResult 字段是 pub 的。
        let result = crate::service::search::SearchResult {
            path: "test.log".into(),
            lines: vec!["match line".into()],
            merged: vec![(0, 0)],
            encoding: Some("UTF-8".into()),
            archive_path: None,
            source_type: Default::default(),
        };

        tx.send(SearchEvent::Success(result)).await.unwrap();

        if let Some(res) = stream.next().await {
            let item = res.unwrap();
            let json: serde_json::Value = serde_json::from_slice(&item).unwrap();
            assert_eq!(json["type"], "result");
            assert_eq!(json["data"]["path"], "test.log");
            assert_eq!(json["data"]["chunks"].as_array().unwrap().len(), 1);
        } else {
            panic!("Expected item from stream");
        }
    }

    #[test]
    fn test_search_response_serialization() {
        let res = SearchResponse::Complete {
            source: "test".into(),
            elapsed_ms: 100
        };
        let json = serde_json::to_string(&res).unwrap();
        assert!(json.contains("\"type\":\"complete\""));
        assert!(json.contains("\"source\":\"test\""));
    }
}
