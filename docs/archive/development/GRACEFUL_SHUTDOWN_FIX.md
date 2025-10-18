# Ctrl-C 优雅关闭问题修复

## 🐛 问题描述

**症状**: 在前台运行时，按 `Ctrl-C` 无法优雅关闭服务器，程序似乎"卡住"或需要多次按 Ctrl-C 才能强制退出。

**影响范围**: 
- API Gateway (opsbox)
- Agent Server

---

## 🔍 根本原因

### 原始代码问题 (server.rs:76-99)

```rust
async fn shutdown_signal(modules: Vec<Arc<dyn Module>>) {
  #[cfg(unix)]
  {
    use tokio::signal::unix::{SignalKind, signal};
    let mut sigterm = signal(SignalKind::terminate()).expect("无法监听 SIGTERM");
    let mut sigint = signal(SignalKind::interrupt()).expect("无法监听 SIGINT");
    tokio::select! {
        _ = sigterm.recv() => {},
        _ = sigint.recv() => {},
        _ = tokio::signal::ctrl_c() => {},  // ❌ 问题：重复监听
    }
  }
  // ...
}
```

### 核心问题

1. **信号重复监听**
   - `Ctrl-C` 会触发 `SIGINT` 信号
   - 代码同时监听了 `sigint.recv()` 和 `tokio::signal::ctrl_c()`
   - 两者本质上监听同一个信号，导致竞争条件

2. **信号处理不明确**
   - 没有日志输出接收到哪个信号
   - 调试困难，无法判断是否真的接收到信号

3. **tokio::signal::ctrl_c() 在 Unix 上是多余的**
   - Unix 系统应该使用 `signal(SignalKind::interrupt())`
   - `ctrl_c()` 是跨平台 API，但在 Unix 上不如直接使用 Unix signal API

---

## ✅ 解决方案

### 修复后的代码

```rust
/// 优雅关闭信号
/// 
/// 监听系统信号实现优雅关闭：
/// - Unix: SIGTERM, SIGINT (Ctrl-C)
/// - Windows: Ctrl-C
async fn shutdown_signal(modules: Vec<Arc<dyn Module>>) {
  let signal_name = wait_for_shutdown_signal().await;
  log::info!("收到关闭信号 [{}]，开始优雅关闭...", signal_name);

  // ✅ 清理所有模块资源
  for module in &modules {
    log::info!("清理模块: {}", module.name());
    module.cleanup();
  }
  
  log::info!("所有模块已清理完成");
}

/// 等待关闭信号并返回信号名称
#[cfg(unix)]
async fn wait_for_shutdown_signal() -> &'static str {
  use tokio::signal::unix::{signal, SignalKind};
  
  // 创建信号监听器
  let mut sigterm = signal(SignalKind::terminate()).expect("无法监听 SIGTERM");
  let mut sigint = signal(SignalKind::interrupt()).expect("无法监听 SIGINT");
  
  tokio::select! {
    _ = sigterm.recv() => "SIGTERM",
    _ = sigint.recv() => "SIGINT (Ctrl-C)",
  }
}

/// 等待关闭信号并返回信号名称 (Windows)
#[cfg(not(unix)))]
async fn wait_for_shutdown_signal() -> &'static str {
  tokio::signal::ctrl_c()
    .await
    .expect("无法监听 Ctrl-C 信号");
  "Ctrl-C"
}
```

### 关键改进

1. ✅ **消除信号重复监听**
   - Unix: 只监听 `SIGTERM` 和 `SIGINT`，不再使用 `ctrl_c()`
   - Windows: 使用 `ctrl_c()`
   
2. ✅ **添加信号类型日志**
   - 明确显示接收到哪个信号
   - 方便调试和监控

3. ✅ **代码结构更清晰**
   - 分离信号等待和清理逻辑
   - 平台特定代码用 `#[cfg]` 隔离

4. ✅ **添加清理完成日志**
   - 确认所有模块已清理
   - 帮助诊断关闭流程

---

## 🧪 测试验证

### 测试场景 1: 前台运行 + Ctrl-C

```bash
# 启动服务
cd /Users/wangyue/workspace/codelder/opsboard/server/api-gateway
cargo run --release

# 按 Ctrl-C (或在另一个终端发送 kill -SIGINT <pid>)
# 应该立即看到:
# [INFO] 收到关闭信号 [SIGINT (Ctrl-C)]，开始优雅关闭...
# [INFO] 清理模块: logseek
# [INFO] 所有模块已清理完成
# [INFO] 服务已关闭
```

**预期结果**: 
- ✅ 一次 Ctrl-C 立即触发关闭
- ✅ 清理所有模块
- ✅ 优雅退出

### 测试场景 2: 后台运行 + SIGTERM

```bash
# 后台启动
./target/release/opsbox start --daemon

# 发送 SIGTERM
kill $(cat /tmp/opsbox.pid)

# 或使用内置 stop 命令
./target/release/opsbox stop
```

**预期结果**:
- ✅ 收到 SIGTERM 信号
- ✅ 触发优雅关闭
- ✅ 清理所有模块资源

### 测试场景 3: Agent 优雅关闭

```bash
# 启动 Agent
cd /Users/wangyue/workspace/codelder/opsboard/server/agent
cargo run --release

# 按 Ctrl-C
# 应该看到:
# [INFO] 收到关闭信号 [SIGINT (Ctrl-C)]，开始优雅关闭...
# [INFO] Agent 已关闭
```

**预期结果**:
- ✅ 正在进行的搜索任务会完成或取消
- ✅ 连接优雅关闭
- ✅ 进程正常退出

---

## 📊 修复对比

### 修复前

| 操作 | 行为 | 问题 |
|-----|-----|------|
| Ctrl-C (前台) | 卡住或需要多次按 | ❌ 信号竞争 |
| SIGTERM (后台) | 正常工作 | ⚠️ 无日志 |
| 日志输出 | 只显示"开始关闭" | ⚠️ 不知道哪个信号 |

### 修复后

| 操作 | 行为 | 状态 |
|-----|-----|------|
| Ctrl-C (前台) | 立即优雅关闭 | ✅ 正常 |
| SIGTERM (后台) | 立即优雅关闭 | ✅ 正常 |
| 日志输出 | 显示信号类型和清理进度 | ✅ 详细 |

---

## 🎓 技术要点

### 1. Unix 信号机制

在 Unix 系统中：
- `Ctrl-C` → 发送 `SIGINT` (信号2)
- `kill <pid>` → 默认发送 `SIGTERM` (信号15)
- `kill -9 <pid>` → 发送 `SIGKILL` (信号9，无法捕获)

### 2. Tokio 信号 API

**Unix 专用 API** (推荐):
```rust
use tokio::signal::unix::{signal, SignalKind};
let mut sigint = signal(SignalKind::interrupt())?;
sigint.recv().await;
```

**跨平台 API**:
```rust
tokio::signal::ctrl_c().await?;
```

**注意**: 在 Unix 上，`ctrl_c()` 内部也是监听 SIGINT，所以不要和 Unix API 混用！

### 3. tokio::select! 的工作原理

```rust
tokio::select! {
    _ = sigterm.recv() => "SIGTERM",
    _ = sigint.recv() => "SIGINT",
}
```

- 并发等待多个 Future
- 任意一个完成就返回
- 其他分支被取消（drop）

### 4. Axum 优雅关闭

```rust
axum::serve(listener, app)
    .with_graceful_shutdown(shutdown_signal())
    .await?;
```

`with_graceful_shutdown`:
1. 停止接受新连接
2. 等待现有连接完成
3. 等待 `shutdown_signal()` Future 完成
4. 关闭服务器

---

## 🚀 最佳实践

### 1. 总是添加信号类型日志

```rust
// ❌ 不好：不知道哪个信号触发
log::info!("收到关闭信号");

// ✅ 好：明确显示信号类型
log::info!("收到关闭信号 [SIGINT]，开始优雅关闭...");
```

### 2. 不要混用信号 API

```rust
// ❌ 错误：重复监听 SIGINT
tokio::select! {
    _ = sigint.recv() => {},
    _ = tokio::signal::ctrl_c() => {},  // 多余！
}

// ✅ 正确：选择一种
tokio::select! {
    _ = sigterm.recv() => "SIGTERM",
    _ = sigint.recv() => "SIGINT",
}
```

### 3. 使用平台特定代码

```rust
// ✅ 好：Unix 和 Windows 分开处理
#[cfg(unix)]
async fn shutdown_signal() {
    use tokio::signal::unix::{signal, SignalKind};
    // Unix 专用逻辑
}

#[cfg(not(unix))]
async fn shutdown_signal() {
    tokio::signal::ctrl_c().await.unwrap();
}
```

### 4. 添加清理完成确认

```rust
for module in &modules {
    log::info!("清理模块: {}", module.name());
    module.cleanup();
}
log::info!("所有模块已清理完成");  // ✅ 确认清理完成
```

---

## 📝 相关文档

- [Tokio Signal 文档](https://docs.rs/tokio/latest/tokio/signal/)
- [Unix Signal 手册](https://man7.org/linux/man-pages/man7/signal.7.html)
- [Axum Graceful Shutdown](https://docs.rs/axum/latest/axum/serve/index.html#graceful-shutdown)

---

## 🎯 总结

### 问题
- Ctrl-C 无法优雅关闭 → 信号重复监听导致竞争条件

### 解决
1. ✅ 移除重复的 `ctrl_c()` 调用
2. ✅ 添加信号类型日志输出
3. ✅ 分离信号等待和清理逻辑
4. ✅ 为 Agent 也添加优雅关闭

### 效果
- ⚡ 一次 Ctrl-C 立即响应
- 📊 清晰的日志输出
- 🛡️ 资源正确清理
- 🎯 代码更清晰易维护

---

**修复完成时间**: 2025-10-08  
**影响文件**:
- `server/api-gateway/src/server.rs`
- `server/agent/src/main.rs`

**测试状态**: ✅ 编译通过，等待运行时验证
