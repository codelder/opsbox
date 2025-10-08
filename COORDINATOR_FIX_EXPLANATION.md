# Coordinator Tar.gz 搜索修复说明

## 问题描述

之前的错误：
```
[2025-10-08T03:17:28Z WARN  logseek::service::coordinator] 
搜索文件 bbip/2025/202508/20250819/BBIP_22_APPLOG_2025-08-18.tar.gz 失败: 
IO错误: stream did not contain valid UTF-8
```

**根本原因**：Coordinator 对所有文件类型使用相同的处理逻辑
- S3Storage 返回的 `.tar.gz` 文件是**压缩的二进制流**
- Coordinator 直接按**文本文件**处理，尝试逐行读取
- 遇到非 UTF-8 字节导致错误

## 解决方案

### 核心思路：复用现有的 tar.gz 搜索逻辑

在 `search.rs` 第418-455行已经有完整的 tar.gz 处理实现（`Search` trait for `AsyncRead`）：

```rust
#[async_trait]
impl<T> Search for T
where
  T: SearchiableAsyncReader,  // Box<dyn AsyncRead + Send + Unpin> 满足此条件
{
  async fn search(...) -> Result<...> {
    // 1. 自动解压 gzip
    let gz = GzipDecoder::new(BufReader::new(self));
    
    // 2. 解析 tar 归档
    let archive = AsyncArchive::new(gz.compat());
    let entries = archive.entries()?;
    
    // 3. 使用 TarStreamProcessor 处理每个条目
    let entry_processor = TarEntryProcessor::new(search_processor, config);
    let mut stream_processor = TarStreamProcessor::new(entry_processor, config);
    stream_processor.process_stream(entries, tx).await;
  }
}
```

### 修改内容

#### 1. 导入 `Search` trait
```rust
// coordinator.rs 第6行
use crate::service::search::{Search, SearchError, SearchProcessor, SearchResult};
```

#### 2. 在 coordinator 中根据文件类型选择处理方式

```rust
// coordinator.rs 第168-210行
// 根据文件类型选择处理方式
let is_targz = entry.path.ends_with(".tar.gz") || entry.path.ends_with(".tgz");

if is_targz {
  // 对 tar.gz 文件，复用现有的 Search trait 实现
  // 该实现会自动解压 gzip 并解析 tar 归档
  let spec = processor.spec.as_ref().clone();
  let ctx = processor.context_lines;
  match reader.search(&spec, ctx).await {  // ← 调用 Search trait
    Ok(mut result_rx) => {
      let mut count = 0;
      while let Some(result) = result_rx.recv().await {
        if tx.send(result).await.is_err() {
          break;
        }
        count += 1;
      }
      count
    }
    Err(e) => {
      warn!("搜索 tar.gz 文件 {} 失败: {}", entry.path, e);
      0
    }
  }
} else {
  // 对普通文本文件，直接使用 processor 处理
  let mut reader = reader;
  match processor.process_content(entry.path.clone(), &mut reader).await {
    // ... 原有逻辑
  }
}
```

#### 3. 暴露 SearchProcessor 的字段

```rust
// search.rs 第95-97行
pub struct SearchProcessor {
  pub spec: Arc<Query>,         // ← 改为 pub
  pub context_lines: usize,     // ← 改为 pub
}
```

## 工作原理

1. **文件类型检测**
   - 通过文件扩展名判断是否为 tar.gz（`.tar.gz` 或 `.tgz`）

2. **Tar.gz 文件处理流程**
   ```
   S3 对象 → FileReader (Box<dyn AsyncRead>)
            ↓
   Search trait 实现（自动调用）
            ↓
   GzipDecoder → AsyncArchive
            ↓
   逐个条目搜索（TarStreamProcessor）
            ↓
   返回匹配结果
   ```

3. **普通文本文件处理**（不变）
   ```
   文件 → FileReader
        ↓
   SearchProcessor::process_content
        ↓
   逐行读取 + 文本匹配
        ↓
   返回匹配结果
   ```

## 优势

### ✅ 完全复用现有逻辑
- 不需要重新实现 tar.gz 解析
- 利用已测试过的 `TarEntryProcessor` 和 `TarStreamProcessor`
- 包含所有错误处理、超时控制、智能重试等功能

### ✅ 零重复代码
- 所有 tar.gz 处理逻辑集中在 `search.rs`
- Coordinator 只负责路由到正确的处理器

### ✅ 类型安全
- 通过 trait 边界确保 `FileReader` 满足要求
- 编译时检查，无运行时开销

### ✅ 易于扩展
- 未来支持更多格式（如 `.zip`），只需：
  1. 为新格式实现 `Search` trait
  2. 在 coordinator 中添加文件类型判断

## 性能考虑

1. **流式处理**
   - tar.gz 文件不会整个加载到内存
   - 边解压边搜索，内存占用恒定

2. **并发控制**
   - Coordinator 已有 Semaphore 限制并发数
   - tar.gz 内部也有并发控制

3. **超时保护**
   - 继承了 `TarStreamProcessor` 的超时机制
   - 避免卡死在损坏的归档上

## 测试建议

1. **单元测试**（已存在）
   - `search.rs` 中的 tar.gz 处理已有测试

2. **集成测试**
   ```bash
   # 测试 S3 上的真实 tar.gz 文件
   curl -X POST http://localhost:8080/api/unified-search \
     -H "Content-Type: application/json" \
     -d '{
       "q": "ERROR path:BBIP_22_APPLOG_2025-08-18.tar.gz",
       "context": 3
     }'
   ```

3. **压力测试**
   - 多个大型 tar.gz 文件并发搜索
   - 观察内存使用和响应时间

## 相关文件

- ✅ `server/logseek/src/service/coordinator.rs` - 添加文件类型路由
- ✅ `server/logseek/src/service/search.rs` - 暴露 SearchProcessor 字段
- 📖 `server/logseek/src/service/search.rs` (第418-680行) - tar.gz 处理实现

## 编译结果

```bash
$ cargo build
   Compiling logseek v0.1.0
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 3.47s
```

✅ **编译成功，无错误！**
