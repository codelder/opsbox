//! 文件查看路由
//!
//! 处理 /view.cache.json 端点，从缓存中读取文件内容

use crate::agent::SearchService;
use crate::api::{LogSeekApiError, models::ViewParams};
use crate::domain::Odfi;
use crate::domain::config::{Endpoint, Source, Target};
use crate::domain::{EndpointType, TargetType};
use crate::repository::{RepositoryError, cache::cache as simple_cache};
use crate::service::ServiceError;
use axum::{
  body::Body,
  extract::{Query, State},
  http::{HeaderValue, Response as HttpResponse, header::CONTENT_TYPE},
};
use futures::StreamExt;
use opsbox_core::SqlitePool;
use serde::Deserialize;
// use tokio::io::AsyncBufReadExt;
use crate::service::entry_stream::EntryStreamFactory;
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

  // 解析 Odfi
  let file_url: Odfi = match params.file.parse() {
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

  // 读取 keywords
  let keywords = simple_cache().get_keywords(&params.sid).await.unwrap_or_default();

  debug!("🔍 Server查找缓存: sid={}, file_url={}", params.sid, file_url);

  // 1. 尝试从缓存获取
  let cache_result = simple_cache()
    .get_lines_slice(
      &params.sid,
      &file_url,
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
        file_url,
        v.0,
        v.1.len()
      );
      v
    }
    None => {
      debug!(
        "❌ Server缓存未命中，尝试回源读取: sid={}, file_url={}",
        params.sid, file_url
      );

      // 构造 Source 配置
      let source = odfi_to_source(&file_url)
        .map_err(|e| LogSeekApiError::Service(ServiceError::ProcessingError(format!("无法解析源信息: {}", e))))?;

      debug!(
        "✨ 构造 Source 成功: endpoint={:?}, target={:?}",
        source.endpoint, source.target
      );

      // 读取所有行
      let mut all_lines: Vec<String> = Vec::new();
      let mut encoding_name = "UTF-8".to_string();

      match &source.endpoint {
        Endpoint::Agent { agent_id, .. } => {
          // Agent 来源：使用 search API 读取
          debug!(
            "🚀 准备调用 Agent 读取文件: agent_id={}, path={}",
            agent_id, file_url.path
          );

          let client = crate::agent::create_agent_client_by_id(agent_id.clone())
            .await
            .map_err(|e| {
              LogSeekApiError::Service(ServiceError::ProcessingError(format!("无法创建 Agent 客户端: {}", e)))
            })?;

          let options = crate::agent::SearchOptions {
            path_filter: None, // Target::Files 已指定文件，无需额外路径过滤，避免 Glob 匹配问题
            target: source.target.clone(),
            timeout_secs: Some(30),
            ..Default::default()
          };

          debug!("📤 发送 Agent 搜索请求: query='/.*/', options={:?}", options);

          // 使用通配符查询以匹配所有行 (regex /.*/)
          // Agent 的 SearchService 返回 Result<Stream>
          let mut stream = client
            .search("/.*/", 0, options)
            .await
            .map_err(|e| LogSeekApiError::Service(ServiceError::ProcessingError(format!("Agent 搜索失败: {}", e))))?;

          debug!("📥以此 Agent 搜索流建立成功，开始接收数据...");

          while let Some(item) = stream.next().await {
            match item {
              Ok(res) => {
                // 这里的 res.lines 是匹配的行，如果查询是 "."，则是所有非空行
                // 注意：如果文件有空行，grep "." 可能会跳过。
                // 但这是目前 Agent API 的限制，暂且接受
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
        _ => {
          // Local/S3 来源：使用 EntryStreamFactory
          let factory = EntryStreamFactory::new(pool.clone());

          let mut stream = factory
            .create_stream(source)
            .await
            .map_err(|e| LogSeekApiError::Service(ServiceError::ProcessingError(format!("创建流失败: {}", e))))?;

          // 读取条目（预期只有一个文件）
          if let Some(entry_res) = stream
            .next_entry()
            .await
            .map_err(|e| LogSeekApiError::Service(ServiceError::ProcessingError(format!("读取流失败: {}", e))))?
          {
            let (_, mut reader) = entry_res;
            // 读取所有行
            // 读取所有行（带编码检测）
            let result = crate::service::encoding::read_text_file(&mut reader, None)
              .await
              .map_err(|e| LogSeekApiError::Service(ServiceError::ProcessingError(format!("读取文件失败: {}", e))))?;

            if let Some((lines, encoding)) = result {
              tracing::debug!("文件编码: {}", encoding);
              all_lines = lines;
              encoding_name = encoding;
            } else {
              tracing::warn!("文件被检测为二进制或为空: {}", file_url);
            }
          } else {
            tracing::warn!("流未返回任何条目: {}", file_url);
            // 空文件?
          }
        }
      }

      // 将完整内容写入缓存
      let total = all_lines.len();
      debug!("✅ 回源读取成功: lines={}", total);
      simple_cache()
        .put_lines(&params.sid, &file_url, all_lines.clone(), encoding_name.clone())
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

/// 辅助函数：将 Odfi 转换为 Source 配置
fn odfi_to_source(odfi: &Odfi) -> Result<crate::domain::config::Source, String> {
  let endpoint = match odfi.endpoint_type {
    EndpointType::Local => {
      // 本地文件：认为 root 为 /, path 为绝对路径
      // 注意：odfi.path 通常不带 leading slash 如果是从 pathbuf 转的?
      // 但 unix 绝对路径应该带 /。
      // 为安全起见，root 设为 /，path 设为 odfi.path (需确保 odfi.path 是绝对路径或相对于 root)
      // 如果 odfi.path 包含 /，root=/ 应该没问题。
      Endpoint::Local { root: "/".to_string() }
    }
    EndpointType::S3 => {
      // S3: id 是 profile, path 是 bucket/key
      // 分割 bucket 和 key
      let parts: Vec<&str> = odfi.path.splitn(2, '/').collect();
      if parts.len() != 2 {
        return Err(format!("S3 路径格式错误 (需要 bucket/key): {}", odfi.path));
      }
      Endpoint::S3 {
        profile: odfi.endpoint_id.clone(),
        bucket: parts[0].to_string(),
      }
    }
    EndpointType::Agent => {
      Endpoint::Agent {
        agent_id: odfi.endpoint_id.clone(),
        subpath: "".to_string(), // 暂时假设无 subpath 限制或由 path 完整指定
      }
    }
  };

  let target = match odfi.endpoint_type {
    EndpointType::Agent => {
      // Agent 总是使用 Files 目标以读取单文件
      // 确保路径为绝对路径（ODFI 解析可能丢失开头的 /）
      let path = if !odfi.path.starts_with('/') {
        format!("/{}", odfi.path)
      } else {
        odfi.path.clone()
      };
      Target::Files { paths: vec![path] }
    }
    _ => {
      match odfi.target_type {
        TargetType::Dir => {
          // 虽然是 Dir 类型，但如果是 Odfi 指向的具体文件，我们用 Files Target
          // 这样 EntryStream 就会只读取这个文件
          Target::Files {
            paths: vec![odfi.path.clone()],
          }
        }
        TargetType::Archive => {
          // 如果是归档，path 是归档文件路径
          // 如果有 entry_path，目前 EntryStream 处理整个归档流
          // 我们的 view 逻辑需要过滤吗？
          // 目前 EntryStream 会 yield 归档内所有文件。
          // 这是一个潜在性能问题：回源读取大归档只为一个文件。
          // 但根据目前架构，EntryStream 就是这样工作的。
          // 至少 S3 需要 Target::Archive 才能工作。
          // 对于 S3，如果 path 是 bucket/key，这里 target path 应该是 key
          let parts: Vec<&str> = odfi.path.splitn(2, '/').collect();
          let path = if let Endpoint::S3 { .. } = endpoint {
            if parts.len() == 2 {
              parts[1].to_string()
            } else {
              odfi.path.clone()
            }
          } else {
            odfi.path.clone()
          };
          Target::Archive { path }
        }
      }
    }
  };

  Ok(Source {
    endpoint,
    target,
    display_name: None,
    filter_glob: None,
  })
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
  tracing::debug!("download-request: sid={} file={}", params.sid, params.file);

  // 解析 Odfi
  let file_url: Odfi = match params.file.parse() {
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
  let (total, lines, _) = match simple_cache()
    .get_lines_slice(
      &params.sid,
      &file_url,
      1,          // 从第1行开始
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
    .header(CONTENT_TYPE, HeaderValue::from_static("text/plain; charset=utf-8"))
    .body(Body::from(content))
    .map_err(|e| LogSeekApiError::Service(ServiceError::ProcessingError(format!("构建下载响应失败: {}", e))))
}
