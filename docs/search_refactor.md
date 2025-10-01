# search.rs 重构与优化建议（优先级排序）

本文档汇总对 server/logseek/src/search.rs 的高影响力重构与改进建议，按优先级从高到低排列。

## 1) 正确性与语义
- 多关键词语义：当前逻辑要求“每个关键词都在文件中出现”（对每个 kw 都要 hit_any，否则返回 None）。如果你希望的是“任意关键词匹配”，应在发现第一个关键词命中后即短路，而不是因为有的关键词未命中就拒绝文件。如果你要“全部匹配”，可以保留，但请清晰文档化并添加测试。
- 上下文范围结束值：`e = min(idx + context_lines, lines.len() - 1)` 将 end 视为“包含端点”。确保下游渲染也按“包含端点”处理。可以考虑命名为 `RangeInclusive`，并在注释中说明。
- 空关键词：当 `keywords = []` 时，`ranges` 为空并返回 `None`。请决定期望行为：通常建议在 `keywords.is_empty()` 时尽早返回 `Ok(None)`。

## 2) 性能与内存
- 避免不必要的整文件加载：
  - 设置 `max_hits` 上限并在达到后提前退出，避免在超大文件中过度扫描。
  - 当满足任意/全部关键词策略后，尽早停止继续扫描后续行。
- 使用 Aho-Corasick 处理多模式匹配：
  - 目前逐行对每个关键词执行 `.contains`，复杂度为 `O(行数 × 关键词数)`。预构建 Aho-Corasick 自动机可大幅加速。
- 流式获取上下文：
  - 现在会把每一行都 push 到 `lines`（大文件很占内存）。可以用环形缓冲区，仅保留命中附近的上下文窗口；或增加配置 `return_full_file: bool`，当为 `false` 时仅返回命中窗口，节省内存。
- 二进制检测的采样：
  - 目前边构建 `String` 边采样。可先 peek 固定 4–8 KiB 的原始字节判断文本/二进制，再决定是否构建字符串，避免为二进制文件进行 UTF-8 分配。

## 3) API 易用性
- 引入 `SearchOptions`：
  - `{ context_lines, case_insensitive, whole_word, require_all_keywords, max_hits, max_file_bytes, return_full_file }`
  - 用该结构替代多个分散参数，并贯穿两种 `Search` 实现。
- 结果模型：
  - 大输出时可用枚举 `SearchPayload { Full { lines, merged }, Windows(Vec<Window>) }`，支持只返回命中窗口。
  - 可选返回每次命中的行号与命中关键词，方便前端高亮。

## 4) 并发、背压与鲁棒性
- 通道容量与背压：
  - 目录搜索使用 `mpsc(128)`，流式搜索用 `mpsc(8)`。建议可配置容量，或使用有背压意识的策略（例如发送端 `await` 等待缓解压力，或按需丢弃最旧消息，如果业务允许）。
- 任务与信号量：
  - 在 `spawn` 前 `acquire` 很好。也可以对目录条目做分批处理，避免在极大目录上产生过多任务创建开销。
- 错误处理：
  - 许多错误被静默忽略（`Err(_) => continue`）。至少用 `tracing` 在 `debug` 级别记录，便于诊断被跳过的条目。
  - 可通过单独通道发送轻量错误元信息，或在最终发送一条汇总（错误计数）。

## 5) 文本检测鲁棒性
- 改进 `is_probably_text_bytes`：
  - 现有启发式可用，但可考虑：
    - 略降阈值（如 `ratio >= 0.9`），对小样本先尝试 `from_utf8` 成功即快速返回 `true`。
    - 将样本大小做成可配置（默认 4 KiB）。
    - 支持 UTF-16 BOM 检测，决定作为文本处理或明确跳过。

## 6) TAR+GZIP 流路径
- 格式检测：
  - TODO 提到 `AsyncRead` 不一定是 tar。可对 gzip 头（`0x1F 0x8B`）做简单探测，对 tar 做 `ustar` 头启发式检查，或让调用方显式指定格式。
- 条目读取：
  - 目前会把条目读完整行再决定输出。可结合 `max_hits` 提前终止，避免处理超大归档。
  - 路径处理：`entry.path()` 可能失败或非 UTF‑8。内部可保留 `OsString`，仅在展示时做有损转换；内部逻辑尽量保留原始路径。

## 7) 代码可读性与测试
- 用 rustdoc 注释清晰说明关键词策略与范围端点是否包含，尤其是 `grep_context_from_reader_async`。
- 单元测试建议：
  - 任意 vs 全部关键词策略
  - 上下文区间合并与边界（重叠、首尾行）
  - 二进制检测：二进制拒绝、UTF‑8 接受、空文件接受
  - 流式 TAR：单条目命中、多条目、非 UTF‑8 路径

---

### 示例片段（思路）

- Options 结构与 Aho-Corasick 集成
  - 在 `Cargo.toml` 增加：`aho-corasick = "1"`
- 定义选项：
  - `pub struct SearchOptions { pub context_lines: usize, pub case_insensitive: bool, pub whole_word: bool, pub require_all_keywords: bool, pub max_hits: Option<usize>, pub sample_bytes: usize, pub return_full_file: bool }`
- 构建匹配器：
  - `let pat_iter = if opts.case_insensitive { keywords.iter().map(|s| s.to_lowercase()) } else { keywords.iter().cloned() };`
  - `let ac = aho_corasick::AhoCorasickBuilder::new().ascii_case_insensitive(opts.case_insensitive).build(pat_iter)?;`
- 行匹配：
  - `let hay = if opts.case_insensitive { line.to_lowercase() } else { line.to_string() };`
  - `let found = ac.find(&hay).is_some();`
- 提前停止（`max_hits`）：
  - `hits += 1; if let Some(max) = opts.max_hits { if hits >= max { break; } }`

- 区间合并（包含端点）：
  - 保留现有合并逻辑，抽成小函数并加单测。

---

### 细节优化
- 内部优先使用 `&Path`；仅在对外展示时转字符串。
- 将并发度逻辑 `unwrap_or(4).saturating_mul(2).min(256)` 抽成 helper 并文档化；2×CPU 合理，但建议可配置。
- 用 `tracing` 替代静默 `continue`；如 `trace::debug!(path=?path, "skip symlink");` 等。

