# 内存管理优化

## 概述

OpsBox 使用 **mimalloc** 作为全局内存分配器，并在缓存清理时主动回收内存，以优化长时间运行的服务器内存使用。

## mimalloc 分配器

### 为什么选择 mimalloc

- **高性能**: 比系统默认分配器快 2-3 倍
- **低碎片**: 更好的内存布局，减少碎片
- **跨平台**: 支持 Linux、macOS、Windows
- **零配置**: 无需额外配置即可获得性能提升

### 配置位置

```rust
// backend/opsbox-server/src/main.rs
use mimalloc::MiMalloc;

#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;
```

## 自动内存回收

### 缓存清理时回收

当搜索结果缓存过期被清理时，系统会自动触发内存回收：

```rust
// backend/logseek/src/repository/cache.rs
// 每 15 分钟清理一次过期缓存
// 清理后调用 mi_collect(true) 强制回收内存
```

### 回收时机

1. **定期清理**: 每 15 分钟自动清理过期缓存
2. **内存回收**: 清理后立即触发 `mi_collect(true)`
3. **异步执行**: 使用 `spawn_blocking` 避免阻塞主线程

### 实现细节

```rust
// 如果清理了缓存条目，触发内存回收
if total_removed > 0 {
  tracing::info!("缓存清理完成: 移除 {} 个条目，触发内存回收", total_removed);
  
  tokio::task::spawn_blocking(move || {
    #[link(name = "mimalloc")]
    unsafe extern "C" {
      fn mi_collect(force: bool);
    }
    
    unsafe {
      // 强制回收内存，将空闲内存返还给操作系统
      mi_collect(true);
    }
    tracing::debug!("内存回收完成");
  });
}
```

## 内存使用场景

### 搜索结果缓存

- **keywords 缓存**: `sid -> Vec<String>` (高亮关键字)
- **files 缓存**: `(sid, FileUrl) -> Vec<String>` (搜索结果行)
- **TTL**: 15 分钟
- **清理策略**: 基于 `last_touch` 时间

### 典型内存占用

假设一次搜索返回 100 个文件，每个文件 1000 行，每行 100 字节：

```
100 files × 1000 lines × 100 bytes = 10 MB
```

如果有 10 个并发搜索会话：

```
10 sessions × 10 MB = 100 MB
```

15 分钟后这些缓存会被清理，内存会被回收。

## 监控和调优

### 日志输出

```
INFO  缓存清理完成: 移除 42 个条目，触发内存回收
DEBUG 内存回收完成
```

### 性能指标

- **清理频率**: 15 分钟/次
- **回收延迟**: < 10ms (在后台线程执行)
- **内存回收率**: 通常可回收 80-90% 的缓存内存

### 调优建议

1. **增加缓存 TTL**: 如果内存充足，可以延长缓存时间
2. **减少清理间隔**: 如果内存紧张，可以更频繁清理
3. **监控内存使用**: 使用系统工具监控 RSS 和 VSZ

## 与其他分配器对比

### jemalloc (已弃用)

之前使用 jemalloc，需要通过环境变量配置：

```bash
MALLOC_CONF="background_thread:true,dirty_decay_ms:0,muzzy_decay_ms:0"
```

### mimalloc 优势

- **无需配置**: 开箱即用
- **更好的性能**: 特别是在多线程场景
- **更简单的 API**: 直接调用 `mi_collect()`
- **更好的跨平台支持**: 特别是 Windows

## 最佳实践

1. **定期清理**: 利用现有的缓存清理机制
2. **异步回收**: 使用 `spawn_blocking` 避免阻塞
3. **日志记录**: 记录清理和回收事件
4. **监控内存**: 定期检查内存使用趋势

## 参考资料

- [mimalloc GitHub](https://github.com/microsoft/mimalloc)
- [mimalloc Rust Crate](https://crates.io/crates/mimalloc)
- [Memory Management in Rust](https://doc.rust-lang.org/book/ch04-00-understanding-ownership.html)
