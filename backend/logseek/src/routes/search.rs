//! 搜索路由
//!
//! 处理 /search.ndjson 端点，实现多存储源并行搜索

use crate::agent::{AgentClient, SearchOptions, SearchService};
use crate::api::models::{AppError, SearchBody};
use crate::repository::cache::{cache as simple_cache, new_sid};
use crate::repository::settings;
use crate::service::entry_stream::{EntryStreamFactory, EntryStreamProcessor};
use crate::service::search::SearchProcessor;
use crate::utils::bbip_service::derive_plan;
use crate::utils::renderer::render_json_chunks;
use axum::{
  body::Body,
  extract::{Json, State},
  http::{HeaderValue, Response as HttpResponse, header::CONTENT_TYPE},
};
use chrono::{Datelike, Duration};
use futures::StreamExt;
use opsbox_core::SqlitePool;
use problemdetails::Problem;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;

use super::helpers::{s3_max_concurrency, stream_channel_capacity};

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
pub async fn get_storage_source_configs(
  pool: &SqlitePool,
  query: &str,
) -> Result<(Vec<crate::domain::config::SourceConfig>, String), AppError> {
  use crate::domain::config::SourceConfig;

  // 从数据库加载所有 S3 Profiles
  let profiles = settings::list_s3_profiles(pool).await.map_err(|e| {
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

        log::debug!("添加 S3 存储源: profile={}, key={}", profile.profile_name, key);
      }

      d += Duration::days(1);
    }
  }

  log::info!("[Search] 共生成 {} 个存储源配置", configs.len());

  // TODO: 后续可以在这里添加 Agent 存储源和本地文件系统存储源
  // 例如：
  // configs.push(SourceConfig::Agent {
  //   endpoint: "http://agent1.example.com:8090".to_string(),
  // });

  // 返回存储源配置和清理后的查询（移除了 dt:/fdt:/tdt: 等日期限定符）
  Ok((configs, plan.cleaned_query))
}

/// 搜索处理函数（多存储源并行搜索）
fn respond_empty_stream(
  tx: mpsc::Sender<Result<bytes::Bytes, std::io::Error>>,
  rx: mpsc::Receiver<Result<bytes::Bytes, std::io::Error>>,
) -> Result<HttpResponse<Body>, Problem> {
  drop(tx);
  let body = Body::from_stream(ReceiverStream::new(rx));
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

pub async fn stream_search(
  State(pool): State<SqlitePool>,
  Json(body): Json<SearchBody>,
) -> Result<HttpResponse<Body>, Problem> {
  log::info!("[Search] 开始搜索: q={}", body.q);

  let cap = stream_channel_capacity();
  let (tx, rx) = mpsc::channel::<Result<bytes::Bytes, std::io::Error>>(cap);
  log::debug!("profiling: [Search] 建立响应通道，容量={}", cap);

  // 分层限流——IO 并发
  let io_sem = Arc::new(tokio::sync::Semaphore::new(s3_max_concurrency()));

  // 已简化：仅使用固定的 IO 并发上限

  let overall_start = std::time::Instant::now();

  // 1. 获取存储源配置列表（同时获取清理后的查询）
  let (source_configs, cleaned_query) = match get_storage_source_configs(&pool, &body.q).await {
    Ok((configs, cleaned)) => (configs, cleaned),
    Err(e) => {
      log::error!("[Search] 获取存储源配置失败: {:?}", e);
      return respond_empty_stream(tx, rx);
    }
  };
  log::info!("[Search] 获取到 {} 个存储源配置", source_configs.len());

  if source_configs.is_empty() {
    log::warn!("[Search] 没有可用的存储源配置");
    return respond_empty_stream(tx, rx);
  }

  // 3. 解析查询并准备搜索参数
  let ctx = body.context.unwrap_or(3);
  let parse_start = std::time::Instant::now();
  let spec =
    crate::query::Query::parse_github_like(&cleaned_query).map_err(|e| Problem::from(AppError::QueryParse(e)))?;
  let parse_dur = parse_start.elapsed();
  let highlights = spec.highlights.clone();
  let sid = new_sid();
  simple_cache().put_keywords(&sid, highlights.clone()).await;
  log::debug!("profiling: [Search] 查询解析完成，ctx={}, 耗时={:?}", ctx, parse_dur);

  log::info!(
    "[Search] 开始并行搜索: 原始query={}, 清理后query={}, context={}, sid={}, sources={}",
    body.q,
    cleaned_query,
    ctx,
    sid,
    source_configs.len()
  );

  // 4. 为每个存储源启动搜索任务（带并发控制，基于 EntryStreamFactory；Agent 走 SearchService）
  let spec = Arc::new(spec);
  for (idx, config) in source_configs.iter().enumerate() {
    let tx_clone = tx.clone();
    let spec_clone = spec.clone();
    let highlights_clone = highlights.clone();
    let sid_clone = sid.clone();
    let io_sem_clone = io_sem.clone();
    let pool_clone = pool.clone();
    let config_clone = config.clone();
    let cleaned_query_clone = cleaned_query.clone();

    tokio::spawn(async move {
      let task_start = std::time::Instant::now();

      log::debug!(
        "profiling: [Search] 任务开始排队 source_idx={} io_avail={}",
        idx,
        io_sem_clone.available_permits()
      );

      // 获取 IO 并发许可
      let io_wait_start = std::time::Instant::now();
      let _io_permit = match io_sem_clone.acquire_owned().await {
        Ok(p) => p,
        Err(_) => {
          log::warn!("profiling: [Search] 获取 IO 许可失败，跳过 source_idx={}", idx);
          return;
        }
      };
      let io_wait_time = io_wait_start.elapsed();

      log::debug!(
        "profiling: [Search] 获得 IO 许可 source_idx={}, 等待={:.3}s",
        idx,
        io_wait_time.as_secs_f64()
      );

      // 对 Agent 来源走远程 SearchService；其它来源走 EntryStream 路径
      if let crate::domain::config::SourceConfig::Agent { endpoint } = &config_clone {
        // 直接构造 AgentClient（使用 endpoint 作为 agent_id）
        if !endpoint.starts_with("http://") && !endpoint.starts_with("https://") {
          log::error!(
            "[Search] 非法的 Agent endpoint，需以 http:// 或 https:// 开头: {}",
            endpoint
          );
          return;
        }
        let client = AgentClient::new(endpoint.clone(), endpoint.clone());

        // 可选健康检查
        if !client.health_check().await {
          log::error!("[Search] Agent 健康检查失败，跳过: {}", endpoint);
          return;
        }

        // 调用远程搜索
        let mut stream = match client.search(&cleaned_query_clone, ctx, SearchOptions::default()).await {
          Ok(st) => st,
          Err(e) => {
            log::error!(
              "[Search] 调用 Agent 搜索失败 source_idx={} endpoint={} err={}",
              idx,
              endpoint,
              e
            );
            return;
          }
        };

        // 直接消费结果流并发送 NDJSON
        let tx_bytes = tx_clone.clone();
        let highlights_s = highlights_clone.clone();
        let sid_for_cache = sid_clone.clone();
        let endpoint_clone = endpoint.clone();
        let service_task = tokio::spawn(async move {
          use crate::domain::file_url::FileUrl;
          while let Some(item) = stream.next().await {
            let Ok(res) = item else {
              log::warn!("profiling: [Search] Agent 返回错误条目，已跳过");
              continue;
            };
            if tx_bytes.is_closed() {
              break;
            }
            // 构造 agent://<endpoint>/<path>
            let file_url = FileUrl::agent(endpoint_clone.clone(), &res.path);
            let file_id = file_url.to_string();

            // 缓存结果
            simple_cache()
              .put_lines(&sid_for_cache, &file_url, res.lines.clone())
              .await;

            // 渲染 JSON 并发送
            let json_obj = render_json_chunks(&file_id, res.merged.clone(), res.lines.clone(), &highlights_s);
            match serde_json::to_vec(&json_obj) {
              Ok(mut v) => {
                v.push(b'\n');
                if tx_bytes.send(Ok(bytes::Bytes::from(v))).await.is_err() {
                  break;
                }
              }
              Err(e) => {
                log::warn!("profiling: [Search] 序列化失败: {}", e);
              }
            }
          }
        });

        let total_time = task_start.elapsed();
        let _ = service_task.await; // 等待发送结束
        log::info!(
          "profiling: [Search] 任务完成 source_idx={} name=AgentService 总耗时={:.3}s, io_wait={:.3}s",
          idx,
          total_time.as_secs_f64(),
          io_wait_time.as_secs_f64()
        );
        return; // 结束该任务
      }

      // 基于 EntryStreamFactory 创建条目流
      let factory = EntryStreamFactory::new(pool_clone);
      let mut estream = match factory.create_stream(config_clone.clone()).await {
        Ok(s) => s,
        Err(e) => {
          log::error!("[Search] 创建条目流失败 source_idx={} err={}", idx, e);
          return;
        }
      };

      // 准备搜索处理器与条目流处理器
      let search_proc = Arc::new(SearchProcessor::new(spec_clone, ctx));
      let mut processor = EntryStreamProcessor::new(search_proc);

      // 中转通道：SearchResult -> NDJSON 字节
      let (sr_tx, mut sr_rx) = mpsc::channel::<crate::service::search::SearchResult>(32);

      // 后台发送 NDJSON
      let tx_bytes = tx_clone.clone();
      let highlights_s = highlights_clone.clone();
      let cfg_for_url = config_clone.clone();
      let sid_for_cache = sid_clone.clone();
      let sender_task = tokio::spawn(async move {
        use crate::domain::file_url::{FileUrl, TarCompression};
        while let Some(res) = sr_rx.recv().await {
          if tx_bytes.is_closed() {
            break;
          }

          // 构造 FileUrl（S3 tar.gz、Local、Agent 区分）
          let (file_url, file_id) = match &cfg_for_url {
            crate::domain::config::SourceConfig::S3 {
              profile, bucket, key, ..
            } => {
              let bucket_name = bucket.as_deref().unwrap_or("unknown");
              let base = if let Some(k) = key {
                FileUrl::s3_with_profile(profile, bucket_name, k)
              } else {
                FileUrl::s3_with_profile(profile, bucket_name, &res.path)
              };
              match FileUrl::tar_entry(TarCompression::Gzip, base, &res.path) {
                Ok(url) => {
                  let id = url.to_string();
                  (url, id)
                }
                Err(e) => {
                  log::warn!("profiling: [Search] 构造 FileUrl 失败: {}", e);
                  continue;
                }
              }
            }
            crate::domain::config::SourceConfig::Local { path, .. } => {
              // 使用 dir+file:///root:relative 的形式编码来源根目录与相对路径
              let base = FileUrl::local(path);
              match FileUrl::dir_entry(base, &res.path) {
                Ok(url) => {
                  let id = url.to_string();
                  (url, id)
                }
                Err(_) => {
                  // 回退：使用绝对路径
                  let joined = std::path::Path::new(path).join(&res.path);
                  let url = FileUrl::local(joined.to_string_lossy().to_string());
                  let id = url.to_string();
                  (url, id)
                }
              }
            }
            crate::domain::config::SourceConfig::Agent { endpoint } => {
              let url = FileUrl::agent(endpoint, &res.path);
              let id = url.to_string();
              (url, id)
            }
          };

          // 缓存结果
          simple_cache()
            .put_lines(&sid_for_cache, &file_url, res.lines.clone())
            .await;

          // 渲染 JSON 并发送
          let json_obj = render_json_chunks(&file_id, res.merged.clone(), res.lines.clone(), &highlights_s);
          match serde_json::to_vec(&json_obj) {
            Ok(mut v) => {
              v.push(b'\n');
              if tx_bytes.send(Ok(bytes::Bytes::from(v))).await.is_err() {
                break;
              }
            }
            Err(e) => {
              log::warn!("profiling: [Search] 序列化失败: {}", e);
            }
          }
        }
      });

      // 处理条目流
      if let Err(e) = processor.process_stream(&mut *estream, sr_tx).await {
        log::error!("[Search] 处理条目流失败 source_idx={} err={}", idx, e);
      }

      let total_time = task_start.elapsed();
      let _ = sender_task.await; // 等待发送任务结束
      log::info!(
        "profiling: [Search] 任务完成 source_idx={} name=EntryStream 总耗时={:.3}s, io_wait={:.3}s",
        idx,
        total_time.as_secs_f64(),
        io_wait_time.as_secs_f64()
      );
    });
  }

  log::info!("[Search] 搜索任务已启动，耗时={:?}", overall_start.elapsed());

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
