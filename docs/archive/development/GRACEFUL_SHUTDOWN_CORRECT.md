# 优雅关闭正确实现 ✅

## 🎯 问题演进历史

### v1: Ctrl-C 无响应 ❌
- **问题**: 重复监听 SIGINT 信号
- **修复**: 移除重复监听

### v2: 信号后永久卡住 ❌  
- **问题**: 无超时机制
- **修复**: 添加超时（但实现错误）

### v3: 无连接也等10秒 ❌
- **问题**: 无条件 sleep 10秒
- **修复**: 用 timeout 包裹（但包裹了整个服务器）

### v4: 服务只能运行10秒 ❌
- **问题**: timeout 包裹了整个服务器运行过程
- **修复**: ✅ 在信号处理函数内启动后台超时任务

---

## ✅ 正确的实现（当前版本）

### 核心思想

**问题**: Axum 的 `with_graceful_shutdown` 会在信号处理完成后继续等待所有连接关闭，这个等待可能是无限的。

**解决**: 在信号处理函数返回前，启动一个后台任务，10秒后强制退出进程。

### 代码实现

```rust
async fn shutdown_with_timeout(modules: Vec<Arc<dyn Module>>) {
  // 1. 等待关闭信号
  let signal_name = wait_for_shutdown_signal().await;
  log::info!("收到关闭信号 [{}]，开始优雅关闭...", signal_name);

  // 2. 清理所有模块资源
  for module in &modules {
    log::info!("清理模块: {}", module.name());
    module.cleanup();
  }
  
  log::info!("所有模块已清理完成，等待活跃连接关闭...");
  
  // 3. 启动后台超时任务（关键！）
  tokio::spawn(async move {
    tokio::time::sleep(Duration::from_secs(10)).await;
    log::warn!("优雅关闭超时（10秒），仍有活跃连接未关闭");
    log::warn!("强制退出进程...");
    std::process::exit(0);  // 强制退出
  });
  
  // 4. 返回，让 Axum 开始等待连接关闭
  // 如果连接在10秒内关闭，上面的 spawn 不会执行 exit
  // 如果连接10秒后还未关闭，上面的 spawn 会强制退出
}

pub async fn run(addr: SocketAddr, db_pool: SqlitePool, modules: Vec<Arc<dyn Module>>) {
  // ...
  
  // 启动服务器（正常运行，不受超时影响）
  axum::serve(listener, app)
    .with_graceful_shutdown(shutdown_with_timeout(modules))
    .await
    .expect("服务启动失败");

  log::info!("服务已关闭");
}
```

### 工作流程

```
正常运行阶段（无时间限制）：
┌─────────────────────────────────────┐
│ Axum 服务器正常运行                  │
│ 处理 HTTP 请求                       │
│ 可以运行几小时、几天、几个月...        │
└─────────────────────────────────────┘
                 ↓
        (用户按 Ctrl-C)
                 ↓
关闭阶段（有10秒超时）：
┌─────────────────────────────────────┐
│ 1. 收到 SIGINT 信号                  │
│ 2. 清理模块资源                      │
│ 3. 启动后台超时任务（10秒后 exit(0)）│
│ 4. 返回，Axum 开始等待连接关闭       │
└─────────────────────────────────────┘
                 ↓
        两种可能的结果：
                 ↓
    ┌────────────┴────────────┐
    ↓                          ↓
情况 A:                   情况 B:
连接在10秒内关闭           连接10秒后仍未关闭
    ↓                          ↓
正常退出                   后台任务触发 exit(0)
(< 10秒)                   (10秒强制退出)
```

---

## 🧪 测试验证

### 场景 1: 无活跃连接

```bash
cargo run --release
# 立即按 Ctrl-C
```

**预期**: ⚡ < 1秒退出

**日志**:
```log
^C[INFO] 收到关闭信号 [SIGINT (Ctrl-C)]，开始优雅关闭...
[INFO] 清理模块: LogSeek
[INFO] 所有模块已清理完成，等待活跃连接关闭...
[INFO] 服务已关闭
```

---

### 场景 2: 短搜索任务

```bash
cargo run --release
# 发起搜索（假设3秒完成）
# 搜索进行中按 Ctrl-C
```

**预期**: ⏱️ ~3秒退出（等搜索完成）

**日志**:
```log
[INFO] 找到匹配结果...
^C[INFO] 收到关闭信号 [SIGINT (Ctrl-C)]，开始优雅关闭...
[INFO] 清理模块: LogSeek
[INFO] 所有模块已清理完成，等待活跃连接关闭...
# 等待3秒...
[INFO] 服务已关闭
```

---

### 场景 3: 长搜索任务

```bash
cargo run --release
# 发起大量搜索（假设需要30秒）
# 搜索进行中按 Ctrl-C
```

**预期**: ⏱️ ~10秒强制退出

**日志**:
```log
[INFO] 找到匹配结果...
^C[INFO] 收到关闭信号 [SIGINT (Ctrl-C)]，开始优雅关闭...
[INFO] 清理模块: LogSeek
[INFO] 所有模块已清理完成，等待活跃连接关闭...
# 等待10秒...
[WARN] 优雅关闭超时（10秒），仍有活跃连接未关闭
[WARN] 强制退出进程...
# 进程立即退出
```

---

## 📊 行为对比

| 场景 | v1 | v2 | v3 | v4 | v5 (当前) |
|-----|----|----|----|----|-----------|
| **正常运行** | ✅ | ✅ | ✅ | ❌ 10秒 | ✅ 无限制 |
| **无连接Ctrl-C** | ❌ 卡 | ❌ 卡 | ⚠️ 10s | ❌ 10s | ✅ < 1s |
| **短任务Ctrl-C** | ❌ 卡 | ❌ 卡 | ⚠️ 10s | ❌ 10s | ✅ ~任务时长 |
| **长任务Ctrl-C** | ❌ 永久卡 | ❌ 永久卡 | ⚠️ 10s | ❌ 10s | ✅ ~10s |

---

## 💡 关键技术点

### 1. tokio::spawn 后台任务

```rust
// 启动一个独立的后台任务
tokio::spawn(async move {
    tokio::time::sleep(Duration::from_secs(10)).await;
    std::process::exit(0);  // 10秒后强制退出
});

// 主函数继续执行，不会阻塞
return;  // 让 Axum 继续等待连接
```

**关键**: 
- `spawn` 创建独立任务，不阻塞当前函数
- 如果主进程在10秒内正常退出，spawn 任务也会被清理
- 如果主进程10秒后还在运行，spawn 任务会强制 exit

### 2. std::process::exit(0)

```rust
std::process::exit(0);  // 立即终止整个进程
```

**效果**:
- 不会等待任何 async 任务完成
- 不会调用析构函数（destructors）
- 立即退出，代码 0 表示正常退出

### 3. 为什么这样有效

```rust
async fn shutdown_with_timeout(...) {
    // ... 清理 ...
    
    tokio::spawn(async move {
        sleep(10s).await;
        exit(0);  // ← 这是"保险"
    });
    
    return;  // ← 让 Axum 等待连接
}
```

**时间线**:
- T+0s: 按 Ctrl-C，清理完成，启动 spawn，函数返回
- T+0s ~ T+10s: Axum 等待连接关闭
  - 如果连接在这期间关闭 → 正常退出 ✅
- T+10s: spawn 任务触发 exit(0) → 强制退出 ✅

---

## ⚠️ 之前错误的实现

### 错误1: 无条件 sleep

```rust
// ❌ 错误
async fn shutdown(...) {
    cleanup();
    tokio::time::sleep(Duration::from_secs(10)).await;  // 总是等10秒
}
```

**问题**: 即使没有连接也等10秒

### 错误2: timeout 包裹整个服务器

```rust
// ❌ 错误
tokio::time::timeout(
    Duration::from_secs(10),
    axum::serve(listener, app).with_graceful_shutdown(signal)
).await
```

**问题**: 服务器运行10秒后就被强制终止

### 正确: 后台任务 + exit

```rust
// ✅ 正确
async fn shutdown(...) {
    cleanup();
    tokio::spawn(async move {
        sleep(10s).await;
        exit(0);
    });
    return;  // 让 Axum 继续等待
}
```

**效果**: 服务器正常运行，只在关闭时才有10秒限制

---

## ✅ 验证清单

- [x] 编译通过
- [x] 服务器可以正常运行（无10秒限制）
- [x] 无连接时 Ctrl-C 立即退出
- [x] 有短任务时等任务完成后退出
- [x] 有长任务时10秒强制退出
- [x] 日志清晰明确

---

## 🎉 最终效果

### 正常运行
- ✅ 无时间限制，可以运行任意长时间
- ✅ 处理所有 HTTP 请求

### 优雅关闭
- ⚡ **无连接**: < 1秒退出
- ⏱️ **短任务**: 等任务完成（几秒）
- 🛡️ **长任务**: 最多10秒强制退出
- 📊 **清晰日志**: 知道发生了什么

---

**修复完成**: 2025-10-08  
**版本**: v5 (最终正确版)  
**状态**: ✅ 生产就绪，已验证
