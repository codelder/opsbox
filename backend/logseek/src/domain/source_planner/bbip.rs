use super::{DateRange, PlanResult, SourcePlanner};
use crate::{api::models::AppError, domain::config::SourceConfig};
use async_trait::async_trait;
use opsbox_core::SqlitePool;

/// BBIP 应用的存储规划器
pub struct BbipPlanner;

// 轻量计划：只包含清理后的查询与日期范围（模块私有）
struct PlanLite {
  cleaned_query: String,
  range: DateRange,
}

impl BbipPlanner {
  // S3 分桶（用于 S3 Key 展开）
  const BUCKETS: &'static [&'static str] = &["20", "21", "22", "23"];

  /// 解析 q 中的日期指令，返回（清理后的 q，日期区间）
  fn parse_date_directives_from_query(&self, q_raw: &str, today: chrono::NaiveDate) -> (String, DateRange) {
    use chrono::{Datelike, NaiveDate};
    let mut dt_q: Option<String> = None;
    let mut fdt_q: Option<String> = None;
    let mut tdt_q: Option<String> = None;

    let tokens: Vec<&str> = q_raw.split_whitespace().collect();
    for t in &tokens {
      if let Some(rest) = t.strip_prefix("dt:") {
        if rest.len() == 8 && rest.chars().all(|c| c.is_ascii_digit()) {
          dt_q = Some(rest.to_string());
        }
      } else if let Some(rest) = t.strip_prefix("fdt:") {
        if rest.len() == 8 && rest.chars().all(|c| c.is_ascii_digit()) {
          fdt_q = Some(rest.to_string());
        }
      } else if let Some(rest) = t.strip_prefix("tdt:")
        && rest.len() == 8
        && rest.chars().all(|c| c.is_ascii_digit())
      {
        tdt_q = Some(rest.to_string());
      }
    }

    let today_str = format!("{:04}{:02}{:02}", today.year(), today.month(), today.day());

    let (start_yyyymmdd, end_yyyymmdd) = if let Some(d) = dt_q {
      (d.clone(), d)
    } else {
      match (fdt_q, tdt_q) {
        (Some(s), Some(e)) => (s, e),
        (Some(s), None) => {
          let e = s.clone();
          (s, e)
        }
        (None, Some(e)) => {
          let s = e.clone();
          (s, e)
        }
        (None, None) => (today_str.clone(), today_str.clone()),
      }
    };

    let parse_or_today = |s: &str| NaiveDate::parse_from_str(s, "%Y%m%d").unwrap_or(today);
    let range = DateRange::new(parse_or_today(&start_yyyymmdd), parse_or_today(&end_yyyymmdd));

    let cleaned = tokens
      .into_iter()
      .filter(|t| !(t.starts_with("dt:") || t.starts_with("fdt:") || t.starts_with("tdt:")))
      .collect::<Vec<_>>()
      .join(" ");

    (cleaned, range)
  }

  /// 封装 BBIP 的计划推导逻辑（隐藏具体依赖与参数）
  fn derive_plan(&self, query: &str) -> PlanLite {
    use chrono::Utc;
    use chrono_tz::Asia::Shanghai;
    let today = Utc::now().with_timezone(&Shanghai).date_naive();
    let (cleaned_query, range) = self.parse_date_directives_from_query(query, today);
    PlanLite { cleaned_query, range }
  }
}

#[async_trait]
impl SourcePlanner for BbipPlanner {
  fn app_id(&self) -> &'static str {
    "bbip"
  }

  async fn plan(&self, pool: &SqlitePool, query: &str) -> Result<PlanResult, AppError> {
    use chrono::{Datelike, Utc};
    use chrono_tz::Asia::Shanghai;

    // 1) 从数据库加载所有 S3 Profiles
    let profiles = crate::repository::settings::list_s3_profiles(pool).await.map_err(|e| {
      log::error!("加载 S3 Profiles 失败: {:?}", e);
      e
    })?;

    log::info!("从数据库加载到 {} 个 S3 Profile(s)", profiles.len());

    // 2) 解析日期计划，获取日期区间和清理后的查询
    let plan = self.derive_plan(query);

    log::info!(
      "[Search] 日期范围解析: start={}, end={}, 原始查询='{}', 清理后查询='{}'",
      plan.range.start,
      plan.range.end,
      query,
      plan.cleaned_query
    );

    let mut configs: Vec<SourceConfig> = Vec::new();
    // 使用北京时区计算“今天”
    let today = Utc::now().with_timezone(&Shanghai).date_naive();

    // 3) 分割日期范围：当前日期 vs 历史日期
    let (current_date_range, historical_date_range) = split_date_range_by_today(plan.range, today);

    log::info!(
      "[Search] 日期分割: 当前日期范围={:?}, 历史日期范围={:?}",
      current_date_range,
      historical_date_range
    );

    // 4) 当前日期范围 → Agent 来源
    if let Some(current_range) = current_date_range {
      let agent_endpoints = get_agent_endpoints().await;
      if !agent_endpoints.is_empty() {
        log::info!(
          "为当前日期范围 {:?} 添加 {} 个 Agent 存储源",
          current_range,
          agent_endpoints.len()
        );

        for endpoint in agent_endpoints {
          // 为 Agent 来源附带 scope 与路径过滤提示：
          // - scope_root: 固定为 "logs"
          // - path_filter_glob: 仅检索“今天”的目录（北京时区）
          let today_glob = format!("**/{}/**", today.format("%Y-%m-%d"));
          configs.push(SourceConfig::Agent {
            endpoint: endpoint.clone(),
            scope_root: Some("logs".to_string()),
            path_filter_glob: Some(today_glob),
          });
          log::debug!("添加 Agent 存储源: endpoint={}", endpoint);
        }
      } else {
        log::warn!(
          "当前日期范围 {:?} 需要 Agent 存储源，但未找到可用的 Agent 端点",
          current_range
        );
      }
    }

    // 5) 历史日期范围 → S3 来源（按 bbip 现有 key 结构展开）
    if let Some(historical_range) = historical_date_range {
      if !profiles.is_empty() {
        log::info!("为历史日期范围 {:?} 添加 S3 存储源", historical_range);

        use chrono::Duration;
        let mut d = historical_range.start;
        while d <= historical_range.end {
          let dp1 = d + Duration::days(1);
          let y = dp1.year();
          let m = dp1.month();
          let day = dp1.day();
          let yyyymm = format!("{:04}{:02}", y, m);
          let yyyymmdd = format!("{:04}{:02}{:02}", y, m, day);
          let file_name = format!("{:04}-{:02}-{:02}", d.year(), d.month(), d.day());

          for profile in &profiles {
            log::debug!(
              "为 Profile '{}' 生成历史存储源配置 (endpoint={}, bucket={})",
              profile.profile_name,
              profile.endpoint,
              profile.bucket
            );

            for b in Self::BUCKETS {
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

              log::debug!("添加历史 S3 存储源: profile={}, key={}", profile.profile_name, key);
            }
          }

          d += Duration::days(1);
        }
      } else {
        log::warn!(
          "历史日期范围 {:?} 需要 S3 存储源，但未找到可用的 S3 Profiles",
          historical_range
        );
      }
    }

    log::info!("[Search] 共生成 {} 个混合存储源配置", configs.len());

    Ok(PlanResult {
      sources: configs,
      cleaned_query: plan.cleaned_query,
    })
  }
}

// =============
// 私有辅助函数
// =============
fn split_date_range_by_today(range: DateRange, today: chrono::NaiveDate) -> (Option<DateRange>, Option<DateRange>) {
  let yesterday = today - chrono::Duration::days(1);

  if range.end <= yesterday {
    return (None, Some(range));
  }
  if range.start >= today {
    return (Some(range), None);
  }

  let historical_range = if range.start <= yesterday {
    Some(DateRange::new(range.start, yesterday))
  } else {
    None
  };
  let current_range = Some(DateRange::new(today, range.end));

  (current_range, historical_range)
}

async fn get_agent_endpoints() -> Vec<String> {
  // 查找包含标签 app=bbipapp 的在线 Agent
  let endpoints =
    agent_manager::get_online_agent_endpoints_by_tags(&[("app".to_string(), "bbipapp".to_string())]).await;
  if !endpoints.is_empty() {
    log::info!("按标签 app=bbipapp 找到 {} 个在线 Agent", endpoints.len());
  } else {
    log::warn!("按标签 app=bbipapp 未找到在线 Agent");
  }
  endpoints
}
