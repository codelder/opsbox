//! 文件查看路由
//!
//! 处理 /view.cache.json 端点，从缓存中读取文件内容

use crate::agent::SearchService;
use crate::api::{LogSeekApiError, models::ViewParams};
use crate::repository::{RepositoryError, cache::cache as simple_cache};
use crate::service::ServiceError;
use axum::{
  body::Body,
  extract::{Query, State},
  http::{HeaderValue, Response as HttpResponse, header::CONTENT_TYPE},
};
use futures::StreamExt;
use opsbox_core::SqlitePool;
use opsbox_core::odfs::orl::{EndpointType, ORL, TargetType};
use serde::Deserialize;
// use tokio::io::AsyncBufReadExt;
use crate::service::entry_stream::create_entry_stream;
use tracing::debug;

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

  // 解析 ORL
  let orl = ORL::parse(&params.file).map_err(LogSeekApiError::Domain)?;

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

      match orl.endpoint_type().map_err(LogSeekApiError::Domain)? {
        EndpointType::Agent => {
          let agent_id = orl.effective_id();
          // 检查是否是归档条目
          let is_archive_target = orl.target_type() == TargetType::Archive;
            // 归档条目：使用 create_entry_stream 下载归档并读取条目
          if is_archive_target {
            debug!(
              "🚀 Agent 归档条目：使用 create_entry_stream 读取: agent_id={}, path={}",
              agent_id, orl.path()
            );

            let mut stream = create_entry_stream(&pool, &orl)
              .await
              .map_err(|e| LogSeekApiError::Service(ServiceError::ProcessingError(format!("创建流失败: {}", e))))?;

            // 获取要查找的 entry 路径
            let target_entry = orl.entry_path().map(|c| c.into_owned());

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
                let meta_path = meta.path.trim_start_matches('/').trim_start_matches("./");
                let target_path = target.trim_start_matches('/').trim_start_matches("./");
                if meta_path != target_path {
                  continue;
                }
              }

              let result = crate::service::encoding::read_text_file(&mut reader, None)
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
              break;
            }

            if !found {
               tracing::warn!("在归档中未找到指定条目: {:?}, 共检查 {} 个条目", target_entry, checked_count);
            }
          } else {
            // 普通文件：使用 Agent search API 读取 (or should we use file_raw API?)
            // Legacy uses search API for text files?
            // "普通文件：使用 search API 读取"
            debug!(
              "🚀 准备调用 Agent 读取文件: agent_id={}, path={}",
              agent_id, orl.path()
            );

            let client = crate::agent::create_agent_client_by_id(&pool, agent_id.to_string())
              .await
              .map_err(|e| {
                LogSeekApiError::Service(ServiceError::ProcessingError(format!("无法创建 Agent 客户端: {}", e)))
              })?;

            // Reconstruct exact target options for Agent Search
            let target = match orl.target_type() {
               TargetType::Dir => crate::domain::config::Target::Dir { path: orl.path().to_string(), recursive: true },
               // ORL for single file usually mapped to Dir/Files target in legacy logic if not archive entry
               // But usually we just want to search *this file*.
               // Agent SearchOptions needs a target.
               // If it's a file, we want to read it.
               // Using Files target.
               _ => crate::domain::config::Target::Files { paths: vec![orl.path().to_string()] }
            };

            let options = crate::agent::SearchOptions {
              path_filter: None,
              target,
              timeout_secs: Some(30),
              ..Default::default()
            };

            debug!("📤 发送 Agent 搜索请求: query='/.*/', options={:?}", options);

            let mut stream = client
              .search("/.*/", 0, options)
              .await
              .map_err(|e| LogSeekApiError::Service(ServiceError::ProcessingError(format!("Agent 搜索失败: {}", e))))?;

            debug!("📥以此 Agent 搜索流建立成功，开始接收数据...");

            while let Some(item) = stream.next().await {
              match item {
                Ok(res) => {
                  all_lines.extend(res.lines);
                  if let Some(enc) = res.encoding
                    && encoding_name == "UTF-8"
                  {
                    encoding_name = enc;
                  }
                }
                Err(e) => {
                  tracing::warn!("Agent 流返回错误: {}", e);
                }
              }
            }
          }
        }
        _ => {
          // Local/S3 来源：使用 create_entry_stream
          let mut stream = create_entry_stream(&pool, &orl)
            .await
            .map_err(|e| LogSeekApiError::Service(ServiceError::ProcessingError(format!("创建流失败: {}", e))))?;

          // 获取要查找的 entry 路径（如果有）
          let target_entry = orl.entry_path().map(|c| c.into_owned());

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
            let result = crate::service::encoding::read_text_file(&mut reader, None)
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
  let _orl = ORL::parse(&params.file).map_err(LogSeekApiError::Domain)?;

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
  let orl = ORL::parse(&params.file).map_err(LogSeekApiError::Domain)?;

  // 2. 检查来源类型
  match orl.endpoint_type().map_err(LogSeekApiError::Domain)? {
    EndpointType::Agent => {
      let agent_id = orl.effective_id();
      // 创建 Agent 客户端
      let client = crate::agent::create_agent_client_by_id(&pool, agent_id.to_string())
        .await
        .map_err(|e| {
          LogSeekApiError::Service(ServiceError::ProcessingError(format!("无法创建 Agent 客户端: {}", e)))
        })?;

      // 构造请求路径
      // Agent /api/v1/file_raw needs path param.
      // orl.path() should be adequate.
      let target_path = orl.path();
      let query_path = format!("/api/v1/file_raw?path={}", urlencoding::encode(target_path));

      tracing::debug!("Agent 原始文件请求: agent_id={}, query={}", agent_id, query_path);

      // 调用 Agent API
      let response = client
        .get_raw(&query_path)
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
      let mut stream = create_entry_stream(&pool, &orl)
        .await
        .map_err(|e| LogSeekApiError::Service(ServiceError::ProcessingError(format!("创建流失败: {}", e))))?;

      // 读取第一个条目
      if let Some((meta, reader)) = stream
        .next_entry()
        .await
        .map_err(|e| LogSeekApiError::Service(ServiceError::ProcessingError(format!("读取流失败: {}", e))))?
      {
        // 头部嗅探 MIME 类型
        let mut buf_reader = tokio::io::BufReader::new(reader);
        let mut head = vec![0u8; 1024]; // 1KB 样本
        let mut n = 0;
        use tokio::io::AsyncReadExt;

        // 尽力读取
        while n < 1024 {
             let read_n = buf_reader.read(&mut head[n..]).await.unwrap_or(0);
             if read_n == 0 { break; }
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
          .header(CONTENT_TYPE, HeaderValue::from_str(&mime).unwrap_or(HeaderValue::from_static("application/octet-stream")))
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
    use axum::extract::Query;
    use axum::http::StatusCode;
    use crate::repository::cache::cache as simple_cache;
    use crate::api::models::ViewParams;

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
        let body_bytes = axum::body::to_bytes(resp.into_body(), 1024 * 1024).await.unwrap();
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
        simple_cache().put_lines(&sid, &file1, &["line1".to_string()], "UTF-8".to_string()).await;
        simple_cache().put_lines(&sid, &file2, &["line2".to_string()], "UTF-8".to_string()).await;

        let params = FileListParams { sid: sid.clone() };
        let resp = get_file_list_json(Query(params)).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let body_bytes = axum::body::to_bytes(resp.into_body(), 1024 * 1024).await.unwrap();
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

        let body_bytes = axum::body::to_bytes(resp.into_body(), 1024 * 1024).await.unwrap();
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

        let body_bytes = axum::body::to_bytes(resp.into_body(), 1024 * 1024).await.unwrap();
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
        std::fs::write(&file_path, &png_header).unwrap();

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

        let body_bytes = axum::body::to_bytes(resp.into_body(), 1024 * 1024).await.unwrap();
        assert_eq!(&body_bytes[..8], &png_header);
    }
}
