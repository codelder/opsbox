# 修复总结 - dt: 日期限定符处理

## 🐛 问题描述

用户报告：**现在的处理好像把 `dt:` 也当成搜索条件了**

### 问题根源

1. **`dt:` 是日期过滤器**，不是搜索关键词
   - 格式：`dt:20250818`（单日）、`fdt:20250818 tdt:20250820`（日期范围）
   - 作用：控制搜索哪些日期的 tar.gz 文件

2. **处理流程问题**：
   ```
   原始查询: "ERROR dt:20250818 timeout"
          ↓
   get_storage_source_configs() 解析 dt: 并生成文件列表
          ↓
   但传递给 coordinator.search() 的仍是原始查询 "ERROR dt:20250818 timeout"
          ↓
   Query parser 不认识 dt:，把它当成 Literal 关键词
          ↓
   ❌ 结果：在日志中搜索包含 "dt:20250818" 的行
   ```

3. **实际应该**：
   ```
   原始查询: "ERROR dt:20250818 timeout"
          ↓
   derive_plan() 解析并移除 dt:
          ↓
   清理后查询: "ERROR timeout"
          ↓
   传递给 coordinator.search()
          ↓
   ✅ 结果：只搜索 "ERROR" 和 "timeout"
   ```

## ✅ 解决方案

### 修改点

#### 1. 修改返回类型（返回清理后的查询）

**文件**: `server/logseek/src/routes.rs` (第797-800行)

```rust
async fn get_storage_source_configs(
  pool: &SqlitePool,
  query: &str,
) -> Result<(Vec<crate::storage::factory::SourceConfig>, String), AppError> {
//          ↑ 修改：返回 tuple，包含配置和清理后的查询
```

#### 2. 解析日期并获取清理后的查询

**文件**: `server/logseek/src/routes.rs` (第813-829行)

```rust
// 解析日期计划，获取日期区间和清理后的查询（无论是否有 profiles 都需要清理查询）
let base_dir = "/unused/for/s3";
let buckets = ["20", "21", "22", "23"];
let plan = derive_plan(base_dir, &buckets, query);

log::info!(
  "[UnifiedSearch] 日期范围解析: start={}, end={}, 原始查询='{}', 清理后查询='{}'",
  plan.range.start,
  plan.range.end,
  query,
  plan.cleaned_query  // ← 清理后的查询（已移除 dt:/fdt:/tdt:）
);

// 如果没有 profiles，直接返回空配置和清理后的查询
if profiles.is_empty() {
  return Ok((Vec::new(), plan.cleaned_query));
}
```

#### 3. 返回清理后的查询

**文件**: `server/logseek/src/routes.rs` (第887-889行)

```rust
// 返回存储源配置和清理后的查询（移除了 dt:/fdt:/tdt: 等日期限定符）
Ok((configs, plan.cleaned_query))
```

#### 4. 使用清理后的查询进行搜索

**文件**: `server/logseek/src/routes.rs` (第904-1005行)

```rust
// 1. 获取存储源配置列表（同时获取清理后的查询）
let (source_configs, cleaned_query) = match get_storage_source_configs(&pool, &body.q).await {
  Ok((configs, cleaned)) => (configs, cleaned),
  // ...
};

// 4. 执行搜索（使用清理后的查询，移除了 dt:/fdt:/tdt: 等日期限定符）
let ctx = body.context.unwrap_or(3);

// 解析清理后的查询以获取 highlights
let spec = crate::query::Query::parse_github_like(&cleaned_query)
  .map_err(|e| Problem::from(AppError::QueryParse(e)))?;
let highlights = spec.highlights.clone();

log::info!(
  "[UnifiedSearch] 开始并行搜索: 原始query={}, 清理后query={}, context={}, sid={}",
  body.q, cleaned_query, ctx, sid
);

let query_for_search = cleaned_query.clone();

// 在后台任务中执行搜索
tokio::spawn(async move {
  match coordinator.search(&query_for_search, ctx).await {
    // ↑ 使用清理后的查询
    // ...
  }
});
```

## 🔍 工作原理

### `derive_plan` 函数（在 `bbip_service.rs` 中）

```rust
fn parse_date_directives_from_query(q_raw: &str, today: NaiveDate) -> (String, DateRange) {
  // 1. 解析 dt:/fdt:/tdt: 指令
  // 2. 过滤掉这些指令
  let cleaned = tokens
    .into_iter()
    .filter(|t| !(t.starts_with("dt:") || t.starts_with("fdt:") || t.starts_with("tdt:")))
    .collect::<Vec<_>>()
    .join(" ");
  
  (cleaned, range)
}
```

### 示例

#### 输入
```
原始查询: "ERROR dt:20250818 timeout path:*.log"
```

#### 处理过程
1. **derive_plan** 解析：
   - 提取日期：`dt:20250818` → 2025-08-18
   - 清理查询：`"ERROR timeout path:*.log"`

2. **生成文件列表**：
   ```
   BBIP_20_APPLOG_2025-08-18.tar.gz
   BBIP_21_APPLOG_2025-08-18.tar.gz
   BBIP_22_APPLOG_2025-08-18.tar.gz
   BBIP_23_APPLOG_2025-08-18.tar.gz
   ```

3. **Query parser** 解析清理后的查询：
   ```
   terms: ["ERROR", "timeout"]
   path_filter: *.log
   ```

4. **搜索执行**：
   - 在 4 个 tar.gz 文件中搜索
   - 匹配包含 "ERROR" 和 "timeout" 的行
   - 过滤路径匹配 `*.log` 的文件
   - ✅ **不会搜索 "dt:20250818"**

## 📊 测试验证

### 日志输出示例

```log
[2025-10-08T03:35:26Z INFO] [UnifiedSearch] 开始统一搜索: q=ERROR dt:20250818 timeout

[2025-10-08T03:35:26Z INFO] [UnifiedSearch] 日期范围解析: 
  start=2025-08-18, 
  end=2025-08-18, 
  原始查询='ERROR dt:20250818 timeout', 
  清理后查询='ERROR timeout'

[2025-10-08T03:35:26Z INFO] [UnifiedSearch] 开始并行搜索: 
  原始query=ERROR dt:20250818 timeout, 
  清理后query=ERROR timeout,  ← 确认使用清理后的查询
  context=3, 
  sid=xxx
```

### 预期行为

✅ **正确**：只搜索包含 "ERROR" 和 "timeout" 的日志行  
❌ **错误**：搜索包含 "dt:20250818" 的行（已修复）

## 📁 相关文件

- ✅ `server/logseek/src/routes.rs` (第797-1005行) - unified_search 处理
- 📖 `server/logseek/src/utils/bbip_service.rs` (第48-102行) - 日期解析逻辑
- 📖 `server/logseek/src/query/parser.rs` - 查询解析器（不识别 dt:）

## 🎯 编译结果

```bash
$ cargo build
   Compiling logseek v0.1.0
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 1.34s
```

✅ **编译成功，无错误！**

## 🔄 配套修复

这次修复是在之前 tar.gz 处理修复的基础上进行的：

1. ✅ **Tar.gz 处理修复** - 复用现有的 Search trait 实现
2. ✅ **日期限定符修复** - 移除 dt:/fdt:/tdt: 避免作为搜索关键词

两个修复共同确保：
- tar.gz 文件能正确解压和搜索
- 日期过滤器不会干扰实际的搜索关键词
