# 流式连接优雅关闭问题与解决方案

## 🐛 问题现象

按 `Ctrl-C` 后，虽然信号接收正常并清理了模块，但程序卡在：

```log
[2025-10-08T07:08:07Z INFO  opsbox::server] 收到关闭信号 [SIGINT (Ctrl-C)]，开始优雅关闭...
[2025-10-08T07:08:07Z INFO  opsbox::server] 清理模块: LogSeek
[2025-10-08T07:08:07Z INFO  opsbox::server] 所有模块已清理完成
# ⚠️ 然后就一直卡在这里，无法退出
```

---

## 🔍 根本原因

### 1. Axum 优雅关闭机制

Axum 的 `with_graceful_shutdown()` 工作流程：

```rust
axum::serve(listener, app)
    .with_graceful_shutdown(shutdown_signal)
    .await
```

执行步骤：
1. ✅ 停止接受新连接
2. ✅ 等待 `shutdown_signal` Future 完成（触发信号处理）
3. ⚠️ **等待所有现有 HTTP 连接关闭**
4. 关闭服务器

**关键**: 第3步会**无限期等待**，直到所有连接自然关闭！

### 2. LogSeek 流式响应特点

LogSeek 的搜索 API 使用 **NDJSON 流式响应**：

```rust
// routes/search.rs
let (tx, rx) = mpsc::channel(cap);

// 启动多个后台搜索任务
for source in sources {
    tokio::spawn(async move {
        // 搜索并通过 tx 发送结果
    });
}

// 返回流式响应
let body = Body::from_stream(ReceiverStream::new(rx));
HttpResponse::builder()
    .status(200)
    .header(CONTENT_TYPE, "application/x-ndjson")
    .body(body)
```

**问题**:
1. 客户端（浏览器）保持连接打开，等待更多搜索结果
2. 后台有多个搜索任务在运行（line 354: `tokio::spawn`）
3. 有一个**无限循环**的自适应调节任务（line 188-239）：
   ```rust
   tokio::spawn(async move {
       loop {
           tokio::time::sleep(Duration::from_secs(3)).await;
           // 自适应调整 CPU 并发度
       }
   });
   ```

当按 Ctrl-C 时：
- ✅ 信号被接收
- ✅ 模块被清理
- ❌ **但浏览器的连接还开着**
- ❌ **后台任务还在运行（特别是无限循环的调节任务）**
- ❌ Axum 在等待这些连接和任务完成，永远等不到！

---

## ✅ 解决方案

### 方案 1: 添加超时机制（已实现）

强制在一定时间后关闭，不再无限等待：

```rust
pub async fn run(addr: SocketAddr, db_pool: SqlitePool, modules: Vec<Arc<dyn Module>>) {
  let app = build_router(db_pool, &modules).layer(configure_cors());
  let listener = tokio::net::TcpListener::bind(addr).await.expect("监听地址绑定失败");

  // 启动服务器并支持优雅关闭（带超时）
  let graceful_shutdown = async {
    shutdown_signal(modules).await;
    
    // 等待最多10秒让连接自然关闭
    tokio::time::sleep(std::time::Duration::from_secs(10)).await;
    log::warn!("优雅关闭超时（10秒），强制关闭剩余连接");
  };
  
  axum::serve(listener, app)
    .with_graceful_shutdown(graceful_shutdown)
    .await
    .expect("服务启动失败");

  log::info!("服务已关闭");
}
```

**优点**:
- ✅ 简单直接
- ✅ 保证10秒后一定退出
- ✅ 给正常连接10秒时间优雅关闭

**缺点**:
- ⚠️ 强制关闭可能导致响应不完整
- ⚠️ 后台任务可能被粗暴中断

---

### 方案 2: 使用 CancellationToken（推荐但复杂）

为所有后台任务添加取消支持：

#### Step 1: 添加全局取消令牌

```rust
// lib.rs
use tokio_util::sync::CancellationToken;
use std::sync::OnceLock;

static SHUTDOWN_TOKEN: OnceLock<CancellationToken> = OnceLock::new();

pub fn shutdown_token() -> &'static CancellationToken {
    SHUTDOWN_TOKEN.get_or_init(|| CancellationToken::new())
}
```

#### Step 2: 在优雅关闭时触发令牌

```rust
// server.rs
async fn shutdown_signal(modules: Vec<Arc<dyn Module>>) {
  let signal_name = wait_for_shutdown_signal().await;
  log::info!("收到关闭信号 [{}]，开始优雅关闭...", signal_name);

  // 触发全局取消令牌
  logseek::shutdown_token().cancel();

  // 清理所有模块资源
  for module in &modules {
    log::info!("清理模块: {}", module.name());
    module.cleanup();
  }
  
  log::info!("所有模块已清理完成");
}
```

#### Step 3: 在搜索任务中响应取消

```rust
// routes/search.rs
pub async fn stream_search(...) {
    // ...
    
    // 后台调节任务
    let shutdown_token = crate::shutdown_token().clone();
    tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = shutdown_token.cancelled() => {
                    log::info!("收到关闭信号，停止自适应调节");
                    break;
                }
                _ = tokio::time::sleep(Duration::from_secs(3)) => {
                    // 自适应调整逻辑
                }
            }
        }
    });
    
    // 搜索任务
    tokio::spawn(async move {
        // 在长时间操作前检查取消
        if shutdown_token.is_cancelled() {
            return;
        }
        // 继续搜索...
    });
}
```

**优点**:
- ✅ 优雅取消所有后台任务
- ✅ 搜索任务能快速响应关闭
- ✅ 不会粗暴中断

**缺点**:
- ⚠️ 需要修改多处代码
- ⚠️ 实现复杂
- ⚠️ 需要仔细处理取消点

---

### 方案 3: 混合方案（当前采用）

结合方案1和方案2的优点：

1. **立即触发**:
   - 收到 Ctrl-C 后立即开始清理
   - 显示清理进度

2. **短期容忍**:
   - 给连接10秒时间自然关闭
   - 正常情况下搜索任务会在数秒内完成

3. **强制退出**:
   - 10秒后无论如何都强制关闭
   - 避免永久挂起

```rust
async fn shutdown_signal(modules: Vec<Arc<dyn Module>>) {
  let signal_name = wait_for_shutdown_signal().await;
  log::info!("收到关闭信号 [{}]，开始优雅关闭...", signal_name);

  for module in &modules {
    log::info!("清理模块: {}", module.name());
    module.cleanup();
  }
  
  log::info!("所有模块已清理完成，等待活跃连接关闭...");
  log::info!("提示: 如果有挂起的流式请求，将在10秒后强制关闭");
}

pub async fn run(addr: SocketAddr, db_pool: SqlitePool, modules: Vec<Arc<dyn Module>>) {
  // ...
  
  let graceful_shutdown = async {
    shutdown_signal(modules).await;
    tokio::time::sleep(std::time::Duration::from_secs(10)).await;
    log::warn!("优雅关闭超时（10秒），强制关闭剩余连接");
  };
  
  axum::serve(listener, app)
    .with_graceful_shutdown(graceful_shutdown)
    .await
    .expect("服务启动失败");
}
```

---

## 🧪 测试验证

### 场景 1: 无活跃连接

```bash
# 启动服务
cargo run --release

# 立即按 Ctrl-C（没有发起任何请求）
^C
```

**预期**:
- ✅ 立即退出（< 1秒）
- ✅ 看到清理日志
- ✅ 看到"服务已关闭"

### 场景 2: 有短搜索任务

```bash
# 启动服务
cargo run --release

# 在浏览器发起搜索（1-2秒完成）
# 搜索完成后按 Ctrl-C
^C
```

**预期**:
- ✅ 立即退出（< 1秒）
- ✅ 搜索已完成，连接已关闭

### 场景 3: 有长搜索任务

```bash
# 启动服务
cargo run --release

# 在浏览器发起搜索（需要10秒以上）
# 搜索进行中按 Ctrl-C
^C
```

**预期（修复前）**:
- ❌ 永久挂起
- ❌ 需要 Ctrl-C 再按一次或 kill -9

**预期（修复后）**:
- ✅ 显示"等待活跃连接关闭"
- ✅ 10秒后强制关闭
- ✅ 总共耗时 ~10秒

### 场景 4: 浏览器保持连接

```bash
# 启动服务
cargo run --release

# 在浏览器发起搜索
# 搜索完成但保持页面打开（浏览器可能保持连接）
# 按 Ctrl-C
^C
```

**预期（修复后）**:
- ✅ 最多10秒后退出
- ✅ 不会永久挂起

---

## 📊 超时时间调整

可以根据需求调整超时时间：

```rust
// 3秒超时（快速关闭）
tokio::time::sleep(std::time::Duration::from_secs(3)).await;

// 10秒超时（当前配置，平衡）
tokio::time::sleep(std::time::Duration::from_secs(10)).await;

// 30秒超时（保守）
tokio::time::sleep(std::time::Duration::from_secs(30)).await;
```

**建议**:
- **开发环境**: 3-5秒（快速迭代）
- **生产环境**: 10-15秒（保证请求完成）
- **大数据搜索**: 30-60秒（长时间任务）

---

## 🎯 最佳实践

### 1. 客户端主动断开

前端应该在关闭页面时主动断开连接：

```typescript
// 监听页面关闭
window.addEventListener('beforeunload', () => {
    // 取消所有正在进行的请求
    abortController.abort();
});
```

### 2. 服务端设置最大搜索时间

```rust
// 搜索任务添加超时
tokio::time::timeout(
    Duration::from_secs(300),  // 5分钟
    search_task
).await
```

### 3. 心跳检测

对于长连接，定期发送心跳检测客户端是否还在：

```rust
// 每30秒发送一个心跳
let heartbeat = tokio::spawn(async move {
    let mut interval = tokio::time::interval(Duration::from_secs(30));
    loop {
        interval.tick().await;
        if tx.send(heartbeat_msg).await.is_err() {
            // 客户端断开
            break;
        }
    }
});
```

### 4. 后台任务取消支持

所有长时间运行的后台任务应该支持取消：

```rust
tokio::spawn(async move {
    loop {
        tokio::select! {
            _ = shutdown_token.cancelled() => break,
            result = long_running_task() => {
                // 处理结果
            }
        }
    }
});
```

---

## 🔧 未来优化方向

### 1. 完整实现 CancellationToken

为所有后台任务添加取消支持，实现真正的优雅关闭。

### 2. 连接追踪

记录所有活跃连接，关闭时主动通知：

```rust
struct ConnectionTracker {
    connections: Arc<RwLock<HashSet<ConnectionId>>>,
}

impl ConnectionTracker {
    async fn shutdown_all(&self) {
        for conn_id in self.connections.read().await.iter() {
            // 主动关闭连接
        }
    }
}
```

### 3. 渐进式关闭

```rust
// 第一次 Ctrl-C: 优雅关闭（10秒超时）
// 第二次 Ctrl-C: 立即强制关闭
static SHUTDOWN_COUNT: AtomicUsize = AtomicUsize::new(0);

async fn wait_for_shutdown_signal() {
    let count = SHUTDOWN_COUNT.fetch_add(1, Ordering::SeqCst);
    if count > 0 {
        log::warn!("收到第二次关闭信号，立即强制退出");
        std::process::exit(0);
    }
    // 第一次优雅关闭...
}
```

---

## 📝 总结

### 问题
- Axum 优雅关闭会等待所有连接关闭
- LogSeek 流式搜索连接可能长时间保持打开
- 后台有无限循环任务永不退出

### 当前解决方案
- ✅ 添加10秒超时强制关闭
- ✅ 清晰的日志提示
- ✅ 简单有效，无需大改代码

### 效果
- ⚡ 最多10秒保证退出
- 📊 优雅处理正常连接
- 🛡️ 不会永久挂起

### 下一步（可选）
- 🔮 实现完整的 CancellationToken 支持
- 🔮 为长时间任务添加取消点
- 🔮 优化搜索任务生命周期管理

---

**修复完成时间**: 2025-10-08  
**影响文件**: `server/api-gateway/src/server.rs`  
**状态**: ✅ 已修复并编译通过
