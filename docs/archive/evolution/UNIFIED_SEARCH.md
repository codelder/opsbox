# 统一搜索功能 (Unified Search)

## 概述

统一搜索功能允许在一次搜索请求中，同时搜索多个存储源（S3、Agent、本地文件系统等），并将结果合并返回给前端。这大大简化了多源数据检索的复杂度。

## 功能特点

### 1. 多存储源并行搜索
- 支持同时搜索多个 S3 Profile（不同的 MinIO 实例或 AWS S3 bucket）
- 支持同时搜索多个 Agent（远程搜索服务）
- 支持搜索本地文件系统
- 所有存储源并行执行，最大化搜索效率

### 2. 智能协调
- 使用 `SearchCoordinator` 协调器统一管理多个存储源
- 自动识别存储源类型（DataSource vs SearchService）
- DataSource（S3、本地）: Server 端执行搜索
- SearchService（Agent）: 远程执行搜索，只返回结果

### 3. 存储源工厂
- `StorageFactory` 负责根据配置动态创建存储源实例
- 支持从数据库加载 S3 Profile 配置
- 自动验证 Agent 健康状态
- 批量创建时提供详细的错误信息

## 架构设计

```
前端请求
   ↓
POST /api/v1/logseek/search.unified.ndjson
   ↓
统一搜索路由 (stream_unified_search)
   ↓
获取存储源配置列表 (get_storage_source_configs)
   ↓
StorageFactory 创建存储源实例
   ↓
SearchCoordinator 并行执行搜索
   ├── DataSource 1 (S3-1)      ← Server 端搜索
   ├── DataSource 2 (S3-2)      ← Server 端搜索
   ├── SearchService 1 (Agent-1) ← 远程搜索
   ├── SearchService 2 (Agent-2) ← 远程搜索
   └── ...
   ↓
合并结果流 (NDJSON)
   ↓
返回前端
```

## 核心模块

### 1. StorageFactory (`server/logseek/src/storage/factory.rs`)

**存储源配置类型：**
```rust
pub enum SourceConfig {
  Local { path: String, recursive: bool },
  S3 { 
    profile: String, 
    prefix: Option<String>, 
    pattern: Option<String>,
    key: Option<String>,  // 新增：指定特定对象键
  },
  Agent { endpoint: String },
}
```

**主要方法：**
- `create_source()`: 根据配置创建单个存储源
- `create_sources()`: 批量创建多个存储源
- `create_local_source()`: 创建本地文件系统源
- `create_s3_source()`: 从数据库加载 Profile 并创建 S3 源
- `create_agent_source()`: 创建并验证 Agent 客户端

### 2. SearchCoordinator (`server/logseek/src/service/coordinator.rs`)

**主要方法：**
- `add_source()`: 添加存储源
- `search()`: 并行执行搜索
- `search_data_source()`: 处理 DataSource 的搜索
- `search_service()`: 处理 SearchService 的搜索

### 3. 统一搜索路由 (`server/logseek/src/routes.rs`)

**端点：** `POST /api/v1/logseek/search.unified.ndjson`

**请求体：**
```json
{
  "q": "error AND (timeout OR failure)",
  "context": 3
}
```

**响应：**
- Content-Type: `application/x-ndjson`
- Header: `X-Logseek-SID` (会话 ID)
- Body: NDJSON 流

## 存储源配置策略

### 当前实现：从数据库动态加载

`get_storage_source_configs()` 函数现在从数据库加载所有 S3 Profiles，并根据查询中的日期范围生成多个 tar.gz 文件配置：

```rust
async fn get_storage_source_configs(
  pool: &SqlitePool,
  query: &str,
) -> Result<Vec<SourceConfig>, AppError> {
  // 1. 从数据库加载所有 S3 Profiles
  let profiles = settings::list_s3_profiles(pool).await?;
  
  // 2. 解析查询中的日期范围（使用 derive_plan）
  let buckets = ["20", "21", "22", "23"];
  let plan = derive_plan(base_dir, &buckets, query);
  
  // 3. 为每个 Profile 的每个日期+bucket 生成一个 SourceConfig
  for profile in profiles {
    for date in plan.range {
      for bucket in buckets {
        let key = format!(
          "bbip/{}/{}/{}/BBIP_{}_APPLOG_{}.tar.gz",
          y, yyyymm, yyyymmdd, bucket, file_name
        );
        configs.push(SourceConfig::S3 {
          profile: profile.profile_name.clone(),
          key: Some(key),  // 直接指定 tar.gz 文件
          ...
        });
      }
    }
  }
}
```

**关键特性：**
- ✅ 从数据库动态加载 S3 Profiles
- ✅ 根据查询中的日期指令（dt/fdt/tdt）解析日期范围
- ✅ 每个 tar.gz 文件作为一个独立的存储源（并行搜索）
- ✅ 支持多 Profile（例如同时搜索生产和备份 S3）
- ✅ 日期范围默认为“昨天”，可通过 dt/fdt/tdt 指定

### TODO: 后续改进

1. **从数据库读取配置**
   - 创建 `storage_sources` 表存储配置
   - 支持动态增删改查存储源

2. **权限管理**
   - 不同用户/角色看到不同的存储源
   - 基于标签的访问控制

3. **标签/分组过滤**
   - 支持给存储源打标签（如 "production", "staging"）
   - 用户可以选择只搜索特定标签的存储源

4. **动态启用/禁用**
   - 运行时动态启用或禁用某些存储源
   - 支持维护模式

5. **优先级和负载均衡**
   - 给存储源设置优先级
   - 根据负载情况动态调整并发数

## 前端 API 使用

### 调用统一搜索

```typescript
import { startUnifiedSearch, extractSessionId } from '$lib/modules/logseek/api';

// 开始统一搜索
const response = await startUnifiedSearch('error AND timeout');
const sessionId = extractSessionId(response);

// 读取 NDJSON 流
const reader = response.body?.getReader();
const decoder = new TextDecoder();

while (true) {
  const { done, value } = await reader.read();
  if (done) break;
  
  const chunk = decoder.decode(value, { stream: true });
  const lines = chunk.split('\n').filter(line => line.trim());
  
  for (const line of lines) {
    const result = JSON.parse(line);
    console.log('搜索结果:', result);
  }
}
```

## 性能特点

1. **并行执行**
   - 所有存储源同时开始搜索
   - 不会因为单个慢速存储源阻塞其他源

2. **流式返回**
   - 结果即时返回，无需等待所有源完成
   - NDJSON 格式支持逐行解析

3. **智能限流**
   - S3 搜索自动应用 IO 并发限制和 CPU 并发限制
   - Agent 搜索尊重远程服务的并发能力

4. **错误隔离**
   - 单个存储源失败不影响其他源
   - 详细记录失败原因

## 与现有搜索接口的关系

| 接口 | 路径 | 适用场景 |
|------|------|----------|
| 本地搜索 | `/stream.ndjson` | 只搜索本地文件系统 |
| S3 搜索 | `/stream.s3.ndjson` | 只搜索单个 S3 Profile |
| **统一搜索** | `/search.unified.ndjson` | **同时搜索所有配置的存储源** |

## 日志示例

启动统一搜索时的日志输出：

```
[UnifiedSearch] 开始统一搜索: q=error
[UnifiedSearch] 获取到 3 个存储源配置
创建 S3 存储源: profile=default, prefix=None
创建 S3 存储源: profile=backup, prefix=Some("logs/")
创建 Agent 客户端: endpoint=http://agent1:8090
[UnifiedSearch] 成功创建 3 个存储源
[UnifiedSearch] 开始并行搜索: query=error, context=3, sid=abc123
开始搜索数据源 #0: S3Storage
开始搜索数据源 #1: S3Storage
开始调用搜索服务 #2: AgentClient
[UnifiedSearch] 找到匹配: file=s3://logs/app.log, lines=5
[UnifiedSearch] 找到匹配: file=s3://backup/error.log, lines=3
[UnifiedSearch] 搜索完成: 总计 8 个结果
```

## 测试建议

### 1. 单一存储源测试
```bash
# 配置只有一个 S3 Profile
# 验证结果正确性
```

### 2. 多 S3 Profile 测试
```bash
# 配置 2 个 S3 Profile
# 验证结果包含来自两个 bucket 的数据
```

### 3. 混合存储源测试
```bash
# 配置 1 个 S3 + 2 个 Agent
# 验证结果正确合并
```

### 4. 错误处理测试
```bash
# 配置一个无效的 S3 Profile
# 验证其他存储源仍能正常工作
```

### 5. 性能测试
```bash
# 配置 5+ 个存储源
# 验证并发搜索性能
# 监控内存和 CPU 使用
```

## 未来展望

1. **搜索结果排序**
   - 按相关度排序
   - 按时间排序
   - 按存储源优先级排序

2. **增量搜索**
   - 支持持续监听新结果
   - WebSocket 推送

3. **搜索进度反馈**
   - 显示每个存储源的搜索进度
   - 预估完成时间

4. **结果去重**
   - 识别重复结果
   - 智能合并相似结果

5. **分布式追踪**
   - OpenTelemetry 集成
   - 端到端性能分析

## 相关文件

### 后端
- `server/logseek/src/storage/factory.rs` - 存储源工厂
- `server/logseek/src/storage/mod.rs` - 存储抽象层
- `server/logseek/src/storage/s3.rs` - S3 存储实现
- `server/logseek/src/storage/agent.rs` - Agent 客户端实现
- `server/logseek/src/storage/local.rs` - 本地文件系统实现
- `server/logseek/src/service/coordinator.rs` - 搜索协调器
- `server/logseek/src/routes.rs` - 统一搜索路由

### 前端
- `ui/src/lib/modules/logseek/api/search.ts` - 搜索 API 客户端

## 编译和运行

```bash
# 编译后端
cargo build --manifest-path server/Cargo.toml -p api-gateway --release

# 运行后端
./server/target/release/opsbox --port 4000

# 测试统一搜索
curl -X POST http://127.0.0.1:4000/api/v1/logseek/search.unified.ndjson \
  -H "Content-Type: application/json" \
  -d '{"q":"error","context":3}'
```

## 总结

统一搜索功能通过 **存储源抽象** + **工厂模式** + **协调器模式**，实现了对多个异构存储源的统一搜索能力。

核心优势：
- ✅ 简化前端调用（单一 API）
- ✅ 并行搜索提升性能
- ✅ 易于扩展新的存储源类型
- ✅ 错误隔离保证可用性
- ✅ 流式返回改善用户体验

这为后续构建企业级日志搜索平台奠定了坚实基础！
