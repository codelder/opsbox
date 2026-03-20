# 异常处理分层 - 快速参考

## 快速对比：现状 vs 改进

### 现在的做法 ❌
```rust
// routes/search.rs - 直接使用 AppError
pub async fn stream_search(...) -> Result<HttpResponse<Body>, AppError> {
  let config = repository::load_config(pool)
    .await?;  // AppError 直接冒泡到 routes 层
  
  let result = service::search(&config)
    .await?;  // AppError 直接冒泡
  
  // 问题：不知道错误来自哪一层
}

// repository/settings.rs
pub async fn load_config(...) -> Result<Config, AppError> {
  sqlx::query(...)
    .fetch(pool)
    .await
    .map_err(|e| AppError::internal(e.to_string()))?  // 丢失上下文
}
```

**问题**：
- ❌ 所有层都返回 `AppError`，丢失层级信息
- ❌ 无法知道错误确切来自哪一层
- ❌ 难以编写单元测试（需要 mock AppError）
- ❌ 客户端错误响应不够精确

### 改进后的做法 ✅
```rust
// routes/search.rs - 使用统一的 LogSeekApiError
pub async fn stream_search(...) -> Result<HttpResponse<Body>, LogSeekApiError> {
  let config = repository::load_config(pool)
    .await?;  // RepositoryError 自动转换为 LogSeekApiError
  
  let result = service::search(&config)
    .await?;  // ServiceError 自动转换为 LogSeekApiError
  
  Ok(response)
}

// repository/error.rs
#[derive(Debug, Error)]
pub enum RepositoryError {
  #[error("查询失败: {0}")]
  QueryFailed(String),
  #[error("资源不存在: {0}")]
  NotFound(String),
}

// api/error.rs
impl From<RepositoryError> for LogSeekApiError {
  fn from(err: RepositoryError) -> Self {
    LogSeekApiError::Repository(err)  // 清晰地记录层级
  }
}

// repository/settings.rs
pub async fn load_config(...) -> Result<Config, RepositoryError> {
  sqlx::query(...)
    .fetch(pool)
    .await
    .map_err(|e| RepositoryError::QueryFailed(format!("查询失败: {}", e)))?
}
```

**优势**：
- ✅ 每层错误类型清晰
- ✅ 错误来源可追踪
- ✅ 便于单元测试
- ✅ 客户端收到精确的错误码

---

## 代码生成器 - 复制粘贴即可用

### 1. 创建 Repository Error (repository/error.rs)

```rust
use thiserror::Error;

/// Repository 层错误
#[derive(Debug, Error)]
pub enum RepositoryError {
  #[error("查询失败: {0}")]
  QueryFailed(String),
  
  #[error("对象存储错误: {0}")]
  StorageError(String),
  
  #[error("资源不存在: {0}")]
  NotFound(String),
  
  #[error("缓存操作失败: {0}")]
  CacheFailed(String),
  
  #[error("数据库错误: {0}")]
  Database(String),
}

impl From<sqlx::Error> for RepositoryError {
  fn from(err: sqlx::Error) -> Self {
    Self::Database(err.to_string())
  }
}

// 在 mod.rs 中导出
pub mod error;
pub use error::RepositoryError;
```

### 2. 创建 Service Error (service/error.rs)

```rust
use thiserror::Error;

/// Service 层错误
#[derive(Debug, Error)]
pub enum ServiceError {
  #[error("配置错误: {0}")]
  ConfigError(String),
  
  #[error("搜索失败 - 路径: {path}, 错误: {error}")]
  SearchFailed { path: String, error: String },
  
  #[error("数据处理错误: {0}")]
  ProcessingError(String),
  
  #[error("Repository 错误: {0}")]
  Repository(#[from] crate::repository::RepositoryError),
}

pub type Result<T> = std::result::Result<T, ServiceError>;

// 在 service/mod.rs 中导出
pub mod error;
pub use error::{ServiceError, Result};
```

### 3. 改进 API Error (api/error.rs)

```rust
use axum::extract::rejection::JsonRejection;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use problemdetails::Problem;
use serde_json::json;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum LogSeekApiError {
  #[error(transparent)]
  Service(#[from] crate::service::ServiceError),
  
  #[error(transparent)]
  Repository(#[from] crate::repository::RepositoryError),
  
  #[error(transparent)]
  Domain(#[from] crate::domain::FileUrlError),
  
  #[error("JSON 解析失败: {0}")]
  BadJson(#[from] JsonRejection),
  
  #[error("查询语法错误: {0}")]
  QueryParse(#[from] crate::query::ParseError),
  
  #[error(transparent)]
  Internal(#[from] opsbox_core::AppError),
}

impl From<LogSeekApiError> for Problem {
  fn from(error: LogSeekApiError) -> Self {
    let (status, title, detail) = match &error {
      // Service 层错误映射
      LogSeekApiError::Service(e) => match e {
        crate::service::ServiceError::ConfigError(_) => (
          StatusCode::INTERNAL_SERVER_ERROR,
          "配置错误",
          e.to_string(),
        ),
        crate::service::ServiceError::SearchFailed { .. } => (
          StatusCode::INTERNAL_SERVER_ERROR,
          "搜索失败",
          e.to_string(),
        ),
        crate::service::ServiceError::ProcessingError(_) => (
          StatusCode::INTERNAL_SERVER_ERROR,
          "数据处理失败",
          e.to_string(),
        ),
        _ => (
          StatusCode::INTERNAL_SERVER_ERROR,
          "服务错误",
          e.to_string(),
        ),
      },
      
      // Repository 层错误映射
      LogSeekApiError::Repository(e) => match e {
        crate::repository::RepositoryError::NotFound(_) => (
          StatusCode::NOT_FOUND,
          "资源不存在",
          e.to_string(),
        ),
        crate::repository::RepositoryError::StorageError(_) => (
          StatusCode::BAD_GATEWAY,
          "存储服务错误",
          e.to_string(),
        ),
        _ => (
          StatusCode::INTERNAL_SERVER_ERROR,
          "数据访问错误",
          e.to_string(),
        ),
      },
      
      // Domain 层错误映射
      LogSeekApiError::Domain(e) => (
        StatusCode::BAD_REQUEST,
        "业务验证失败",
        e.to_string(),
      ),
      
      // 协议级错误
      LogSeekApiError::BadJson(_) => (
        StatusCode::BAD_REQUEST,
        "JSON 请求格式错误",
        error.to_string(),
      ),
      LogSeekApiError::QueryParse(_) => (
        StatusCode::BAD_REQUEST,
        "查询语法错误",
        error.to_string(),
      ),
      
      _ => (
        StatusCode::INTERNAL_SERVER_ERROR,
        "内部错误",
        error.to_string(),
      ),
    };

    log::error!("[{}] {}", title, detail);
    
    problemdetails::new(status)
      .with_title(title)
      .with_detail(detail)
  }
}

impl IntoResponse for LogSeekApiError {
  fn into_response(self) -> Response {
    let problem = Problem::from(self);
    let status = problem.status.unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    (status, axum::Json(problem)).into_response()
  }
}

pub type Result<T> = std::result::Result<T, LogSeekApiError>;
```

### 4. Routes 层使用示例

```rust
// routes/s3.rs
use crate::api::error::Result;

pub async fn get_s3_settings(
  State(pool): State<SqlitePool>,
) -> Result<Json<S3SettingsPayload>> {
  let settings = repository::s3::load_s3_settings(&pool)
    .await?;  // RepositoryError 自动转换为 LogSeekApiError
  
  let mut payload = settings.map_or_else(S3SettingsPayload::default, Into::into);
  payload.configured = true;
  Ok(Json(payload))
}

pub async fn save_s3_settings(
  State(pool): State<SqlitePool>,
  Json(payload): Json<S3SettingsPayload>,
) -> Result<StatusCode> {
  let settings = S3Settings::from(payload);
  
  repository::s3::save_s3_settings(&pool, &settings)
    .await?;  // RepositoryError 自动转换为 LogSeekApiError
  
  Ok(StatusCode::NO_CONTENT)
}
```

### 5. Service 层使用示例

```rust
// service/search.rs
use crate::service::error::{ServiceError, Result};

pub async fn search_in_s3(
  pool: &SqlitePool,
  query: &str,
) -> Result<Vec<SearchResult>> {
  // 获取配置（RepositoryError 自动转为 ServiceError）
  let config = crate::repository::settings::load_required_s3_settings(pool)
    .await?;
  
  // 调用存储层
  let objects = crate::utils::storage::list_objects(&config)
    .await
    .map_err(|e| ServiceError::ProcessingError(e.to_string()))?;
  
  // ... 处理搜索
  
  Ok(results)
}
```

### 6. Repository 层使用示例

```rust
// repository/s3.rs
use crate::repository::error::Result;

pub async fn load_required_s3_settings(pool: &SqlitePool) 
  -> Result<S3Settings> {
  
  let row = sqlx::query_as::<_, (String, String, String)>(
    "SELECT endpoint, access_key, secret_key FROM s3_profiles WHERE profile_name = 'default'"
  )
  .fetch_optional(pool)
  .await?;  // sqlx::Error 自动转换为 RepositoryError::Database
  
  row
    .map(|(endpoint, access_key, secret_key)| S3Settings {
      endpoint,
      access_key,
      secret_key,
    })
    .ok_or(RepositoryError::NotFound(
      "S3 配置不存在".to_string()
    ))
}
```

---

## 快速检查清单

在提交前检查：

- [ ] 每个错误类型是否定义在正确的层？
  - Domain: `FileUrlError`, `ParseError`
  - Service: `ServiceError`
  - Repository: `RepositoryError`
  - API: `LogSeekApiError`

- [ ] 是否使用了正确的 `Result` 类型别名？
  ```rust
  // repository/error.rs
  pub type Result<T> = std::result::Result<T, RepositoryError>;
  
  // service/error.rs
  pub type Result<T> = std::result::Result<T, ServiceError>;
  
  // api/error.rs
  pub type Result<T> = std::result::Result<T, LogSeekApiError>;
  ```

- [ ] 是否实现了所有必要的 `From` 特征？
  ```rust
  impl From<RepositoryError> for LogSeekApiError { ... }
  impl From<ServiceError> for LogSeekApiError { ... }
  impl From<FileUrlError> for LogSeekApiError { ... }
  impl From<ParseError> for LogSeekApiError { ... }
  ```

- [ ] Routes 层是否都返回 `Result<T, LogSeekApiError>`？
  ```rust
  pub async fn my_handler(...) -> Result<Json<Response>> {
    // ...
  }
  ```

- [ ] 是否在日志中记录了错误信息？
  ```rust
  log::error!("操作失败: {:?}", error);
  ```

---

## 常见错误及修复

### 错误 1: 在 Routes 返回 AppError
```rust
// ❌ 错误
pub async fn search(...) -> Result<..., AppError> { }

// ✅ 正确
pub async fn search(...) -> Result<..., LogSeekApiError> { }
```

### 错误 2: 在 Service 返回 RepositoryError
```rust
// ❌ 错误
pub async fn process(...) -> Result<..., RepositoryError> { }

// ✅ 正确
pub async fn process(...) -> Result<..., ServiceError> { }
```

### 错误 3: 忘记 From 实现
```rust
// ❌ 错误 - 手动转换
pub async fn handler(...) -> Result<...> {
  let config = repo::load(&pool).await.map_err(|e| {
    LogSeekApiError::Repository(e)  // 需要手动转换
  })?;
}

// ✅ 正确 - 自动转换
pub async fn handler(...) -> Result<...> {
  let config = repo::load(&pool).await?;  // 自动使用 From
}
```

### 错误 4: 在流式 API 中吞掉错误
```rust
// ❌ 错误
match get_config().await {
  Ok(c) => { ... }
  Err(e) => {
    log::error!("错误: {}", e);
    return empty_stream();  // 客户端看不到错误
  }
}

// ✅ 正确
match get_config().await {
  Ok(c) => { ... }
  Err(e) => {
    let msg = format!(r#"{{"error":"{}"}}"#, e);
    let _ = tx.send(Ok(msg.into())).await;
    return;
  }
}
```

---

## 文件结构

创建后的项目结构：

```
backend/logseek/src/
├── api/
│   ├── error.rs        ← 新增/改进
│   ├── models.rs
│   └── mod.rs
├── service/
│   ├── error.rs        ← 新增
│   ├── search.rs
│   └── mod.rs
├── repository/
│   ├── error.rs        ← 新增
│   ├── settings.rs
│   └── mod.rs
├── domain/
│   ├── file_url.rs     ← 保持不变
│   └── mod.rs
└── routes/
    └── ...
```

---

## 单元测试示例

```rust
#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_repository_error_to_api_error() {
    let repo_err = RepositoryError::NotFound("config".into());
    let api_err: LogSeekApiError = repo_err.into();
    
    // 验证转换后的错误类型
    match api_err {
      LogSeekApiError::Repository(RepositoryError::NotFound(_)) => {}
      _ => panic!("转换失败"),
    }
  }

  #[test]
  fn test_error_response_mapping() {
    let repo_err = RepositoryError::NotFound("config".into());
    let api_err: LogSeekApiError = repo_err.into();
    let problem = Problem::from(api_err);
    
    assert_eq!(problem.status, Some(StatusCode::NOT_FOUND));
  }
}
```

---

## 总结

分层错误处理的核心：
1. 每层定义自己的错误类型
2. 使用 `From` 实现自动转换
3. API 层作为最后的聚合点
4. Routes 层统一返回 `LogSeekApiError`
