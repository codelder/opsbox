use grep_searcher::{Sink, SinkContext, SinkMatch};
use std::io;

/// 用于 grep-searcher 的 Sink 实现
///
/// 它可以：
/// 1. 收集所有必要的行（匹配行 + 上下文）
/// 2. 在匹配行上运行 `Query` 的布尔逻辑，更新全局 `occurs` 状态
/// 3. 但由于 Sink 必须即时决策，而布尔逻辑（如 (A OR B)）可能分布在文件不同行
///    因此这个 Sink 实际上是 "Greedy Collection"：
///    它会收集所有命中了 **任何一个 Term** 的行（及其上下文）。
///    最终的过滤仍然需要依赖全局的 `eval_file(occurs)`。
///    **但是**，Context Lines 的拼接和合并是难点。
///
/// **修正策略**：
/// 我们不再让 Sink 负责 "Context" 的最终组装。
/// Sink 的任务是：
/// 1. 找到匹配项。
/// 2. 也是最重要的，更新 Query 的 `occurs` 状态。
/// 3. 收集 **所有** 匹配到的行和上下文行到一个临时的 Buffer 中。
///
/// 如果最终 `eval_file` 为 true，我们就返回这些行。
/// 如果为 false，就丢弃。
///
/// 难点：`grep-searcher` 的 Sink 是按 "Match" 驱动的。
/// 这意味着，如果我有 regex A|B。每次遇到 A 或 B，sink 都会被调用。
/// grep-searcher 会自动处理 context 重叠。
///
/// 所以，我们只需要在 `SinkMatch` 回调中：
/// 1. 拿到匹配行的 bytes。
/// 2. 用 `Query` 里的 matcher 再扫一遍这行，看看到底命中了 A 还是 B，然后更新 `occurs`。
/// 3. 将这行（以及 context lines）存入我们的 `lines` 列表。
pub struct BooleanContextSink<'a, 'b> {
  pub query: &'a crate::query::Query,
  pub occurs: &'b mut [bool],
  pub matched_lines: &'b mut Vec<usize>,
  pub matched_count: &'b mut u64,
  pub encoding: Option<&'a str>,
}

impl<'a, 'b> BooleanContextSink<'a, 'b> {
  pub fn new(
    query: &'a crate::query::Query,
    occurs: &'b mut [bool],
    matched_lines: &'b mut Vec<usize>,
    matched_count: &'b mut u64,
    encoding: Option<&'a str>,
  ) -> Self {
    Self {
      query,
      occurs,
      matched_lines,
      matched_count,
      encoding,
    }
  }
}

impl<'a, 'b> Sink for BooleanContextSink<'a, 'b> {
  type Error = io::Error;

  fn matched(&mut self, _searcher: &grep_searcher::Searcher, match_: &SinkMatch) -> Result<bool, io::Error> {
    *self.matched_count += 1;

    // 1. 更新 Occurs (布尔逻辑状态)
    let line_bytes = match_.bytes();

    for (i, matcher) in self.query.byte_matchers.iter().enumerate() {
      if self.occurs[i] {
        continue;
      }
      if let Some(re) = matcher
        && re.is_match(line_bytes)
      {
        self.occurs[i] = true;
      }
    }

    // 2. 收集匹配行号 (0-based)
    let line_num = match_.line_number().map(|n| n as usize).unwrap_or(0);
    if line_num > 0 {
      self.matched_lines.push(line_num - 1);
    }

    Ok(true)
  }

  fn context(&mut self, _searcher: &grep_searcher::Searcher, _ctx: &SinkContext) -> Result<bool, io::Error> {
    // 我们只收集匹配行，上下文在加载全文后计算
    Ok(true)
  }

  fn context_break(&mut self, _searcher: &grep_searcher::Searcher) -> Result<bool, io::Error> {
    Ok(true)
  }

  fn begin(&mut self, _searcher: &grep_searcher::Searcher) -> Result<bool, io::Error> {
    Ok(true)
  }

  fn finish(
    &mut self,
    _searcher: &grep_searcher::Searcher,
    _finish: &grep_searcher::SinkFinish,
  ) -> Result<(), io::Error> {
    Ok(())
  }
}
