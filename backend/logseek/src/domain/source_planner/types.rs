/// 通用类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DateRange {
  pub start: chrono::NaiveDate,
  pub end: chrono::NaiveDate,
}

impl DateRange {
  /// 构建并自动规范化区间顺序
  pub fn new(a: chrono::NaiveDate, b: chrono::NaiveDate) -> Self {
    if a <= b {
      Self { start: a, end: b }
    } else {
      Self { start: b, end: a }
    }
  }
}
