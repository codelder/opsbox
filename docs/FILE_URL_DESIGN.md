# 统一文件 URL 设计方案

## 📋 概述

本设计提供了一个统一的文件标识符系统，支持多种存储源：
- 本地文件系统
- S3 兼容对象存储（支持多配置）
- Tar/Tar.gz 压缩包内文件
- 远程 Agent 节点文件

---

## 🎯 URL 格式规范

### 1. 本地文件

```
file:///<absolute_path>
```

**示例**:
```
file:///var/log/app.log
file:///Users/admin/logs/error.log
```

### 2. S3 对象存储

#### 使用默认配置
```
s3://<bucket>/<key>
```

#### 使用指定配置
```
s3://<profile>:<bucket>/<key>
```

**示例**:
```
s3://backupdr/logs/2024/app.log              # 使用默认配置
s3://prod:backupdr/logs/2024/app.log         # 使用 prod 配置
s3://dev:test-bucket/debug/trace.log         # 使用 dev 配置
```

### 3. Tar 压缩包内文件

#### Tar 格式
```
tar+<base_url>:<entry_path>
```

#### Tar.gz 格式
```
tar.gz+<base_url>:<entry_path>
```

**示例**:
```
# S3 上的 tar.gz 包（默认配置）
tar.gz+s3://backupdr/archive.tar.gz:home/logs/app.log

# S3 上的 tar.gz 包（指定配置）
tar.gz+s3://prod:backupdr/archive.tar.gz:var/log/nginx.log

# 本地 tar 包
tar+file:///data/backup.tar:etc/config.yaml
```

### 4. Agent 远程文件

```
agent://<agent_id>/<path>
```

**示例**:
```
agent://prod-server-01/var/log/app.log
agent://k8s-node-3/opt/app/logs/debug.log
```

---

## 🔧 数据库配置管理

### S3 Profile 配置表

```sql
CREATE TABLE s3_profiles (
  profile_name TEXT PRIMARY KEY,  -- 配置名称（如 'prod', 'dev', 'backup'）
  endpoint TEXT NOT NULL,          -- S3 服务端点（如 'http://192.168.1.100:9002'）
  access_key TEXT NOT NULL,
  secret_key TEXT NOT NULL,
  created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
  updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

-- 插入示例配置
INSERT INTO s3_profiles (profile_name, endpoint, access_key, secret_key) VALUES
  ('default', 'http://192.168.1.100:9002', 'admin', 'password123'),
  ('prod', 'https://s3.amazonaws.com', 'AKIAEXAMPLE', 'secretkey'),
  ('backup', 'http://backup-minio:9000', 'backup-user', 'backup-pass');
```

### 配置查询示例

```rust
async fn resolve_s3_url(file_url: &FileUrl, pool: &SqlitePool) -> Result<S3Client> {
  match file_url {
    FileUrl::S3 { profile, bucket, key } => {
      let profile_name = profile.as_deref().unwrap_or("default");
      
      // 从数据库加载配置
      let config = sqlx::query_as::<_, S3Profile>(
        "SELECT * FROM s3_profiles WHERE profile_name = ?"
      )
      .bind(profile_name)
      .fetch_one(pool)
      .await?;
      
      // 创建客户端
      get_or_create_s3_client(&config.endpoint, &config.access_key, &config.secret_key)
    }
    _ => Err(Error::InvalidFileType)
  }
}
```

---

## 💻 Rust API 使用示例

### 创建 URL

```rust
use logseek::domain::{FileUrl, TarCompression};

// 本地文件
let local = FileUrl::local("/var/log/app.log");

// S3 对象（默认配置）
let s3_default = FileUrl::s3("backupdr", "logs/2024/app.log");

// S3 对象（指定配置）
let s3_prod = FileUrl::s3_with_profile("prod", "backupdr", "logs/2024/app.log");

// Tar.gz 包内文件
let base = FileUrl::s3("backupdr", "archive.tar.gz");
let tar_entry = FileUrl::tar_entry(
  TarCompression::Gzip, 
  base, 
  "home/logs/app.log"
)?;

// Agent 文件
let agent = FileUrl::agent("prod-server-01", "/var/log/app.log");
```

### 解析 URL

```rust
use std::str::FromStr;

// 从字符串解析
let url: FileUrl = "s3://prod:backupdr/logs/app.log".parse()?;

// 从复杂 URL 解析
let tar_url: FileUrl = "tar.gz+s3://prod:backupdr/archive.tar.gz:logs/app.log".parse()?;

// 模式匹配
match url {
  FileUrl::S3 { profile, bucket, key } => {
    println!("配置: {:?}, 桶: {}, 路径: {}", profile, bucket, key);
  }
  FileUrl::TarEntry { compression, base, entry_path } => {
    println!("压缩格式: {:?}, 基础: {}, 条目: {}", compression, base, entry_path);
  }
  _ => {}
}
```

### URL 转换

```rust
// 转为字符串
let url_string = file_url.to_string();

// 获取文件类型
let file_type = file_url.file_type(); // "local", "s3", "tar-entry", "agent"

// 获取显示名称
let display_name = file_url.display_name(); // "app.log"

// 判断是否为归档文件
if file_url.is_archive() {
  println!("这是一个 tar 包内的文件");
}
```

---

## 🏗️ 实际应用场景

### 场景 1: 多环境配置管理

```rust
// 开发环境搜索本地 tar 包
let dev_url = "tar.gz+file:///tmp/dev-logs.tar.gz:app.log";

// 测试环境搜索测试 MinIO
let test_url = "tar.gz+s3://test:test-bucket/logs.tar.gz:app.log";

// 生产环境搜索 AWS S3
let prod_url = "tar.gz+s3://prod:production-logs/2024/01/logs.tar.gz:app.log";

// 统一处理
for url_str in &[dev_url, test_url, prod_url] {
  let url: FileUrl = url_str.parse()?;
  process_log_file(&url).await?;
}
```

### 场景 2: 文件缓存系统

```rust
use std::collections::HashMap;

struct FileCache {
  cache: HashMap<String, Vec<String>>,
}

impl FileCache {
  async fn get_or_fetch(&mut self, url: &FileUrl) -> Result<Vec<String>> {
    let key = url.to_string();
    
    if let Some(cached) = self.cache.get(&key) {
      return Ok(cached.clone());
    }
    
    // 根据 URL 类型执行不同的读取策略
    let content = match url {
      FileUrl::Local { path } => read_local_file(path).await?,
      FileUrl::S3 { profile, bucket, key } => {
        let client = resolve_s3_client(profile).await?;
        client.get_object(bucket, key).await?
      }
      FileUrl::TarEntry { base, entry_path, .. } => {
        let tar_data = self.get_or_fetch(base).await?;
        extract_tar_entry(&tar_data, entry_path)?
      }
      FileUrl::Agent { agent_id, path } => {
        fetch_from_agent(agent_id, path).await?
      }
    };
    
    self.cache.insert(key, content.clone());
    Ok(content)
  }
}
```

### 场景 3: 搜索结果标识

```rust
#[derive(Serialize)]
struct SearchResult {
  file_id: String,      // 使用 FileUrl 的字符串表示
  lines: Vec<String>,
  highlights: Vec<(usize, usize)>,
}

// 生成搜索结果
async fn search_logs(pattern: &str) -> Vec<SearchResult> {
  let files = vec![
    FileUrl::s3("backupdr", "logs/2024/app.log"),
    FileUrl::tar_entry(
      TarCompression::Gzip,
      FileUrl::s3("backupdr", "archive.tar.gz"),
      "old-logs/2023.log"
    ).unwrap(),
  ];
  
  let mut results = Vec::new();
  for file_url in files {
    let content = read_file(&file_url).await?;
    let matches = grep_pattern(&content, pattern);
    
    results.push(SearchResult {
      file_id: file_url.to_string(), // 唯一标识符
      lines: matches,
      highlights: vec![],
    });
  }
  
  results
}
```

---

## 🔒 安全考虑

### 1. URL 验证
```rust
impl FileUrl {
  pub fn validate(&self) -> Result<()> {
    match self {
      Self::S3 { bucket, key, .. } => {
        if bucket.is_empty() || key.is_empty() {
          return Err(Error::InvalidUrl("S3 URL 缺少必需字段"));
        }
      }
      Self::TarEntry { entry_path, .. } => {
        // 防止路径穿越攻击
        if entry_path.contains("..") {
          return Err(Error::PathTraversal);
        }
      }
      _ => {}
    }
    Ok(())
  }
}
```

### 2. 配置隔离
- 敏感配置（access_key, secret_key）仅存储在数据库
- URL 中只包含 profile 名称，不包含凭证
- 不同环境使用不同的 profile

### 3. 权限控制
```rust
async fn check_access(user_id: &str, file_url: &FileUrl) -> Result<bool> {
  match file_url {
    FileUrl::S3 { profile, .. } => {
      // 检查用户是否有权访问此 profile
      has_profile_permission(user_id, profile).await
    }
    FileUrl::Agent { agent_id, .. } => {
      // 检查用户是否有权访问此 agent
      has_agent_permission(user_id, agent_id).await
    }
    _ => Ok(true)
  }
}
```

---

## 📊 性能优化

### 1. URL 解析缓存
```rust
use once_cell::sync::Lazy;
use std::sync::Mutex;

static URL_CACHE: Lazy<Mutex<HashMap<String, FileUrl>>> = 
  Lazy::new(|| Mutex::new(HashMap::new()));

fn parse_cached(url_str: &str) -> Result<FileUrl> {
  let mut cache = URL_CACHE.lock().unwrap();
  
  if let Some(url) = cache.get(url_str) {
    return Ok(url.clone());
  }
  
  let url: FileUrl = url_str.parse()?;
  cache.insert(url_str.to_string(), url.clone());
  Ok(url)
}
```

### 2. S3 客户端连接池
- 已在 `utils/storage.rs` 中实现
- 按 `endpoint + access_key` 缓存客户端
- 避免重复创建连接

---

## 🧪 测试覆盖

当前 `file_url.rs` 已包含以下测试：
- ✅ 本地文件创建和解析
- ✅ S3 对象（默认配置）
- ✅ S3 对象（指定 profile）
- ✅ Tar/Tar.gz 包内文件
- ✅ Agent 文件
- ✅ 嵌套 tar 拒绝（防止无限递归）
- ✅ URL 往返转换（字符串 ↔ 结构体）

---

## 🚀 未来扩展

### 1. 支持更多协议
```rust
pub enum FileUrl {
  // ... 现有类型
  
  /// HDFS 分布式文件系统
  /// 格式: hdfs://namenode:9000/path/to/file
  Hdfs { namenode: String, port: u16, path: String },
  
  /// HTTP/HTTPS 远程文件
  /// 格式: https://example.com/logs/app.log
  Http { url: String },
  
  /// Git 仓库中的文件
  /// 格式: git://github.com/user/repo:branch:path/to/file
  Git { repo: String, branch: String, path: String },
}
```

### 2. URL 模式匹配
```rust
// 支持通配符搜索
let pattern = "s3://prod:backupdr/logs/2024/**/*.log";
let matching_files = expand_pattern(&pattern).await?;
```

### 3. URL 别名系统
```sql
CREATE TABLE file_url_aliases (
  alias TEXT PRIMARY KEY,
  actual_url TEXT NOT NULL
);

-- INSERT INTO file_url_aliases VALUES
--   ('latest-prod-log', 's3://prod:backupdr/logs/2024/latest.log');
```

---

## 📝 总结

本设计提供了：
- ✅ **统一标识符** - 一个 URL 格式支持所有存储源
- ✅ **多环境支持** - Profile 机制实现配置隔离
- ✅ **可扩展性** - 易于添加新的存储类型
- ✅ **类型安全** - 编译时检查，避免运行时错误
- ✅ **易于使用** - 清晰的 API，丰富的文档

**下一步**：
1. 在 `routes.rs` 中使用 `FileUrl` 替换现有的字符串拼接
2. 实现 `s3_profiles` 表和查询逻辑
3. 更新前端 API 使用新的 URL 格式

