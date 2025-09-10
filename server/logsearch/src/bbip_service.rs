use chrono::{Datelike, Local, NaiveDate};

/// BBIP 查询规划结果
/// - cleaned_q: 去除日期属性（dt/fdt/tdt）后的查询字符串
/// - files: 根据日期区间与分桶生成的文件绝对路径列表
/// - start/end: 实际生效的日期区间（闭区间）
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BbipQueryPlan {
  pub cleaned_q: String,
  pub files: Vec<String>,
  pub start: NaiveDate,
  pub end: NaiveDate,
}

/// 依据日期区间与分桶生成 BBIP 本地文件路径列表
/// 文件名模式："{base}/BBIP_{bucket}_APPLOG_YYYY-MM-DD.tar.gz"
fn generate_bbip_paths(base_dir: &str, buckets: &[&str], start: NaiveDate, end: NaiveDate) -> Vec<String> {
  let (mut cur, end) = if start <= end { (start, end) } else { (end, start) };
  let mut files = Vec::new();
  while cur <= end {
    let date = format!("{}-{:02}-{:02}", cur.year(), cur.month(), cur.day());
    for b in buckets {
      files.push(format!("{}/BBIP_{}_APPLOG_{}.tar.gz", base_dir, b, date));
    }
    cur = cur + chrono::Duration::days(1);
  }
  files
}

/// 从 q 中解析日期属性并生成 BBIP 查询规划（使用系统当前日期确定“前一日”）
/// 支持的日期属性：
/// - dt:YYYYMMDD （单日，与 fdt/tdt 互斥）
/// - fdt:YYYYMMDD （起始）
/// - tdt:YYYYMMDD （终止）
/// 规则：
/// - 若提供 dt，则只查询该日期；忽略 fdt/tdt
/// - 若仅提供 fdt 或 tdt 其中之一，另一端等同该值
/// - 若三者都未提供，默认取“当前日期的前一日”
/// 返回的 cleaned_q 会移除 dt/fdt/tdt 三种属性
pub fn plan_from_q(base_dir: &str, buckets: &[&str], q_raw: &str) -> BbipQueryPlan {
  // 计算“前一日”
  let today = Local::now().naive_local().date();
  plan_from_q_with_today(base_dir, buckets, q_raw, today)
}

/// 和 plan_from_q 相同，但允许外部指定 today，便于测试
pub fn plan_from_q_with_today(
  base_dir: &str,
  buckets: &[&str],
  q_raw: &str,
  today: NaiveDate,
) -> BbipQueryPlan {
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
  let parse_or_prev = |s: &str| chrono::NaiveDate::parse_from_str(s, "%Y%m%d").unwrap_or(prev);
  let mut start = parse_or_prev(&start_yyyymmdd);
  let mut end = parse_or_prev(&end_yyyymmdd);
  if start > end {
    std::mem::swap(&mut start, &mut end);
  }

  let files = generate_bbip_paths(base_dir, buckets, start, end);

  // 过滤掉日期前缀，拼装 cleaned_q
  let cleaned_q = tokens
    .into_iter()
    .filter(|t| !(t.starts_with("dt:") || t.starts_with("fdt:") || t.starts_with("tdt:")))
    .collect::<Vec<_>>()
    .join(" ");

  BbipQueryPlan { cleaned_q, files, start, end }
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
    let plan = plan_from_q_with_today(base, &buckets, q, today);
    assert_eq!(plan.cleaned_q, "error foo");
    assert_eq!(plan.start, NaiveDate::from_ymd_opt(2025, 9, 9).unwrap());
    assert_eq!(plan.end, NaiveDate::from_ymd_opt(2025, 9, 9).unwrap());
    assert_eq!(
      plan.files,
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
    let plan = plan_from_q_with_today(base, &buckets, q, today);
    assert_eq!(plan.cleaned_q, "timeout");
    assert_eq!(plan.start, NaiveDate::from_ymd_opt(2025, 9, 8).unwrap());
    assert_eq!(plan.end, NaiveDate::from_ymd_opt(2025, 9, 10).unwrap());
    assert_eq!(
      plan.files,
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
    let plan = plan_from_q_with_today(base, &buckets, q, today);
    assert_eq!(plan.cleaned_q, "warn");
    assert_eq!(plan.start, NaiveDate::from_ymd_opt(2025, 9, 8).unwrap());
    assert_eq!(plan.end, NaiveDate::from_ymd_opt(2025, 9, 8).unwrap());
  }

  #[test]
  fn only_tdt_equivalent_single_day() {
    let today = NaiveDate::from_ymd_opt(2025, 9, 10).unwrap();
    let base = "/b";
    let buckets = ["23"];
    let q = "fail tdt:20250908";
    let plan = plan_from_q_with_today(base, &buckets, q, today);
    assert_eq!(plan.cleaned_q, "fail");
    assert_eq!(plan.start, NaiveDate::from_ymd_opt(2025, 9, 8).unwrap());
    assert_eq!(plan.end, NaiveDate::from_ymd_opt(2025, 9, 8).unwrap());
  }

  #[test]
  fn default_previous_day_when_no_dates() {
    let today = NaiveDate::from_ymd_opt(2025, 9, 10).unwrap();
    let base = "/b";
    let buckets = ["20"];
    let q = "login";
    let plan = plan_from_q_with_today(base, &buckets, q, today);
    assert_eq!(plan.cleaned_q, "login");
    assert_eq!(plan.start, NaiveDate::from_ymd_opt(2025, 9, 9).unwrap());
    assert_eq!(plan.end, NaiveDate::from_ymd_opt(2025, 9, 9).unwrap());
  }

  #[test]
  fn swapped_when_start_gt_end() {
    let today = NaiveDate::from_ymd_opt(2025, 9, 10).unwrap();
    let base = "/b";
    let buckets = ["20"];
    let q = "fdt:20250912 tdt:20250910 alpha";
    let plan = plan_from_q_with_today(base, &buckets, q, today);
    assert_eq!(plan.cleaned_q, "alpha");
    assert_eq!(plan.start, NaiveDate::from_ymd_opt(2025, 9, 10).unwrap());
    assert_eq!(plan.end, NaiveDate::from_ymd_opt(2025, 9, 12).unwrap());
  }

  #[test]
  fn ignore_invalid_tokens() {
    let today = NaiveDate::from_ymd_opt(2025, 9, 10).unwrap();
    let base = "/b";
    let buckets = ["20"];
    let q = "dt:2025-09-09 something tdt:abc fdt:99990101x other";
    let plan = plan_from_q_with_today(base, &buckets, q, today);
    // 所有日期都无效 => 回退到昨天（2025-09-09）
    assert_eq!(plan.cleaned_q, "something other");
    assert_eq!(plan.start, NaiveDate::from_ymd_opt(2025, 9, 9).unwrap());
    assert_eq!(plan.end, NaiveDate::from_ymd_opt(2025, 9, 9).unwrap());
  }
}

