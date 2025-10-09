//! 搜索路由
//!
//! 处理 /search.ndjson 端点，实现多存储源并行搜索

use crate::api::models::{AppError, SearchBody};
use crate::repository::cache::{cache as simple_cache, new_sid};
use crate::repository::settings;
use crate::utils::bbip_service::derive_plan;
use crate::utils::renderer::render_json_chunks;
use crate::service::entry_stream::{EntryStreamFactory, EntryStreamProcessor};
use crate::service::search::SearchProcessor;
use axum::{
  body::Body,
  extract::{Json, State},
  http::{HeaderValue, Response as HttpResponse, header::CONTENT_TYPE},
};
use chrono::{Datelike, Duration};
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
) -> Result<(Vec<crate::storage::factory::SourceConfig>, String), AppError> {
  use crate::storage::factory::SourceConfig;

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

  // 4. 为每个存储源启动搜索任务（带并发控制，基于 EntryStreamFactory）
  let spec = Arc::new(spec);
  for (idx, config) in source_configs.iter().enumerate() {
    let tx_clone = tx.clone();
    let spec_clone = spec.clone();
    let highlights_clone = highlights.clone();
    let sid_clone = sid.clone();
    let io_sem_clone = io_sem.clone();
    let pool_clone = pool.clone();
    let config_clone = config.clone();

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

      // 基于 EntryStreamFactory 创建条目流
      let factory = EntryStreamFactory::new(pool_clone);
      let mut estream = match factory.from_source(config_clone.clone()).await {
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
      let (sr_tx, mut sr_rx) = tokio::sync::mpsc::channel::<crate::service::search::SearchResult>(32);

      // 后台发送 NDJSON
      let tx_bytes = tx_clone.clone();
      let highlights_s = highlights_clone.clone();
      let cfg_for_url = config_clone.clone();
      let sid_for_cache = sid_clone.clone();
      let sender_task = tokio::spawn(async move {
        use crate::domain::file_url::{FileUrl, TarCompression};
        while let Some(res) = sr_rx.recv().await {
          if tx_bytes.is_closed() { break; }

          // 构造 FileUrl（S3 tar.gz 与 Local 区分）
          let (file_url, file_id) = match &cfg_for_url {
            crate::storage::factory::SourceConfig::S3 { profile, bucket, key, .. } => {
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
            crate::storage::factory::SourceConfig::Local { path, .. } => {
              let joined = std::path::Path::new(path).join(&res.path);
              let url = FileUrl::local(joined.to_string_lossy().to_string());
              let id = url.to_string();
              (url, id)
            }
            _ => {
              let url = FileUrl::local(&res.path);
              let id = url.to_string();
              (url, id)
            }
          };

          // 缓存结果
          simple_cache().put_lines(&sid_for_cache, &file_url, res.lines.clone()).await;

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
/// 带并发控制的 DataSource 搜索
pub async fn search_data_source_with_concurrency(
  data_source: Arc<dyn crate::storage::DataSource>,
  source_config: crate::storage::factory::SourceConfig,
  spec: Arc<crate::query::Query>,
  context_lines: usize,
  tx: mpsc::Sender<Result<bytes::Bytes, std::io::Error>>,
  sid: String,
  highlights: Vec<String>,
  cpu_sem: Arc<tokio::sync::Semaphore>,
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
        log::warn!("profiling: [Search] source_idx={} 文件条目读取失败: {}", source_idx, e);
        continue;
      }
    };

    file_count += 1;

    // 检查下游是否关闭
    if tx.is_closed() {
      log::debug!("profiling: [Search] source_idx={} 下游通道关闭，提前结束", source_idx);
      break;
    }

    // 获取 CPU 许可（限制解压/搜索并发）
    let cpu_wait_start = std::time::Instant::now();
    let _cpu_permit = match cpu_sem.clone().acquire_owned().await {
      Ok(p) => p,
      Err(_) => {
        log::warn!("profiling: [Search] source_idx={} 获取 CPU 许可失败", source_idx);
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
              crate::storage::factory::SourceConfig::Local { path, .. } => FileUrl::local(path),
              crate::storage::factory::SourceConfig::S3 {
                profile, bucket, key, ..
              } => {
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
            let json_obj = render_json_chunks(&file_id, result.merged.clone(), result.lines.clone(), &highlights);

            match serde_json::to_vec(&json_obj) {
              Ok(mut v) => {
                v.push(b'\n');
                if tx.send(Ok(bytes::Bytes::from(v))).await.is_err() {
                  break;
                }
                count += 1;
              }
              Err(e) => {
                log::warn!("profiling: [Search] source_idx={} 序列化失败: {}", source_idx, e);
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
            crate::storage::factory::SourceConfig::Local { .. } => FileUrl::local(&entry.path),
            crate::storage::factory::SourceConfig::S3 {
              profile, bucket, key, ..
            } => {
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

          let json_obj = render_json_chunks(&file_id, result.merged.clone(), result.lines.clone(), &highlights);

          match serde_json::to_vec(&json_obj) {
            Ok(mut v) => {
              v.push(b'\n');
              if tx.send(Ok(bytes::Bytes::from(v))).await.is_ok() {
                1
              } else {
                0
              }
            }
            Err(e) => {
              log::warn!("profiling: [Search] source_idx={} 序列化失败: {}", source_idx, e);
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
