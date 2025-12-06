//! 文件查看路由
//!
//! 处理 /view.cache.json 端点，从缓存中读取文件内容

use crate::api::{LogSeekApiError, models::ViewParams};
use crate::domain::FileUrl;
use crate::repository::{RepositoryError, cache::cache as simple_cache};
use crate::service::ServiceError;
use axum::{
  body::Body,
  extract::Query,
  http::{HeaderValue, Response as HttpResponse, header::CONTENT_TYPE},
};
use serde::Deserialize;
use tracing::debug;

/// 查看缓存中的文件内容
pub async fn view_cache_json(Query(params): Query<ViewParams>) -> Result<HttpResponse<Body>, LogSeekApiError> {
  tracing::debug!(
    "view-request: sid={} file={} start={:?} end={:?}",
    params.sid,
    params.file,
    params.start,
    params.end
  );

  // 解析 FileUrl
  let file_url: FileUrl = match params.file.parse() {
    Ok(url) => url,
    Err(e) => {
      tracing::warn!(
        "view-parse-error: sid={} file={} error={:?}",
        params.sid,
        params.file,
        e
      );
      return Err(LogSeekApiError::Domain(e));
    }
  };

  // 读取 keywords 与行切片
  let keywords = simple_cache().get_keywords(&params.sid).await.unwrap_or_default();
  debug!("🔍 Server查找缓存: sid={}, file_url={}", params.sid, file_url);
  let (total, slice) = match simple_cache()
    .get_lines_slice(
      &params.sid,
      &file_url,
      params.start.unwrap_or(1),
      params.end.unwrap_or(1000),
    )
    .await
  {
    Some(v) => {
      debug!(
        "✅ Server缓存命中: sid={}, file_url={}, total={}, slice_len={}",
        params.sid,
        file_url,
        v.0,
        v.1.len()
      );
      v
    }
    None => {
      debug!("❌ Server缓存未命中: sid={}, file_url={}", params.sid, file_url);
      return Err(LogSeekApiError::Repository(RepositoryError::NotFound(format!(
        "Cache not found or expired for sid={}, file={}",
        params.sid, file_url
      ))));
    }
  };
  let start = params.start.unwrap_or(1).max(1);
  let end = (start + slice.len().saturating_sub(1)).min(total.max(1));
  let mut out_lines: Vec<serde_json::Value> = Vec::with_capacity(slice.len());
  for (i, line) in slice.iter().enumerate() {
    out_lines.push(serde_json::json!({ "no": start + i, "text": line }));
  }
  tracing::debug!(
    "view-hit: sid={} file={} total={} slice={} range=[{}..{}]",
    params.sid,
    params.file,
    total,
    slice.len(),
    start,
    end
  );
  let obj = serde_json::json!({
    "file": params.file,
    "total": total,
    "start": start,
    "end": end,
    "keywords": keywords,
    "lines": out_lines,
  });
  let body = serde_json::to_vec(&obj).unwrap_or_else(|_| b"{}".to_vec());
  HttpResponse::builder()
    .status(200)
    .header(
      CONTENT_TYPE,
      HeaderValue::from_static("application/json; charset=utf-8"),
    )
    .body(Body::from(body))
    .map_err(|e| LogSeekApiError::Service(ServiceError::ProcessingError(format!("构建 HTTP 响应失败: {}", e))))
}

/// 获取会话的文件列表参数
#[derive(Debug, Deserialize)]
pub struct FileListParams {
  pub sid: String,
}

/// 获取会话的所有文件列表
pub async fn get_file_list_json(Query(params): Query<FileListParams>) -> Result<HttpResponse<Body>, LogSeekApiError> {
  tracing::debug!("file-list-request: sid={}", params.sid);

  // 从缓存中获取文件列表
  let file_urls = match simple_cache().get_file_list(&params.sid).await {
    Some(files) => files,
    None => {
      tracing::warn!("file-list-not-found: sid={}", params.sid);
      return Err(LogSeekApiError::Repository(RepositoryError::NotFound(format!(
        "Session not found or expired: sid={}",
        params.sid
      ))));
    }
  };

  // 转换为字符串列表
  let files: Vec<String> = file_urls.iter().map(|url| url.to_string()).collect();

  tracing::debug!("file-list-found: sid={} count={}", params.sid, files.len());

  let obj = serde_json::json!({
    "sid": params.sid,
    "files": files,
    "count": files.len(),
  });

  let body = serde_json::to_vec(&obj).unwrap_or_else(|_| b"{}".to_vec());
  HttpResponse::builder()
    .status(200)
    .header(
      CONTENT_TYPE,
      HeaderValue::from_static("application/json; charset=utf-8"),
    )
    .body(Body::from(body))
    .map_err(|e| LogSeekApiError::Service(ServiceError::ProcessingError(format!("构建 HTTP 响应失败: {}", e))))
}

/// 下载完整文件内容
pub async fn download_file(Query(params): Query<ViewParams>) -> Result<HttpResponse<Body>, LogSeekApiError> {
  tracing::debug!(
    "download-request: sid={} file={}",
    params.sid,
    params.file
  );

  // 解析 FileUrl
  let file_url: FileUrl = match params.file.parse() {
    Ok(url) => url,
    Err(e) => {
      tracing::warn!(
        "download-parse-error: sid={} file={} error={:?}",
        params.sid,
        params.file,
        e
      );
      return Err(LogSeekApiError::Domain(e));
    }
  };

  // 从缓存获取完整文件内容
  let (total, lines) = match simple_cache()
    .get_lines_slice(
      &params.sid,
      &file_url,
      1, // 从第1行开始
      usize::MAX, // 到最大行（内部会限制到total）
    )
    .await
  {
    Some(v) => {
      tracing::debug!(
        "✅ 下载缓存命中: sid={}, file_url={}, total={}, lines_len={}",
        params.sid,
        file_url,
        v.0,
        v.1.len()
      );
      v
    }
    None => {
      tracing::debug!("❌ 下载缓存未命中: sid={}, file_url={}", params.sid, file_url);
      return Err(LogSeekApiError::Repository(RepositoryError::NotFound(format!(
        "Cache not found or expired for sid={}, file={}",
        params.sid, file_url
      ))));
    }
  };

  // 验证行数是否匹配total
  if lines.len() != total {
    tracing::warn!(
      "download-line-count-mismatch: sid={} file={} total={} actual={}",
      params.sid,
      params.file,
      total,
      lines.len()
    );
  }

  // 将行拼接为文本，每行以换行符分隔
  let content = lines.join("\n");

  HttpResponse::builder()
    .status(200)
    .header(
      CONTENT_TYPE,
      HeaderValue::from_static("text/plain; charset=utf-8"),
    )
    .body(Body::from(content))
    .map_err(|e| LogSeekApiError::Service(ServiceError::ProcessingError(format!("构建下载响应失败: {}", e))))
}
