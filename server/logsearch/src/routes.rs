use crate::bbip_service::derive_plan;
use crate::settings;
use crate::simple_cache::{cache as simple_cache, new_sid};
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
use chrono::{Datelike, Duration};
use problemdetails::Problem;
use serde::{Deserialize, Serialize};
use serde_json;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use std::sync::atomic::{AtomicU64, Ordering};

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
  #[error("设置存储错误")]
  Settings(#[from] settings::SettingsError),
}

#[derive(Debug, Clone, serde::Deserialize)]
struct ViewParams {
  sid: String,
  file: String,
  start: Option<usize>,
  end: Option<usize>,
}

async fn view_cache_json(Query(params): Query<ViewParams>) -> Result<HttpResponse<Body>, Problem> {
  log::debug!(
    "view-request: sid={} file={} start={:?} end={:?}",
    params.sid,
    params.file,
    params.start,
    params.end
  );
  // 读取 keywords 与行切片
  let keywords = simple_cache().get_keywords(&params.sid).await.unwrap_or_default();
  let (total, slice) = match simple_cache()
    .get_lines_slice(
      &params.sid,
      &params.file,
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
      AppError::Settings(e) => match e {
        settings::SettingsError::NotConfigured => problemdetails::new(StatusCode::SERVICE_UNAVAILABLE)
          .with_title("MinIO 未配置")
          .with_detail("请先完成 MinIO 设置"),
        settings::SettingsError::Connection(msg) => problemdetails::new(StatusCode::BAD_REQUEST)
          .with_title("MinIO 连接失败")
          .with_detail(msg),
        settings::SettingsError::Database(err) => problemdetails::new(StatusCode::INTERNAL_SERVER_ERROR)
          .with_title("设置存储错误")
          .with_detail(err.to_string()),
      },
    }
  }
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct SearchBody {
  pub q: String,
  pub context: Option<usize>,
}

#[derive(Debug, Clone, serde::Serialize)]
struct NL2QOut {
  q: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct MinioSettingsPayload {
  endpoint: String,
  bucket: String,
  access_key: String,
  secret_key: String,
  #[serde(default)]
  configured: bool,
  #[serde(default)]
  connection_error: Option<String>,
}

impl From<MinioSettingsPayload> for settings::MinioSettings {
  fn from(value: MinioSettingsPayload) -> Self {
    Self {
      endpoint: value.endpoint,
      bucket: value.bucket,
      access_key: value.access_key,
      secret_key: value.secret_key,
    }
  }
}

impl From<settings::MinioSettings> for MinioSettingsPayload {
  fn from(value: settings::MinioSettings) -> Self {
    Self {
      endpoint: value.endpoint,
      bucket: value.bucket,
      access_key: value.access_key,
      secret_key: value.secret_key,
      configured: false,
      connection_error: None,
    }
  }
}

impl Default for MinioSettingsPayload {
  fn default() -> Self {
    Self {
      endpoint: String::new(),
      bucket: String::new(),
      access_key: String::new(),
      secret_key: String::new(),
      configured: false,
      connection_error: None,
    }
  }
}

fn stream_channel_capacity() -> usize {
  // 优先使用全局调参；未设置则回退到环境变量；再回退到默认值
  if let Some(t) = crate::tuning::get() { return t.stream_ch_cap.clamp(8, 10_000); }
  match std::env::var("LOGSEARCH_STREAM_CH_CAP").ok().and_then(|s| s.parse::<usize>().ok()) {
    Some(v) => v.clamp(8, 10_000),
    None => 256usize,
  }
}

// 中文注释：读取 S3 IO 并发上限（限制同时打开/读取的对象数）
fn s3_max_concurrency() -> usize {
  if let Some(t) = crate::tuning::get() { return t.s3_max_concurrency.clamp(1, 128); }
  std::env::var("LOGSEARCH_S3_MAX_CONCURRENCY")
    .ok()
    .and_then(|s| s.parse::<usize>().ok())
    .map(|v| v.clamp(1, 128))
    .unwrap_or(12)
}

// 中文注释：读取 CPU 并发上限（限制同时进行解压/检索的任务数）
fn cpu_max_concurrency() -> usize {
  if let Some(t) = crate::tuning::get() { return t.cpu_concurrency.clamp(1, 128); }
  std::env::var("LOGSEARCH_CPU_CONCURRENCY")
    .ok()
    .and_then(|s| s.parse::<usize>().ok())
    .map(|v| v.clamp(1, 128))
    .unwrap_or(16)
}

pub fn router() -> Router {
  Router::new()
    .route("/stream", post(stream_markdown))
    .route("/stream.ndjson", post(stream_local_ndjson))
    .route("/stream.s3.ndjson", post(stream_s3_ndjson))
    .route("/view.cache.json", get(view_cache_json))
    .route("/settings/minio", get(get_minio_settings).post(save_minio_settings))
    // 中文注释：自然语言 → 查询字符串
    .route("/nl2q", post(nl2q))
}

async fn get_minio_settings() -> Result<Json<MinioSettingsPayload>, Problem> {
  let settings_opt = settings::load_minio_settings().await.map_err(AppError::Settings)?;
  let mut payload = settings_opt
    .clone()
    .map_or_else(MinioSettingsPayload::default, Into::into);

  if let Some(settings_value) = settings_opt {
    match settings::verify_minio_settings(&settings_value).await {
      Ok(_) => {
        payload.configured = true;
      }
      Err(settings::SettingsError::Connection(msg)) => {
        payload.configured = false;
        payload.connection_error = Some(format!("无法连接 MinIO：{}", msg));
      }
      Err(err) => return Err(AppError::Settings(err).into()),
    }
  }

  Ok(Json(payload))
}

async fn save_minio_settings(Json(payload): Json<MinioSettingsPayload>) -> Result<StatusCode, Problem> {
  let settings: settings::MinioSettings = payload.into();
  settings::save_minio_settings(&settings)
    .await
    .map_err(AppError::Settings)?;
  Ok(StatusCode::NO_CONTENT)
}

async fn stream_markdown(Json(body): Json<SearchBody>) -> Result<HttpResponse<Body>, Problem> {
  let (tx, rx) = mpsc::channel::<Result<bytes::Bytes, std::io::Error>>(8);

  let _ = tx.send(Ok(bytes::Bytes::from("# 搜索结果\n\n"))).await;

  let minio_cfg = settings::load_required_minio_settings()
    .await
    .map_err(AppError::Settings)?;

  let s3reader = S3ReaderProvider::new(
    &minio_cfg.endpoint,
    &minio_cfg.access_key,
    &minio_cfg.secret_key,
    &minio_cfg.bucket,
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

// 中文注释：NL → Q 端点，实现将自然语言转换为查询字符串
async fn nl2q(Json(body): Json<crate::nl2q::NLBody>) -> Result<Json<NL2QOut>, Problem> {
  log::info!("NL2Q API请求: {}", body.nl);

  let start = std::time::Instant::now();
  let q = crate::nl2q::call_ollama(&body.nl).await.map_err(|e| {
    log::error!("NL2Q API失败: {}", e);
    problemdetails::new(StatusCode::BAD_GATEWAY)
      .with_title("AI 生成失败")
      .with_detail(e.to_string())
  })?;

  log::info!("NL2Q API成功: {} -> '{}', 耗时: {:?}", body.nl, q, start.elapsed());
  Ok(Json(NL2QOut { q }))
}

async fn stream_local_ndjson(Json(body): Json<SearchBody>) -> Result<HttpResponse<Body>, Problem> {
  let cap = stream_channel_capacity();
  let (tx, rx) = mpsc::channel::<Result<bytes::Bytes, std::io::Error>>(cap);
  log::debug!("profiling: 建立响应通道，容量={}", cap);

  // 整体起始时间（仅用于粗粒度耗时调试）
  let overall_start = std::time::Instant::now();

  // 通过服务从 q 中解析日期属性并生成文件路径，同时返回清理后的 q
  let plan_start = std::time::Instant::now();
  let base_dir = "/Users/wangyue/Downloads/log";
  let buckets = ["20", "21", "22", "23"];
  let plan = derive_plan(base_dir, &buckets, &body.q);
  let files = plan.paths;
  let q_for_search = plan.cleaned_query;
  log::debug!(
    "profiling: 规划完成，文件数={}，日期区间=[{}..={}], 规划耗时={:?}",
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
  log::debug!("cache-keywords: sid={} keywords={:?}", sid, highlights);
  let ctx = body.context.unwrap_or(3);
  log::debug!("profiling: 查询语法解析完成，ctx={}，耗时={:?}", ctx, parse_dur);

  for path in files {
    let txc = tx.clone();
    let specc = spec.clone();
    let highlights_c = highlights.clone();
    let sid_c = sid.clone();
    tokio::spawn(async move {
      let file_start = std::time::Instant::now();
      let Ok(reader) = tokio::fs::File::open(&path).await.map_err(|e| StorageError::from(e)) else {
        log::warn!("profiling: 打开文件失败 path={}", path);
        return;
      };

      let Ok(mut stream) = reader.search(&specc, ctx).await else {
        log::warn!("profiling: 启动检索失败 path={}", path);
        return;
      };

      let mut produced: usize = 0;
      while let Some(result) = stream.recv().await {
        // 如前端已停止读取，尽快退出，避免无效开销
        if txc.is_closed() {
          log::debug!("profiling: 下游通道已关闭，提前结束 path={}", path);
          return;
        }
        let file_id = format!("{}:{}", path, &result.path);
        log::debug!(
          "cache-put: sid={} file_id={} lines={}",
          sid_c,
          file_id,
          result.lines.len()
        );
        simple_cache().put_lines(&sid_c, &file_id, result.lines.clone()).await;

        let json_obj = render_json_chunks(&file_id, result.merged.clone(), result.lines.clone(), &highlights_c);
        match serde_json::to_vec(&json_obj) {
          Ok(mut v) => {
            v.push(b'\n');
            if let Err(_e) = txc.send(Ok(bytes::Bytes::from(v))).await {
              // 接收端已关闭（客户端断开或响应结束），终止该任务
              log::debug!("profiling: 发送失败(接收端关闭) path={}", path);
              return;
            }
            produced += 1;
            log::debug!("profiling: 发送成功 path={}，输出记录={}", path, produced);
          }
          Err(e) => {
            log::warn!("profiling: 序列化失败 path={}，err={}", path, e);
          }
        }
      }
      log::debug!(
        "profiling: 文件处理完成 path={}，输出记录={}，耗时={:?}",
        path,
        produced,
        file_start.elapsed()
      );
    });
  }

  log::debug!(
    "profiling: 任务已派发，整体耗时(至返回响应构建前)={:?}",
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
      .header(
        "X-Logsearch-SID",
        HeaderValue::from_str(&sid).unwrap_or(HeaderValue::from_static("")),
      )
      .body(body)
      .unwrap(),
  )
}

async fn stream_s3_ndjson(Json(body): Json<SearchBody>) -> Result<HttpResponse<Body>, Problem> {
  let cap = stream_channel_capacity();
  let (tx, rx) = mpsc::channel::<Result<bytes::Bytes, std::io::Error>>(cap);
  log::debug!("profiling: [S3] 建立响应通道，容量={}", cap);

  // 中文注释：分层限流——IO 并发与 CPU 并发
  let io_sem = Arc::new(tokio::sync::Semaphore::new(s3_max_concurrency()));
  let cpu_max = cpu_max_concurrency();
  let cpu_sem = Arc::new(tokio::sync::Semaphore::new(cpu_max));

  // 中文注释：自适应护栏 - 统计与控制器
  struct Stats { produced: AtomicU64, s3_errors: AtomicU64 }
  impl Stats { fn new() -> Self { Self { produced: AtomicU64::new(0), s3_errors: AtomicU64::new(0) } } }
  let stats = Arc::new(Stats::new());
  struct CpuController { max: usize, target: usize, held: Vec<tokio::sync::OwnedSemaphorePermit> }
  impl CpuController { fn current_effective(&self) -> usize { self.max.saturating_sub(self.held.len()) } }
  let cpu_ctrl = Arc::new(tokio::sync::Mutex::new(CpuController { max: cpu_max, target: cpu_max.min(2), held: Vec::new() }));

  // 中文注释：后台调节任务（每 3s 调整一次，AIMD 策略）
  {
    let stats_c = Arc::clone(&stats);
    let cpu_sem_c = Arc::clone(&cpu_sem);
    let cpu_ctrl_c = Arc::clone(&cpu_ctrl);
    tokio::spawn(async move {
      let mut prev_prod = 0u64;
      let mut prev_err = 0u64;
      let mut prev_tp = 0.0f64;
      loop {
        tokio::time::sleep(std::time::Duration::from_secs(3)).await;
        let prod = stats_c.produced.load(Ordering::Relaxed);
        let err = stats_c.s3_errors.load(Ordering::Relaxed);
        let dprod = prod.saturating_sub(prev_prod);
        let derr = err.saturating_sub(prev_err);
        prev_prod = prod; prev_err = err;
        let denom = (dprod + derr) as f64;
        let err_rate = if denom > 0.0 { derr as f64 / denom } else { 0.0 };
        let tp = dprod as f64 / 3.0; // 条/秒

        let mut ctrl = cpu_ctrl_c.lock().await;
        let cur_eff = ctrl.current_effective();
        // 决策：高错误率则乘性减小；否则若吞吐不下降则加一
        if err_rate > 0.02 && cur_eff > 1 {
          let new_target = ((cur_eff as f64) * 0.7).floor().max(1.0) as usize;
          ctrl.target = new_target;
        } else if tp >= prev_tp * 0.98 && ctrl.target < ctrl.max {
          ctrl.target += 1;
        }
        prev_tp = tp;

        // 通过持有/释放许可来收敛到目标（仅在 [1..=max] 范围内调整）
        let desired_held = ctrl.max.saturating_sub(ctrl.target);
        if desired_held > ctrl.held.len() {
          let need = desired_held - ctrl.held.len();
          for _ in 0..need {
            match cpu_sem_c.clone().try_acquire_owned() {
              Ok(p) => ctrl.held.push(p),
              Err(_) => break, // 无可用许可，下一轮再试
            }
          }
        } else if desired_held < ctrl.held.len() {
          let release = ctrl.held.len() - desired_held;
          for _ in 0..release { let _ = ctrl.held.pop(); } // drop -> 释放许可
        }
        log::debug!(
          "adaptive: cpu target={} effective={} err_rate={:.3}% tp={:.2}/s",
          ctrl.target,
          ctrl.current_effective(),
          err_rate * 100.0,
          tp
        );
      }
    });
  }

  // 粗粒度耗时
  let overall_start = std::time::Instant::now();

  // 解析日期计划，仅用于得到日期区间与清理后的查询
  let plan_start = std::time::Instant::now();
  let base_dir = "/unused/for/s3"; // 仅为复用 derive_plan 获取日期区间与 cleaned_query
  let buckets = ["20", "21", "22", "23"];
  let plan = derive_plan(base_dir, &buckets, &body.q);
  let q_for_search = plan.cleaned_query;
  log::debug!(
    "profiling: [S3] 规划完成，日期区间=[{}..={}], 规划耗时={:?}",
    plan.range.start,
    plan.range.end,
    plan_start.elapsed()
  );

  // 解析查询
  let parse_start = std::time::Instant::now();
  let spec =
    crate::query::Query::parse_github_like(&q_for_search).map_err(|e| Problem::from(AppError::QueryParse(e)))?;
  let parse_dur = parse_start.elapsed();
  let highlights = spec.highlights.clone();
  let sid = new_sid();
  simple_cache().put_keywords(&sid, highlights.clone()).await;
  log::debug!("cache-keywords: sid={} keywords={:?}", sid, highlights);
  let ctx = body.context.unwrap_or(3);
  log::debug!("profiling: [S3] 查询语法解析完成，ctx={}，耗时={:?}", ctx, parse_dur);

  let minio_cfg = Arc::new(
    settings::load_required_minio_settings()
      .await
      .map_err(AppError::Settings)?,
  );

  // 生成 S3 对象键：每天 4 个 bucket，文件名日期 = d；前缀路径日期 = d+1

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
        y, yyyymm, yyyymmdd, b, file_name
      );

      let txc = tx.clone();
      let specc = spec.clone();
      let highlights_c = highlights.clone();
      let sid_c = sid.clone();
      let cfg = Arc::clone(&minio_cfg);
      let io_sem_c = io_sem.clone();
      let cpu_sem_c = cpu_sem.clone();
      let stats_c2 = Arc::clone(&stats);
      tokio::spawn(async move {
        let file_start = std::time::Instant::now();

        // 中文注释：获取 IO 并发许可，限制同时打开/读取的对象数
        let _io_permit = match io_sem_c.acquire_owned().await {
          Ok(p) => p,
          Err(_) => {
            log::warn!("profiling: [S3] 获取 IO 许可失败，跳过 key={}", key);
            return;
          }
        };

        let s3rp = S3ReaderProvider::new(&cfg.endpoint, &cfg.access_key, &cfg.secret_key, &cfg.bucket, &key);
        let Ok(reader) = s3rp.open().await.map_err(|e| AppError::StorageError(e)) else {
          log::warn!("profiling: [S3] 打开对象失败 key={}", key);
          // 统计：S3 错误
          stats_c2.s3_errors.fetch_add(1, Ordering::Relaxed);
          return;
        };

        // 中文注释：获取 CPU 并发许可，限制同时进行解压/检索的任务数
        let _cpu_permit = match cpu_sem_c.acquire_owned().await {
          Ok(p) => p,
          Err(_) => {
            log::warn!("profiling: [S3] 获取 CPU 许可失败，跳过 key={}", key);
            return;
          }
        };

        let Ok(mut stream) = reader.search(&specc, ctx).await else {
          log::warn!("profiling: [S3] 启动检索失败 key={}", key);
          stats_c2.s3_errors.fetch_add(1, Ordering::Relaxed);
          return;
        };

        let mut produced: usize = 0;
        while let Some(result) = stream.recv().await {
          if txc.is_closed() {
            log::debug!("profiling: [S3] 下游通道已关闭，提前结束 key={}", key);
            return;
          }

          let bucket_name = cfg.bucket.clone();
          let file_id = format!("{}/{}:{}", bucket_name, key, &result.path);
          log::debug!(
            "cache-put: sid={} file_id={} lines={}",
            sid_c,
            file_id,
            result.lines.len()
          );
          simple_cache().put_lines(&sid_c, &file_id, result.lines.clone()).await;

          let json_obj = render_json_chunks(&file_id, result.merged.clone(), result.lines.clone(), &highlights_c);

          match serde_json::to_vec(&json_obj) {
            Ok(mut v) => {
              v.push(b'\n');
              if let Err(_e) = txc.send(Ok(bytes::Bytes::from(v))).await {
                log::debug!("profiling: [S3] 发送失败(接收端关闭) key={}", key);
                return;
              }
              produced += 1;
              stats_c2.produced.fetch_add(1, Ordering::Relaxed);
              log::info!("profiling: 发送成功 path={}，输出记录={}", &result.path, produced);
            }
            Err(e) => {
              log::warn!("profiling: [S3] 序列化失败 key={}，err={}", key, e);
            }
          }
        }

        log::debug!(
          "profiling: [S3] 对象处理完成 key={}，输出记录={}，耗时={:?}",
          key,
          produced,
          file_start.elapsed()
        );
      });
    }

    d = d + Duration::days(1);
  }

  log::debug!(
    "profiling: [S3] 任务已派发，整体耗时(至返回响应构建前)={:?}",
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
      .header(
        "X-Logsearch-SID",
        HeaderValue::from_str(&sid).unwrap_or(HeaderValue::from_static("")),
      )
      .body(body)
      .unwrap(),
  )
}

