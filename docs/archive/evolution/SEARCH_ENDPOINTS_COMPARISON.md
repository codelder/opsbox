# 搜索端点功能对比分析

## 📋 三个端点概览

| 端点 | 路由 | 主要用途 |
|------|------|---------|
| `stream_local_ndjson` | `/stream.ndjson` | 搜索本地文件系统的 tar.gz 文件 |
| `stream_s3_ndjson` | `/stream.s3.ndjson` | 搜索 S3/MinIO 上的 tar.gz 文件 |
| `stream_unified_search` | `/search.unified.ndjson` | **统一搜索多种存储源** |

## 🔍 详细功能对比

### 1. 存储源支持

| 功能 | stream_local_ndjson | stream_s3_ndjson | stream_unified_search |
|------|---------------------|------------------|----------------------|
| **本地文件系统** | ✅ 硬编码路径 | ❌ | ✅ 通过 LocalFileSystem |
| **S3/MinIO** | ❌ | ✅ 单个配置 | ✅ 多 Profile 支持 |
| **Agent 远程** | ❌ | ❌ | ✅ 架构支持（待实现） |
| **多存储源** | ❌ | ❌ | ✅ 并行搜索 |

**关键差异**：
- `stream_local_ndjson`: 只支持**硬编码的本地路径** (`/Users/wangyue/Downloads/log`)
- `stream_s3_ndjson`: 只支持**单个 S3 配置**（从数据库读取）
- `stream_unified_search`: 支持**多种存储源，动态配置**

### 2. 日期过滤

| 功能 | stream_local_ndjson | stream_s3_ndjson | stream_unified_search |
|------|---------------------|------------------|----------------------|
| **dt: 支持** | ✅ | ✅ | ✅ |
| **fdt:/tdt: 范围** | ✅ | ✅ | ✅ |
| **查询清理** | ✅ | ✅ | ✅ |
| **默认日期** | ✅ 前一日 | ✅ 前一日 | ✅ 前一日 |

**结论**：✅ **完全相同**

### 3. 文件类型处理

| 功能 | stream_local_ndjson | stream_s3_ndjson | stream_unified_search |
|------|---------------------|------------------|----------------------|
| **tar.gz** | ✅ | ✅ | ✅ |
| **普通文本** | ❌ | ❌ | ✅ |
| **自动识别** | ❌（假设都是 tar.gz） | ❌（假设都是 tar.gz） | ✅（根据扩展名） |

**关键差异**：
- `stream_local_ndjson`: 假设所有文件都是 tar.gz，直接调用 `reader.search()`
- `stream_s3_ndjson`: 假设所有文件都是 tar.gz
- `stream_unified_search`: **智能识别文件类型**，分别处理

### 4. 并发控制

| 功能 | stream_local_ndjson | stream_s3_ndjson | stream_unified_search |
|------|---------------------|------------------|----------------------|
| **文件级并发** | ❌ 无限制 | ✅ IO Semaphore (12) | ✅ IO Semaphore (12) |
| **解压级并发** | ❌ 无限制 | ✅ CPU Semaphore (16) | ✅ CPU Semaphore (16) |
| **自适应调节** | ❌ | ✅ AIMD | ✅ AIMD |
| **并发层次** | ❌ 单层 | ✅ 双层 | ✅ 双层 |

**关键差异**：
- `stream_local_ndjson`: **无并发控制**，所有文件同时处理（可能导致资源耗尽）
- `stream_s3_ndjson`: 完整的分层限流 + 自适应调节
- `stream_unified_search`: 完整的分层限流 + 自适应调节

### 5. 性能监控

| 功能 | stream_local_ndjson | stream_s3_ndjson | stream_unified_search |
|------|---------------------|------------------|----------------------|
| **基本日志** | ✅ | ✅ | ✅ |
| **Profiling** | ✅ 简单 | ✅ 详细 | ✅ 详细 |
| **耗时拆分** | ❌ | ✅ io_wait/cpu_wait/open/search | ✅ io_wait/cpu_wait/open/search |
| **错误统计** | ❌ | ✅ s3_errors | ✅ source_errors |
| **吞吐量监控** | ❌ | ✅ | ✅ |

**关键差异**：
- `stream_local_ndjson`: 只有基本的 profiling
- `stream_s3_ndjson`: 详细的性能监控
- `stream_unified_search`: 详细的性能监控

### 6. 缓存支持

| 功能 | stream_local_ndjson | stream_s3_ndjson | stream_unified_search |
|------|---------------------|------------------|----------------------|
| **Keywords 缓存** | ✅ | ✅ | ✅ |
| **Lines 缓存** | ✅ | ✅ | ⚠️ TODO |
| **FileUrl 构造** | ✅ local + tar | ✅ s3 + tar | ⚠️ 简化版 |

**关键差异**：
- `stream_local_ndjson`: 完整的 `FileUrl::local()` + `FileUrl::tar_entry()`
- `stream_s3_ndjson`: 完整的 `FileUrl::s3()` + `FileUrl::tar_entry()`
- `stream_unified_search`: **TODO**，当前使用简化的 file_id

### 7. 配置来源

| 功能 | stream_local_ndjson | stream_s3_ndjson | stream_unified_search |
|------|---------------------|------------------|----------------------|
| **硬编码** | ✅ base_dir/buckets | ❌ | ❌ |
| **数据库配置** | ❌ | ✅ 单个 S3 | ✅ 多 Profile |
| **动态配置** | ❌ | ❌ | ✅ 通过 factory |

**关键差异**：
- `stream_local_ndjson`: **硬编码路径** `/Users/wangyue/Downloads/log`
- `stream_s3_ndjson`: 从数据库读取单个 S3 配置
- `stream_unified_search`: 从数据库读取多个配置，动态创建存储源

### 8. 错误处理

| 功能 | stream_local_ndjson | stream_s3_ndjson | stream_unified_search |
|------|---------------------|------------------|----------------------|
| **文件打开失败** | ⚠️ warn + continue | ⚠️ warn + continue + 统计 | ⚠️ warn + continue + 统计 |
| **搜索失败** | ⚠️ warn + continue | ⚠️ warn + continue + 统计 | ⚠️ warn + continue + 统计 |
| **错误隔离** | ✅ | ✅ | ✅ |
| **自动降级** | ❌ | ✅ | ✅ |

## 🎯 功能覆盖分析

### stream_unified_search 覆盖 stream_local_ndjson？

| 检查项 | 结果 | 说明 |
|--------|------|------|
| 本地文件支持 | ⚠️ **部分** | unified 需要配置，local 是硬编码 |
| tar.gz 处理 | ✅ | 完全相同 |
| 并发控制 | ✅ **更好** | unified 有，local 没有 |
| 性能监控 | ✅ **更好** | unified 更详细 |
| 缓存支持 | ⚠️ **部分** | unified 的 FileUrl TODO |

**结论**: ⚠️ **基本覆盖，但有差异**

### stream_unified_search 覆盖 stream_s3_ndjson？

| 检查项 | 结果 | 说明 |
|--------|------|------|
| S3 支持 | ✅ **更好** | unified 支持多 Profile |
| tar.gz 处理 | ✅ | 完全相同 |
| 并发控制 | ✅ | 完全相同 |
| 自适应调节 | ✅ | 完全相同 |
| 性能监控 | ✅ | 完全相同 |
| 缓存支持 | ⚠️ **部分** | unified 的 FileUrl TODO |

**结论**: ⚠️ **基本覆盖，但有差异**

## 🔧 需要补充的功能

### 1. FileUrl 完整支持 ✅ **重要**

当前 `unified_search` 使用简化的 file_id：
```rust
// 当前
let file_id = format!("{}#{}", entry.path, result.path);
```

**需要**：
```rust
// 应该
let base_url = match source_type {
  "LocalFileSystem" => FileUrl::local(&entry.path),
  "S3Storage" => FileUrl::s3(&bucket, &key),
  _ => return,
};
let file_url = FileUrl::tar_entry(TarCompression::Gzip, base_url, &result.path)?;
let file_id = file_url.to_string();

// 缓存
simple_cache().put_lines(&sid, &file_url, result.lines.clone()).await;
```

### 2. 本地文件系统存储源配置 ⚠️ **中等**

当前 `unified_search` 需要从数据库读取配置，不支持本地文件系统。

**需要**：
```rust
// 在 get_storage_source_configs 中添加
configs.push(SourceConfig::Local {
  base_dir: "/Users/wangyue/Downloads/log".to_string(),
  pattern: Some("*.tar.gz".to_string()),
});
```

或者：
- 从数据库读取本地路径配置
- 或者从环境变量读取

### 3. 存储源类型传递 ⚠️ **中等**

在 `search_data_source_with_concurrency` 中需要知道存储源类型，才能正确构造 FileUrl。

**当前问题**：
```rust
let source_type = data_source.source_type();  // 返回 "S3Storage" 或 "LocalFileSystem"
// 但不知道具体的 bucket/key 或 base_dir
```

**需要**：
- 在 DataSource trait 中添加获取详细信息的方法
- 或者传递额外的元数据

## 📊 总结对比表

| 特性维度 | local | s3 | unified | 推荐 |
|---------|-------|-----|---------|------|
| **存储源灵活性** | ⭐ | ⭐⭐ | ⭐⭐⭐⭐⭐ | unified |
| **并发控制** | ❌ | ⭐⭐⭐⭐⭐ | ⭐⭐⭐⭐⭐ | unified/s3 |
| **自适应调节** | ❌ | ⭐⭐⭐⭐⭐ | ⭐⭐⭐⭐⭐ | unified/s3 |
| **性能监控** | ⭐⭐ | ⭐⭐⭐⭐⭐ | ⭐⭐⭐⭐⭐ | unified/s3 |
| **配置简单性** | ⭐⭐⭐⭐⭐ | ⭐⭐⭐ | ⭐⭐ | local |
| **功能完整性** | ⭐⭐ | ⭐⭐⭐⭐ | ⭐⭐⭐⭐ | unified |
| **缓存支持** | ⭐⭐⭐⭐ | ⭐⭐⭐⭐ | ⭐⭐ | local/s3 |

## 🎯 最终结论

### unified_search 是否完全覆盖？

**答案**: ⚠️ **90% 覆盖，但需要补充 FileUrl 支持**

#### 已覆盖的功能（90%）
✅ 搜索逻辑（tar.gz + 文本）  
✅ 日期过滤  
✅ 并发控制  
✅ 自适应调节  
✅ 性能监控  
✅ 错误处理  
✅ 多存储源支持  

#### 未覆盖的功能（10%）
❌ FileUrl 完整构造和缓存（对于 `/view.cache.json` 很重要）  
❌ 本地文件系统的零配置支持（local 是硬编码的）  

### 建议

#### 短期（保留三个端点）
```
/stream.ndjson          → 快速本地测试（硬编码路径）
/stream.s3.ndjson       → 单一 S3 源搜索（向后兼容）
/search.unified.ndjson  → 生产环境推荐（多源 + 完整功能）
```

#### 中期（补充 FileUrl 后）
```
/stream.ndjson          → Deprecated（引导迁移到 unified）
/stream.s3.ndjson       → Deprecated（引导迁移到 unified）
/search.unified.ndjson  → **主要端点**
```

#### 长期（完全迁移后）
```
/api/search             → unified_search 重命名
/api/search/local       → 本地搜索快捷方式（内部调用 unified）
/api/search/s3          → S3 搜索快捷方式（内部调用 unified）
```

## 🔨 立即行动项

### Priority 1: FileUrl 支持（必需）
修改 `search_data_source_with_concurrency`，添加完整的 FileUrl 构造和缓存。

### Priority 2: 本地文件系统配置（可选）
添加 `SourceConfig::Local` 支持，或者从环境变量读取。

### Priority 3: 向后兼容（推荐）
保留现有三个端点，但在文档中推荐使用 unified。
