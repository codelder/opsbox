//! 文件查看路由
//!
//! 处理 /view.cache.json 端点，从缓存中读取文件内容

use crate::api::{LogSeekApiError, models::ViewParams};
use crate::repository::{RepositoryError, cache::cache as simple_cache};
use crate::service::ServiceError;
use crate::service::encoding::read_text_file;
use crate::service::entry_stream::create_search_entry_stream_from_resource;
use axum::{
  body::Body,
  extract::{Query, State},
  http::{HeaderValue, Response as HttpResponse, header::CONTENT_TYPE},
};
use opsbox_core::SqlitePool;
use opsbox_core::dfs::{Location, OrlParser};
use serde::{Deserialize, Serialize};
use tracing::debug;

#[derive(Debug, Serialize)]
struct AgentRawFileQuery {
  path: String,
  #[serde(skip_serializing_if = "Option::is_none")]
  entry: Option<String>,
}

/// 查看缓存中的文件内容
pub async fn view_cache_json(
  State(pool): State<SqlitePool>,
  Query(params): Query<ViewParams>,
) -> Result<HttpResponse<Body>, LogSeekApiError> {
  tracing::debug!(
    "view-request: sid={} file={} start={:?} end={:?}",
    params.sid,
    params.file,
    params.start,
    params.end
  );

  // 解析 ORL 字符串
  let resource = OrlParser::parse(&params.file)?;

  // 获取 agent_id（用于后续使用）
  let agent_id = resource.endpoint.identity.clone();

  // 读取 keywords
  let keywords = simple_cache().get_keywords(&params.sid).await.unwrap_or_default();

  debug!("🔍 Server查找缓存: sid={}, file_url={}", params.sid, params.file);

  // 1. 尝试从缓存获取
  let cache_result = simple_cache()
    .get_lines_slice(
      &params.sid,
      &params.file, // 使用字符串 key
      params.start.unwrap_or(1),
      params.end.unwrap_or(1000),
    )
    .await;

  // 2. 如果缓存未命中，尝试从源加载
  let (total, slice, encoding) = match cache_result {
    Some(v) => {
      debug!(
        "✅ Server缓存命中: sid={}, file_url={}, total={}, slice_len={}",
        params.sid,
        params.file,
        v.0,
        v.1.len()
      );
      v
    }
    None => {
      debug!(
        "❌ Server缓存未命中，尝试回源读取: sid={}, file_url={}",
        params.sid, params.file
      );

      // 读取所有行
      let mut all_lines: Vec<String> = Vec::new();
      let mut encoding_name = "UTF-8".to_string();

      match resource.endpoint.location {
        Location::Remote { .. } => {
          // 对于 Agent 来源，直接走 file_raw，避免将 archive entry 误转换为文件系统路径
          let path_str = resource.primary_path.to_string();
          let entry = resource.archive_context.as_ref().map(|ctx| ctx.inner_path.to_string());

          debug!(
            "🚀 准备调用 Agent 读取文件: agent_id={}, path={}, entry={:?}",
            agent_id, path_str, entry
          );

          let client = crate::agent::create_agent_client_by_id(&pool, agent_id.to_string())
            .await
            .map_err(|e| {
              LogSeekApiError::Service(ServiceError::ProcessingError(format!("无法创建 Agent 客户端: {}", e)))
            })?;

          let query = AgentRawFileQuery {
            path: path_str.clone(),
            entry,
          };

          let response = client
            .get_raw_with_query("/api/v1/file_raw", &query)
            .await
            .map_err(|e| {
              LogSeekApiError::Service(ServiceError::ProcessingError(format!("Agent 原始文件请求失败: {}", e)))
            })?;

          let bytes = response.bytes().await.map_err(|e| {
            LogSeekApiError::Service(ServiceError::ProcessingError(format!("读取 Agent 响应失败: {}", e)))
          })?;

          let mut reader = bytes.as_ref();
          let result = read_text_file(&mut reader, None)
            .await
            .map_err(|e| LogSeekApiError::Service(ServiceError::ProcessingError(format!("读取文件失败: {}", e))))?;

          if let Some((lines, encoding)) = result {
            all_lines = lines;
            encoding_name = encoding;
          } else {
            tracing::warn!(
              "Agent 文件被检测为二进制或为空: path={} entry={:?}",
              path_str,
              query.entry
            );
          }
        }
        _ => {
          // Local/S3 来源：archive entry 必须走 archive-aware 流，否则会把 .gz/.tar 等当普通文件处理。
          let mut stream = create_search_entry_stream_from_resource(&pool, &resource)
            .await
            .map_err(|e| LogSeekApiError::Service(ServiceError::ProcessingError(format!("创建流失败: {}", e))))?;

          // 获取要查找的 entry 路径（如果有）
          let target_entry = resource.archive_context.as_ref().map(|c| c.inner_path.to_string());

          // 读取条目
          let mut found = false;
          let mut checked_count = 0;
          while let Some(entry_res) = stream
            .next_entry()
            .await
            .map_err(|e| LogSeekApiError::Service(ServiceError::ProcessingError(format!("读取流失败: {}", e))))?
          {
            let (meta, mut reader) = entry_res;
            checked_count += 1;

            // 如果指定了 entry 路径，检查是否匹配
            if let Some(ref target) = target_entry {
              // 规范化路径比较（去除开头的 / 和 ./）
              let meta_path = meta.path.trim_start_matches('/').trim_start_matches("./");
              let target_path = target.trim_start_matches('/').trim_start_matches("./");
              if meta_path != target_path {
                continue; // 跳过不匹配的条目
              }
            }

            // 读取匹配条目的内容
            let result = read_text_file(&mut reader, None)
              .await
              .map_err(|e| LogSeekApiError::Service(ServiceError::ProcessingError(format!("读取文件失败: {}", e))))?;

            if let Some((lines, encoding)) = result {
              tracing::debug!("文件编码: {}", encoding);
              all_lines = lines;
              encoding_name = encoding;
              found = true;
            } else {
              tracing::warn!("文件被检测为二进制或为空: {}", params.file);
            }
            break; // 找到第一个匹配的条目后停止
          }

          if !found {
            tracing::warn!("未找到条目或流为空: {}, 共检查 {} 个条目", params.file, checked_count);
          }
        }
      }

      // 将完整内容写入缓存
      let total = all_lines.len();
      debug!("✅ 回源读取成功: lines={}", total);
      simple_cache()
        .put_lines(&params.sid, &params.file, &all_lines, encoding_name.clone())
        .await;

      // 从全量数据中切片返回
      let start = params.start.unwrap_or(1);
      let end = params.end.unwrap_or(1000);

      // compact_lines.get_slice logic implemented manually for Vec
      let s = start.max(1).min(total.max(1));
      let eidx = end.max(s).min(total);

      // 0-based index slicing
      let slice = if total > 0 && s <= total {
        all_lines[s - 1..eidx].to_vec()
      } else {
        Vec::new()
      };

      (total, slice, encoding_name)
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
    "encoding": encoding,
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

  // 从缓存中获取文件列表 (keys are now strings)
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

  // 已经是 String 列表
  let files = file_urls;

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
  tracing::debug!("download-request: sid={} file={}", params.sid, params.file);

  // 验证 ORL
  let _resource = OrlParser::parse(&params.file)?;

  // 从缓存获取完整文件内容
  let (total, lines, _) = match simple_cache()
    .get_lines_slice(
      &params.sid,
      &params.file,
      1,          // 从第1行开始
      usize::MAX, // 到最大行（内部会限制到total）
    )
    .await
  {
    Some(v) => {
      tracing::debug!(
        "✅ 下载缓存命中: sid={}, file_url={}, total={}, lines_len={}",
        params.sid,
        params.file,
        v.0,
        v.1.len()
      );
      v
    }
    None => {
      tracing::debug!("❌ 下载缓存未命中: sid={}, file_url={}", params.sid, params.file);
      return Err(LogSeekApiError::Repository(RepositoryError::NotFound(format!(
        "Cache not found or expired for sid={}, file={}",
        params.sid, params.file
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
    .header(CONTENT_TYPE, HeaderValue::from_static("text/plain; charset=utf-8"))
    .body(Body::from(content))
    .map_err(|e| LogSeekApiError::Service(ServiceError::ProcessingError(format!("构建下载响应失败: {}", e))))
}

/// 流式传输文件原始内容（用于图片查看等）
///
/// 直接从 Source 读取文件流，不经过缓存，支持二进制文件
pub async fn view_raw_file(
  State(pool): State<SqlitePool>,
  Query(params): Query<ViewParams>,
) -> Result<HttpResponse<Body>, LogSeekApiError> {
  tracing::debug!("view-raw-request: sid={} file={}", params.sid, params.file);

  // 1. 解析 ORL
  let resource = OrlParser::parse(&params.file)?;

  // 2. 检查来源类型
  match resource.endpoint.location {
    Location::Remote { .. } => {
      let agent_id = resource.endpoint.identity.clone();
      // 创建 Agent 客户端
      let client = crate::agent::create_agent_client_by_id(&pool, agent_id.to_string())
        .await
        .map_err(|e| {
          LogSeekApiError::Service(ServiceError::ProcessingError(format!("无法创建 Agent 客户端: {}", e)))
        })?;

      let path_str = resource.primary_path.to_string();
      let query = AgentRawFileQuery {
        path: path_str.clone(),
        entry: resource.archive_context.as_ref().map(|ctx| ctx.inner_path.to_string()),
      };

      tracing::debug!(
        "Agent 原始文件请求: agent_id={}, path={}, entry={:?}",
        agent_id,
        path_str,
        query.entry
      );

      // 调用 Agent API
      let response = client
        .get_raw_with_query("/api/v1/file_raw", &query)
        .await
        .map_err(|e| LogSeekApiError::Service(ServiceError::ProcessingError(format!("Agent 请求失败: {}", e))))?;

      // 代理响应
      let headers = response.headers().clone();
      let content_type = headers
        .get(CONTENT_TYPE)
        .cloned()
        .unwrap_or_else(|| HeaderValue::from_static("application/octet-stream"));

      use futures::TryStreamExt;
      let stream = response.bytes_stream().map_err(std::io::Error::other);
      let body = Body::from_stream(stream);

      HttpResponse::builder()
        .status(200)
        .header(CONTENT_TYPE, content_type)
        .body(body)
        .map_err(|e| LogSeekApiError::Service(ServiceError::ProcessingError(format!("构建响应失败: {}", e))))
    }
    _ => {
      // Local / S3
      let mut stream = create_search_entry_stream_from_resource(&pool, &resource)
        .await
        .map_err(|e| LogSeekApiError::Service(ServiceError::ProcessingError(format!("创建流失败: {}", e))))?;

      let target_entry = resource.archive_context.as_ref().map(|ctx| ctx.inner_path.to_string());
      let mut selected: Option<(opsbox_core::fs::EntryMeta, Box<dyn tokio::io::AsyncRead + Send + Unpin>)> = None;

      while let Some((meta, reader)) = stream
        .next_entry()
        .await
        .map_err(|e| LogSeekApiError::Service(ServiceError::ProcessingError(format!("读取流失败: {}", e))))?
      {
        if let Some(ref target) = target_entry {
          let meta_path = meta.path.trim_start_matches('/').trim_start_matches("./");
          let target_path = target.trim_start_matches('/').trim_start_matches("./");
          if meta_path != target_path {
            continue;
          }
        }
        selected = Some((meta, reader));
        break;
      }

      if let Some((meta, reader)) = selected {
        // 头部嗅探 MIME 类型
        let mut buf_reader = tokio::io::BufReader::new(reader);
        let mut head = vec![0u8; 1024]; // 1KB 样本
        let mut n = 0;
        use tokio::io::AsyncReadExt;

        // 尽力读取
        while n < 1024 {
          let read_n = buf_reader.read(&mut head[n..]).await.unwrap_or(0);
          if read_n == 0 {
            break;
          }
          n += read_n;
        }
        head.truncate(n);

        let kind = opsbox_core::fs::sniff_file_type(&head);
        let mime = kind.mime_type().to_string(); // Return owned string

        // 重构流
        let prefixed = opsbox_core::fs::PrefixedReader::new(head, buf_reader);

        // 转换为 Stream
        let stream = tokio_util::io::ReaderStream::new(prefixed);

        tracing::debug!("开始流式传输文件: {}, mime={}", meta.path, mime);

        HttpResponse::builder()
          .status(200)
          .header(
            CONTENT_TYPE,
            HeaderValue::from_str(&mime).unwrap_or(HeaderValue::from_static("application/octet-stream")),
          )
          // .header("Cache-Control", "public, max-age=3600") // 可选：添加缓存控制
          .body(Body::from_stream(stream))
          .map_err(|e| LogSeekApiError::Service(ServiceError::ProcessingError(format!("构建响应失败: {}", e))))
      } else {
        Err(LogSeekApiError::Repository(RepositoryError::NotFound(
          "文件未找到或为空".to_string(),
        )))
      }
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::api::models::ViewParams;
  use crate::repository::cache::cache as simple_cache;
  use axum::extract::Query;
  use axum::http::StatusCode;
  use flate2::{Compression, write::GzEncoder};
  use std::io::Write;
  use tar::{Builder, Header};

  /// 测试中的响应体最大读取大小（1MB）
  const TEST_MAX_BODY_SIZE: usize = 1024 * 1024;

  fn create_test_gzip_file(file_path: &std::path::Path, content: &str) {
    let file = std::fs::File::create(file_path).unwrap();
    let mut encoder = GzEncoder::new(file, Compression::default());
    encoder.write_all(content.as_bytes()).unwrap();
    encoder.finish().unwrap();
  }

  fn create_test_tar_gz_file(file_path: &std::path::Path, entry_name: &str, content: &str) {
    let file = std::fs::File::create(file_path).unwrap();
    let encoder = GzEncoder::new(file, Compression::default());
    let mut builder = Builder::new(encoder);

    let mut header = Header::new_gnu();
    header.set_path(entry_name).unwrap();
    header.set_size(content.len() as u64);
    header.set_mode(0o644);
    header.set_cksum();
    builder.append(&header, content.as_bytes()).unwrap();

    let encoder = builder.into_inner().unwrap();
    encoder.finish().unwrap();
  }

  #[tokio::test]
  async fn test_view_cache_json_hit() {
    let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
    let sid = "test-sid-hit".to_string();
    let file = "orl://local/tmp/test.log".to_string();
    let lines = vec!["line 1".to_string(), "line 2".to_string(), "line 3".to_string()];

    // Populate cache
    simple_cache().put_lines(&sid, &file, &lines, "UTF-8".to_string()).await;

    let params = ViewParams {
      sid: sid.clone(),
      file: file.clone(),
      start: Some(1),
      end: Some(2),
    };

    let resp = view_cache_json(State(pool), Query(params)).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    // Use axum::body::to_bytes to read the response body
    let body_bytes = axum::body::to_bytes(resp.into_body(), TEST_MAX_BODY_SIZE)
      .await
      .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();

    assert_eq!(json["total"], 3);
    assert_eq!(json["start"], 1);
    assert_eq!(json["end"], 2);
    assert_eq!(json["lines"].as_array().unwrap().len(), 2);
    assert_eq!(json["lines"][0]["no"], 1);
    assert_eq!(json["lines"][0]["text"], "line 1");
  }

  #[tokio::test]
  async fn test_get_file_list_json_success() {
    let sid = "test-sid-list".to_string();
    let file1 = "file1".to_string();
    let file2 = "file2".to_string();

    // Populate cache via put_lines (indirectly creates file list)
    simple_cache()
      .put_lines(&sid, &file1, &["line1".to_string()], "UTF-8".to_string())
      .await;
    simple_cache()
      .put_lines(&sid, &file2, &["line2".to_string()], "UTF-8".to_string())
      .await;

    let params = FileListParams { sid: sid.clone() };
    let resp = get_file_list_json(Query(params)).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body_bytes = axum::body::to_bytes(resp.into_body(), TEST_MAX_BODY_SIZE)
      .await
      .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();

    assert_eq!(json["sid"], sid);
    assert_eq!(json["count"], 2);
    let files = json["files"].as_array().unwrap();
    assert_eq!(files.len(), 2);
    assert!(files.contains(&serde_json::json!("file1")));
    assert!(files.contains(&serde_json::json!("file2")));
  }

  #[tokio::test]
  async fn test_download_file_success() {
    let sid = "test-sid-download".to_string();
    let file = "orl://local/tmp/down.log".to_string();
    let lines = vec!["hello".to_string(), "world".to_string()];

    simple_cache().put_lines(&sid, &file, &lines, "UTF-8".to_string()).await;

    let params = ViewParams {
      sid: sid.clone(),
      file: file.clone(),
      start: None,
      end: None,
    };

    let resp = download_file(Query(params)).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body_bytes = axum::body::to_bytes(resp.into_body(), TEST_MAX_BODY_SIZE)
      .await
      .unwrap();
    let content = String::from_utf8(body_bytes.to_vec()).unwrap();
    assert_eq!(content, "hello\nworld");
  }

  #[tokio::test]
  async fn test_view_cache_json_miss_local() {
    let temp_dir = tempfile::tempdir().unwrap();
    let file_path = temp_dir.path().join("miss.log");
    std::fs::write(&file_path, "miss content line 1\nline 2").unwrap();

    let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
    let sid = "test-sid-miss".to_string();
    let file_url = format!("orl://local{}", file_path.to_str().unwrap());

    let params = ViewParams {
      sid: sid.clone(),
      file: file_url.clone(),
      start: None, // Defaults to 1..1000
      end: None,
    };

    let resp = view_cache_json(State(pool), Query(params)).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body_bytes = axum::body::to_bytes(resp.into_body(), TEST_MAX_BODY_SIZE)
      .await
      .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();

    assert_eq!(json["total"], 2);
    assert_eq!(json["lines"][0]["text"], "miss content line 1");

    // Verify it was cached
    let cached = simple_cache().get_lines_slice(&sid, &file_url, 1, 10).await;
    assert!(cached.is_some());
    assert_eq!(cached.unwrap().0, 2);
  }

  #[tokio::test]
  async fn test_view_raw_file_local() {
    let temp_dir = tempfile::tempdir().unwrap();
    let file_path = temp_dir.path().join("raw.png");
    // Write some bytes to trigger sniff
    let png_header = [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];
    std::fs::write(&file_path, png_header).unwrap();

    let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
    let file_url = format!("orl://local{}", file_path.to_str().unwrap());

    let params = ViewParams {
      sid: "any".into(),
      file: file_url,
      start: None,
      end: None,
    };

    let resp = view_raw_file(State(pool), Query(params)).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(resp.headers().get(CONTENT_TYPE).unwrap(), "image/png");

    let body_bytes = axum::body::to_bytes(resp.into_body(), TEST_MAX_BODY_SIZE)
      .await
      .unwrap();
    assert_eq!(&body_bytes[..8], &png_header);
  }

  #[tokio::test]
  async fn test_view_cache_json_local_gzip_entry() {
    let temp_dir = tempfile::tempdir().unwrap();
    let gzip_path = temp_dir.path().join("sample.log.gz");
    let original_content = "gzip line 1\ngzip line 2\n";
    create_test_gzip_file(&gzip_path, original_content);

    let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
    let sid = "test-sid-gzip-entry".to_string();
    let file_url = format!(
      "orl://local{}?entry=/sample.log",
      gzip_path.to_str().unwrap()
    );

    let params = ViewParams {
      sid: sid.clone(),
      file: file_url.clone(),
      start: None,
      end: None,
    };

    let resp = view_cache_json(State(pool), Query(params)).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body_bytes = axum::body::to_bytes(resp.into_body(), TEST_MAX_BODY_SIZE)
      .await
      .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();

    assert_eq!(json["total"], 2);
    assert_eq!(json["lines"][0]["text"], "gzip line 1");
    assert_eq!(json["lines"][1]["text"], "gzip line 2");

    let cached = simple_cache().get_lines_slice(&sid, &file_url, 1, 10).await;
    assert!(cached.is_some());
    assert_eq!(cached.unwrap().0, 2);
  }

  #[tokio::test]
  async fn test_view_raw_file_local_gzip_entry() {
    let temp_dir = tempfile::tempdir().unwrap();
    let gzip_path = temp_dir.path().join("sample.log.gz");
    let original_content = "gzip raw line 1\ngzip raw line 2\n";
    create_test_gzip_file(&gzip_path, original_content);

    let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
    let file_url = format!(
      "orl://local{}?entry=/sample.log",
      gzip_path.to_str().unwrap()
    );

    let params = ViewParams {
      sid: "test-sid-gzip-raw".into(),
      file: file_url,
      start: None,
      end: None,
    };

    let resp = view_raw_file(State(pool), Query(params)).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(resp.headers().get(CONTENT_TYPE).unwrap(), "text/plain");

    let body_bytes = axum::body::to_bytes(resp.into_body(), TEST_MAX_BODY_SIZE)
      .await
      .unwrap();
    let content = String::from_utf8(body_bytes.to_vec()).unwrap();
    assert_eq!(content, original_content);
  }

  #[tokio::test]
  async fn test_view_cache_json_local_tar_gz_entry() {
    let temp_dir = tempfile::tempdir().unwrap();
    let archive_path = temp_dir.path().join("sample.tar.gz");
    let entry_name = "internal/app.log";
    let original_content = "tar.gz line 1\ntar.gz line 2\n";
    create_test_tar_gz_file(&archive_path, entry_name, original_content);

    let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
    let sid = "test-sid-targz-entry".to_string();
    let file_url = format!(
      "orl://local{}?entry={}",
      archive_path.to_str().unwrap(),
      entry_name
    );

    let params = ViewParams {
      sid: sid.clone(),
      file: file_url.clone(),
      start: None,
      end: None,
    };

    let resp = view_cache_json(State(pool), Query(params)).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body_bytes = axum::body::to_bytes(resp.into_body(), TEST_MAX_BODY_SIZE)
      .await
      .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();

    assert_eq!(json["total"], 2);
    assert_eq!(json["lines"][0]["text"], "tar.gz line 1");
    assert_eq!(json["lines"][1]["text"], "tar.gz line 2");

    let cached = simple_cache().get_lines_slice(&sid, &file_url, 1, 10).await;
    assert!(cached.is_some());
    assert_eq!(cached.unwrap().0, 2);
  }

  #[tokio::test]
  async fn test_view_raw_file_local_tar_gz_entry() {
    let temp_dir = tempfile::tempdir().unwrap();
    let archive_path = temp_dir.path().join("sample.tar.gz");
    let entry_name = "internal/app.log";
    let original_content = "tar.gz raw line 1\ntar.gz raw line 2\n";
    create_test_tar_gz_file(&archive_path, entry_name, original_content);

    let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
    let file_url = format!(
      "orl://local{}?entry={}",
      archive_path.to_str().unwrap(),
      entry_name
    );

    let params = ViewParams {
      sid: "test-sid-targz-raw".into(),
      file: file_url,
      start: None,
      end: None,
    };

    let resp = view_raw_file(State(pool), Query(params)).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(resp.headers().get(CONTENT_TYPE).unwrap(), "text/plain");

    let body_bytes = axum::body::to_bytes(resp.into_body(), TEST_MAX_BODY_SIZE)
      .await
      .unwrap();
    let content = String::from_utf8(body_bytes.to_vec()).unwrap();
    assert_eq!(content, original_content);
  }
}
