# 存储源使用示例

## 1. 本地文件系统搜索

```rust
use logseek::storage::local::LocalFileSystem;
use logseek::service::coordinator::SearchCoordinator;
use std::sync::Arc;

// 创建本地文件系统数据源
let local_fs = LocalFileSystem::new("/var/log/app");

// 创建协调器并添加数据源
let mut coordinator = SearchCoordinator::new();
coordinator.add_data_source(Arc::new(local_fs));

// 执行搜索
let mut results = coordinator.search("error", 3).await?;
while let Some(result) = results.recv().await {
    println!("文件: {}", result.path);
    println!("匹配行数: {}", result.lines.len());
}
```

---

## 2. Tar.gz 归档搜索

```rust
use logseek::storage::targz::TarGzFile;
use logseek::service::coordinator::SearchCoordinator;
use std::path::PathBuf;
use std::sync::Arc;

// 创建 tar.gz 数据源
let targz = TarGzFile::new(PathBuf::from("/backup/logs-2024-01.tar.gz"));

// 创建协调器
let mut coordinator = SearchCoordinator::new();
coordinator.add_data_source(Arc::new(targz));

// 搜索错误日志
let mut results = coordinator.search("ERROR", 5).await?;
while let Some(result) = results.recv().await {
    println!("归档中的文件: {}", result.path);
    for (i, line) in result.lines.iter().enumerate() {
        println!("  {}: {}", i + 1, line);
    }
}
```

---

## 3. MinIO 对象存储搜索

```rust
use logseek::storage::minio::{MinIOConfig, MinIOStorage};
use logseek::service::coordinator::SearchCoordinator;
use std::sync::Arc;

// 配置 MinIO
let config = MinIOConfig {
    url: "http://minio.example.com:9000".to_string(),
    access_key: "admin".to_string(),
    secret_key: "admin123".to_string(),
    bucket: "application-logs".to_string(),
    prefix: Some("2024/01/".to_string()),      // 仅搜索 2024/01/ 下的对象
    pattern: Some(r"\.log$".to_string()),      // 仅搜索 .log 文件
};

// 创建 MinIO 数据源
let minio = MinIOStorage::new(config)?;

// 创建协调器
let mut coordinator = SearchCoordinator::new();
coordinator.add_data_source(Arc::new(minio));

// 搜索警告日志
let mut results = coordinator.search("WARN", 3).await?;
while let Some(result) = results.recv().await {
    println!("S3 对象: {}", result.path);
    println!("匹配: {:?}", result.merged);
}
```

---

## 4. Agent 远程搜索

```rust
use logseek::storage::agent::{AgentClient, AgentInfo};
use logseek::service::coordinator::SearchCoordinator;
use std::sync::Arc;

// 创建 Agent 客户端
let agent_info = AgentInfo {
    agent_id: "agent-01".to_string(),
    base_url: "http://remote-server:8080".to_string(),
    status: "active".to_string(),
    capabilities: Default::default(),
    last_heartbeat: chrono::Utc::now(),
};

let agent = AgentClient::new(agent_info);

// 创建协调器
let mut coordinator = SearchCoordinator::new();
coordinator.add_search_service(Arc::new(agent));

// Agent 在远程执行搜索并返回结果
let mut results = coordinator.search("panic", 5).await?;
while let Some(result) = results.recv().await {
    println!("远程文件: {}", result.path);
}
```

---

## 5. 多数据源组合搜索

```rust
use logseek::storage::{local::LocalFileSystem, targz::TarGzFile, minio::{MinIOConfig, MinIOStorage}};
use logseek::service::coordinator::SearchCoordinator;
use std::sync::Arc;
use std::path::PathBuf;

let mut coordinator = SearchCoordinator::new();

// 添加本地文件系统
coordinator.add_data_source(Arc::new(
    LocalFileSystem::new("/var/log/current")
));

// 添加归档文件
coordinator.add_data_source(Arc::new(
    TarGzFile::new(PathBuf::from("/backup/old-logs.tar.gz"))
));

// 添加 MinIO
let minio_config = MinIOConfig {
    url: "http://minio:9000".to_string(),
    access_key: "admin".to_string(),
    secret_key: "password".to_string(),
    bucket: "historical-logs".to_string(),
    prefix: None,
    pattern: None,
};
coordinator.add_data_source(Arc::new(MinIOStorage::new(minio_config)?));

// 跨所有数据源搜索
let mut results = coordinator.search("critical", 3).await?;
let mut total = 0;
while let Some(result) = results.recv().await {
    total += 1;
    println!("{}: {}", total, result.path);
}
println!("总共找到 {} 个匹配文件", total);
```

---

## 6. 高级查询示例

### 布尔查询
```rust
// 搜索包含 "error" 但不包含 "ignore" 的行
coordinator.search("error -ignore", 3).await?;
```

### 短语搜索
```rust
// 搜索完整短语
coordinator.search(r#""connection timeout""#, 3).await?;
```

### 正则表达式
```rust
// 使用正则表达式
coordinator.search(r#"/error|warn|fatal/i"#, 3).await?;
```

### 路径过滤
```rust
// 仅搜索 .log 文件
coordinator.search("path:*.log error", 3).await?;
```

---

## 7. 环境变量配置

### MinIO 超时设置
```bash
export LOGSEEK_MINIO_TIMEOUT_SEC=120
export LOGSEEK_MINIO_MAX_ATTEMPTS=10
```

### 代理设置
```bash
export NO_PROXY=localhost,minio.local
export HTTP_PROXY=http://proxy:8080
```

---

## 8. 错误处理

```rust
use logseek::storage::StorageError;

match coordinator.search("error", 3).await {
    Ok(mut results) => {
        while let Some(result) = results.recv().await {
            // 处理结果
        }
    }
    Err(StorageError::NotFound(path)) => {
        eprintln!("文件不存在: {}", path);
    }
    Err(StorageError::PermissionDenied(path)) => {
        eprintln!("权限被拒绝: {}", path);
    }
    Err(StorageError::Timeout) => {
        eprintln!("搜索超时");
    }
    Err(e) => {
        eprintln!("搜索失败: {}", e);
    }
}
```

---

## 9. 性能优化建议

### TarGzFile
```rust
// 适合: 小到中型归档 (<100MB)
// 首次访问会加载整个归档到内存
// 后续访问使用缓存，速度极快

let targz = TarGzFile::new(path);
// 预加载（可选）
targz.ensure_initialized().await?;
```

### MinIOStorage
```rust
// 使用前缀缩小搜索范围
let config = MinIOConfig {
    // ...
    prefix: Some("2024/01/app1/".to_string()),  // 限制范围
    pattern: Some(r"error\.log$".to_string()),  // 过滤文件
};
```

### LocalFileSystem
```rust
// 使用具体路径而不是根目录
let local = LocalFileSystem::new("/var/log/myapp");  // ✅ 好
// 避免
let local = LocalFileSystem::new("/");                // ❌ 慢
```

---

## 10. 完整应用示例

```rust
use logseek::{
    storage::{
        local::LocalFileSystem,
        targz::TarGzFile,
        minio::{MinIOConfig, MinIOStorage},
    },
    service::coordinator::SearchCoordinator,
};
use std::sync::Arc;
use std::path::PathBuf;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 初始化日志
    env_logger::init();

    // 创建协调器
    let mut coordinator = SearchCoordinator::new();

    // 添加本地日志
    coordinator.add_data_source(Arc::new(
        LocalFileSystem::new("/var/log/app")
    ));

    // 添加归档
    coordinator.add_data_source(Arc::new(
        TarGzFile::new(PathBuf::from("/backup/logs.tar.gz"))
    ));

    // 添加 MinIO
    let minio_config = MinIOConfig {
        url: std::env::var("MINIO_URL")?,
        access_key: std::env::var("MINIO_ACCESS_KEY")?,
        secret_key: std::env::var("MINIO_SECRET_KEY")?,
        bucket: "logs".to_string(),
        prefix: Some("prod/".to_string()),
        pattern: Some(r"\.log$".to_string()),
    };
    coordinator.add_data_source(Arc::new(MinIOStorage::new(minio_config)?));

    // 执行搜索
    println!("搜索 'error' ...");
    let mut results = coordinator.search("error", 5).await?;
    
    let mut count = 0;
    while let Some(result) = results.recv().await {
        count += 1;
        println!("\n[{}] 文件: {}", count, result.path);
        println!("匹配行数: {}", result.lines.len());
        
        for (i, line) in result.lines.iter().take(3).enumerate() {
            println!("  {}: {}", i + 1, line.trim());
        }
    }

    println!("\n总共找到 {} 个匹配文件", count);
    Ok(())
}
```

---

## 总结

本文档展示了如何使用新的存储抽象层：

- ✅ **LocalFileSystem**: 本地文件搜索
- ✅ **TarGzFile**: 归档文件搜索
- ✅ **MinIOStorage**: 对象存储搜索
- ✅ **AgentClient**: 远程代理搜索
- ✅ **多数据源组合**: 统一搜索接口

所有数据源共享相同的 API，易于扩展和组合使用。

