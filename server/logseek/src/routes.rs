// ============================================================================
// API 层 - HTTP 路由和处理器
// ============================================================================
// 注意：此文件保留以保持向后兼容
// 新代码应使用 api::models 中的类型
// ============================================================================
use crate::api::models::{AppError, NL2QOut, S3ProfileListResponse, S3ProfilePayload, S3SettingsPayload, SearchBody, ViewParams};
use crate::domain::FileUrl;
use crate::repository::cache::{cache as simple_cache, new_sid};
use crate::repository::settings;
use crate::utils::bbip_service::derive_plan;
use crate::utils::renderer::render_json_chunks;
use axum::{
  Router,
  body::Body,
  extract::{Json, Path, Query, State},
  http::{HeaderValue, Response as HttpResponse, StatusCode, header::CONTENT_TYPE},
  routing::{delete, get, post},
};
use chrono::{Datelike, Duration};
use opsbox_core::SqlitePool;
use problemdetails::Problem;
use serde_json;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;

async fn view_cache_json(Query(params): Query<ViewParams>) -> Result<HttpResponse<Body>, Problem> {
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

fn stream_channel_capacity() -> usize {
  // 优先使用全局调参；未设置则回退到环境变量；再回退到默认值
  if let Some(t) = crate::utils::tuning::get() {
    return t.stream_ch_cap.clamp(8, 10_000);
  }
  match std::env::var("LOGSEEK_STREAM_CH_CAP")
    .ok()
    .and_then(|s| s.parse::<usize>().ok())
  {
    Some(v) => v.clamp(8, 10_000),
    None => 256usize,
  }
}

// 读取 S3 IO 并发上限（限制同时打开/读取的对象数）
fn s3_max_concurrency() -> usize {
  if let Some(t) = crate::utils::tuning::get() {
    return t.s3_max_concurrency.clamp(1, 128);
  }
  std::env::var("LOGSEEK_S3_MAX_CONCURRENCY")
    .ok()
    .and_then(|s| s.parse::<usize>().ok())
    .map(|v| v.clamp(1, 128))
    .unwrap_or(12)
}

// 读取 CPU 并发上限（限制同时进行解压/检索的任务数）
fn cpu_max_concurrency() -> usize {
  if let Some(t) = crate::utils::tuning::get() {
    return t.cpu_concurrency.clamp(1, 128);
  }
  std::env::var("LOGSEEK_CPU_CONCURRENCY")
    .ok()
    .and_then(|s| s.parse::<usize>().ok())
    .map(|v| v.clamp(1, 128))
    .unwrap_or(16)
}

pub fn router(db_pool: SqlitePool) -> Router {
  // 创建 Agent 状态
  let agent_state = Arc::new(crate::routes_agent::AgentState::new());

  // 创建 Agent 子路由
  let agent_router = crate::routes_agent::agent_routes().with_state(agent_state);

  Router::new()
    // 搜索路由（多存储源并行搜索）
    .route("/search.ndjson", post(stream_search))
    .route("/view.cache.json", get(view_cache_json))
    .route("/settings/s3", get(get_s3_settings).post(save_s3_settings))
    // S3 Profile 管理
    .route("/profiles", get(list_profiles).post(save_profile))
    .route("/profiles/{name}", delete(delete_profile))  // 使用 {name} 而不是 :name
    // 自然语言 → 查询字符串
    .route("/nl2q", post(nl2q))
    // Agent 管理路由
    .merge(agent_router)
    .with_state(db_pool)
}

async fn get_s3_settings(State(pool): State<SqlitePool>) -> Result<Json<S3SettingsPayload>, Problem> {
  let settings_opt = settings::load_s3_settings(&pool).await.map_err(AppError::Settings)?;
  let mut payload = settings_opt.clone().map_or_else(S3SettingsPayload::default, Into::into);

  if let Some(settings_value) = settings_opt {
    match settings::verify_s3_settings(&settings_value).await {
      Ok(_) => {
        payload.configured = true;
      }
      Err(e) => {
        payload.configured = false;
        payload.connection_error = Some(format!("无法连接 MinIO：{}", e));
      }
    }
  }

  Ok(Json(payload))
}

async fn save_s3_settings(
  State(pool): State<SqlitePool>,
  Json(payload): Json<S3SettingsPayload>,
) -> Result<StatusCode, Problem> {
  let settings: settings::S3Settings = payload.into();
  settings::save_s3_settings(&pool, &settings)
    .await
    .map_err(AppError::Settings)?;
  Ok(StatusCode::NO_CONTENT)
}

/// 列出所有 S3 Profiles
async fn list_profiles(State(pool): State<SqlitePool>) -> Result<Json<S3ProfileListResponse>, Problem> {
  let profiles = settings::list_s3_profiles(&pool)
    .await
    .map_err(AppError::Settings)?;
  
  let payload_list: Vec<S3ProfilePayload> = profiles.into_iter().map(Into::into).collect();
  
  Ok(Json(S3ProfileListResponse {
    profiles: payload_list,
  }))
}

/// 保存或更新 S3 Profile
async fn save_profile(
  State(pool): State<SqlitePool>,
  Json(payload): Json<S3ProfilePayload>,
) -> Result<StatusCode, Problem> {
  let profile: settings::S3Profile = payload.into();
  settings::save_s3_profile(&pool, &profile)
    .await
    .map_err(AppError::Settings)?;
  Ok(StatusCode::NO_CONTENT)
}

/// 删除 S3 Profile
async fn delete_profile(
  State(pool): State<SqlitePool>,
  Path(name): Path<String>,
) -> Result<StatusCode, Problem> {
  settings::delete_s3_profile(&pool, &name)
    .await
    .map_err(AppError::Settings)?;
  Ok(StatusCode::NO_CONTENT)
}


// NL → Q 端点，实现将自然语言转换为查询字符串
async fn nl2q(Json(body): Json<crate::service::nl2q::NLBody>) -> Result<Json<NL2QOut>, Problem> {
  log::info!("NL2Q API请求: {}", body.nl);

  let start = std::time::Instant::now();
  let q = crate::service::nl2q::call_ollama(&body.nl).await.map_err(|e| {
    log::error!("NL2Q API失败: {}", e);
    problemdetails::new(StatusCode::BAD_GATEWAY)
      .with_title("AI 生成失败")
      .with_detail(e.to_string())
  })?;

  log::info!("NL2Q API成功: {} -> '{}', 耗时: {:?}", body.nl, q, start.elapsed());
  Ok(Json(NL2QOut { q }))
}



// ============================================================================
// 搜索（多存储源并行搜索）
// ============================================================================

/// 获取需要搜索的存储源配置列表
///
/// 从数据库加载所有 S3 Profiles，并根据查询中的日期范围生成多个 tar.gz 文件配置
///
/// TODO: 后续扩展：
/// 1. 支持按权限过滤（不同用户看到不同的存储源）
/// 2. 支持按标签/分组过滤（例如 "production" 标签的所有存储源）
/// 3. 支持动态启用/禁用某些存储源
/// 4. 支持 Agent 存储源配置
/// 5. 支持本地文件系统存储源配置
async fn get_storage_source_configs(
  pool: &SqlitePool,
  query: &str,
) -> Result<(Vec<crate::storage::factory::SourceConfig>, String), AppError> {
  use crate::storage::factory::SourceConfig;

  // 从数据库加载所有 S3 Profiles
  let profiles = settings::list_s3_profiles(pool)
    .await
    .map_err(|e| {
      log::error!("加载 S3 Profiles 失败: {:?}", e);
      e
    })?;

  log::info!("从数据库加载到 {} 个 S3 Profile(s)", profiles.len());

  // 解析日期计划，获取日期区间和清理后的查询（无论是否有 profiles 都需要清理查询）
  let base_dir = "/unused/for/s3"; // 仅为复用 derive_plan 获取日期区间
  let buckets = ["20", "21", "22", "23"];
  let plan = derive_plan(base_dir, &buckets, query);
  
  log::info!(
    "[Search] 日期范围解析: start={}, end={}, 原始查询='{}', 清理后查询='{}'",
    plan.range.start,
    plan.range.end,
    query,
    plan.cleaned_query
  );

  // 如果没有 profiles，直接返回空配置和清理后的查询
  if profiles.is_empty() {
    return Ok((Vec::new(), plan.cleaned_query));
  }

  let mut configs: Vec<SourceConfig> = Vec::new();

  // 为每个 Profile 生成多个 tar.gz 文件配置
  for profile in profiles {
    log::debug!(
      "为 Profile '{}' 生成存储源配置 (endpoint={}, bucket={})",
      profile.profile_name,
      profile.endpoint,
      profile.bucket
    );

    // 遍历日期范围
    let mut d = plan.range.start;
    while d <= plan.range.end {
      let dp1 = d + Duration::days(1);
      let y = dp1.year();
      let m = dp1.month();
      let day = dp1.day();
      let yyyymm = format!("{:04}{:02}", y, m);
      let yyyymmdd = format!("{:04}{:02}{:02}", y, m, day);
      let file_name = format!("{:04}-{:02}-{:02}", d.year(), d.month(), d.day());

      // 为每个 bucket 生成一个 S3 对象键
      for b in buckets {
        let key = format!(
          "bbip/{}/{}/{}/BBIP_{}_APPLOG_{}.tar.gz",
          y, yyyymm, yyyymmdd, b, file_name
        );

        configs.push(SourceConfig::S3 {
          profile: profile.profile_name.clone(),
          bucket: Some(profile.bucket.clone()),
          prefix: None,
          pattern: None,
          key: Some(key.clone()),
        });

        log::debug!(
          "添加 S3 存储源: profile={}, key={}",
          profile.profile_name,
          key
        );
      }

      d += Duration::days(1);
    }
  }

  log::info!(
    "[Search] 共生成 {} 个存储源配置",
    configs.len()
  );

  // TODO: 后续可以在这里添加 Agent 存储源和本地文件系统存储源
  // 例如：
  // configs.push(SourceConfig::Agent {
  //   endpoint: "http://agent1.example.com:8090".to_string(),
  // });

  // 返回存储源配置和清理后的查询（移除了 dt:/fdt:/tdt: 等日期限定符）
  Ok((configs, plan.cleaned_query))
}

/// 搜索处理函数（多存储源并行搜索）
async fn stream_search(
  State(pool): State<SqlitePool>,
  Json(body): Json<SearchBody>,
) -> Result<HttpResponse<Body>, Problem> {
  log::info!("[Search] 开始搜索: q={}", body.q);

  let cap = stream_channel_capacity();
  let (tx, rx) = mpsc::channel::<Result<bytes::Bytes, std::io::Error>>(cap);
  log::debug!("profiling: [Search] 建立响应通道，容量={}", cap);

  // 分层限流——IO 并发与 CPU 并发
  let io_sem = Arc::new(tokio::sync::Semaphore::new(s3_max_concurrency()));
  let cpu_max = cpu_max_concurrency();
  let cpu_sem = Arc::new(tokio::sync::Semaphore::new(cpu_max));

  // 自适应护栏 - 统计与控制器
  struct SearchStats {
    produced: Arc<AtomicU64>,
    source_errors: Arc<AtomicU64>,
  }
  impl SearchStats {
    fn new() -> Self {
      Self {
        produced: Arc::new(AtomicU64::new(0)),
        source_errors: Arc::new(AtomicU64::new(0)),
      }
    }
  }
  let stats = Arc::new(SearchStats::new());
  struct CpuController {
    max: usize,
    target: usize,
    held: Vec<tokio::sync::OwnedSemaphorePermit>,
  }
  impl CpuController {
    fn current_effective(&self) -> usize {
      self.max.saturating_sub(self.held.len())
    }
  }
  let cpu_ctrl = Arc::new(tokio::sync::Mutex::new(CpuController {
    max: cpu_max,
    target: cpu_max.min(2),
    held: Vec::new(),
  }));

  // 后台调节任务（每 3s 调整一次，AIMD 策略）
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
        let err = stats_c.source_errors.load(Ordering::Relaxed);
        let dprod = prod.saturating_sub(prev_prod);
        let derr = err.saturating_sub(prev_err);
        prev_prod = prod;
        prev_err = err;
        let denom = (dprod + derr) as f64;
        let err_rate = if denom > 0.0 { derr as f64 / denom } else { 0.0 };
        let tp = dprod as f64 / 3.0; // 条/秒

        let mut ctrl = cpu_ctrl_c.lock().await;
        let cur_eff = ctrl.current_effective();
        // 决策：高错误率则乘性减小；否则若吐吐不下降则加一
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
          for _ in 0..release {
            let _ = ctrl.held.pop();
          } // drop -> 释放许可
        }
        log::debug!(
          "adaptive: [Search] cpu target={} effective={} err_rate={:.3}% tp={:.2}/s",
          ctrl.target,
          ctrl.current_effective(),
          err_rate * 100.0,
          tp
        );
      }
    });
  }

  let overall_start = std::time::Instant::now();

  // 1. 获取存储源配置列表（同时获取清理后的查询）
  let (source_configs, cleaned_query) = match get_storage_source_configs(&pool, &body.q).await {
    Ok((configs, cleaned)) => (configs, cleaned),
    Err(e) => {
      log::error!("[Search] 获取存储源配置失败: {:?}", e);
      drop(tx);
      let body = Body::from_stream(ReceiverStream::new(rx));
      return Ok(
        HttpResponse::builder()
          .status(200)
          .header(
            CONTENT_TYPE,
            HeaderValue::from_static("application/x-ndjson; charset=utf-8"),
          )
          .body(body)
          .unwrap(),
      );
    }
  };
  log::info!(
    "[Search] 获取到 {} 个存储源配置",
    source_configs.len()
  );

  if source_configs.is_empty() {
    log::warn!("[Search] 没有可用的存储源配置");
    drop(tx);
    let body = Body::from_stream(ReceiverStream::new(rx));
    return Ok(
      HttpResponse::builder()
        .status(200)
        .header(
          CONTENT_TYPE,
          HeaderValue::from_static("application/x-ndjson; charset=utf-8"),
        )
        .body(body)
        .unwrap(),
    );
  }

  // 2. 使用工厂创建存储源
  let factory = crate::storage::factory::StorageFactory::new(pool.clone());
  let (sources, errors) = factory.create_sources(source_configs.clone()).await;

  if !errors.is_empty() {
    log::warn!(
      "[Search] {} 个存储源创建失败: {:?}",
      errors.len(),
      errors
    );
  }

  if sources.is_empty() {
    log::error!("[Search] 所有存储源创建失败，无法进行搜索");
    drop(tx);
    let body = Body::from_stream(ReceiverStream::new(rx));
    return Ok(
      HttpResponse::builder()
        .status(200)
        .header(
          CONTENT_TYPE,
          HeaderValue::from_static("application/x-ndjson; charset=utf-8"),
        )
        .body(body)
        .unwrap(),
    );
  }

  log::info!(
    "[Search] 成功创建 {} 个存储源",
    sources.len()
  );

  // 3. 解析查询并准备搜索参数
  let ctx = body.context.unwrap_or(3);
  let parse_start = std::time::Instant::now();
  let spec = crate::query::Query::parse_github_like(&cleaned_query)
    .map_err(|e| Problem::from(AppError::QueryParse(e)))?;
  let parse_dur = parse_start.elapsed();
  let highlights = spec.highlights.clone();
  let sid = new_sid();
  simple_cache().put_keywords(&sid, highlights.clone()).await;
  log::debug!(
    "profiling: [Search] 查询解析完成，ctx={}, 耗时={:?}",
    ctx,
    parse_dur
  );

  log::info!(
    "[Search] 开始并行搜索: 原始query={}, 清理后query={}, context={}, sid={}, sources={}",
    body.q, cleaned_query, ctx, sid, sources.len()
  );

  // 4. 为每个存储源启动搜索任务（带并发控制）
  let spec = Arc::new(spec);
  for (idx, (source, config)) in sources.into_iter().zip(source_configs.iter()).enumerate() {
    let source_name = match &source {
      crate::storage::StorageSource::Data(ds) => ds.source_type(),
      crate::storage::StorageSource::Service(ss) => ss.service_type(),
    };

    let tx_clone = tx.clone();
    let spec_clone = spec.clone();
    let highlights_clone = highlights.clone();
    let sid_clone = sid.clone();
    let io_sem_clone = io_sem.clone();
    let cpu_sem_clone = cpu_sem.clone();
    let stats_clone = stats.clone();
    let config_clone = config.clone();

    tokio::spawn(async move {
      let task_start = std::time::Instant::now();

      log::debug!(
        "profiling: [Search] 任务开始排队 source_idx={} name={}, io_avail={}, cpu_avail={}",
        idx,
        source_name,
        io_sem_clone.available_permits(),
        cpu_sem_clone.available_permits()
      );

      // 获取 IO 并发许可
      let io_wait_start = std::time::Instant::now();
      let _io_permit = match io_sem_clone.acquire_owned().await {
        Ok(p) => p,
        Err(_) => {
          log::warn!(
            "profiling: [Search] 获取 IO 许可失败，跳过 source_idx={}",
            idx
          );
          return;
        }
      };
      let io_wait_time = io_wait_start.elapsed();

      log::debug!(
        "profiling: [Search] 获得 IO 许可 source_idx={}, 等待={:.3}s",
        idx,
        io_wait_time.as_secs_f64()
      );

      // 根据存储源类型调用不同的搜索方法
      let search_result = match source {
        crate::storage::StorageSource::Data(data_source) => {
          // DataSource: Server 端执行搜索
          search_data_source_with_concurrency(
            data_source,
            config_clone,
            spec_clone,
            ctx,
            tx_clone,
            sid_clone,
            highlights_clone,
            cpu_sem_clone,
            Arc::clone(&stats_clone.produced),
            idx,
          )
          .await
        }
        crate::storage::StorageSource::Service(_search_service) => {
          // SearchService: 远程执行搜索
          // TODO: 实现 SearchService 支持
          log::warn!("[Search] SearchService 尚未实现，跳过 source_idx={}", idx);
          Ok(0)
        }
      };

      let total_time = task_start.elapsed();
      match search_result {
        Ok(count) => {
          log::info!(
            "profiling: [Search] 任务完成 source_idx={} name={}, 结果数={}, 总耗时={:.3}s, io_wait={:.3}s",
            idx,
            source_name,
            count,
            total_time.as_secs_f64(),
            io_wait_time.as_secs_f64()
          );
        }
        Err(e) => {
          log::error!(
            "profiling: [Search] 任务失败 source_idx={} name={}, error={}, 耗时={:.3}s",
            idx,
            source_name,
            e,
            total_time.as_secs_f64()
          );
          stats_clone.source_errors.fetch_add(1, Ordering::Relaxed);
        }
      }
    });
  }

  log::info!(
    "[Search] 搜索任务已启动，耗时={:?}",
    overall_start.elapsed()
  );

  // 删除发送端，让任务完成后自动关闭
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
        "X-Logseek-SID",
        HeaderValue::from_str(&sid).unwrap_or(HeaderValue::from_static("")),
      )
      .body(body)
      .unwrap(),
  )
}
/// 带并发控制的 DataSource 搜索
async fn search_data_source_with_concurrency(
  data_source: Arc<dyn crate::storage::DataSource>,
  source_config: crate::storage::factory::SourceConfig,
  spec: Arc<crate::query::Query>,
  context_lines: usize,
  tx: mpsc::Sender<Result<bytes::Bytes, std::io::Error>>,
  sid: String,
  highlights: Vec<String>,
  cpu_sem: Arc<tokio::sync::Semaphore>,
  stats_produced: Arc<AtomicU64>,
  source_idx: usize,
) -> Result<usize, String> {
  use crate::service::search::Search;
  use futures::StreamExt;

  let source_type = data_source.source_type();

  // 获取文件列表
  let list_start = std::time::Instant::now();
  let mut files = data_source
    .list_files()
    .await
    .map_err(|e| format!("文件列举失败: {}", e))?;
  let list_time = list_start.elapsed();

  log::debug!(
    "profiling: [Search] source_idx={} 文件列举完成，耗时={:.3}s",
    source_idx,
    list_time.as_secs_f64()
  );

  let mut file_count = 0;
  let mut result_count = 0;

  // 处理每个文件
  while let Some(entry_result) = files.next().await {
    let entry = match entry_result {
      Ok(e) => e,
      Err(e) => {
        log::warn!(
          "profiling: [Search] source_idx={} 文件条目读取失败: {}",
          source_idx,
          e
        );
        continue;
      }
    };

    file_count += 1;

    // 检查下游是否关闭
    if tx.is_closed() {
      log::debug!(
        "profiling: [Search] source_idx={} 下游通道关闭，提前结束",
        source_idx
      );
      break;
    }

    // 获取 CPU 许可（限制解压/搜索并发）
    let cpu_wait_start = std::time::Instant::now();
    let _cpu_permit = match cpu_sem.clone().acquire_owned().await {
      Ok(p) => p,
      Err(_) => {
        log::warn!(
          "profiling: [Search] source_idx={} 获取 CPU 许可失败",
          source_idx
        );
        break;
      }
    };
    let cpu_wait_time = cpu_wait_start.elapsed();

    // 打开文件
    let open_start = std::time::Instant::now();
    let reader = match data_source.open_file(&entry).await {
      Ok(r) => r,
      Err(e) => {
        log::warn!(
          "profiling: [Search] source_idx={} 打开文件失败 path={}: {}",
          source_idx,
          entry.path,
          e
        );
        continue;
      }
    };
    let open_time = open_start.elapsed();

    // 执行搜索（根据文件类型选择处理方式）
    let search_start = std::time::Instant::now();
    let is_targz = entry.path.ends_with(".tar.gz") || entry.path.ends_with(".tgz");

    let result = if is_targz {
      // tar.gz 文件：使用 Search trait
      match reader.search(&spec, context_lines).await {
        Ok(mut result_rx) => {
          let mut count = 0;
          while let Some(result) = result_rx.recv().await {
            if tx.is_closed() {
              break;
            }

            // 构造完整的 FileUrl
            use crate::domain::file_url::{FileUrl, TarCompression};
            let base_url = match &source_config {
              crate::storage::factory::SourceConfig::Local { path, .. } => {
                FileUrl::local(path)
              }
              crate::storage::factory::SourceConfig::S3 { profile, bucket, key, .. } => {
                let bucket_name = bucket.as_deref().unwrap_or("unknown");
                if let Some(k) = key {
                  FileUrl::s3_with_profile(profile, bucket_name, k)
                } else {
                  FileUrl::s3_with_profile(profile, bucket_name, &entry.path)
                }
              }
              _ => {
                // 对于其他类型，使用简化的 path
                FileUrl::local(&entry.path)
              }
            };

            let file_url = match FileUrl::tar_entry(TarCompression::Gzip, base_url, &result.path) {
              Ok(url) => url,
              Err(e) => {
                log::warn!(
                  "profiling: [Search] source_idx={} 构造 FileUrl 失败 entry={}: {:?}",
                  source_idx,
                  entry.path,
                  e
                );
                continue;
              }
            };
            let file_id = file_url.to_string();

            // 缓存结果
            simple_cache().put_lines(&sid, &file_url, result.lines.clone()).await;
            
            // 渲染 JSON
            let json_obj = render_json_chunks(
              &file_id,
              result.merged.clone(),
              result.lines.clone(),
              &highlights,
            );

            match serde_json::to_vec(&json_obj) {
              Ok(mut v) => {
                v.push(b'\n');
                if tx.send(Ok(bytes::Bytes::from(v))).await.is_err() {
                  break;
                }
                count += 1;
                stats_produced.fetch_add(1, Ordering::Relaxed);
              }
              Err(e) => {
                log::warn!(
                  "profiling: [Search] source_idx={} 序列化失败: {}",
                  source_idx,
                  e
                );
              }
            }
          }
          count
        }
        Err(e) => {
          log::warn!(
            "profiling: [Search] source_idx={} tar.gz 搜索失败 path={}: {}",
            source_idx,
            entry.path,
            e
          );
          0
        }
      }
    } else {
      // 普通文本文件：使用 SearchProcessor
      use crate::service::search::SearchProcessor;
      let processor = SearchProcessor::new(spec.clone(), context_lines);
      
      let mut reader = reader;
      match processor.process_content(entry.path.clone(), &mut reader).await {
        Ok(Some(result)) => {
          // 构造 FileUrl
          use crate::domain::file_url::FileUrl;
          let file_url = match &source_config {
            crate::storage::factory::SourceConfig::Local { .. } => {
              FileUrl::local(&entry.path)
            }
            crate::storage::factory::SourceConfig::S3 { profile, bucket, key, .. } => {
              let bucket_name = bucket.as_deref().unwrap_or("unknown");
              if let Some(k) = key {
                FileUrl::s3_with_profile(profile, bucket_name, k)
              } else {
                FileUrl::s3_with_profile(profile, bucket_name, &entry.path)
              }
            }
            _ => FileUrl::local(&entry.path),
          };
          let file_id = file_url.to_string();
          
          // 缓存结果
          simple_cache().put_lines(&sid, &file_url, result.lines.clone()).await;
          
          let json_obj = render_json_chunks(
            &file_id,
            result.merged.clone(),
            result.lines.clone(),
            &highlights,
          );

          match serde_json::to_vec(&json_obj) {
            Ok(mut v) => {
              v.push(b'\n');
              if tx.send(Ok(bytes::Bytes::from(v))).await.is_ok() {
                stats_produced.fetch_add(1, Ordering::Relaxed);
                1
              } else {
                0
              }
            }
            Err(e) => {
              log::warn!(
                "profiling: [Search] source_idx={} 序列化失败: {}",
                source_idx,
                e
              );
              0
            }
          }
        }
        Ok(None) => 0,
        Err(e) => {
          log::warn!(
            "profiling: [Search] source_idx={} 搜索失败 path={}: {}",
            source_idx,
            entry.path,
            e
          );
          0
        }
      }
    };

    let search_time = search_start.elapsed();
    result_count += result;

    if result > 0 {
      log::info!(
        "profiling: [Search] source_idx={} 文件处理完成 path={}, 结果={}, 耗时={:.3}s [cpu_wait={:.3}s, open={:.3}s, search={:.3}s]",
        source_idx,
        entry.path,
        result,
        (cpu_wait_time + open_time + search_time).as_secs_f64(),
        cpu_wait_time.as_secs_f64(),
        open_time.as_secs_f64(),
        search_time.as_secs_f64()
      );
    }
  }

  log::info!(
    "profiling: [Search] source_idx={} type={} 搜索完成: 文件数={}, 结果数={}",
    source_idx,
    source_type,
    file_count,
    result_count
  );

  Ok(result_count)
}
