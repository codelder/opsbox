# LogSeek 与 DFS 搜索模块重构计划

## Context（背景）

当前架构存在职责混淆：
1. **DFS**（Distributed File System）包含了搜索相关的抽象和处理器
2. **LogSeek** 有更完善的搜索实现（包括 `contains` 过滤、更合理的配置）
3. 两者之间存在代码重复和功能缺失

**重构目标**：
1. DFS 专注于**文件系统抽象**
2. 创建独立的 **processing 模块**处理内容处理逻辑
3. LogSeek 实现具体的搜索逻辑

---

## 目标架构

```
opsbox-core/src/
├── dfs/                          # 纯文件系统抽象
│   ├── mod.rs                    # 导出核心类型
│   ├── endpoint.rs               # Endpoint, Location
│   ├── filesystem.rs             # OpbxFileSystem trait
│   ├── searchable.rs             # Searchable trait（文件遍历能力）
│   ├── resource.rs               # Resource
│   ├── path.rs                   # ResourcePath
│   ├── archive.rs                # ArchiveType, ArchiveContext
│   ├── factory.rs                # create_fs
│   ├── orl_parser.rs             # ORL 解析
│   └── impls/                    # 文件系统实现
│       ├── local.rs              # LocalFileSystem
│       ├── s3.rs                 # S3Storage
│       ├── agent.rs              # AgentProxyFS
│       └── archive.rs            # ArchiveFileSystem
│
├── processing/                   # 内容处理框架（新模块）
│   ├── mod.rs                    # 导出 ContentProcessor, EntryStreamProcessor, PathFilter
│   ├── types.rs                  # ContentProcessor trait, ProcessedContent
│   ├── processor.rs              # EntryStreamProcessor（从 DFS/search 移动）
│   ├── filter.rs                 # PathFilter（从 DFS/search 移动，添加 contains 支持）
│   └── preload.rs                # preload_entry, PreloadResult
│
├── fs/                           # 文件系统工具
│   ├── mod.rs
│   └── entry_stream.rs           # EntryStream trait
│
└── ...

logseek/src/
├── query/
│   ├── mod.rs
│   ├── parser.rs                 # Query 解析
│   └── ...
│
├── service/
│   ├── mod.rs
│   ├── search.rs                 # SearchProcessor (impl ContentProcessor)
│   ├── search_executor.rs        # 搜索协调
│   ├── searchable.rs             # SearchableFileSystem trait + Providers
│   └── entry_stream.rs           # 删除（使用 processing 模块）
│
└── ...
```

---

## 实施步骤

### Phase 1: 创建 processing 模块

**目标**：在 opsbox-core 中创建独立的 processing 模块

**步骤**：
1. 创建 `opsbox-core/src/processing/` 目录
2. 创建 `mod.rs` 导出公共 API
3. 从 `dfs/search/types.rs` 移动 `ContentProcessor` trait 到 `processing/types.rs`
4. 从 `dfs/search/types.rs` 移动 `ProcessedContent` 到 `processing/types.rs`
5. 从 `dfs/search/types.rs` 移动 `preload_entry` 和 `PreloadResult` 到 `processing/preload.rs`

**文件结构**：
```
opsbox-core/src/processing/
├── mod.rs
├── types.rs          # ContentProcessor, ProcessedContent
└── preload.rs        # preload_entry, PreloadResult
```

### Phase 2: 移动并增强 PathFilter

**目标**：移动 PathFilter 并添加 contains 支持

**步骤**：
1. 创建 `processing/filter.rs`
2. 从 `dfs/search/processor.rs` 移动 `PathFilter` 结构体
3. **添加 `include_contains` 和 `exclude_contains` 字段**（从 LogSeek 移植）
4. 更新 `is_allowed()` 方法支持 contains 检查

**修改后的 PathFilter**：
```rust
// processing/filter.rs
#[derive(Clone, Default)]
pub struct PathFilter {
    pub include: Option<globset::GlobSet>,
    pub exclude: Option<globset::GlobSet>,
    pub include_contains: Vec<String>,   // 新增
    pub exclude_contains: Vec<String>,   // 新增
}

impl PathFilter {
    pub fn is_allowed(&self, path: &str) -> bool {
        // 1. 检查排除 glob
        if let Some(ref exclude) = self.exclude && exclude.is_match(path) {
            return false;
        }
        // 2. 检查排除 contains（新增）
        if self.exclude_contains.iter().any(|s| path.contains(s)) {
            return false;
        }
        // 3. 检查包含 glob
        if let Some(ref include) = self.include && !include.is_match(path) {
            return false;
        }
        // 4. 检查包含 contains（新增）
        if !self.include_contains.is_empty()
            && !self.include_contains.iter().any(|s| path.contains(s)) {
            return false;
        }
        true
    }
}
```

### Phase 3: 移动并优化 EntryStreamProcessor

**目标**：移动 EntryStreamProcessor 并统一配置

**步骤**：
1. 创建 `processing/processor.rs`
2. 从 `dfs/search/processor.rs` 移动 `EntryStreamProcessor`
3. **统一配置参数**：
   - `content_timeout`: 60秒（原 DFS 30秒）
   - `preload_threshold`: 120MB（原 DFS 50MB）
4. 保留泛型设计 `<P: ContentProcessor>`

**修改后的默认值**：
```rust
impl<P: ContentProcessor + 'static> EntryStreamProcessor<P> {
    pub fn new(processor: Arc<P>) -> Self {
        Self {
            processor,
            content_timeout: Duration::from_secs(60),  // 改为 60秒
            extra_path_filters: Vec::new(),
            cancel_token: None,
            base_path: None,
            preload_threshold: 120 * 1024 * 1024,      // 改为 120MB
            is_stopped_fn: None,
            error_callback: None,
        }
    }
}
```

### Phase 4: 删除 DFS/search 模块

**目标**：清理 DFS 中的 search 子模块

**步骤**：
1. 删除 `opsbox-core/src/dfs/search/` 目录
2. 更新 `opsbox-core/src/dfs/mod.rs`，移除 search 导出
3. 添加 processing 模块导出

**修改 `dfs/mod.rs`**：
```rust
// 移除这行
// pub mod search;
// pub use search::{ContentProcessor, EntryStreamProcessor, PathFilter, ProcessedContent};
```

**修改 `opsbox-core/src/lib.rs`**：
```rust
// 添加 processing 模块
pub mod processing;
```

### Phase 5: 更新 LogSeek 依赖

**目标**：更新 LogSeek 使用新的 processing 模块

**步骤**：
1. 更新 `logseek/src/service/searchable.rs` 导入
2. 删除 `logseek/src/service/entry_stream.rs` 中的死代码
3. 保留 `create_entry_stream_from_resource` 函数

**更新导入**：
```rust
// searchable.rs
// 之前
use opsbox_core::dfs::search::{ContentProcessor, ProcessedContent};
use opsbox_core::dfs::search::{EntryStreamProcessor, PathFilter};

// 之后
use opsbox_core::processing::{ContentProcessor, ProcessedContent};
use opsbox_core::processing::{EntryStreamProcessor, PathFilter};
```

**清理 entry_stream.rs**：
- 删除 `EntryStreamProcessor`（使用 processing 模块）
- 删除 `PreloadResult` 和 `preload_entry`（使用 processing 模块）
- 保留 `create_entry_stream_from_resource` 和 `create_entry_stream`

### Phase 6: 更新其他模块

**检查并更新**：
- `backend/agent/` - Agent 的搜索实现
- 测试文件中的导入

---

## 关键文件修改清单

| 文件 | 操作 | 说明 |
|------|------|------|
| `opsbox-core/src/processing/mod.rs` | 新建 | 模块入口 |
| `opsbox-core/src/processing/types.rs` | 新建 | ContentProcessor trait |
| `opsbox-core/src/processing/filter.rs` | 新建 | PathFilter（含 contains） |
| `opsbox-core/src/processing/processor.rs` | 新建 | EntryStreamProcessor |
| `opsbox-core/src/processing/preload.rs` | 新建 | preload_entry |
| `opsbox-core/src/dfs/search/` | 删除 | 整个目录 |
| `opsbox-core/src/dfs/mod.rs` | 修改 | 移除 search 导出 |
| `opsbox-core/src/lib.rs` | 修改 | 添加 processing 模块 |
| `logseek/src/service/searchable.rs` | 修改 | 更新导入 |
| `logseek/src/service/entry_stream.rs` | 清理 | 删除重复代码 |
| `logseek/src/service/search.rs` | 修改 | 更新 ContentProcessor 导入 |

---

## 验证方案

### 测试命令
```bash
# 运行所有测试
OPSBOX_NO_PROXY=1 cargo test --manifest-path backend/Cargo.toml

# 运行 opsbox-core 测试
cargo test -p opsbox-core

# 运行 logseek 测试
cargo test -p logseek

# 检查编译
cargo build --manifest-path backend/Cargo.toml
```

### 功能验证
1. 搜索功能正常工作
2. `path:src` 查询正确过滤（contains 功能）
3. 归档文件搜索正常
4. Agent 搜索正常
5. 取消搜索功能正常

---

## 预期收益

1. **职责清晰**：
   - DFS = 文件系统抽象
   - Processing = 内容处理框架
   - LogSeek = 日志搜索实现

2. **消除重复**：只保留一套实现

3. **功能完整**：PathFilter 的 contains 功能得到保留

4. **更好的扩展性**：processing 模块可以被其他模块复用

5. **统一配置**：超时、预读阈值等参数一致

---

## 不会丢失的功能

以下功能保留在 LogSeek 中，不受重构影响：

- **grep 高性能搜索**：字节级搜索、mmap 加速、gzip 流式解压
- **编码检测**：GBK/UTF-8 智能检测、二进制文件检测
- **查询解析**：GitHub 风格语法、布尔表达式、正则表达式
- **NL2Q**：自然语言转查询
- **来源规划器**：Starlark 脚本、智能日志源选择
- **缓存系统**：LRU 缓存搜索结果
- **多 Agent 并行搜索**：健康检查、标签过滤
