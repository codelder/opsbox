# Agent Manager - OpsBox 独立模块

## ✅ 架构设计

Agent Manager 现在是一个**独立的 OpsBox 模块**，而不是 LogSeek 的子模块。

### 为什么这样设计？

1. ✅ **Agent 是 OpsBox 级别的服务** - 不属于任何特定模块
2. ✅ **所有模块都可以使用 Agent** - LogSeek、Analytics、Monitoring 等
3. ✅ **符合模块化架构** - 职责清晰，便于维护
4. ✅ **便于独立扩展** - Agent 功能可以独立迭代

---

## 📁 项目结构

```
backend/
├── agent-manager/           # ✅ 新创建的独立模块
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs          # 模块入口，注册到 OpsBox
│       ├── models.rs       # Agent 数据模型
│       ├── manager.rs      # Agent 管理器
│       └── routes.rs       # API 路由
├── agent/                   # Agent 客户端（独立运行）
├── logseek/                 # LogSeek 模块
└── opsbox-server/           # OpsBox Server（引用 agent-manager）
```

---

## 🔌 API 端点

Agent Manager 模块提供以下端点（前缀 `/api/v1/agents`）：

| 端点 | 方法 | 说明 |
|------|------|------|
| `/register` | POST | Agent 注册 |
| `/` | GET | 列出所有 Agent |
| `/:agent_id` | GET | 获取 Agent 信息 |
| `/:agent_id` | DELETE | 注销 Agent |
| `/:agent_id/heartbeat` | POST | Agent 心跳 |

### 完整路径

由于模块前缀是 `/api/v1/agents`，完整路径为：

- `POST http://localhost:4000/api/v1/agents/register`
- `GET http://localhost:4000/api/v1/agents`
- `GET http://localhost:4000/api/v1/agents/{agent_id}`
- `DELETE http://localhost:4000/api/v1/agents/{agent_id}`
- `POST http://localhost:4000/api/v1/agents/{agent_id}/heartbeat`

---

## 🚀 使用方法

### 1. 启动 OpsBox Server

```bash
cd PROJECT_ROOT/backend/opsbox-server
cargo run --release

# 应该看到:
# [INFO] 发现 2 个模块
# [INFO] 配置模块: LogSeek
# [INFO] Agent Manager 模块配置完成
# [INFO] 配置模块: AgentManager
# [INFO] 初始化模块数据库: LogSeek
# [INFO] 初始化模块数据库: AgentManager
# [INFO] Agent Manager: 暂不需要数据库表
# [INFO] 注册路由: LogSeek -> /api/v1/logseek
# [INFO] 注册路由: AgentManager -> /api/v1/agents
# [INFO] OpsBox 服务启动成功，访问地址: http://127.0.0.1:4000
```

### 2. 启动 Agent

```bash
cd PROJECT_ROOT/backend/agent

# 配置环境变量（注意：路径不再包含 /logseek）
export SERVER_ENDPOINT="http://localhost:4000"
export AGENT_ID="agent-$(hostname)"
export AGENT_NAME="Test Agent"
export SEARCH_ROOTS="/var/log"
export AGENT_PORT=8090

cargo run --release

# 应该看到:
# [INFO] ╔══════════════════════════════════════════╗
# [INFO] ║     LogSeek Agent 启动中...              ║
# [INFO] ╚══════════════════════════════════════════╝
# [INFO] ✓ 已成功向 Server 注册
# [INFO] Agent HTTP 服务监听: 0.0.0.0:8090
```

### 3. 验证注册

```bash
# 列出所有 Agent
curl http://localhost:4000/api/v1/agents

# 应该返回:
# {
#   "agents": [
#     {
#       "id": "agent-hostname",
#       "name": "Test Agent",
#       ...
#     }
#   ],
#   "total": 1
# }

# 获取特定 Agent
curl http://localhost:4000/api/v1/agents/agent-hostname
```

---

## 🔧 Agent 配置更新

由于 Agent Manager 现在是独立模块，Agent 注册路径需要更新：

### 旧路径（LogSeek 子模块）❌
```
POST http://localhost:8080/api/v1/logseek/agents/register
POST http://localhost:8080/api/v1/logseek/agents/{id}/heartbeat
```

### 新路径（独立模块）✅
```
POST http://localhost:4000/api/v1/agents/register
POST http://localhost:4000/api/v1/agents/{id}/heartbeat
```

### Agent 代码需要修改

`backend/agent/src/main.rs` 中的路径需要更新：

```rust
// 旧代码 ❌
let url = format!("{}/api/v1/logseek/agents/register", config.server_endpoint);

// 新代码 ✅
let url = format!("{}/api/v1/agents/register", config.server_endpoint);
```

---

## 🏗️ 模块设计

### 1. AgentManagerModule (模块入口)

```rust
#[async_trait::async_trait]
impl opsbox_core::Module for AgentManagerModule {
    fn name(&self) -> &'static str {
        "AgentManager"
    }

    fn api_prefix(&self) -> &'static str {
        "/api/v1/agents"  // ← 独立的 API 前缀
    }

    fn router(&self, _pool: SqlitePool) -> Router {
        let manager = Arc::new(AgentManager::new());
        routes::create_routes(manager)
    }

    fn cleanup(&self) {
        log::info!("Agent Manager 模块清理完成");
    }
}

// 自动注册到 OpsBox
opsbox_core::register_module!(AgentManagerModule);
```

### 2. AgentManager (核心逻辑)

提供 Agent 管理功能：
- `register_agent()` - 注册 Agent
- `unregister_agent()` - 注销 Agent
- `heartbeat()` - 更新心跳
- `get_agent()` - 获取 Agent 信息
- `list_agents()` - 列出所有 Agent
- `list_online_agents()` - 列出在线 Agent
- `cleanup_offline_agents()` - 清理离线 Agent

### 3. 路由处理器

独立的路由处理器，不依赖其他模块。

---

## 🔄 与 LogSeek 的关系

### 现在的架构

```
┌─────────────────────┐
│     OpsBox Core     │
└──────────┬──────────┘
           │
           ├─────────────────┐
           │                 │
    ┌──────▼──────┐   ┌──────▼──────────┐
    │   LogSeek   │   │ Agent Manager   │
    │   模块      │   │   模块          │
    └─────────────┘   └─────────────────┘
           │                 │
           │        可以使用  │
           └────────►────────┘
```

### LogSeek 如何使用 Agent

LogSeek 可以通过以下方式使用 Agent：

1. **导入 agent-manager 的数据类型**:
   ```rust
   use agent_manager::models::AgentInfo;
   use agent_manager::manager::AgentManager;
   ```

2. **通过共享的 AgentManager 实例** (未来可以实现)

3. **通过 HTTP API 调用** (当前方式)

---

## ✅ 优势

### 1. 职责清晰
- Agent Manager: 管理 Agent 注册和生命周期
- LogSeek: 使用 Agent 进行日志搜索
- 其他模块: 可以独立使用 Agent

### 2. 模块独立
- Agent Manager 可以独立测试
- Agent Manager 可以独立升级
- 不影响其他模块

### 3. 便于扩展
- 未来添加的模块（Analytics、Monitoring）都可以使用 Agent
- Agent Manager 功能可以独立迭代

### 4. 符合 OpsBox 架构
- 使用 `inventory` 自动注册
- 使用统一的模块接口
- 使用统一的 API 前缀规范

---

## 🧪 测试

```bash
# 运行 Agent Manager 单元测试
cd PROJECT_ROOT/backend/agent-manager
cargo test

# 运行集成测试
cd PROJECT_ROOT/backend/opsbox-server
cargo test --release
```

---

## 📊 当前状态

- ✅ Agent Manager 模块已创建
- ✅ 已注册到 OpsBox
- ✅ 编译成功
- ⚠️ Agent 客户端路径需要更新
- ⏳ 单元测试待运行
- ⏳ 集成测试待运行

---

## 📝 下一步

### 1. 更新 Agent 客户端路径
修改 `backend/agent/src/main.rs`:
- 注册路径: `/api/v1/agents/register`
- 心跳路径: `/api/v1/agents/{id}/heartbeat`

### 2. 测试 Agent 注册
```bash
# 启动 Server
./target/release/opsbox-server

# 启动 Agent
cd ../agent
export SERVER_ENDPOINT="http://localhost:4000"
cargo run --release
```

### 3. 验证功能
```bash
# 列出 Agent
curl http://localhost:4000/api/v1/agents

# 查看特定 Agent
curl http://localhost:4000/api/v1/agents/agent-hostname
```

---

**创建时间**: 2025-10-08  
**状态**: ✅ 模块创建完成，等待测试
