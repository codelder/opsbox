use crate::{
  renderer::{render_json_chunks, render_markdown},
  search::{Search as _, SearchError},
  storage::{ReaderProvider as _, S3ReaderProvider, StorageError},
};
use axum::{
  Router,
  body::Body,
  extract::{Json, rejection::JsonRejection},
  http::{HeaderValue, Response as HttpResponse, StatusCode, header::CONTENT_TYPE},
  routing::post,
};
use problemdetails::Problem;
use serde_json;
use thiserror::Error;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use crate::bbip_service::plan_from_q;

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

pub fn router() -> Router {
  Router::new()
    .route("/stream", post(stream_markdown))
    .route("/stream.ndjson", post(stream_local_ndjson))
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
  let (tx, rx) = mpsc::channel::<Result<bytes::Bytes, std::io::Error>>(8);

  // 通过服务从 q 中解析日期属性并生成文件路径，同时返回清理后的 q
  let base_dir = "/Users/wangyue/Downloads/log";
  let buckets = ["20", "21", "22", "23"];
  let plan = plan_from_q(base_dir, &buckets, &body.q);
  let files = plan.files;
  let q_for_search = plan.cleaned_q;

  let spec = crate::query::Query::parse_github_like(&q_for_search).map_err(|e| Problem::from(AppError::QueryParse(e)))?;
  let highlights = spec.highlights.clone();
  let ctx = body.context.unwrap_or(3);

  for path in files {
    let txc = tx.clone();
    let specc = spec.clone();
    let highlights_c = highlights.clone();
    tokio::spawn(async move {
      let Ok(reader) = tokio::fs::File::open(&path).await.map_err(|e| StorageError::from(e)) else {
        return;
      };

      let Ok(mut stream) = reader.search(&specc, ctx).await else {
        return;
      };

      while let Some(result) = stream.recv().await {
        let json_obj = render_json_chunks(
          &format!("{}:{}", path, &result.path),
          result.merged.clone(),
          result.lines.clone(),
          &highlights_c,
        );
        if let Ok(mut v) = serde_json::to_vec(&json_obj) {
          v.push(b'\n');
          let _ = txc.send(Ok(bytes::Bytes::from(v))).await;
        }
      }
    });
  }

  // 关闭原始发送端，待各并发任务结束后自动结束流
  drop(tx);

  let body = axum::body::Body::from_stream(ReceiverStream::new(rx));

  Ok(
    HttpResponse::builder()
      .status(200)
      .header(
        CONTENT_TYPE,
        HeaderValue::from_static("application/x-ndjson; charset=utf-8"),
      )
      .body(body)
      .unwrap(),
  )
}
