# 模块化架构

## 当前实现

OpsBox 的模块化不是运行时插件系统，而是：

- `inventory` 做编译期注册
- `optional dependencies` + Cargo feature 控制是否编进 `opsbox-server`
- `opsbox_core::Module` 作为统一接口

当前 `opsbox-server` 默认启用的模块是：

- `logseek`
- `agent-manager`
- `explorer`

## 核心接口

模块接口定义在 `backend/opsbox-core/src/module.rs`：

```rust
#[async_trait]
pub trait Module: Send + Sync {
    fn name(&self) -> &'static str;
    fn api_prefix(&self) -> &'static str;
    fn configure(&self) {}
    async fn init_schema(&self, pool: &SqlitePool) -> Result<(), Box<dyn std::error::Error>>;
    fn router(&self, pool: SqlitePool) -> Router;
    fn cleanup(&self) {}
}
```

生命周期是：

1. `opsbox-server` 调用 `opsbox_core::get_all_modules()`
2. 逐个执行 `configure()`
3. 逐个执行 `init_schema()`
4. 把 `router()` 挂到各自 `api_prefix()`
5. 关闭时调用 `cleanup()`

## 注册机制

模块 crate 内部通过：

```rust
opsbox_core::register_module!(YourModule);
```

把 `ModuleFactory` 提交给 `inventory`。

但是这里有一个当前实现必须注意的点：

仅仅在模块 crate 里 `register_module!` 还不够。`opsbox-server` 还必须显式引用这个可选依赖，否则 release 链接时可能把整个 crate 优化掉，导致 `inventory::submit!` 不生效。

这也是 `backend/opsbox-server/src/main.rs` 里存在这些声明的原因：

```rust
#[cfg(feature = "logseek")]
extern crate logseek;

#[cfg(feature = "agent-manager")]
extern crate agent_manager;

#[cfg(feature = "explorer")]
extern crate explorer;
```

所以“加完模块后完全不用碰 `main.rs`”这句话在当前实现里是不成立的。

## 添加新模块的真实步骤

### 1. 创建独立 crate 并实现 `Module`

最少需要：

- 依赖 `opsbox-core`
- 依赖 `inventory`
- 提供 `Default`
- 在 crate 内调用 `register_module!`

### 2. 在 `backend/opsbox-server/Cargo.toml` 加 optional dependency

示例：

```toml
[dependencies]
analytics = { path = "../analytics", optional = true }

[features]
default = ["logseek", "agent-manager", "explorer", "analytics"]
```

### 3. 在 `backend/opsbox-server/src/main.rs` 显式引用

示例：

```rust
#[cfg(feature = "analytics")]
extern crate analytics;
```

### 4. 重新构建并验证启动日志

启动时应看到：

- 模块被发现
- `configure()` 被调用
- `init_schema()` 被调用
- 路由被注册

## 编译行为

默认构建：

```bash
cargo build
```

只启用指定模块：

```bash
cargo build --features "logseek" --no-default-features
```

最小构建：

```bash
cargo build --no-default-features
```

此时仍然会有：

- `/healthy`
- 系统日志配置接口
- 内嵌前端静态资源 fallback

但不会有业务模块路由。

## 运行时特征

模块加载顺序不保证，因此模块之间不应依赖初始化顺序。

更准确地说，当前架构做到的是：

- 业务模块通过统一接口接入
- 编译期开关控制模块组合
- Server 对模块实现保持低耦合

但不是“完全零侵入扩展”，因为 `opsbox-server` 入口仍需保留可选 crate 的显式引用。
5. ✅ **零运行时开销**：无反射、无动态加载

这是 Rust 插件架构的最佳实践！🎯
