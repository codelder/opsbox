//! 文件查看路由
//!
//! 处理 /view.cache.json 端点，从缓存中读取文件内容

use crate::api::models::ViewParams;
use crate::domain::FileUrl;
use crate::repository::cache::cache as simple_cache;
use axum::{
  body::Body,
  extract::Query,
  http::{HeaderValue, Response as HttpResponse, StatusCode, header::CONTENT_TYPE},
};
use problemdetails::Problem;

/// 查看缓存中的文件内容
pub async fn view_cache_json(Query(params): Query<ViewParams>) -> Result<HttpResponse<Body>, Problem> {
  log::debug!(
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
      log::warn!(
        "view-parse-error: sid={} file={} error={:?}",
        params.sid,
        params.file,
        e
      );
      return Ok(
        HttpResponse::builder()
          .status(StatusCode::BAD_REQUEST)
          .body(Body::from(format!("Invalid file URL: {}", e)))
          .unwrap(),
      );
    }
  };

  // 读取 keywords 与行切片
  let keywords = simple_cache().get_keywords(&params.sid).await.unwrap_or_default();
  let (total, slice) = match simple_cache()
    .get_lines_slice(
      &params.sid,
      &file_url,
      params.start.unwrap_or(1),
      params.end.unwrap_or(1000),
    )
    .await
  {
    Some(v) => v,
    None => {
      log::debug!("view-miss: sid={} file={}", params.sid, params.file);
      return Ok(
        HttpResponse::builder()
          .status(404)
          .header(
            CONTENT_TYPE,
            HeaderValue::from_static("application/json; charset=utf-8"),
          )
          .body(Body::from("{\"error\":\"not_found_or_expired\"}"))
          .unwrap(),
      );
    }
  };
  let start = params.start.unwrap_or(1).max(1);
  let end = (start + slice.len().saturating_sub(1)).min(total.max(1));
  let mut out_lines: Vec<serde_json::Value> = Vec::with_capacity(slice.len());
  for (i, line) in slice.iter().enumerate() {
    out_lines.push(serde_json::json!({ "no": start + i, "text": line }));
  }
  log::debug!(
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
  Ok(
    HttpResponse::builder()
      .status(200)
      .header(
        CONTENT_TYPE,
        HeaderValue::from_static("application/json; charset=utf-8"),
      )
      .body(Body::from(body))
      .unwrap(),
  )
}
