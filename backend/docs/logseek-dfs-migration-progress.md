# LogSeek DFS 迁移进度

## 概述

将 LogSeek 从 ODFS 迁移到 DFS（Distributed File System）模块。

## 已完成

### Phase 1: 扩展 DFS OpbxFileSystem trait ✅
- 文件: `opsbox-core/src/dfs/filesystem.rs`
- 添加了 `as_entry_stream` 方法到 trait

### Phase 2: 实现 as_entry_stream ✅
- `opsbox-core/src/dfs/impls/local.rs` - LocalFileSystem ✅
- `opsbox-core/src/dfs/impls/s3.rs` - S3Storage + S3EntryStream ✅
- `opsbox-core/src/dfs/impls/archive.rs` - ArchiveFileSystem ✅
- `opsbox-core/src/dfs/impls/agent.rs` - AgentProxyFS + AgentEntryStream ✅
- `explorer/src/fs/agent_discovery.rs` - AgentDiscoveryFileSystem ✅
- `explorer/src/fs/s3_discovery.rs` - S3DiscoveryFileSystem ✅

### Phase 3: 添加归档缓存 ✅
- 文件: `opsbox-core/src/dfs/archive_cache.rs` (新建)
- 全局静态缓存，LRU 驱逐，TTL 过期
- 导出: `ArchiveCacheKey`, `get_cached_archive`, `cache_archive`, `download_and_cache_archive`

## 进行中

### Phase 4: 更新 LogSeek 使用 DFS 🔄

**已修改的文件:**
1. `logseek/src/service/searchable.rs`
   - 将 `ORL` 改为 `Resource`
   - 将 `LocalOpsFS`, `S3OpsFS` 改为 `LocalFileSystem`, `S3Storage`
   - 添加 `create_search_fs()`, `execute_search()`, `execute_agent_search()`
   - `SearchContext` 现在使用 `resource: Resource` + `orl_str: String`

2. `logseek/src/service/search_executor.rs`
   - 导入改为 DFS 类型
   - `SearchResultHandler` 使用 `Resource` 代替 `ORL`
   - `plan()` 返回 `Vec<String>` 代替 `Vec<ORL>`

**需要继续修改:**
- [ ] `search_executor.rs` 第 279-329 行：构造 SearchContext 和调用搜索
- [ ] `logseek/src/service/entry_stream.rs` - 移除 OrlManager 使用
- [ ] `logseek/src/routes/view.rs` - 使用 OrlParser
- [ ] `logseek/src/domain/source_planner.rs` - 返回 String 代替 ORL
- [ ] `logseek/src/api/error.rs` - 移除 OrlError
- [ ] `logseek/src/routes/planners.rs` - 使用 OrlParser

## 待开始

### Phase 5: 清理 ODFS 依赖
- 移除 logseek 对 `opsbox_core::odfs` 的所有引用
- 运行完整测试验证

## 关键类型映射

| ODFS | DFS |
|------|-----|
| `ORL` | `Resource` (通过 `OrlParser::parse()`) |
| `EndpointType` | `Location` (enum) |
| `TargetType::Archive` | `resource.archive_context.is_some()` |
| `OpsFileSystem` | `OpbxFileSystem` |
| `LocalOpsFS` | `LocalFileSystem` |
| `S3OpsFS` | `S3Storage` |
| `OrlManager` | 直接调用 `fs.as_entry_stream()` |

## 编译错误（当前）

```
error[E0432]: unresolved import `crate::service::searchable::create_search_provider`
error[E0560]: struct `SearchContext` has no field named `orl`
```

需要修复 `search_executor.rs` 中的调用代码。

## 下次继续

1. 修复 `search_executor.rs` 第 279-329 行
2. 更新 `entry_stream.rs`
3. 更新 `routes/view.rs`
4. 更新其他 ODFS 依赖文件
5. 运行测试验证

## 测试命令

```bash
# 编译检查
cargo check -p logseek

# 运行测试
OPSBOX_NO_PROXY=1 cargo test --manifest-path backend/Cargo.toml
```
