# OpsBox 代码重构设计文档

## 概述

本文档详细说明 OpsBox 日志检索平台代码重构的技术方案。重构遵循以下原则：

- **零功能变更**: 重构不改变任何外部行为和 API 接口
- **渐进式改进**: 分阶段实施,每个阶段可独立验证
- **向后兼容**: 保持与现有代码的兼容性
- **测试先行**: 重构前后都有测试保障

## 架构概览

### 当前架构

```
┌─────────────────────────────────────────────┐
│ Routes Layer (routes/search.rs)             │
│ - stream_search() [280+ 行]                 │
│ - 包含业务逻辑、并发控制、结果转换          │
│ - 直接使用 AppError                         │
└─────────────────┬───────────────────────────┘
                  │
┌─────────────────▼───────────────────────────┐
│ Service Layer                                │
│ - SearchProcessor (搜索处理)                │
│ - EntryStreamFactory (条目流工厂)           │
│ - 缺少统一的搜索执行器                      │
└─────────────────┬───────────────────────────┘
                  │
┌─────────────────▼───────────────────────────┐
│ Repository Layer                             │
│ - settings, cache, llm, planners            │
└──────────────────────────────────────────────┘
```

### 目标架构

```
┌─────────────────────────────────────────────┐
│ Routes Layer (routes/search.rs)             │
│ - stream_search() [< 100 行]                │
│ - 仅处理 HTTP 请求响应                      │
│ - 使用分层错误类型                          │
└─────────────────┬───────────────────────────┘
                  │
┌─────────────────▼───────────────────────────┐
│ Service Layer                                │
│ + SearchExecutor (新增)                     │
│   - 多数据源协调                            │
│   - 并发控制                                │
│   - 结果聚合                                │
│ - SearchProcessor                           │
│ - EntryStreamFactory                        │
└─────────────────┬───────────────────────────┘
                  │
┌─────────────────▼───────────────────────────┐
│ Repository Layer                             │
│ - settings, cache, llm, planners            │
└──────────────────────────────────────────────┘
```

## 组件设计

### 1. SearchExecutor 服务类

#### 职责

- 协调多数据源并行搜索
- 管理并发控制(IO Semaphore)
- 聚合搜索结果
- 处理搜索生命周期

#### 接口设计

```rust
// backend/logseek/src/service/search_executor.rs

use crate::domain::config::Source;
use crate::query::Query;
use crate::service::search::SearchEvent;
use crate::service::ServiceError;
use opsbox_core::SqlitePool;
use std::sync::Arc;
use tokio::sync::{Semaphore, mpsc};

/// 搜索执行器配置
pub struct SearchExecutorConfig {
    /// IO 并发数(S3 访问)
    pub io_max_concurrency: usize,
    /// 流通道容量
    pub stream_channel_capacity: usize,
}

impl Default for SearchExecutorConfig {
    fn default() -> Self {
        Self {
            io_max_concurrency: 12,
            stream_channel_capacity: 128,
        }
    }
}

/// 搜索执行器
pub struct SearchExecutor {
    pool: SqlitePool,
    config: SearchExecutorConfig,
    io_semaphore: Arc<Semaphore>,
}

impl SearchExecutor {
    /// 创建搜索执行器
    pub fn new(pool: SqlitePool, config: SearchExecutorConfig) -> Self {
        let io_semaphore = Arc::new(Semaphore::new(config.io_max_concurrency));
        Self {
            pool,
            config,
            io_semaphore,
        }
    }

    /// 执行多数据源并行搜索
    ///
    /// # 参数
    /// - query: 查询字符串
    /// - context_lines: 上下文行数
    ///
    /// # 返回
    /// - Receiver<SearchEvent>: 搜索事件流
    pub async fn search(
        &self,
        query: &str,
        context_lines: usize,
    ) -> Result<mpsc::Receiver<SearchEvent>, ServiceError> {
        // 1. 获取存储源配置
        let sources = self.get_sources(query).await?;
        
        // 2. 解析查询
        let spec = self.parse_query(query)?;
        
        // 3. 创建结果通道
        let (tx, rx) = mpsc::channel(self.config.stream_channel_capacity);
        
        // 4. 为每个源启动搜索任务
        for source in sources {
            self.spawn_source_search(source, spec.clone(), context_lines, tx.clone());
        }
        
        Ok(rx)
    }

    /// 获取存储源配置列表
    async fn get_sources(&self, query: &str) -> Result<Vec<Source>, ServiceError> {
        // 从数据库加载配置或使用 Planner 生成
        todo!()
    }

    /// 解析查询字符串
    fn parse_query(&self, query: &str) -> Result<Arc<Query>, ServiceError> {
        todo!()
    }

    /// 为单个数据源启动搜索任务
    fn spawn_source_search(
        &self,
        source: Source,
        spec: Arc<Query>,
        context_lines: usize,
        tx: mpsc::Sender<SearchEvent>,
    ) {
        let io_sem = self.io_semaphore.clone();
        let pool = self.pool.clone();
        
        tokio::spawn(async move {
            // 获取 IO 许可
            let _permit = io_sem.acquire_owned().await.ok()?;
            
            // 执行搜索
            let result = search_single_source(pool, source, spec, context_lines).await;
            
            // 发送结果
            match result {
                Ok(events) => {
                    for event in events {
                        let _ = tx.send(event).await;
                    }
                }
                Err(e) => {
                    let _ = tx.send(SearchEvent::Error {
                        source: "executor".to_string(),
                        message: e.to_string(),
                        recoverable: true,
                    }).await;
                }
            }
            
            Some(())
        });
    }
}

/// 搜索单个数据源
async fn search_single_source(
    pool: SqlitePool,
    source: Source,
    spec: Arc<Query>,
    context_lines: usize,
) -> Result<Vec<SearchEvent>, ServiceError> {
    // 根据 source.endpoint 类型选择搜索策略
    // - Agent: 使用 AgentClient
    // - Local/S3: 使用 EntryStreamFactory
    todo!()
}
```

### 2. 错误处理统一

#### 当前问题

部分代码直接使用 `opsbox_core::AppError`:

```rust
// ❌ 不好的做法
LogSeekApiError::Internal(opsbox_core::AppError::bad_request("错误"))
```

#### 重构方案

统一使用分层错误类型:

```rust
// ✅ 好的做法
ServiceError::ConfigError("错误".to_string())
```

#### 重构步骤

1. **识别所有 AppError 使用位置**
   ```bash
   grep -r "AppError::" backend/logseek/src/
   ```

2. **逐个文件替换**
   - routes/view.rs: 使用 ServiceError::NotFound
   - routes/planners.rs: 使用 ServiceError::ConfigError
   - routes/nl2q.rs: 使用 ServiceError::ProcessingError
   - domain/source_planner: 使用 DomainError 或 ServiceError

3. **保留上下文信息**
   ```rust
   // 替换前
   .map_err(|e| AppError::internal(e.to_string()))
   
   // 替换后
   .map_err(|e| ServiceError::ProcessingError(
       format!("Starlark 脚本执行失败: {}", e)
   ))
   ```

### 3. 路由层简化

#### 重构前 (routes/search.rs)

```rust
pub async fn stream_search(
    State(pool): State<SqlitePool>,
    Json(body): Json<SearchBody>,
) -> Result<HttpResponse<Body>, LogSeekApiError> {
    // 280+ 行代码
    // - 获取配置
    // - 解析查询
    // - 并发控制
    // - 多数据源协调
    // - 结果转换
    // - NDJSON 序列化
}
```

#### 重构后 (routes/search.rs)

```rust
pub async fn stream_search(
    State(pool): State<SqlitePool>,
    Json(body): Json<SearchBody>,
) -> Result<HttpResponse<Body>, LogSeekApiError> {
    log::info!("[Search] 开始搜索: q={}", body.q);
    
    // 1. 解析请求参数
    let ctx = body.context.unwrap_or(3);
    
    // 2. 创建搜索执行器
    let config = SearchExecutorConfig::default();
    let executor = SearchExecutor::new(pool, config);
    
    // 3. 执行搜索
    let result_rx = executor.search(&body.q, ctx).await?;
    
    // 4. 转换为 NDJSON 流
    let stream = convert_to_ndjson_stream(result_rx);
    
    // 5. 构建 HTTP 响应
    Ok(build_ndjson_response(stream))
}

/// 将 SearchEvent 流转换为 NDJSON 字节流
fn convert_to_ndjson_stream(
    mut rx: mpsc::Receiver<SearchEvent>
) -> impl Stream<Item = Result<Bytes, std::io::Error>> {
    async_stream::stream! {
        while let Some(event) = rx.recv().await {
            match serde_json::to_vec(&event) {
                Ok(mut bytes) => {
                    bytes.push(b'\n');
                    yield Ok(Bytes::from(bytes));
                }
                Err(e) => {
                    log::warn!("序列化失败: {}", e);
                }
            }
        }
    }
}

/// 构建 NDJSON HTTP 响应
fn build_ndjson_response(
    stream: impl Stream<Item = Result<Bytes, std::io::Error>> + Send + 'static
) -> HttpResponse<Body> {
    HttpResponse::builder()
        .status(StatusCode::OK)
        .header(CONTENT_TYPE, "application/x-ndjson; charset=utf-8")
        .body(Body::from_stream(stream))
        .unwrap()
}
```

## 测试策略

### 1. SearchExecutor 单元测试

```rust
// backend/logseek/src/service/search_executor.rs

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_search_executor_basic() {
        // 创建测试数据库
        let pool = create_test_pool().await;
        
        // 创建执行器
        let config = SearchExecutorConfig::default();
        let executor = SearchExecutor::new(pool, config);
        
        // 执行搜索
        let mut rx = executor.search("error", 3).await.unwrap();
        
        // 验证结果
        let mut count = 0;
        while let Some(event) = rx.recv().await {
            match event {
                SearchEvent::Success(_) => count += 1,
                SearchEvent::Complete { .. } => break,
                _ => {}
            }
        }
        
        assert!(count > 0);
    }
    
    #[tokio::test]
    async fn test_search_executor_error_handling() {
        // 测试错误处理
    }
    
    #[tokio::test]
    async fn test_search_executor_concurrency() {
        // 测试并发控制
    }
}
```

### 2. 错误转换测试

```rust
// backend/logseek/src/api/error.rs

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_service_error_to_api_error() {
        let service_err = ServiceError::ConfigError("test".to_string());
        let api_err: LogSeekApiError = service_err.into();
        
        // 验证转换正确
        assert!(matches!(api_err, LogSeekApiError::Service(_)));
    }
    
    #[test]
    fn test_api_error_to_http_response() {
        let api_err = LogSeekApiError::Service(
            ServiceError::ConfigError("test".to_string())
        );
        
        let response = api_err.into_response();
        
        // 验证 HTTP 状态码
        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
        
        // 验证 Content-Type
        assert_eq!(
            response.headers().get(CONTENT_TYPE).unwrap(),
            "application/problem+json; charset=utf-8"
        );
    }
}
```

### 3. 集成测试

```rust
// backend/logseek/tests/search_integration.rs

#[tokio::test]
async fn test_search_api_end_to_end() {
    // 启动测试服务器
    let app = create_test_app().await;
    
    // 发送搜索请求
    let response = app
        .post("/api/v1/logseek/search.ndjson")
        .json(&json!({ "q": "error" }))
        .send()
        .await
        .unwrap();
    
    // 验证响应
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers().get(CONTENT_TYPE).unwrap(),
        "application/x-ndjson; charset=utf-8"
    );
    
    // 解析 NDJSON 流
    let body = response.text().await.unwrap();
    let lines: Vec<&str> = body.lines().collect();
    
    assert!(!lines.is_empty());
    
    // 验证每行都是有效的 JSON
    for line in lines {
        let _: SearchEvent = serde_json::from_str(line).unwrap();
    }
}
```

## 实施计划

### 阶段 1: 错误处理统一 (1-2 天)

**目标**: 移除所有 AppError 使用,统一使用分层错误类型

**步骤**:
1. 识别所有 AppError 使用位置
2. 逐个文件替换为对应的错误类型
3. 运行测试确保功能不变
4. 提交代码审查

**验收标准**:
- ✅ 代码中不再有 `AppError::` 的直接使用
- ✅ 所有测试通过
- ✅ API 响应格式不变

### 阶段 2: SearchExecutor 提取 (2-3 天)

**目标**: 将搜索逻辑从路由层提取到服务层

**步骤**:
1. 创建 `service/search_executor.rs`
2. 实现 `SearchExecutor` 基本结构
3. 迁移并发控制逻辑
4. 迁移多数据源协调逻辑
5. 简化 `routes/search.rs`
6. 编写单元测试
7. 运行集成测试

**验收标准**:
- ✅ `routes/search.rs` 少于 100 行
- ✅ `SearchExecutor` 有完整的单元测试
- ✅ 所有集成测试通过
- ✅ API 行为完全一致

### 阶段 3: 测试覆盖率提升 (2-3 天)

**目标**: 为核心模块增加测试覆盖

**步骤**:
1. 为 `SearchExecutor` 添加测试
2. 为错误转换添加测试
3. 为 `EntryStreamFactory` 添加测试
4. 运行覆盖率报告
5. 补充缺失的测试用例

**验收标准**:
- ✅ 服务层测试覆盖率 > 70%
- ✅ 所有公共 API 有测试
- ✅ 边界情况有测试覆盖

## 风险评估

### 风险 1: 重构引入回归问题

**缓解措施**:
- 重构前确保现有测试通过
- 每个阶段独立验证
- 使用集成测试保障端到端行为
- 代码审查

### 风险 2: 性能下降

**缓解措施**:
- 重构前后进行性能基准测试
- 保持相同的并发策略
- 避免引入不必要的抽象层

### 风险 3: 代码冲突

**缓解措施**:
- 小步快跑,频繁提交
- 与团队沟通重构计划
- 使用 feature branch 隔离变更

## 成功指标

- ✅ 所有测试通过(单元测试 + 集成测试)
- ✅ API 行为完全一致(通过集成测试验证)
- ✅ 代码行数减少 > 20%
- ✅ 测试覆盖率提升 > 15%
- ✅ 代码审查通过
- ✅ 性能无明显下降(< 5%)

## 回滚计划

如果重构出现问题:

1. **立即回滚**: 使用 Git revert 回退到重构前的提交
2. **问题分析**: 分析失败原因,修复问题
3. **重新实施**: 修复后重新提交

每个阶段都应该是可独立回滚的。
