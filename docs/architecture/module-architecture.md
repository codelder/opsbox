# 模块化架构文档

**文档版本**: v1.1  
**最后更新**: 2026-02-24

## 概述

OpsBox 采用 **inventory + optional dependencies** 的模块化架构，实现了真正的插件式开发：

- ✅ **编译时自动发现**：使用 `inventory` crate 在编译时收集所有注册的模块
- ✅ **运行时零配置**：`opsbox-server` 无需修改代码，自动加载所有已编译的模块
- ✅ **可选依赖**：通过 Cargo features 控制编译哪些模块
- ✅ **完全解耦**：`opsbox-server` 不直接依赖具体业务模块

---

## 架构图

```
                    ┌──────────────┐
                    │ opsbox-core  │
                    │ (Module trait│
                    │  + Registry) │
                    └──────────────┘
                         ↑     ↑
                 ┌───────┘     └───────┐
                 │                     │
        ┌────────┴──────┐     ┌────────┴──────┐
        │ opsbox-server │     │   logseek     │
        │ (optional=true│     │ (implements   │
        │  dependencies)│     │   Module)     │
        └───────────────┘     └───────────────┘
              ↓
        自动发现所有模块
        get_all_modules()
```

---

## 核心组件

### 1️⃣ `opsbox-core/src/module.rs` - 模块接口

```rust
#[async_trait]
pub trait Module: Send + Sync {
    fn name(&self) -> &'static str;
    fn api_prefix(&self) -> &'static str;
    async fn init_schema(&self, pool: &SqlitePool) -> Result<(), Box<dyn std::error::Error>>;
    fn router(&self, pool: SqlitePool) -> Router;
    fn cleanup(&self) {}
}
```

### 2️⃣ 模块注册宏

```rust
opsbox_core::register_module!(LogSeekModule);
```

展开后：
```rust
inventory::submit! {
    opsbox_core::module::ModuleFactory::new(|| {
        std::sync::Arc::new(LogSeekModule::default())
    })
}
```

### 3️⃣ 自动发现

```rust
let modules = opsbox_core::get_all_modules();  // 编译时收集的所有模块
```

---

## 添加新模块（3 步走）

### 步骤 1：创建模块并实现 Module trait

**`server/analytics/src/lib.rs`**:
```rust
use opsbox_core::{Module, SqlitePool};
use axum::Router;

#[derive(Default)]
pub struct AnalyticsModule;

#[async_trait::async_trait]
impl Module for AnalyticsModule {
    fn name(&self) -> &'static str {
        "Analytics"
    }

    fn api_prefix(&self) -> &'static str {
        "/api/v1/analytics"
    }

    async fn init_schema(&self, pool: &SqlitePool) -> std::result::Result<(), Box<dyn std::error::Error>> {
        // 初始化数据库
        Ok(())
    }

    fn router(&self, pool: SqlitePool) -> Router {
        // 创建路由
        Router::new()
    }
}

// ✅ 自动注册（只需一行）
opsbox_core::register_module!(AnalyticsModule);
```

**`backend/your-module/Cargo.toml`**:
```toml
[dependencies]
opsbox-core = { path = "../opsbox-core" }
inventory = "0.3"  # 必需
```

### 步骤 2：在 opsbox-server 添加 optional 依赖

**`backend/opsbox-server/Cargo.toml`**:
```toml
[dependencies]
logseek = { path = "../logseek", optional = true }
analytics = { path = "../analytics", optional = true }  # ← 添加这一行

[features]
default = ["logseek", "analytics"]  # ← 更新这一行
```

### 步骤 3：完成！🎉

无需修改 `opsbox-server/src/main.rs` 或 `server.rs`，模块会自动被发现和加载！

---

## 编译选项

### 默认编译（所有模块）
```bash
cargo build
```

### 只编译 logseek 模块
```bash
cargo build --features logseek --no-default-features
```

### 最小化编译（无业务模块）
```bash
cargo build --no-default-features
# 只有健康检查和 SPA fallback，体积最小
```

### 自定义组合
```bash
cargo build --features "logseek,analytics" --no-default-features
```

---

## 运行时行为

### 启动日志示例
```
[INFO] 数据库连接池初始化成功
[INFO] 发现 2 个模块
[INFO] 初始化模块: LogSeek
[INFO] 初始化模块: Analytics
[INFO] 所有模块初始化完成
[INFO] 启动 HTTP 服务器，监听地址: 0.0.0.0:4000
[INFO] 注册路由: LogSeek -> /api/v1/logseek
[INFO] 注册路由: Analytics -> /api/v1/analytics
[INFO] OpsBox 服务启动成功，访问地址: http://0.0.0.0:4000
```

### 优雅关闭
```
^C[INFO] 收到关闭信号，开始优雅关闭...
[INFO] 清理模块: LogSeek
[INFO] 清理模块: Analytics
[INFO] 服务已关闭
```

---

## 技术细节

### inventory 工作原理

1. **编译时收集**：
   - `inventory::submit!` 将模块工厂函数放入特殊的 ELF section
   - 链接器将所有 section 合并

2. **运行时迭代**：
   - `inventory::iter()` 读取这些 section 的内容
   - 创建模块实例

3. **无运行时开销**：
   - 不使用反射或动态加载
   - 所有模块在编译时确定

### 为什么不用 ctor？

| 特性 | inventory | ctor |
|-----|-----------|------|
| 执行时机 | 按需迭代 | 程序启动前 |
| 性能 | 延迟初始化 | 启动变慢 |
| 可靠性 | 更稳定 | 初始化顺序不确定 |
| 可调试性 | 容易追踪 | 难以调试 |

---

## 最佳实践

### ✅ 推荐

1. **模块独立**：每个模块应该是独立的 Rust crate
2. **最小依赖**：只依赖 `opsbox-core`
3. **单一职责**：一个模块只做一件事
4. **向后兼容**：API 变更需要考虑兼容性

### ❌ 避免

1. **模块间直接依赖**：不要让 `analytics` 依赖 `logseek`
2. **全局状态**：尽量使用依赖注入而非全局变量
3. **硬编码配置**：使用环境变量或配置文件

---

## 常见问题

### Q: 如何禁用某个模块？
A: 在编译时不包含它：
```bash
cargo build --features logseek --no-default-features
```

### Q: 模块加载顺序是否保证？
A: 不保证。模块应该相互独立，不依赖加载顺序。

### Q: 如何在模块间共享代码？
A: 将共享代码放入 `opsbox-core` 或创建独立的 utility crate。

### Q: 为什么需要在模块中添加 inventory 依赖？
A: `register_module!` 宏展开后会调用 `inventory::submit!`，所以模块需要依赖它。

---

## 总结

这个架构实现了：

1. ✅ **opsbox-server 完全解耦**：添加新模块不需要修改它
2. ✅ **编译时灵活性**：通过 features 控制编译内容
3. ✅ **运行时自动发现**：无需手动注册
4. ✅ **类型安全**：在编译时检查模块实现
5. ✅ **零运行时开销**：无反射、无动态加载

这是 Rust 插件架构的最佳实践！🎯

