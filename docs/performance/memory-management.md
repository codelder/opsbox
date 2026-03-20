# 内存管理与缓存回收

**文档版本**: v1.1  
**最后更新**: 2026-03-20

本文档描述当前仓库里真实存在的内存管理机制。

## 当前分配器策略

### `opsbox-server`

主服务使用 `mimalloc` 作为全局分配器：

```rust
use mimalloc::MiMalloc;

#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;
```

位置：

- `backend/opsbox-server/src/main.rs`

### `logseek`

`logseek` 本身没有单独设置全局分配器，但在 `opsbox-server` 中以如下方式启用：

- `logseek` 依赖开启 `mimalloc-collect` feature

这样缓存清理后可以调用：

- `libmimalloc_sys::mi_collect(true)`

尝试把空闲内存归还给系统。

## 当前缓存模型

`logseek` 有一个进程内搜索缓存，定义在：

- `backend/logseek/src/repository/cache.rs`

缓存按 `sid` 组织：

- `keywords: Vec<KeywordHighlight>`
- `files: HashMap<String, CompactLines>`

其中：

- `keywords` 用于查看页高亮
- `files` 的 key 是 ORL 字符串
- 行内容不再按 `Vec<String>` 原样长期存，而是压缩到 `CompactLines`

## `CompactLines` 的作用

当前缓存中的文件内容使用：

- `content: String`
- `line_starts: Vec<usize>`
- `encoding: String`

这种结构比直接缓存 `Vec<String>` 更紧凑，优势是：

- 降低小字符串分配数量
- 减少碎片
- 切片读取时仍能按行恢复

## TTL 与清理周期

这是两个不同概念：

### TTL

单个会话的缓存 TTL 当前是：

- `15 分钟`

### 后台清理周期

后台 cleaner 当前每：

- `1 分钟`

执行一次扫描，删除超时会话。

也就是说：

- 不是“每 15 分钟清理一次”
- 而是“每分钟扫描一次，删除超过 15 分钟没触碰的会话”

## 回收触发机制

后台 cleaner 启动后会：

1. 定期扫描并删除过期会话
2. 统计移除大小和当前活跃缓存大小
3. 如果启用了 `mimalloc-collect` feature，则异步调用 `mi_collect(true)`

实现特点：

- cleaner 用 `CancellationToken` 支持优雅关闭
- 回收调用放在 `spawn_blocking` 中，避免阻塞 async 运行时
- 即使本轮没有删除条目，也会尝试触发一次底层分配器回收

## 日志行为

清理时可能看到类似日志：

```text
缓存清理完成: 移除 3 个过期会话 (12.50 MB), 当前活跃: 2 个 (4.10 MB)
```

如果启用了 `mimalloc-collect`，还可能看到：

```text
libmimalloc_sys::mi_collect(true) 调用完成
```

## 生命周期

缓存会在以下场景清理：

### 1. 会话主动清理

搜索页关闭或显式删除会话时：

- `remove_sid(sid)`

### 2. 被动过期清理

超过 TTL 且未访问的会话会被 cleaner 删除。

### 3. 服务优雅关闭

`LogSeekModule::cleanup()` 会调用：

- `Cache::stop_cleaner()`

停止后台清理任务。

## 当前实现边界

- 主动内存归还只在 `opsbox-server` 集成 `logseek` 且开启 `mimalloc-collect` 时生效
- `opsbox-agent` 没有使用同样的全局 `mimalloc` 设置
- RSS 是否立刻下降取决于分配器和操作系统，不保证每次都可见
- 当前没有对外暴露专门的内存指标接口

## 结论

当前内存管理不是单纯“用了 mimalloc”，而是三层叠加：

1. `opsbox-server` 使用 `mimalloc`
2. `logseek` 缓存使用更紧凑的 `CompactLines`
3. cleaner 定时删除过期会话，并在可用时调用 `mi_collect(true)`

这也是当前仓库里和内存相关的主要实现事实。
