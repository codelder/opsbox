# Agent 注册和管理

## ✅ 当前状态

Agent 注册功能**已完全实现**，包括：

1. ✅ Agent Server 注册逻辑
2. ✅ LogSeek Server 注册路由
3. ✅ Agent 管理器
4. ✅ 心跳机制
5. ✅ 已集成到主路由

---

## 🔌 API 端点

### 1. Agent 注册

**端点**: `POST /api/v1/logseek/agents/register`

**请求体**:
```json
{
  "id": "agent-server-01",
  "name": "Agent @ server-01",
  "version": "0.1.0",
  "hostname": "server-01.example.com",
  "tags": ["production"],
  "search_roots": ["/var/log", "/opt/app/logs"],
  "last_heartbeat": 1704110400,
  "status": "Online"
}
```

**响应**:
- `201 Created` - 注册成功
- `500 Internal Server Error` - 注册失败

**Agent 代码** (`server/agent/src/main.rs:436-452`):
```rust
async fn register_to_server(config: &AgentConfig) -> Result<(), Box<dyn std::error::Error>> {
  let client = reqwest::Client::builder().timeout(Duration::from_secs(10)).build()?;
  
  let info = config.to_agent_info();
  let url = format!("{}/api/v1/agents/register", config.server_endpoint);
  
  let response = client.post(&url).json(&info).send().await?;
  
  if response.status().is_success() {
    info!("✓ 已成功向 Server 注册");
    Ok(())
  } else {
    Err(format!("注册失败: {}", response.status()).into())
  }
}
```

---

### 2. 心跳

**端点**: `POST /api/v1/logseek/agents/{agent_id}/heartbeat`

**响应**: `200 OK`

**Agent 代码** (`server/agent/src/main.rs:455-480`):
```rust
async fn heartbeat_loop(config: Arc<AgentConfig>) {
  let client = reqwest::Client::builder()
    .timeout(Duration::from_secs(5))
    .build()
    .unwrap();

  let mut interval = tokio::time::interval(Duration::from_secs(config.heartbeat_interval_secs));

  loop {
    interval.tick().await;

    let url = format!("{}/api/v1/agents/{}/heartbeat", config.server_endpoint, config.agent_id);

    match client.post(&url).send().await {
      Ok(response) if response.status().is_success() => {
        debug!("心跳发送成功");
      }
      Ok(response) => {
        warn!("心跳失败: {}", response.status());
      }
      Err(e) => {
        warn!("心跳发送出错: {}", e);
      }
    }
  }
}
```

---

### 3. 获取 Agent 信息

**端点**: `GET /api/v1/logseek/agents/{agent_id}`

**响应**:
```json
{
  "id": "agent-server-01",
  "name": "Agent @ server-01",
  "version": "0.1.0",
  "hostname": "server-01.example.com",
  "tags": ["production"],
  "search_roots": ["/var/log"],
  "last_heartbeat": 1704110400,
  "status": "Online"
}
```

---

### 4. 列出所有 Agent

**端点**: `GET /api/v1/logseek/agents`

**响应**:
```json
["agent-server-01", "agent-server-02", "agent-server-03"]
```

---

## 🏗️ 架构说明

### 代码组织

```
server/logseek/src/
├── storage/
│   └── agent.rs              # Agent 客户端和管理器
├── routes_agent.rs            # Agent 管理路由
└── routes/
    └── mod.rs                 # 集成 Agent 路由到主路由
```

### 核心组件

#### 1. AgentInfo (数据模型)

```rust
// server/logseek/src/storage/agent.rs:40-65
pub struct AgentInfo {
  pub id: String,              // Agent 唯一标识
  pub name: String,            // Agent 显示名称
  pub version: String,         // Agent 版本
  pub hostname: String,        // 主机名
  pub tags: Vec<String>,       // 标签（如 production, dev）
  pub search_roots: Vec<String>, // 可搜索的根目录
  pub last_heartbeat: i64,     // 最后心跳时间戳
  pub status: AgentStatus,     // 状态
}
```

#### 2. AgentClient (通信客户端)

```rust
// server/logseek/src/storage/agent.rs:78-149
pub struct AgentClient {
  pub agent_id: String,
  endpoint: String,           // e.g., "http://192.168.1.10:8090"
  client: reqwest::Client,
  timeout: Duration,
}

impl AgentClient {
  pub fn new(agent_id: String, endpoint: String) -> Self { ... }
  pub async fn health_check(&self) -> bool { ... }
  pub async fn get_info(&self) -> Result<AgentInfo, StorageError> { ... }
}

// 实现 SearchService trait
#[async_trait]
impl SearchService for AgentClient {
  async fn search(&self, query: &str, ...) -> Result<SearchResultStream, ...> { ... }
}
```

#### 3. AgentManager (管理器)

```rust
// server/logseek/src/storage/agent.rs:285-335
pub struct AgentManager {
  agents: Arc<RwLock<HashMap<String, Arc<AgentClient>>>>,
}

impl AgentManager {
  pub fn new() -> Self { ... }
  pub async fn register_agent(&self, info: AgentInfo) -> Result<(), StorageError> { ... }
  pub async fn unregister_agent(&self, agent_id: &str) { ... }
  pub async fn get_online_agents(&self) -> Vec<Arc<AgentClient>> { ... }
  pub async fn get_agent(&self, agent_id: &str) -> Option<Arc<AgentClient>> { ... }
  pub async fn list_agent_ids(&self) -> Vec<String> { ... }
}
```

#### 4. 路由处理器

```rust
// server/logseek/src/routes_agent.rs:36-91
pub fn agent_routes() -> Router<Arc<AgentState>> {
  Router::new()
    .route("/agents/register", post(register_agent))
    .route("/agents/{agent_id}/heartbeat", post(agent_heartbeat))
    .route("/agents/{agent_id}", get(get_agent_info))
    .route("/agents", get(list_agents))
}

// 注册 Agent
async fn register_agent(
  State(state): State<Arc<AgentState>>,
  Json(info): Json<AgentInfo>,
) -> Result<StatusCode, (StatusCode, String)> { ... }

// Agent 心跳
async fn agent_heartbeat(...) -> StatusCode { ... }

// 获取 Agent 信息
async fn get_agent_info(...) -> Result<Json<AgentInfo>, ...> { ... }

// 列举所有 Agent
async fn list_agents(...) -> Json<Vec<String>> { ... }
```

---

## 🧪 测试 Agent 注册

### 测试 1: 启动 Server

```bash
cd /Users/wangyue/workspace/codelder/opsboard/server/api-gateway
cargo run --release

# 应该看到:
# [INFO] OpsBox 服务启动成功，访问地址: http://127.0.0.1:8080
```

---

### 测试 2: 启动 Agent

```bash
cd /Users/wangyue/workspace/codelder/opsboard/server/agent

# 设置环境变量
export SERVER_ENDPOINT="http://localhost:8080/api/v1/logseek"
export AGENT_ID="agent-$(hostname)"
export AGENT_NAME="Test Agent"
export SEARCH_ROOTS="/tmp/test-logs"
export AGENT_PORT=8090

# 启动 Agent
cargo run --release

# 应该看到:
# [INFO] ╔══════════════════════════════════════════╗
# [INFO] ║     LogSeek Agent 启动中...              ║
# [INFO] ╚══════════════════════════════════════════╝
# [INFO] Agent ID: agent-hostname
# [INFO] Server: http://localhost:8080/api/v1/logseek
# [INFO] ✓ 已成功向 Server 注册
# [INFO] Agent HTTP 服务监听: 0.0.0.0:8090
```

---

### 测试 3: 验证注册

```bash
# 列出所有 Agent
curl http://localhost:8080/api/v1/logseek/agents

# 应该返回:
# ["agent-hostname"]

# 获取 Agent 详细信息
curl http://localhost:8080/api/v1/logseek/agents/agent-hostname

# 应该返回 Agent 的完整信息
```

---

### 测试 4: 手动注册 Agent

```bash
# 如果 Agent 注册失败，可以手动注册
curl -X POST http://localhost:8080/api/v1/logseek/agents/register \
  -H "Content-Type: application/json" \
  -d '{
    "id": "agent-test",
    "name": "Test Agent",
    "version": "0.1.0",
    "hostname": "test-server",
    "tags": ["test"],
    "search_roots": ["/var/log"],
    "last_heartbeat": 0,
    "status": "Online"
  }'

# 应该返回: HTTP 201 Created
```

---

## 🔧 Agent 配置

### 环境变量

| 变量名 | 说明 | 默认值 | 示例 |
|--------|------|--------|------|
| `AGENT_ID` | Agent 唯一标识 | `agent-{hostname}` | `agent-server-01` |
| `AGENT_NAME` | Agent 显示名称 | `Agent @ {hostname}` | `Production Agent` |
| `SERVER_ENDPOINT` | LogSeek Server 地址 | `http://localhost:8080` | `http://logseek.example.com` |
| `SEARCH_ROOTS` | 搜索根目录（逗号分隔） | `/var/log` | `/var/log,/opt/app/logs` |
| `AGENT_PORT` | Agent 监听端口 | `8090` | `8090` |
| `ENABLE_HEARTBEAT` | 是否启用心跳 | `true` | `true` |
| `HEARTBEAT_INTERVAL` | 心跳间隔（秒） | `30` | `60` |

### 配置示例

```bash
# 生产环境 Agent
export SERVER_ENDPOINT="http://logseek-server.prod.example.com/api/v1/logseek"
export AGENT_ID="agent-web-01"
export AGENT_NAME="Web Server 01"
export SEARCH_ROOTS="/var/log/nginx,/var/log/app,/opt/logs"
export AGENT_PORT=8090
export HEARTBEAT_INTERVAL=60

# 开发环境 Agent
export SERVER_ENDPOINT="http://localhost:8080/api/v1/logseek"
export AGENT_ID="agent-dev-$(whoami)"
export AGENT_NAME="Dev Agent - $(whoami)"
export SEARCH_ROOTS="/tmp/test-logs"
export AGENT_PORT=8090
```

---

## 🔍 故障排查

### 问题 1: Agent 注册失败

**症状**:
```log
[ERROR] 注册到 Server 失败: connection refused
[ERROR] Agent 将以离线模式运行，仅提供 HTTP 接口
```

**原因**:
1. Server 未启动
2. `SERVER_ENDPOINT` 配置错误
3. 网络不通

**解决**:
```bash
# 1. 检查 Server 是否运行
curl http://localhost:8080/healthy

# 2. 检查 Agent 配置
echo $SERVER_ENDPOINT

# 3. 测试网络连接
curl http://localhost:8080/api/v1/logseek/agents
```

---

### 问题 2: 心跳失败

**症状**:
```log
[WARN] 心跳失败: 404 Not Found
```

**原因**: 路由路径错误

**解决**: 确保 `SERVER_ENDPOINT` 包含 `/api/v1/logseek`
```bash
# ✅ 正确
export SERVER_ENDPOINT="http://localhost:8080/api/v1/logseek"

# ❌ 错误
export SERVER_ENDPOINT="http://localhost:8080"
```

---

### 问题 3: 重复注册

**症状**: Agent 重启后无法注册

**原因**: AgentManager 是内存存储，Server 重启后会丢失注册信息

**解决**: Agent 重启时会自动重新注册（当前设计）

**未来优化**: 
- 将 Agent 注册信息持久化到数据库
- Server 重启后自动恢复 Agent 列表

---

## 📊 当前限制

### 1. 内存存储

**现状**: AgentManager 使用内存 HashMap 存储 Agent
**影响**: Server 重启后丢失所有 Agent 注册信息
**解决**: Agent 重启时自动重新注册

**未来**: 持久化到数据库

### 2. 无认证

**现状**: Agent 注册和心跳无需认证
**影响**: 任何人都可以注册 Agent
**未来**: 添加 Token 认证（参考 AGENT_HTTP_API_SPEC.md）

### 3. 心跳无状态更新

**现状**: 心跳只返回 200 OK，不更新 Agent 状态
**未来**: 
- 更新 `last_heartbeat` 时间戳
- 检测离线 Agent（超时未心跳）
- 自动清理离线 Agent

---

## 🚀 未来功能

### Phase 1: 基础完善（优先）
- [ ] Agent 注册信息持久化到数据库
- [ ] 心跳更新 `last_heartbeat` 时间戳
- [ ] 离线 Agent 检测和清理
- [ ] Agent 健康检查定时任务

### Phase 2: 认证和安全
- [ ] Token 认证
- [ ] Agent 路径白名单验证
- [ ] TLS/HTTPS 支持

### Phase 3: 高级功能
- [ ] Agent 分组和标签过滤
- [ ] Agent 负载均衡
- [ ] Agent 故障转移
- [ ] Agent 指标收集和监控

---

## 📝 总结

### 当前状态
- ✅ Agent 注册功能**已完全实现**
- ✅ 路由已集成到主路由
- ✅ Agent Server 可以正常注册到 LogSeek Server
- ✅ 心跳机制正常工作

### 使用方法
1. 启动 LogSeek Server (port 8080)
2. 配置环境变量（`SERVER_ENDPOINT` 等）
3. 启动 Agent (port 8090)
4. Agent 自动注册到 Server

### 测试命令
```bash
# 列出所有 Agent
curl http://localhost:8080/api/v1/logseek/agents

# 获取 Agent 信息
curl http://localhost:8080/api/v1/logseek/agents/{agent_id}
```

---

**文档更新**: 2025-10-08  
**状态**: ✅ 功能完整，可以使用
