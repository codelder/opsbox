# Unified Search 并发控制增强总结

## 🎯 目标

为 `stream_unified_search` 添加完整的并发控制机制，参照 `stream_s3_ndjson` 的实现。

## ✅ 实现的功能

### 1. 分层限流机制

#### IO 并发控制
- 使用 Semaphore 限制同时打开/读取的存储源数量
- 默认值：12 个（可通过 `LOGSEEK_S3_MAX_CONCURRENCY` 或全局调参配置）
- 避免过多的网络连接和文件句柄消耗

```rust
let io_sem = Arc::new(tokio::sync::Semaphore::new(s3_max_concurrency()));
```

#### CPU 并发控制
- 使用 Semaphore 限制同时进行解压/检索的任务数
- 默认值：16 个（可通过 `LOGSEEK_CPU_CONCURRENCY` 或全局调参配置）
- 避免 CPU 过载，特别是处理 tar.gz 文件时

```rust
let cpu_max = cpu_max_concurrency();
let cpu_sem = Arc::new(tokio::sync::Semaphore::new(cpu_max));
```

### 2. 自适应护栏（AIMD 策略）

#### 统计信息收集
```rust
struct UnifiedSearchStats {
  produced: Arc<AtomicU64>,      // 已生成结果数
  source_errors: Arc<AtomicU64>,  // 错误数
}
```

#### 动态调节控制器
- **CPU 并发数自适应调整**
- **AIMD 策略**（Additive Increase, Multiplicative Decrease）:
  - 高错误率（>2%）→ 乘性减小（× 0.7）
  - 吞吐量稳定 → 加性增加（+1）
- **调节周期**：每 3 秒检查一次
- **调节范围**：[1, cpu_max]

```rust
struct CpuController {
  max: usize,      // 最大值
  target: usize,   // 目标值
  held: Vec<OwnedSemaphorePermit>,  // 持有的许可（用于控制实际并发数）
}
```

#### 调节逻辑
```rust
let err_rate = errors / (produced + errors);
if err_rate > 0.02 && current > 1 {
  target = current * 0.7;  // 乘性减小
} else if throughput >= prev_throughput * 0.98 && target < max {
  target += 1;  // 加性增加
}
```

### 3. 详细的性能监控

#### 任务级监控
```rust
profiling: [UnifiedSearch] 任务开始排队 source_idx=0 name=S3Storage, io_avail=12, cpu_avail=16
profiling: [UnifiedSearch] 获得 IO 许可 source_idx=0, 等待=0.005s
profiling: [UnifiedSearch] 文件列举完成，耗时=0.234s
```

#### 文件级监控
```rust
profiling: [UnifiedSearch] 文件处理完成 path=BBIP_20_APPLOG_2025-08-18.tar.gz, 
  结果=15, 
  耗时=2.345s [cpu_wait=0.050s, open=0.123s, search=2.172s]
```

#### 存储源级监控
```rust
profiling: [UnifiedSearch] source_idx=0 type=S3Storage 搜索完成: 文件数=4, 结果数=150
profiling: [UnifiedSearch] 任务完成 source_idx=0 name=S3Storage, 结果数=150, 
  总耗时=5.678s, io_wait=0.005s
```

#### 自适应调节监控
```rust
adaptive: [UnifiedSearch] cpu target=14 effective=14 err_rate=1.2% tp=45.50/s
```

### 4. 直接任务派发（替代 Coordinator）

#### 原来的方式
```rust
// 使用 Coordinator 统一调度
let mut coordinator = SearchCoordinator::new();
coordinator.add_source(source);
coordinator.search(&query, ctx).await
```

#### 现在的方式
```rust
// 为每个存储源直接启动任务（带并发控制）
for (idx, source) in sources.into_iter().enumerate() {
  tokio::spawn(async move {
    // 获取 IO 许可
    let _io_permit = io_sem.acquire_owned().await?;
    
    // 根据存储源类型调用不同的搜索方法
    match source {
      StorageSource::Data(ds) => {
        search_data_source_with_concurrency(ds, ...).await
      }
      StorageSource::Service(ss) => {
        // TODO: 实现 SearchService 支持
      }
    }
  });
}
```

**优势**：
- ✅ 完全控制每个存储源的执行
- ✅ 可以精确监控每个存储源的性能
- ✅ 可以针对不同存储源类型应用不同的策略

### 5. 智能文件类型处理

在 `search_data_source_with_concurrency` 中：

```rust
let is_targz = entry.path.ends_with(".tar.gz") || entry.path.ends_with(".tgz");

if is_targz {
  // tar.gz 文件：使用 Search trait（自动解压+解析）
  reader.search(&spec, context_lines).await
} else {
  // 普通文本文件：使用 SearchProcessor
  let processor = SearchProcessor::new(spec, context_lines);
  processor.process_content(entry.path, &mut reader).await
}
```

## 📊 性能特性

### 并发控制层次
```
请求
  ↓
存储源级并发（IO Semaphore: 12）
  ↓
文件级并发（CPU Semaphore: 16，自适应调节）
  ↓
tar.gz 内部并发（由 Search trait 内部控制）
```

### 资源保护
- **网络连接**: 最多 12 个存储源同时访问
- **CPU 负载**: 最多 16 个文件同时解压/搜索（动态调节）
- **内存使用**: 流式处理，避免一次性加载大文件
- **错误隔离**: 单个文件/存储源失败不影响其他

### 自适应能力
- **错误率上升** → 自动降低并发数 → 减少系统压力
- **运行稳定** → 逐步提升并发数 → 提高吞吐量
- **动态平衡** → 在稳定性和性能之间自动寻找最佳点

## 🔄 与 stream_s3_ndjson 的对比

| 特性 | stream_s3_ndjson | stream_unified_search (新) |
|------|------------------|---------------------------|
| 并发控制 | ✅ IO + CPU Semaphore | ✅ IO + CPU Semaphore |
| 自适应调节 | ✅ AIMD 策略 | ✅ AIMD 策略 |
| 性能监控 | ✅ 详细 profiling | ✅ 详细 profiling |
| 存储源支持 | ❌ 仅 S3 | ✅ 多种存储源 |
| 文件类型 | ✅ tar.gz | ✅ tar.gz + 普通文本 |
| 任务派发 | ✅ 直接循环 | ✅ 直接循环 |
| 错误统计 | ✅ s3_errors | ✅ source_errors |

## 📁 修改的文件

### server/logseek/src/routes.rs

#### 新增功能
1. **UnifiedSearchStats** 结构（第910-920行）
2. **CpuController** 结构（第924-932行）
3. **自适应调节后台任务**（第940-996行）
4. **直接任务派发循环**（第1093-1189行）
5. **search_data_source_with_concurrency** 辅助函数（第1216-1440行）

#### 修改部分
- 添加 IO/CPU Semaphore 创建
- 添加统计信息收集
- 移除 Coordinator 使用，改为直接任务派发
- 添加详细的性能监控日志

## 🧪 测试建议

### 1. 基本功能测试
```bash
curl -X POST http://localhost:8080/api/search.unified.ndjson \
  -H "Content-Type: application/json" \
  -d '{
    "q": "ERROR dt:20250818",
    "context": 3
  }'
```

### 2. 并发控制测试
```bash
# 调整并发参数
export LOGSEEK_S3_MAX_CONCURRENCY=8
export LOGSEEK_CPU_CONCURRENCY=12

# 观察日志中的并发调节信息
grep "adaptive:" logs/logseek.log
```

### 3. 错误恢复测试
- 故意断开某个 S3 连接
- 观察错误率上升时的自适应降级
- 验证其他存储源不受影响

### 4. 性能测试
- 多存储源并发搜索
- 大量 tar.gz 文件处理
- 观察 CPU 和内存使用

## 📈 预期效果

### 稳定性提升
- ✅ 防止资源耗尽（连接数、CPU、内存）
- ✅ 错误隔离（单个失败不影响全局）
- ✅ 自动降级（高错误率时降低并发）

### 性能提升
- ✅ 合理的并发控制（不过载，不浪费）
- ✅ 自适应调节（根据实际情况优化）
- ✅ 流式处理（低内存占用）

### 可观测性提升
- ✅ 详细的 profiling 日志
- ✅ 实时的并发状态监控
- ✅ 清晰的错误追踪

## 🚀 下一步优化

1. **SearchService 支持** - 实现远程搜索服务的并发控制
2. **更细粒度的监控** - 添加 metrics 导出（Prometheus）
3. **配置热更新** - 运行时调整并发参数
4. **智能预热** - 根据历史数据预测最优并发数
5. **负载均衡** - 在多个存储源之间智能分配负载

## ✅ 编译结果

```bash
$ cargo build
   Compiling logseek v0.1.0
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 1.67s
```

✅ **编译成功！**
