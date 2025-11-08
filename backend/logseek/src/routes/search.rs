//! 搜索路由
//!
//! 处理 /search.ndjson 端点，实现多存储源并行搜索

use crate::agent::{AgentClient, SearchOptions, SearchScope, SearchService};
use crate::api::models::{AppError, SearchBody};
use crate::repository::cache::{cache as simple_cache, new_sid};
use crate::service::entry_stream::{EntryStreamFactory, EntryStreamProcessor};
use crate::service::search::SearchProcessor;
use crate::utils::renderer::render_json_chunks;
use axum::{
  body::Body,
  extract::{Json, State},
  http::{HeaderValue, Response as HttpResponse, header::CONTENT_TYPE},
};
use futures::StreamExt;
use log::debug;
use opsbox_core::SqlitePool;
use problemdetails::Problem;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;

use super::helpers::{s3_max_concurrency, stream_channel_capacity};

// ============================================================================
// 搜索（多存储源并行搜索）
// ============================================================================

/// 获取存储源配置列表（支持混合数据源）
///
/// 混合数据源策略：
/// - 当前日期（今天）：使用 Agent 存储源
/// - 历史日期（昨天及以前）：使用 S3 存储源
///
/// 当前实现：
/// 1. 从数据库加载所有 S3 Profiles
/// 2. 解析查询中的日期指令（dt:/fdt:/tdt:）
/// 3. 根据日期分割策略分配数据源：
///    - 当前日期范围 → Agent 配置
///    - 历史日期范围 → S3 配置
/// 4. 返回混合配置列表和清理后的查询
///
/// TODO: 后续扩展：
/// 1. 支持按权限过滤（不同用户看到不同的存储源）
/// 2. 支持按标签/分组过滤（例如 "production" 标签的所有存储源）
/// 3. 支持动态启用/禁用某些存储源
/// 4. 支持本地文件系统存储源配置
/// 5. 支持可配置的日期分割策略
pub async fn get_storage_source_configs(
  pool: &SqlitePool,
  query: &str,
) -> Result<(Vec<crate::domain::config::Source>, String), AppError> {
  // 从查询字符串中提取 app 限定词（形如 app:bbip / app:bbos），未指定则默认为 bbip
  // 同时移除该限定词以得到传入规划器的“清理前”查询（随后规划器还会继续清理日期指令等）
  let mut app: Option<String> = None;
  let mut tokens: Vec<&str> = Vec::new();
  for t in query.split_whitespace() {
    if let Some(rest) = t.strip_prefix("app:")
      && !rest.is_empty()
    {
      app = Some(rest.to_string());
      continue; // 跳过该限定词，不纳入后续查询
    }
    tokens.push(t);
  }
  let cleaned_before_plan = tokens.join(" ");

  // 通过 Starlark 调度：优先使用 app 对应脚本，不存在时回退到内置 bbip.star
  let plan = crate::domain::source_planner::plan_with_starlark(pool, app.as_deref(), &cleaned_before_plan).await?;
  Ok((plan.sources, plan.cleaned_query))
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
      if let crate::domain::config::Endpoint::Agent { agent_id, .. } = &config_clone.endpoint {
        // 使用 Agent ID 构造 AgentClient（标准格式）
        let client = match AgentClient::new_by_agent_id(agent_id.clone()).await {
          Ok(client) => client,
          Err(e) => {
            log::error!(
              "[Search] 无法创建 Agent 客户端 source_idx={} agent_id={} err={}",
              idx,
              agent_id,
              e
            );
            return;
          }
        };

        // 可选健康检查
        if !client.health_check().await {
          log::error!("[Search] Agent 健康检查失败，跳过: {}", agent_id);
          return;
        }

        // 从来源规划中读取 endpoint/target/filter，并转换为 Agent 的 SearchScope
        use crate::domain::config::{Endpoint, Target};
        let path_glob = config_clone.filter_glob.clone();
        let search_scope = match (&config_clone.endpoint, &config_clone.target) {
          (Endpoint::Agent { root, .. }, Target::Dir { path, recursive }) => {
            let joined = if path == "." {
              root.clone()
            } else {
              format!("{}/{}", root, path)
            };
            SearchScope::Directory {
              path: Some(joined),
              recursive: *recursive,
            }
          }
          (Endpoint::Agent { root, .. }, Target::Files { paths }) => {
            let ps = paths
              .iter()
              .map(|p| {
                if p.starts_with('/') {
                  p.clone()
                } else {
                  format!("{}/{}", root, p)
                }
              })
              .collect();
            SearchScope::Files { paths: ps }
          }
          (Endpoint::Agent { .. }, Target::Archive { .. }) => {
            log::warn!("[Search] Agent + archive 目前不支持，跳过 source_idx={}", idx);
            SearchScope::All
          }
          (Endpoint::Agent { root, .. }, Target::All) => SearchScope::Directory {
            path: Some(root.clone()),
            recursive: true,
          },
          _ => SearchScope::All,
        };
        let search_options = SearchOptions {
          scope: search_scope,
          path_filter: path_glob,
          ..Default::default()
        };

        // 调用远程搜索
        let mut stream = match client.search(&cleaned_query_clone, ctx, search_options).await {
          Ok(st) => st,
          Err(e) => {
            log::error!(
              "[Search] 调用 Agent 搜索失败 source_idx={} agent_id={} err={}",
              idx,
              agent_id,
              e
            );
            return;
          }
        };

        // 直接消费结果流并发送 NDJSON
        let tx_bytes = tx_clone.clone();
        let highlights_s = highlights_clone.clone();
        let sid_for_cache = sid_clone.clone();
        let agent_id_clone = agent_id.clone();
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
            // 构造 agent://<agent_id>/<path>
            let file_url = FileUrl::agent(agent_id_clone.clone(), &res.path);
            let file_id = file_url.to_string();

            // 缓存结果
            debug!(
              "🔍 Server缓存Agent结果: sid={}, file_url={}, lines_count={}",
              sid_for_cache,
              file_url,
              res.lines.len()
            );
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
        use crate::domain::file_url::build_file_url_for_result;
        while let Some(res) = sr_rx.recv().await {
          if tx_bytes.is_closed() {
            break;
          }

          // 构造 FileUrl（基于来源+相对路径）
          let (file_url, file_id) = match build_file_url_for_result(&cfg_for_url, &res.path) {
            Some((url, id)) => (url, id),
            None => {
              log::warn!("profiling: [Search] 无法构造 FileUrl, path={}", res.path);
              continue;
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

/// 根据今天分割日期范围
///
/// 返回：(当前日期范围, 历史日期范围)
/// - 当前日期范围：包含今天及以后的日期
/// - 历史日期范围：包含昨天及以前的日期
#[allow(dead_code)]
fn split_date_range_by_today(
  range: crate::domain::source_planner::DateRange,
  today: chrono::NaiveDate,
) -> (
  Option<crate::domain::source_planner::DateRange>,
  Option<crate::domain::source_planner::DateRange>,
) {
  use crate::domain::source_planner::DateRange;

  let yesterday = today - chrono::Duration::days(1);

  // 如果整个范围都在昨天及以前，全部作为历史日期
  if range.end <= yesterday {
    return (None, Some(range));
  }

  // 如果整个范围都在今天及以后，全部作为当前日期
  if range.start >= today {
    return (Some(range), None);
  }

  // 范围跨越今天，需要分割
  let historical_range = if range.start <= yesterday {
    Some(DateRange::new(range.start, yesterday))
  } else {
    None
  };

  let current_range = Some(DateRange::new(today, range.end));

  (current_range, historical_range)
}

/// 获取可用的 Agent 端点列表
///
/// 获取包含 app=bbipapp 标签的在线 Agent 端点
#[allow(dead_code)]
async fn get_agent_endpoints() -> Vec<String> {
  // 获取包含 app=bbipapp 标签的在线 Agent
  let _tags = [("app".to_string(), "bbipapp".to_string())];
  // let endpoints = agent_manager::get_online_agent_endpoints_by_tags(&_tags).await;
  let endpoints = agent_manager::get_online_agent_endpoints().await;

  if !endpoints.is_empty() {
    log::info!("找到 {} 个包含 app=bbipapp 标签的在线 Agent 端点", endpoints.len());
  } else {
    log::warn!("没有找到包含 app=bbipapp 标签的在线 Agent");
  }

  endpoints
}

#[cfg(test)]
mod tests {
  use crate::domain::source_planner::DateRange;

  use super::*;
  use chrono::NaiveDate;

  #[test]
  fn test_split_date_range_by_today() {
    let today = NaiveDate::from_ymd_opt(2024, 10, 12).unwrap();

    // 测试1: 整个范围都在昨天及以前
    let historical_range = DateRange::new(
      NaiveDate::from_ymd_opt(2024, 10, 10).unwrap(),
      NaiveDate::from_ymd_opt(2024, 10, 11).unwrap(),
    );
    let (current, historical) = split_date_range_by_today(historical_range, today);
    assert!(current.is_none());
    assert!(historical.is_some());
    assert_eq!(historical.unwrap(), historical_range);

    // 测试2: 整个范围都在今天及以后
    let future_range = DateRange::new(
      NaiveDate::from_ymd_opt(2024, 10, 12).unwrap(),
      NaiveDate::from_ymd_opt(2024, 10, 15).unwrap(),
    );
    let (current, historical) = split_date_range_by_today(future_range, today);
    assert!(current.is_some());
    assert!(historical.is_none());
    assert_eq!(current.unwrap(), future_range);

    // 测试3: 范围跨越今天
    let mixed_range = DateRange::new(
      NaiveDate::from_ymd_opt(2024, 10, 10).unwrap(),
      NaiveDate::from_ymd_opt(2024, 10, 15).unwrap(),
    );
    let (current, historical) = split_date_range_by_today(mixed_range, today);

    assert!(current.is_some());
    assert!(historical.is_some());

    let current_range = current.unwrap();
    let historical_range = historical.unwrap();

    // 当前日期范围应该是从今天开始
    assert_eq!(current_range.start, today);
    assert_eq!(current_range.end, NaiveDate::from_ymd_opt(2024, 10, 15).unwrap());

    // 历史日期范围应该是到昨天结束
    assert_eq!(historical_range.start, NaiveDate::from_ymd_opt(2024, 10, 10).unwrap());
    assert_eq!(historical_range.end, NaiveDate::from_ymd_opt(2024, 10, 11).unwrap());
  }

  #[tokio::test]
  async fn test_get_agent_endpoints() {
    // 测试获取包含 app=bbipapp 标签的 Agent 端点
    // 由于没有真实的 AgentManager 实例，这个测试会返回空列表
    let endpoints = get_agent_endpoints().await;
    // 在没有 AgentManager 实例的情况下，应该返回空列表
    assert_eq!(endpoints, Vec::<String>::new());
  }
}
