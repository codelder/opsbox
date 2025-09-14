use crate::bbip_service::derive_plan;
use crate::{
  renderer::{render_json_chunks, render_markdown},
  search::{Search as _, SearchError},
  storage::{ReaderProvider as _, S3ReaderProvider, StorageError},
};
use axum::{
  Router,
  body::Body,
  extract::{Json, Query, rejection::JsonRejection},
  http::{HeaderValue, Response as HttpResponse, StatusCode, header::CONTENT_TYPE},
  routing::{get, post},
};
use problemdetails::Problem;
use serde_json;
use thiserror::Error;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use chrono::{Datelike, Duration};
use crate::simple_cache::{cache as simple_cache, new_sid};

#[derive(Debug, Error)]
pub enum AppError {
  #[error("存储错误")]
  StorageError(StorageError),
  #[error("检索错误")]
  SearchError(SearchError),
  #[error(transparent)]
  BadJson(#[from] JsonRejection),
  #[error("查询语法错误")]
  QueryParse(#[from] crate::query::ParseError),
}

#[derive(Debug, Clone, serde::Deserialize)]
struct ViewParams {
  sid: String,
  file: String,
  start: Option<usize>,
  end: Option<usize>,
}

async fn view_cache_json(Query(params): Query<ViewParams>) -> Result<HttpResponse<Body>, Problem> {
  eprintln!("DEBUG view-request: sid={} file={} start={:?} end={:?}", params.sid, params.file, params.start, params.end);
  // 读取 keywords 与行切片
  let keywords = simple_cache().get_keywords(&params.sid).await.unwrap_or_default();
  let (total, slice) = match simple_cache().get_lines_slice(&params.sid, &params.file, params.start.unwrap_or(1), params.end.unwrap_or(1000)).await {
    Some(v) => v,
    None => {
      eprintln!("DEBUG view-miss: sid={} file={}", params.sid, params.file);
      return Ok(
        HttpResponse::builder()
          .status(404)
          .header(CONTENT_TYPE, HeaderValue::from_static("application/json; charset=utf-8"))
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
  eprintln!("DEBUG view-hit: sid={} file={} total={} slice={} range=[{}..{}]", params.sid, params.file, total, slice.len(), start, end);
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
      .header(CONTENT_TYPE, HeaderValue::from_static("application/json; charset=utf-8"))
      .body(Body::from(body))
      .unwrap(),
  )
}

impl From<AppError> for Problem {
  fn from(error: AppError) -> Self {
    match error {
      AppError::StorageError(e) => problemdetails::new(StatusCode::INTERNAL_SERVER_ERROR)
        .with_title("存储错误")
        .with_detail(e.to_string()),
      AppError::SearchError(e) => problemdetails::new(StatusCode::INTERNAL_SERVER_ERROR)
        .with_title("检索错误")
        .with_detail(e.to_string()),
      AppError::BadJson(e) => problemdetails::new(StatusCode::BAD_REQUEST)
        .with_title("JSON请求错误")
        .with_detail(e.to_string()),
      AppError::QueryParse(e) => problemdetails::new(StatusCode::BAD_REQUEST)
        .with_title("查询语法错误")
        .with_detail(e.to_string()),
    }
  }
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct SearchBody {
  pub q: String,
  pub context: Option<usize>,
}

fn stream_channel_capacity() -> usize {
  // 允许通过环境变量覆盖，默认 256，限定在 [8, 10000]
  let default_cap = 256usize;
  match std::env::var("LOGSEARCH_STREAM_CH_CAP")
    .ok()
    .and_then(|s| s.parse::<usize>().ok())
  {
    Some(v) => v.clamp(8, 10_000),
    None => default_cap,
  }
}

pub fn router() -> Router {
  Router::new()
    .route("/stream", post(stream_markdown))
    .route("/stream.ndjson", post(stream_local_ndjson))
    .route("/stream.s3.ndjson", post(stream_s3_ndjson))
    .route("/view.cache.json", get(view_cache_json))
}

async fn stream_markdown(Json(body): Json<SearchBody>) -> Result<HttpResponse<Body>, Problem> {
  let (tx, rx) = mpsc::channel::<Result<bytes::Bytes, std::io::Error>>(8);

  let _ = tx.send(Ok(bytes::Bytes::from("# 搜索结果\n\n"))).await;

  let s3reader = S3ReaderProvider::new(
    "http://192.168.50.61:9002",
    "admin",
    "G5t3o6f2",
    "backupdr",
    "bbip/2025/202508/20250819/BBIP_20_APPLOG_2025-08-18.tar.gz",
  )
  .open()
  .await
  .map_err(|e| AppError::StorageError(e))?;

  let spec = crate::query::Query::parse_github_like(&body.q).map_err(|e| Problem::from(AppError::QueryParse(e)))?;

  let highlights = spec.highlights.clone();

  let fut = async move {
    let Ok(mut stream) = s3reader.search(&spec, body.context.unwrap_or(3)).await else {
      return;
    };

    while let Some(result) = stream.recv().await {
      let buf = render_markdown(&result.path, result.merged, result.lines, &highlights);
      let _ = tx.send(Ok(bytes::Bytes::from(buf))).await;
    }
  };

  tokio::spawn(fut);

  let body = axum::body::Body::from_stream(ReceiverStream::new(rx));

  Ok(
    HttpResponse::builder()
      .status(200)
      .header(CONTENT_TYPE, HeaderValue::from_static("text/markdown; charset=utf-8"))
      .body(body)
      .unwrap(),
  )
}

async fn stream_local_ndjson(Json(body): Json<SearchBody>) -> Result<HttpResponse<Body>, Problem> {
  let cap = stream_channel_capacity();
  let (tx, rx) = mpsc::channel::<Result<bytes::Bytes, std::io::Error>>(cap);
  eprintln!("耗时调试: 建立响应通道，容量={}", cap);

  // 整体起始时间（仅用于粗粒度耗时调试）
  let overall_start = std::time::Instant::now();

  // 通过服务从 q 中解析日期属性并生成文件路径，同时返回清理后的 q
  let plan_start = std::time::Instant::now();
  let base_dir = "/Users/wangyue/Downloads/log";
  let buckets = ["20", "21", "22", "23"];
  let plan = derive_plan(base_dir, &buckets, &body.q);
  let files = plan.paths;
  let q_for_search = plan.cleaned_query;
  eprintln!(
    "耗时调试: 规划完成，文件数={}，日期区间=[{}..={}], 规划耗时={:?}",
    files.len(),
    plan.range.start,
    plan.range.end,
    plan_start.elapsed()
  );

  let parse_start = std::time::Instant::now();
  let spec =
    crate::query::Query::parse_github_like(&q_for_search).map_err(|e| Problem::from(AppError::QueryParse(e)))?;
  let parse_dur = parse_start.elapsed();
  let highlights = spec.highlights.clone();
  let sid = new_sid();
  simple_cache().put_keywords(&sid, highlights.clone()).await;
  eprintln!("DEBUG cache-keywords: sid={} keywords={:?}", sid, highlights);
  let ctx = body.context.unwrap_or(3);
  eprintln!("耗时调试: 查询语法解析完成，ctx={}，耗时={:?}", ctx, parse_dur);

  for path in files {
    let txc = tx.clone();
    let specc = spec.clone();
    let highlights_c = highlights.clone();
    let sid_c = sid.clone();
    tokio::spawn(async move {
      let file_start = std::time::Instant::now();
      let Ok(reader) = tokio::fs::File::open(&path).await.map_err(|e| StorageError::from(e)) else {
        eprintln!("耗时调试: 打开文件失败 path={}", path);
        return;
      };

      let Ok(mut stream) = reader.search(&specc, ctx).await else {
        eprintln!("耗时调试: 启动检索失败 path={}", path);
        return;
      };

      let mut produced: usize = 0;
      while let Some(result) = stream.recv().await {
        // 如前端已停止读取，尽快退出，避免无效开销
        if txc.is_closed() {
          eprintln!("耗时调试: 下游通道已关闭，提前结束 path={}", path);
          return;
        }
        let file_id = format!("{}:{}", path, &result.path);
        eprintln!("DEBUG cache-put: sid={} file_id={} lines={}", sid_c, file_id, result.lines.len());
        simple_cache().put_lines(&sid_c, &file_id, result.lines.clone()).await;

        let json_obj = render_json_chunks(
          &file_id,
          result.merged.clone(),
          result.lines.clone(),
          &highlights_c,
        );
        match serde_json::to_vec(&json_obj) {
          Ok(mut v) => {
            v.push(b'\n');
            if let Err(_e) = txc.send(Ok(bytes::Bytes::from(v))).await {
              // 接收端已关闭（客户端断开或响应结束），终止该任务
              eprintln!("耗时调试: 发送失败(接收端关闭) path={}", path);
              return;
            }
            produced += 1;
          }
          Err(e) => {
            eprintln!("耗时调试: 序列化失败 path={}，err={}", path, e);
          }
        }
      }
      eprintln!(
        "耗时调试: 文件处理完成 path={}，输出记录={}，耗时={:?}",
        path,
        produced,
        file_start.elapsed()
      );
    });
  }

  eprintln!(
    "耗时调试: 任务已派发，整体耗时(至返回响应构建前)={:?}",
    overall_start.elapsed()
  );

  // 关闭原始发送端，待各并发任务结束后自动结束流
  drop(tx);

  let body = Body::from_stream(ReceiverStream::new(rx));

  Ok(
    HttpResponse::builder()
      .status(200)
      .header(
        CONTENT_TYPE,
        HeaderValue::from_static("application/x-ndjson; charset=utf-8"),
      )
      .header("X-Logsearch-SID", HeaderValue::from_str(&sid).unwrap_or(HeaderValue::from_static("")))
      .body(body)
      .unwrap(),
  )
}

async fn stream_s3_ndjson(Json(body): Json<SearchBody>) -> Result<HttpResponse<Body>, Problem> {
  let cap = stream_channel_capacity();
  let (tx, rx) = mpsc::channel::<Result<bytes::Bytes, std::io::Error>>(cap);
  eprintln!("耗时调试: [S3] 建立响应通道，容量={}", cap);

  // 粗粒度耗时
  let overall_start = std::time::Instant::now();

  // 解析日期计划，仅用于得到日期区间与清理后的查询
  let plan_start = std::time::Instant::now();
  let base_dir = "/unused/for/s3"; // 仅为复用 derive_plan 获取日期区间与 cleaned_query
  let buckets = ["20", "21", "22", "23"];
  let plan = derive_plan(base_dir, &buckets, &body.q);
  let q_for_search = plan.cleaned_query;
  eprintln!(
    "耗时调试: [S3] 规划完成，日期区间=[{}..={}], 规划耗时={:?}",
    plan.range.start,
    plan.range.end,
    plan_start.elapsed()
  );

  // 解析查询
  let parse_start = std::time::Instant::now();
  let spec = crate::query::Query::parse_github_like(&q_for_search).map_err(|e| Problem::from(AppError::QueryParse(e)))?;
  let parse_dur = parse_start.elapsed();
  let highlights = spec.highlights.clone();
  let sid = new_sid();
  simple_cache().put_keywords(&sid, highlights.clone()).await;
  eprintln!("DEBUG cache-keywords: sid={} keywords={:?}", sid, highlights);
  let ctx = body.context.unwrap_or(3);
  eprintln!("耗时调试: [S3] 查询语法解析完成，ctx={}，耗时={:?}", ctx, parse_dur);

  // 生成 S3 对象键：每天 4 个 bucket，文件名日期 = d；前缀路径日期 = d+1
  let endpoint = "http://192.168.50.61:9002";
  let access_key = "admin";
  let secret_key = "G5t3o6f2";
  let bucket_name = "backupdr";

  let mut d = plan.range.start;
  while d <= plan.range.end {
    let dp1 = d + Duration::days(1);
    let y = dp1.year();
    let m = dp1.month();
    let day = dp1.day();
    let yyyymm = format!("{:04}{:02}", y, m);
    let yyyymmdd = format!("{:04}{:02}{:02}", y, m, day);
    let file_name = format!("{:04}-{:02}-{:02}", d.year(), d.month(), d.day());

    for b in buckets {
      let key = format!(
        "bbip/{}/{}/{}/BBIP_{}_APPLOG_{}.tar.gz",
        y,
        yyyymm,
        yyyymmdd,
        b,
        file_name
      );

      let txc = tx.clone();
      let specc = spec.clone();
      let highlights_c = highlights.clone();
      let sid_c = sid.clone();
      tokio::spawn(async move {
        let file_start = std::time::Instant::now();
        let s3rp = S3ReaderProvider::new(endpoint, access_key, secret_key, bucket_name, &key);
        let Ok(reader) = s3rp.open().await.map_err(|e| AppError::StorageError(e)) else {
          eprintln!("耗时调试: [S3] 打开对象失败 key={}", key);
          return;
        };

        let Ok(mut stream) = reader.search(&specc, ctx).await else {
          eprintln!("耗时调试: [S3] 启动检索失败 key={}", key);
          return;
        };

        let mut produced: usize = 0;
        while let Some(result) = stream.recv().await {
          if txc.is_closed() {
            eprintln!("耗时调试: [S3] 下游通道已关闭，提前结束 key={}", key);
            return;
          }

          let file_id = format!("{}/{}:{}", bucket_name, key, &result.path);
          eprintln!("DEBUG cache-put: sid={} file_id={} lines={}", sid_c, file_id, result.lines.len());
          simple_cache().put_lines(&sid_c, &file_id, result.lines.clone()).await;

          let json_obj = render_json_chunks(
            &file_id,
            result.merged.clone(),
            result.lines.clone(),
            &highlights_c,
          );

          match serde_json::to_vec(&json_obj) {
            Ok(mut v) => {
              v.push(b'\n');
              if let Err(_e) = txc.send(Ok(bytes::Bytes::from(v))).await {
                eprintln!("耗时调试: [S3] 发送失败(接收端关闭) key={}", key);
                return;
              }
              produced += 1;
            }
            Err(e) => {
              eprintln!("耗时调试: [S3] 序列化失败 key={}，err={}", key, e);
            }
          }
        }

        eprintln!(
          "耗时调试: [S3] 对象处理完成 key={}，输出记录={}，耗时={:?}",
          key,
          produced,
          file_start.elapsed()
        );
      });
    }

    d = d + Duration::days(1);
  }

  eprintln!(
    "耗时调试: [S3] 任务已派发，整体耗时(至返回响应构建前)={:?}",
    overall_start.elapsed()
  );

  drop(tx);

  let body = Body::from_stream(ReceiverStream::new(rx));
  Ok(
    HttpResponse::builder()
      .status(200)
      .header(
        CONTENT_TYPE,
        HeaderValue::from_static("application/x-ndjson; charset=utf-8"),
      )
      .header("X-Logsearch-SID", HeaderValue::from_str(&sid).unwrap_or(HeaderValue::from_static("")))
      .body(body)
      .unwrap(),
  )
}
