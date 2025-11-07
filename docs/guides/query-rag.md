# 查询字符串（q）语法与使用说明 — RAG 资料

本文档面向大模型检索与工程实践，汇总日志检索服务中“查询字符串（q）”的语法规范、日期指令扩展、接口契约、错误类型、示例问答与核心代码片段（含文件路径与起始行号）。

- 服务路径：/api/v1/logseek
  - POST /api/v1/logseek/search.ndjson → NDJSON 流式
  - GET  /api/v1/logseek/view.cache.json → 查看缓存
- 请求体：{ q: string, context?: number }
- 默认 context：3 行

---

## 目录
- [一、概览与接口契约](#一概览与接口契约)
- [二、查询语言（GitHub-like）语法规范](#二查询语言github-like语法规范)
  - [1) 术语与类型](#1-术语与类型)
  - [2) 布尔运算与优先级](#2-布尔运算与优先级)
  - [3) 路径限定符 path:](#3-路径限定符-path)
  - [4) 正则表达式](#4-正则表达式)
  - [5) 文件级布尔求值与输出行选取](#5-文件级布尔求值与输出行选取)
- [三、日期指令（BBIP 特有扩展）](#三日期指令bbip-特有扩展)
- [四、错误与诊断](#四错误与诊断)
- [五、实践与例子（便于大模型检索的“问-答”对）](#五实践与例子便于大模型检索的问-答对)
- [六、返回内容与高亮策略](#六返回内容与高亮策略)
- [七、端到端处理流程（本地 NDJSON）](#七端到端处理流程本地-ndjson)
- [八、额外注意（性能与鲁棒性）](#八额外注意性能与鲁棒性)
- [九、速查清单（Cheat Sheet）](#九速查清单cheat-sheet)

---

## 一、概览与接口契约

- 接口与请求体定义：
```rs path=PROJECT_ROOT/backend/logseek/src/api/models.rs start=58
#[derive(Debug, Clone, Deserialize)]
pub struct SearchBody {
  pub q: String,
  pub context: Option<usize>,
}
```

```rs path=PROJECT_ROOT/backend/logseek/src/routes/mod.rs start=21
pub fn router(db_pool: SqlitePool) -> Router {
  Router::new()
    // 搜索路由（多存储源并行搜索）
    .route("/search.ndjson", axum::routing::post(search::stream_search))
    .route("/view.cache.json", axum::routing::get(view::view_cache_json))
    .with_state(db_pool)
}
```

- 响应格式：
  - /search.ndjson → Content-Type: application/x-ndjson; charset=utf-8（行分隔 JSON，每行一条 chunk）
  - /view.cache.json → Content-Type: application/json; charset=utf-8（查看缓存）

- NDJSON 对象结构：
```rs path=PROJECT_ROOT/backend/logseek/src/utils/renderer.rs start=97
#[derive(Debug, Serialize)]
pub struct SearchJsonResult {
  pub path: String,
  pub keywords: Vec<String>,
  pub chunks: Vec<JsonChunk>,
}
```

- 请求示例：
  - NDJSON 流式：
```bash path=null start=null
curl -s -X POST \
  http://127.0.0.1:4000/api/v1/logseek/search.ndjson \
  -H 'Content-Type: application/json' \
  -d '{"q":"\"connection reset\" OR /ERR\\d{3}/ dt:20250909"}'
```

---

## 二、查询语言（GitHub-like）语法规范

### 1) 术语与类型
- Term（查询原子）含三类：
  - Literal：普通字面量子串，大小写敏感
  - Phrase：双引号短语，按子串匹配，大小写敏感
  - Regex：/.../ Rust 正则

```rs path=PROJECT_ROOT/backend/logseek/src/query/mod.rs start=30
#[derive(Debug, Clone)]
pub enum Term {
  // 匹配简单子串
  Literal(String),
  // 匹配精确短语（子串语义）
  Phrase(String),
  // 匹配正则（Rust 正则语法）
  Regex(regex::Regex),
}
```

```rs path=PROJECT_ROOT/backend/logseek/src/query/mod.rs start=40
impl Term {
  pub fn matches(&self, line: &str) -> bool {
    match self {
      Term::Literal(s) => line.contains(s),
      Term::Phrase(p) => line.contains(p),
      Term::Regex(r) => r.is_match(line),
    }
  }
}
```

- 高亮显示用 display_text：Regex 不参与高亮。
```rs path=PROJECT_ROOT/backend/logseek/src/query/mod.rs start=49
pub fn display_text(&self) -> Option<String> {
  match self {
    Term::Literal(s) => Some(s.clone()),
    Term::Phrase(p) => Some(p.clone()),
    Term::Regex(_) => None, // 正则不用于高亮，避免混淆
  }
}
```

### 2) 布尔运算与优先级
- 支持：AND、OR、NOT(-)、括号 ()
- 规则：
  - 相邻关键字等价 AND（显式 AND 与隐式 AND 等价）
  - OR 必须大写；小写 or 作为普通字面量
  - 优先级：NOT > AND > OR

```rs path=PROJECT_ROOT/backend/logseek/src/query/parser.rs start=6
pub fn parse_github_like(input: &str) -> Result<Query, ParseError> {
  let tokens = tokenize(input)?;
  // 先提取 path 限定符，再解析布尔表达式
  ...
}
```

- 测试：OR 必须大写
```rs path=PROJECT_ROOT/backend/logseek/src/query/parser.rs start=342
#[test]
fn or_must_be_uppercase() {
  let spec = parse_github_like("foo or bar").expect("parse");
  match spec.expr.unwrap() {
    Expr::And(v) => assert_eq!(v.len(), 3),
    other => panic!("期望 And 表达式包含 3 个关键字，实际为 {:?}", other),
  }
}
```

- 示例：
  - foo bar  等价 foo AND bar
  - (foo OR bar) baz  要求包含 foo 或 bar，同时包含 baz
  - -debug  表示取反

### 3) 路径限定符 path:
- 语法：
  - 包含：path:<pattern>
  - 排除：-path:<pattern>
- 注意：仅识别小写 path，且冒号后不可有空格（path:...）。"PATH:" 或 "path :..." 不生效。
- 模式：
  - 若包含 * ? [ ] → 作为 glob（自动补全为 **/pattern，除非已以 / 或 **/ 开头）
  - 否则 → 作为“路径包含子串”判断
- 判定逻辑：先排除（exclude），再检查包含（include / include_contains）。

```rs path=PROJECT_ROOT/backend/logseek/src/query/mod.rs start=58
#[derive(Debug, Clone, Default)]
pub struct PathFilter {
  include: Option<GlobSet>,
  exclude: Option<GlobSet>,
  // 无通配符时的简单包含判断
  include_contains: Vec<String>,
  exclude_contains: Vec<String>,
}
```

```rs path=PROJECT_ROOT/backend/logseek/src/query/mod.rs start=67
impl PathFilter {
  pub fn is_allowed(&self, path: &str) -> bool {
    if let Some(ex) = &self.exclude { if ex.is_match(path) { return false; } }
    if self.exclude_contains.iter().any(|s| path.contains(s)) { return false; }
    if let Some(inc) = &self.include { if !inc.is_match(path) { return false; } }
    if !self.include_contains.is_empty() {
      if !self.include_contains.iter().any(|s| path.contains(s)) { return false; }
    }
    true
  }
}
```

- 行为示例（测试）：
```rs path=PROJECT_ROOT/backend/logseek/src/query/parser.rs start=319
#[test]
fn path_filter_glob_and_contains() {
  let spec = parse_github_like("path:logs/*.log -path:node_modules/ foo").expect("parse");
  assert!(spec.path_filter.is_allowed("logs/app/app.log"));
  assert!(!spec.path_filter.is_allowed("app/node_modules/x.js"));
  assert!(!spec.path_filter.is_allowed("logs/app/readme.md"));
  assert!(!spec.path_filter.is_allowed("src/app.log"));
}
```

```rs path=PROJECT_ROOT/backend/logseek/src/query/parser.rs start=474
#[test]
fn path_qualifier_requires_no_whitespace() {
  let a = parse_github_like("path:logs/*.log foo").expect("parse a");
  let b = parse_github_like("path :logs/*.log foo").expect("parse b");
  // a: 仅 logs/*.log 生效
  assert!(a.path_filter.is_allowed("logs/app/app.log"));
  assert!(!a.path_filter.is_allowed("src/app.log"));
  // b: 无效限定，全部允许
  assert!(b.path_filter.is_allowed("logs/app/app.log"));
  assert!(b.path_filter.is_allowed("src/app.log"));
}
```

```rs path=PROJECT_ROOT/backend/logseek/src/query/parser.rs start=486
#[test]
fn path_qualifier_is_case_sensitive() {
  let spec = parse_github_like("PATH:logs/*.log foo").expect("parse");
  // 大写 PATH 作为字面量，无路径限制
  assert!(spec.path_filter.is_allowed("logs/x.log"));
  assert!(spec.path_filter.is_allowed("src/x.log"));
}
```

### 4) 正则表达式
- 语法：/pattern/ 使用 Rust regex 语法
- 错误：无效时返回 InvalidRegex（含 span 位置）
- 高亮：正则不参与高亮关键字（仅 Literal 与 Phrase 用于高亮）

### 5) 文件级布尔求值与输出行选取
- 步骤：
  1. 文件级统计各 Term 是否出现
  2. 用表达式树（AND/OR/NOT）判定文件是否匹配
  3. 若匹配，再收集“命中任一正向 Term”的行，并按 context 合并区间

```rs path=PROJECT_ROOT/backend/logseek/src/search.rs start=93
// 文件级布尔计算：检查各关键字是否在文件中出现
let term_count = spec.terms.len();
...
// 若该行命中任一正向关键字，则收录
for &pi in &positive_indices {
  if spec.terms.get(pi).map(|t| t.matches(line)).unwrap_or(false) {
    line_positive = true;
    break;
  }
}
...
// 文件级布尔求值
if !spec.eval_file(&occurs) { return Ok(None); }
```

---

## 三、日期指令（BBIP 特有扩展）

- 用途：仅在本地 NDJSON 模式下用于推导文件集合；会从 q 中剥离日期令牌，保留 cleaned_query 参与内容检索。
- 令牌：
  - dt:YYYYMMDD → 指定单日
  - fdt:YYYYMMDD → 起始日（含）
  - tdt:YYYYMMDD → 终止日（含）
- 规则：
  - fdt & tdt 同时提供 → 区间 [fdt, tdt]
  - 仅 fdt 或仅 tdt → 等价单日
  - 三者皆无 → 默认“昨天”（本地时间）
  - 无效日期会被忽略；若最终都无效，也回退为“昨天”
  - 清理：从 q 中删除 dt/fdt/tdt 令牌，重组 cleaned_query

```rs path=PROJECT_ROOT/backend/logseek/src/bbip_service.rs start=47
/// 内部：从 q 中解析日期指令，返回（清理后的 q，日期区间）
fn parse_date_directives_from_query(q_raw: &str, today: NaiveDate) -> (String, DateRange) {
  ...
  // 去除日期属性，组装 cleaned_query
  let cleaned = tokens
    .into_iter()
    .filter(|t| !(t.starts_with("dt:") || t.starts_with("fdt:") || t.starts_with("tdt:")))
    .collect::<Vec<_>>()
    .join(" ");
  (cleaned, range)
}
```

- 使用链路（NDJSON）：
```rs path=PROJECT_ROOT/backend/logseek/src/routes/mod.rs start=120
// 通过服务从 q 中解析日期属性并生成文件路径，同时返回清理后的 q
let plan = derive_plan(base_dir, &buckets, &body.q);
let files = plan.paths;
let q_for_search = plan.cleaned_query;
...
let spec =
  crate::query::Query::parse_github_like(&q_for_search).map_err(|e| Problem::from(AppError::QueryParse(e)))?;
```

---

## 四、错误与诊断

- 解析错误类型（中文报错信息）：
```rs path=PROJECT_ROOT/backend/logseek/src/query/mod.rs start=7
#[derive(Debug, Error)]
pub enum ParseError {
  #[error("无效正则，位置 {span:?}：{message}")]
  InvalidRegex { message: String, span: (usize, usize) },
  #[error("无效路径模式，位置 {span:?}：{pattern}")]
  InvalidPathPattern {
    pattern: String,
    span: Option<(usize, usize)>,
  },
  #[error("意外的记号，位置 {span:?}")]
  UnexpectedToken { span: (usize, usize) },
  #[error("括号不匹配，起始于 {span:?}")]
  UnbalancedParens { span: (usize, usize) },
}
```

- 常见触发场景：
  - 无效正则："/(foo" 未闭合分组
  - 括号不匹配："foo OR (bar"
  - 运算符后缺失项："foo OR ", "foo -"
  - 无效 path 模式："path:a["

- 测试示例：
```rs path=PROJECT_ROOT/backend/logseek/src/query/parser.rs start=456
#[test]
fn invalid_regex_unclosed_group() {
  let err = parse_github_like("/(foo").unwrap_err();
  matches!(err, ParseError::InvalidRegex { .. });
}
```

```rs path=PROJECT_ROOT/backend/logseek/src/query/parser.rs start=336
#[test]
fn unbalanced_parens_error() {
  let err = parse_github_like("foo OR (bar").unwrap_err();
  matches!(err, ParseError::UnbalancedParens { .. });
}
```

```rs path=PROJECT_ROOT/backend/logseek/src/query/parser.rs start=501
#[test]
fn trailing_minus_is_error() {
  let err = parse_github_like("foo -").unwrap_err();
  matches!(err, ParseError::UnexpectedToken { .. });
}
```

```rs path=PROJECT_ROOT/backend/logseek/src/query/parser.rs start=534
#[test]
fn span_invalid_path_pattern_from_qualifier() {
  // "path:a[" => invalid glob; qualifier token spans 0..7
  match parse_github_like("path:a[").unwrap_err() {
    ParseError::InvalidPathPattern { span, .. } => assert_eq!(span, Some((0, 7))),
    e => panic!("unexpected error: {:?}", e),
  }
}
```

---

## 五、实践与例子（便于大模型检索的“问-答”对）

- 问：如何查询包含 foo 且包含 bar？
  - 答："foo bar" 或 "foo AND bar"
- 问：如何查询包含 foo 或 bar，且包含 baz？
  - 答："(foo OR bar) baz"
- 问：如何排除 debug？
  - 答："foo -debug" 或 "foo AND -debug"
- 问：小写 or 会如何处理？
  - 答：作为普通字面量；需用大写 OR 才是“或”
- 问：如何限定只查 logs/*.log？
  - 答："path:logs/*.log foo"
- 问：如何排除 node_modules 目录？
  - 答："foo -path:node_modules/"
- 问：为什么 PATH:... 不生效？
  - 答：限定符大小写敏感且冒号后不能有空格，必须使用 "path:"。
- 问：如何搜索短语“connection reset”或 ERR 编号？
  - 答："\"connection reset\" OR /ERR\\d{3}/"
- 问：如何选择 2025-09-08 至 2025-09-10 的文件并检索 timeout？
  - 答：q 写 "timeout fdt:20250908 tdt:20250910"
- 问：如果不写日期会查哪一天？
  - 答：默认“昨天”（本地时间）
- 问：正则写错了会怎样？
  - 答：400，错误标题“查询语法错误”，详细为“无效正则 …”并附 span 位置

---

## 六、返回内容与高亮策略

- 高亮关键字：仅来自非取反的 Literal 与 Phrase；Regex 不参与高亮。
- 行合并：以 context 为窗口对命中行合并为区间；Markdown 输出区间间以 "..." 分隔。

```rs path=PROJECT_ROOT/backend/logseek/src/utils/renderer.rs start=64
pub fn render_markdown(path: &str, ranges: Vec<(usize, usize)>, all_lines: Vec<String>, keywords: &[String]) -> String {
  let mut buf = String::new();
  buf.push_str(&format!("\n## 文件 s3://{}/{}::{}\n\n", "test", "codeler.tar.gz", path));
  buf.push_str("<pre>\n");
  for (chunk_idx, (s, e)) in ranges.iter().copied().enumerate() {
    for i in s..=e {
      use std::fmt::Write as _;
      let highlighted = highlight_with_mark(&all_lines[i], &keywords);
      let _ = write!(&mut buf, "{:>6} | {}\n", i + 1, highlighted);
    }
    if chunk_idx + 1 < ranges.len() {
      buf.push_str("       ...\n");
    }
  }
  buf.push_str("</pre>\n\n");
  buf
}
```

---

## 七、端到端处理流程（本地 NDJSON）

1) 接收 q 与 context
2) 派生日期计划：解析 dt/fdt/tdt → 生成日期区间与文件列表，并产出 cleaned_query
3) 用 cleaned_query 进行语法解析 → 构建 Query
4) 遍历文件并异步 grep：文件级布尔评估 + 行提取 + 合并 + 流式发送
5) 下游通道关闭时尽快停止任务

```rs path=PROJECT_ROOT/backend/logseek/src/routes/search.rs start=1
let spec =
  crate::query::Query::parse_github_like(&q_for_search).map_err(|e| Problem::from(AppError::QueryParse(e)))?;
let parse_dur = parse_start.elapsed();
let highlights = spec.highlights.clone();
let ctx = body.context.unwrap_or(3);
...
while let Some(result) = stream.recv().await {
  let json_obj = render_json_chunks(
    &format!("{}:{}", path, &result.path),
    result.merged.clone(),
    result.lines.clone(),
    &highlights_c,
  );
  ...
}
```

---

## 八、额外注意（性能与鲁棒性）

- 自动文本判定：若采样包含 NUL 字节等，判为“可能是二进制”则跳过。
```rs path=PROJECT_ROOT/backend/logseek/src/search.rs start=37
fn is_probably_text_bytes(sample: &[u8]) -> bool {
  if sample.is_empty() { return true; }
  if sample.contains(&0) { return false; }
  let printable = sample
    .iter()
    .filter(|b| matches!(**b, 0x09 | 0x0A | 0x0D | 0x20..=0x7E))
    .count();
  let ratio = printable as f32 / sample.len() as f32;
  if ratio >= 0.95 { return true; }
  std::str::from_utf8(sample).is_ok()
}
```

- 并发控制：基于可用并行度、放大系数与上限，避免任务风暴。
```rs path=PROJECT_ROOT/backend/logseek/src/search.rs start=180
let max_concurrency = std::thread::available_parallelism()
  .map(|n| n.get())
  .unwrap_or(4)
  .saturating_mul(2)
  .min(256);
```

- 路径过滤尽早应用，忽略符号链接。

---

## 九、速查清单（Cheat Sheet）

- AND：显式 AND 或相邻即 AND；优先级高于 OR
- OR：必须大写 OR
- NOT：前缀 -，可用于原子或组
- Phrase：双引号 "..."
- Regex：/.../（Rust 正则）
- path 限定：path:pattern 与 -path:pattern
  - 小写 path、冒号后无空格
  - 含通配符 → glob；否则 → 包含子串
- 日期令牌（仅用于文件选择）：dt/fdt/tdt（YYYYMMDD），清理后再解析查询
- 默认日期：未提供任何日期令牌 → 昨天
- context：命中行上下文（默认 3）

---

如需在前端或文档中嵌入更多示例，请直接采纳上述“问-答”与 curl 样例；如需扩展语法（例如新增限定符），建议在 query::parser 与 PathFilter 处添加测试用例，并在本文档新增章节与示例。

