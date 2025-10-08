# TarGzFile 数据源实施文档

## 概述

实现了 `TarGzFile` 数据源，支持从 tar.gz 归档文件中搜索日志。

## 实现特性

### 核心功能
- ✅ 实现 `DataSource` trait
- ✅ 支持读取和解压 tar.gz 文件
- ✅ 内存缓存机制（首次加载后缓存所有文件）
- ✅ 异步流式文件迭代
- ✅ 完整的错误处理

### 技术栈
```rust
// 依赖
async-compression (futures-io feature)  - Gzip 解压
async-tar                               - Tar 归档处理
futures                                 - 异步流
tokio-util                             - Tokio/Futures 兼容层
```

## API 使用

### 基本使用

```rust
use logseek::storage::targz::TarGzFile;
use logseek::storage::DataSource;
use std::path::PathBuf;

// 创建 TarGzFile 数据源
let targz = TarGzFile::new(PathBuf::from("/path/to/logs.tar.gz"));

// 列举文件
let mut files = targz.list_files().await?;
while let Some(entry) = files.next().await {
    let file_entry = entry?;
    println!("文件: {}", file_entry.path);
}

// 打开并读取文件
let mut reader = targz.open_file(&file_entry).await?;
let mut content = String::new();
reader.read_to_string(&mut content).await?;
```

### 与协调器集成

```rust
use logseek::service::coordinator::SearchCoordinator;
use logseek::storage::targz::TarGzFile;
use std::sync::Arc;

let mut coordinator = SearchCoordinator::new();

// 添加 tar.gz 数据源
coordinator.add_data_source(Arc::new(
    TarGzFile::new(PathBuf::from("/var/log/archive.tar.gz"))
));

// 执行搜索
let mut results = coordinator.search("error", 3).await?;
while let Some(result) = results.recv().await {
    println!("找到: {} ({} 行)", result.path, result.lines.len());
}
```

## 实现细节

### 缓存机制

```rust
struct TarGzFile {
    path: PathBuf,
    // 缓存的文件列表（文件路径 -> 文件内容）
    file_cache: Arc<RwLock<HashMap<String, Vec<u8>>>>,
    // 是否已初始化
    initialized: Arc<RwLock<bool>>,
}
```

**优点**:
- 首次加载后，后续访问极快
- 避免重复解压缩
- 支持并发读取

**缺点**:
- 大文件会占用较多内存
- 初始化时间较长

### 异步流处理

```rust
async fn list_files(&self) -> Result<FileIterator, StorageError> {
    // 确保已初始化并缓存
    self.ensure_initialized().await?;
    
    let cache = self.file_cache.read().await;
    
    // 创建文件条目列表
    let entries: Vec<Result<FileEntry, StorageError>> = cache
        .iter()
        .map(|(path, content)| {
            Ok(FileEntry {
                path: path.clone(),
                metadata: FileMetadata {
                    size: Some(content.len() as u64),
                    modified: None,
                    content_type: None,
                },
            })
        })
        .collect();
    
    // 转换为流
    let stream = futures::stream::iter(entries);
    Ok(Box::new(stream))
}
```

### Gzip 解压缩

```rust
use async_compression::futures::bufread::GzipDecoder;
use tokio_util::compat::TokioAsyncReadCompatExt;

// Tokio File -> Tokio BufReader -> Compat -> Futures BufReader -> GzipDecoder
let reader = tokio::io::BufReader::new(file);
let compat_reader = reader.compat();
let decoder = GzipDecoder::new(futures::io::BufReader::new(compat_reader));
let archive = Archive::new(decoder);
```

**兼容层说明**:
- Tokio 和 Futures 使用不同的 `AsyncRead` trait
- `tokio_util::compat` 提供互操作性
- `async-compression` 使用 `futures-io`
- `async-tar` 使用 `futures-io`

## 测试

### 测试覆盖

```
✅ test_targz_list_files          - 列举文件
✅ test_targz_open_file           - 打开文件
✅ test_targz_nonexistent_file    - 不存在的文件
✅ test_targz_open_nonexistent_entry - 不存在的条目
✅ test_targz_caching             - 缓存机制

总计: 5 个测试
状态: 全部通过 ✅
```

### 运行测试

```bash
# 运行 TarGzFile 测试
cargo test -p logseek storage::targz --lib

# 运行所有测试
cargo test -p logseek --lib
# 结果: 212 个测试通过
```

## 性能考虑

### 内存使用

```
小文件 (<10MB):   低内存占用，适合缓存
中文件 (10-100MB): 适中，可接受
大文件 (>100MB):  高内存占用，考虑流式处理
```

### 优化建议

1. **对于小型归档**: 当前实现已是最优
2. **对于大型归档**: 考虑以下优化
   - 实现流式处理（不缓存所有文件）
   - LRU 缓存策略
   - 文件索引（避免完整解压）
   - 分块加载

## 错误处理

```rust
#[derive(Debug, Error)]
pub enum StorageError {
    #[error("文件不存在: {0}")]
    NotFound(String),
    
    #[error("权限被拒绝: {0}")]
    PermissionDenied(String),
    
    #[error("IO错误: {0}")]
    Io(#[from] std::io::Error),
    
    // ...其他错误
}
```

**支持的错误场景**:
- ✅ 文件不存在
- ✅ 权限不足
- ✅ IO 错误
- ✅ 解压失败
- ✅ 归档格式错误

## 限制

1. **内存限制**: 整个归档加载到内存
2. **修改不支持**: 只读访问
3. **压缩格式**: 仅支持 gzip (.tar.gz, .tgz)
4. **归档格式**: 仅支持 tar

## 未来增强

- [ ] 支持其他压缩格式 (.tar.bz2, .tar.xz)
- [ ] 流式处理选项（无缓存模式）
- [ ] LRU 缓存策略
- [ ] 文件索引优化
- [ ] 进度报告

## 文件结构

```
server/logseek/src/storage/
├── mod.rs          # 导出 targz 模块
└── targz.rs        # TarGzFile 实现 (320 行)
    ├── TarGzFile struct
    ├── DataSource impl
    └── Tests (5 个)
```

## 依赖更新

```toml
# server/logseek/Cargo.toml
async-compression = { version = "0.4", features = ["tokio", "gzip", "futures-io"] }
# 新增: futures-io feature
```

## 总结

✅ **完整实现** TarGzFile 数据源  
✅ **5 个测试** 全部通过  
✅ **212 总测试** 保持通过  
✅ **性能优化** 通过内存缓存  
✅ **错误处理** 完善  
✅ **文档齐全** 包含使用示例  

**推荐使用场景**:
- 日志归档搜索
- 历史日志分析
- 备份文件检索
- 小到中型归档文件

**不推荐场景**:
- 超大归档文件 (>1GB)
- 频繁修改的归档
- 实时日志流

---

**相关文档**:
- [存储抽象架构](./STORAGE_ABSTRACTION_AGENT.md)
- [实施总结](./IMPLEMENTATION_SUMMARY.md)

