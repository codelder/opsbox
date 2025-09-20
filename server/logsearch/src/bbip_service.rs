use chrono::{Datelike, Local, NaiveDate};

/// 日期区间（保证 start <= end）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DateRange {
  pub start: NaiveDate,
  pub end: NaiveDate,
}

impl DateRange {
  /// 构建并自动规范化区间顺序
  pub fn new(a: NaiveDate, b: NaiveDate) -> Self {
    if a <= b {
      Self { start: a, end: b }
    } else {
      Self { start: b, end: a }
    }
  }
}

/// 由查询字符串推导出的文件选择计划
/// - cleaned_query: 去除日期属性（dt/fdt/tdt）后的查询串
/// - range: 实际使用的日期区间
/// - paths: 该区间内展开得到的文件绝对路径
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PathPlan {
  pub cleaned_query: String,
  pub range: DateRange,
  pub paths: Vec<String>,
}

/// 内部：依据日期区间与分桶生成 BBIP 本地文件路径列表
/// 文件名模式："{base}/BBIP_{bucket}_APPLOG_YYYY-MM-DD.tar.gz"
fn build_paths(base_dir: &str, buckets: &[&str], range: DateRange) -> Vec<String> {
  let mut cur = range.start;
  let mut files = Vec::new();
  while cur <= range.end {
    let date = format!("{}-{:02}-{:02}", cur.year(), cur.month(), cur.day());
    for b in buckets {
      files.push(format!("{}/BBIP_{}_APPLOG_{}.tar.gz", base_dir, b, date));
    }
    cur = cur + chrono::Duration::days(1);
  }
  files
}

/// 内部：从 q 中解析日期指令，返回（清理后的 q，日期区间）
fn parse_date_directives_from_query(q_raw: &str, today: NaiveDate) -> (String, DateRange) {
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
    } else if let Some(rest) = t.strip_prefix("tdt:") {
      if rest.len() == 8 && rest.chars().all(|c| c.is_ascii_digit()) {
        tdt_q = Some(rest.to_string());
      }
    }
  }

  let prev = today - chrono::Duration::days(1);
  let prev_str = format!("{:04}{:02}{:02}", prev.year(), prev.month(), prev.day());

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
      (None, None) => (prev_str.clone(), prev_str.clone()),
    }
  };

  // 容错：解析失败则回退为 prev
  let parse_or_prev = |s: &str| NaiveDate::parse_from_str(s, "%Y%m%d").unwrap_or(prev);
  let range = DateRange::new(parse_or_prev(&start_yyyymmdd), parse_or_prev(&end_yyyymmdd));

  // 去除日期属性，组装 cleaned_query
  let cleaned = tokens
    .into_iter()
    .filter(|t| !(t.starts_with("dt:") || t.starts_with("fdt:") || t.starts_with("tdt:")))
    .collect::<Vec<_>>()
    .join(" ");

  (cleaned, range)
}

/// 从查询字符串推导文件选择计划（基于系统“今天”来计算“前一日”）
pub fn derive_plan(base_dir: &str, buckets: &[&str], q_raw: &str) -> PathPlan {
  let today = Local::now().naive_local().date();
  derive_plan_with_today(base_dir, buckets, q_raw, today)
}

/// 同 derive_plan，但可注入 today 以便测试
pub fn derive_plan_with_today(base_dir: &str, buckets: &[&str], q_raw: &str, today: NaiveDate) -> PathPlan {
  let (cleaned_query, range) = parse_date_directives_from_query(q_raw, today);
  let paths = build_paths(base_dir, buckets, range);
  PathPlan {
    cleaned_query,
    range,
    paths,
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn dt_only_single_day() {
    let today = NaiveDate::from_ymd_opt(2025, 9, 10).unwrap();
    let base = "/tmp/base";
    let buckets = ["20", "21"];
    let q = "error dt:20250909 foo";
    let plan = derive_plan_with_today(base, &buckets, q, today);
    assert_eq!(plan.cleaned_query, "error foo");
    assert_eq!(plan.range.start, NaiveDate::from_ymd_opt(2025, 9, 9).unwrap());
    assert_eq!(plan.range.end, NaiveDate::from_ymd_opt(2025, 9, 9).unwrap());
    assert_eq!(
      plan.paths,
      vec![
        "/tmp/base/BBIP_20_APPLOG_2025-09-09.tar.gz",
        "/tmp/base/BBIP_21_APPLOG_2025-09-09.tar.gz",
      ]
    );
  }

  #[test]
  fn range_fdt_tdt() {
    let today = NaiveDate::from_ymd_opt(2025, 9, 10).unwrap();
    let base = "/b";
    let buckets = ["23"];
    let q = "timeout fdt:20250908 tdt:20250910";
    let plan = derive_plan_with_today(base, &buckets, q, today);
    assert_eq!(plan.cleaned_query, "timeout");
    assert_eq!(plan.range.start, NaiveDate::from_ymd_opt(2025, 9, 8).unwrap());
    assert_eq!(plan.range.end, NaiveDate::from_ymd_opt(2025, 9, 10).unwrap());
    assert_eq!(
      plan.paths,
      vec![
        "/b/BBIP_23_APPLOG_2025-09-08.tar.gz",
        "/b/BBIP_23_APPLOG_2025-09-09.tar.gz",
        "/b/BBIP_23_APPLOG_2025-09-10.tar.gz",
      ]
    );
  }

  #[test]
  fn only_fdt_equivalent_single_day() {
    let today = NaiveDate::from_ymd_opt(2025, 9, 10).unwrap();
    let base = "/b";
    let buckets = ["23"];
    let q = "warn fdt:20250908";
    let plan = derive_plan_with_today(base, &buckets, q, today);
    assert_eq!(plan.cleaned_query, "warn");
    assert_eq!(plan.range.start, NaiveDate::from_ymd_opt(2025, 9, 8).unwrap());
    assert_eq!(plan.range.end, NaiveDate::from_ymd_opt(2025, 9, 8).unwrap());
  }

  #[test]
  fn only_tdt_equivalent_single_day() {
    let today = NaiveDate::from_ymd_opt(2025, 9, 10).unwrap();
    let base = "/b";
    let buckets = ["23"];
    let q = "fail tdt:20250908";
    let plan = derive_plan_with_today(base, &buckets, q, today);
    assert_eq!(plan.cleaned_query, "fail");
    assert_eq!(plan.range.start, NaiveDate::from_ymd_opt(2025, 9, 8).unwrap());
    assert_eq!(plan.range.end, NaiveDate::from_ymd_opt(2025, 9, 8).unwrap());
  }

  #[test]
  fn default_previous_day_when_no_dates() {
    let today = NaiveDate::from_ymd_opt(2025, 9, 10).unwrap();
    let base = "/b";
    let buckets = ["20"];
    let q = "login";
    let plan = derive_plan_with_today(base, &buckets, q, today);
    assert_eq!(plan.cleaned_query, "login");
    assert_eq!(plan.range.start, NaiveDate::from_ymd_opt(2025, 9, 9).unwrap());
    assert_eq!(plan.range.end, NaiveDate::from_ymd_opt(2025, 9, 9).unwrap());
  }

  #[test]
  fn swapped_when_start_gt_end() {
    let today = NaiveDate::from_ymd_opt(2025, 9, 10).unwrap();
    let base = "/b";
    let buckets = ["20"];
    let q = "fdt:20250912 tdt:20250910 alpha";
    let plan = derive_plan_with_today(base, &buckets, q, today);
    assert_eq!(plan.cleaned_query, "alpha");
    assert_eq!(plan.range.start, NaiveDate::from_ymd_opt(2025, 9, 10).unwrap());
    assert_eq!(plan.range.end, NaiveDate::from_ymd_opt(2025, 9, 12).unwrap());
  }

  #[test]
  fn ignore_invalid_tokens() {
    let today = NaiveDate::from_ymd_opt(2025, 9, 10).unwrap();
    let base = "/b";
    let buckets = ["20"];
    let q = "dt:2025-09-09 something tdt:abc fdt:99990101x other";
    let plan = derive_plan_with_today(base, &buckets, q, today);
    // 所有日期都无效 => 回退到昨天（2025-09-09）
    assert_eq!(plan.cleaned_query, "something other");
    assert_eq!(plan.range.start, NaiveDate::from_ymd_opt(2025, 9, 9).unwrap());
    assert_eq!(plan.range.end, NaiveDate::from_ymd_opt(2025, 9, 9).unwrap());
  }
}
