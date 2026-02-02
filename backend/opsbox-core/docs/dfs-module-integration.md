# LogSeek & Explorer 与 DFS 集成指南

**版本**: 1.0
**日期**: 2026-02-02
**状态**: 设计草案

---

## 目录

1. [当前使用方式分析](#1-当前使用方式分析)
2. [新设计下的使用方式](#2-新设计下的使用方式)
3. [Explorer 模块集成](#3-explorer-模块集成)
4. [LogSeek 模块集成](#4-logseek-模块集成)
5. [迁移指南](#5-迁移指南)

---

## 1. 当前使用方式分析

### 1.1 Explorer 模块

**当前实现** (`backend/explorer/src/service.rs`):

```rust
pub struct ExplorerService {
    orl_manager: Arc<OrlManager>,
    db_pool: SqlitePool,
    agent_manager: Option<Arc<AgentManager>>,
}

impl ExplorerService {
    pub fn new(db_pool: SqlitePool) -> Self {
        let mut manager = OrlManager::new();

        // 注册默认 Providers
        manager.register("local".to_string(), Arc::new(LocalOpsFS::new(None)));
        manager.register(
            "s3.root".to_string(),
            Arc::new(S3DiscoveryFileSystem::new(db_pool.clone())),
        );

        // 设置 S3 解析器
        let s3_resolver: OpsFileSystemResolver = Box::new(
            move |key: String| -> BoxFuture<'static, Option<Arc<dyn OpsFileSystem>>> {
                // 动态解析 S3 profile
            }
        );
        manager.set_resolver(s3_resolver);

        Self {
            orl_manager: Arc::new(manager),
            db_pool,
            agent_manager: None,
        }
    }

    pub async fn list(&self, orl: &ORL) -> Result<Vec<ResourceItem>, String> {
        // 使用 OrlManager 解析 ORL
        let provider = self.orl_manager.get_provider(orl)?;

        // 检查是否为归档
        if orl.target_type() != TargetType::Archive {
            return provider.read_dir(&orl.to_ops_path().unwrap()).await
        }

        // 归档处理...
    }
}
```

**使用模式**：
1. 注册 Providers 到 `OrlManager`
2. 使用 `ORL` 解析资源位置
3. 通过 `OrlManager` 获取对应的 Provider
4. 调用 Provider 的方法进行文件操作

---

### 1.2 LogSeek 模块

**当前实现** (`backend/logseek/src/service/search_executor.rs`):

```rust
pub struct SearchExecutor {
    // ...
}

impl SearchExecutor {
    /// 构造结果 ORL
    fn build_result_orl(&self, orl: &ORL, res_path: &str) -> String {
        use opsbox_core::odfs::orl::TargetType;

        let scheme = orl.uri().scheme().as_str();
        let authority = orl.uri().authority()
            .map(|a| a.as_str())
            .unwrap_or("local");

        // 判断是否为归档
        if orl.target_type() == TargetType::Archive {
            let entry_encoded = urlencoding::encode(res_path);
            format!("{}://{}{}?entry={}", scheme, authority, orl.path(), entry_encoded)
        } else {
            // 普通文件路径
            format!("{}://{}{}", scheme, authority, res_path)
        }
    }

    pub async fn execute_search(&self, orl: &ORL, query: &SearchQuery) -> Result<()> {
        // 获取 EntryStream
        let provider = self.orl_manager.get_provider(orl)?;
        let stream = provider.as_entry_stream(&orl.to_ops_path()?, true).await?;

        // 处理搜索结果...
    }
}
```

**使用模式**：
1. 解析 ORL 获取端点信息
2. 获取 Provider 并创建 `EntryStream`
3. 处理搜索结果并构造结果 ORL

---

## 2. 新设计下的使用方式

### 2.1 核心变化

| 方面 | 旧设计 | 新设计 |
|------|--------|--------|
| 服务入口 | `OrlManager` | `ResourceResolver` |
| 资源描述 | `ORL` + `TargetType` | `Resource` (包含 `ArchiveContext`) |
| Provider 注册 | 字符串 key | `EndpointKey` (类型安全) |
| 接口获取 | `get_provider()` | `resolve()` (返回 `FileSystem`) |
| 归档处理 | `target_type()` 判断 | `archive_context` 选项 |

### 2.2 新的依赖注入模式

```rust
// 新的服务结构
pub struct ExplorerService {
    resolver: Arc<ResourceResolver>,
    db_pool: SqlitePool,
    agent_manager: Option<Arc<AgentManager>>,
}

impl ExplorerService {
    pub fn new(resolver: Arc<ResourceResolver>, db_pool: SqlitePool) -> Self {
        Self {
            resolver,
            db_pool,
            agent_manager: None,
        }
    }
}
```

---

## 3. Explorer 模块集成

### 3.1 服务初始化

**新实现**:

```rust
use opsbox_core::dfs::services::ResourceResolver;
use opsbox_core::dfs::domain::{Endpoint, Resource};
use opsbox_core::dfs::fs::FileSystem;
use opsbox_core::dfs::implementations::{LocalFileSystem, S3Storage, AgentProxyFS};

pub struct ExplorerService {
    resolver: Arc<ResourceResolver>,
    db_pool: SqlitePool,
    agent_manager: Option<Arc<AgentManager>>,
}

impl ExplorerService {
    pub fn new(db_pool: SqlitePool) -> Self {
        let mut resolver = ResourceResolver::new();

        // 注册本地文件系统
        resolver.register(
            Endpoint::local_fs(),
            Arc::new(LocalFileSystem::new(None)) as Arc<dyn FileSystem>
        );

        // 注册 S3 根端点
        let s3_root = Endpoint::s3("root".to_string());
        resolver.register(
            s3_root.clone(),
            Arc::new(S3DiscoveryFileSystem::new(db_pool.clone()))
        );

        // 设置动态解析器
        let pool_clone = db_pool.clone();
        resolver.set_dynamic_resolver(move |endpoint| {
            Self::resolve_endpoint(&pool_clone, endpoint)
        });

        Self {
            resolver: Arc::new(resolver),
            db_pool,
            agent_manager: None,
        }
    }

    /// 动态解析端点（从数据库加载 S3 profiles 或 Agents）
    async fn resolve_endpoint(
        pool: &SqlitePool,
        endpoint: &Endpoint,
    ) -> Option<Arc<dyn FileSystem>> {
        match &endpoint.backend {
            StorageBackend::ObjectStorage => {
                // 从数据库加载 S3 profile
                let profile_name = &endpoint.identity;
                load_s3_profile(pool, profile_name).await.ok()
            }
            StorageBackend::FileSystem if matches!(endpoint.access_method, AccessMethod::Proxy) => {
                // 从 agent_manager 获取 agent
                let agent_name = &endpoint.identity;
                load_agent_agent(pool, agent_name).await.ok()
            }
            _ => None,
        }
    }
}
```

### 3.2 列表资源

**新实现**:

```rust
impl ExplorerService {
    /// 列出资源
    pub async fn list(&self, resource: &Resource) -> Result<Vec<ResourceItem>, String> {
        // 1. 解析资源，获取文件系统
        let fs = self.resolver.resolve(resource).await?;

        // 2. 获取要访问的路径
        let path = if let Some(archive_ctx) = &resource.archive_context {
            // 访问归档内部
            &archive_ctx.inner_path
        } else {
            // 直接访问
            &resource.primary_path
        };

        // 3. 列出目录
        let entries = fs.read_dir(path).await
            .map_err(|e| format!("Failed to list directory: {}", e))?;

        // 4. 转换为 ResourceItem
        let mut items = Vec::new();
        for entry in entries {
            items.push(self.entry_to_resource_item(resource, &entry).await?);
        }

        Ok(items)
    }

    /// 转换文件系统条目为资源项
    async fn entry_to_resource_item(
        &self,
        parent: &Resource,
        entry: &DirEntry,
    ) -> Result<ResourceItem, String> {
        let name = entry.name.clone();
        let metadata = entry.metadata.clone();

        // 构造子资源
        let child_path = parent.primary_path.join(&ResourcePath::from_str(&name));
        let mut child_resource = Resource {
            endpoint: parent.endpoint.clone(),
            primary_path: child_path,
            archive_context: None,
        };

        // 如果父资源是归档，子资源也在归档内
        if let Some(archive_ctx) = &parent.archive_context {
            let inner_path = archive_ctx.inner_path.join(&ResourcePath::from_str(&name));
            child_resource.archive_context = Some(ArchiveContext {
                inner_path,
                archive_type: archive_ctx.archive_type,
            });
        }

        // 检测子资源是否为归档
        if metadata.is_file() {
            if let Some(archive_type) = ArchiveType::from_extension(&name) {
                child_resource.archive_context = Some(ArchiveContext {
                    inner_path: ResourcePath::from_str(""),  // 归档根目录
                    archive_type: Some(archive_type),
                });
            }
        }

        Ok(ResourceItem {
            name,
            resource: child_resource,
            resource_type: if metadata.is_dir() { ResourceType::Directory } else { ResourceType::File },
            size: metadata.size(),
            modified: metadata.modified().map(|t| t.into()),
        })
    }
}
```

### 3.3 下载文件

**新实现**:

```rust
impl ExplorerService {
    /// 下载文件
    pub async fn download(&self, resource: &Resource) -> Result<Box<dyn AsyncRead + Send + Unpin>, String> {
        // 1. 解析资源
        let fs = self.resolver.resolve(resource).await?;

        // 2. 获取路径
        let path = if let Some(archive_ctx) = &resource.archive_context {
            &archive_ctx.inner_path
        } else {
            &resource.primary_path
        };

        // 3. 打开文件
        fs.open_read(path).await
            .map_err(|e| format!("Failed to open file: {}", e))
    }
}
```

---

## 4. LogSeek 模块集成

### 4.1 搜索服务初始化

**新实现**:

```rust
use opsbox_core::dfs::services::ResourceResolver;
use opsbox_core::dfs::domain::{Resource, Endpoint};
use opsbox_core::dfs::fs::{FileSystem, Searchable};

pub struct SearchExecutor {
    resolver: Arc<ResourceResolver>,
    config: SearchExecutorConfig,
    // ...
}

impl SearchExecutor {
    pub fn new(resolver: Arc<ResourceResolver>, config: SearchExecutorConfig) -> Self {
        Self {
            resolver,
            config,
            // ...
        }
    }
}
```

### 4.2 搜索执行

**新实现**:

```rust
impl SearchExecutor {
    /// 执行搜索
    pub async fn execute_search(
        &self,
        resource: &Resource,
        query: &SearchQuery,
    ) -> Result<Receiver<SearchEvent>, String> {
        // 1. 解析资源
        let fs = self.resolver.resolve(resource).await?;

        // 2. 获取搜索路径
        let search_path = if let Some(archive_ctx) = &resource.archive_context {
            &archive_ctx.inner_path
        } else {
            &resource.primary_path
        };

        // 3. 检查是否支持搜索
        let searchable: Option<&dyn Searchable> = fs.as_any().downcast_ref();

        let stream = match searchable {
            Some(s) => {
                // 使用优化的 EntryStream
                s.as_entry_stream(search_path, true).await?
            }
            None => {
                // 降级到普通 read_dir
                return self.fallback_search(fs, search_path, query).await;
            }
        };

        // 4. 处理搜索流
        self.process_search_stream(resource, stream, query).await
    }

    /// 处理搜索流
    async fn process_search_stream(
        &self,
        parent_resource: &Resource,
        mut stream: Box<dyn EntryStream>,
        query: &SearchQuery,
    ) -> Result<Receiver<SearchEvent>, String> {
        let (tx, rx) = mpsc::channel(self.config.stream_channel_capacity);
        let parent = parent_resource.clone();

        tokio::spawn(async move {
            while let Some(result) = stream.next_entry().await {
                match result {
                    Ok((entry, reader)) => {
                        // 执行搜索
                        if let Ok(matches) = Self::search_reader(reader, query).await {
                            for line in matches {
                                // 构造结果资源
                                let result_resource = Self::build_result_resource(&parent, &entry);

                                tx.send(SearchEvent::Success(SearchResult {
                                    resource: result_resource,
                                    line: line.number,
                                    content: line.content,
                                })).await.ok();
                            }
                        }
                    }
                    Err(e) => {
                        tx.send(SearchEvent::Error(e.to_string())).await.ok();
                        break;
                    }
                }
            }
        });

        Ok(rx)
    }
}
```

### 4.3 结果资源构造

**新实现**:

```rust
impl SearchExecutor {
    /// 构造结果资源
    fn build_result_resource(parent: &Resource, entry: &EntryMeta) -> Resource {
        let mut resource = parent.clone();

        // 更新路径
        if let Some(archive_ctx) = &resource.archive_context {
            // 在归档内，更新 inner_path
            resource.archive_context = Some(ArchiveContext {
                inner_path: archive_ctx.inner_path.join(&ResourcePath::from_str(&entry.name)),
                archive_type: archive_ctx.archive_type,
            });
        } else {
            // 直接访问，更新 primary_path
            resource.primary_path = resource.primary_path.join(&ResourcePath::from_str(&entry.name));
        }

        resource
    }
}
```

---

### 4.4 缓存集成

**新实现**:

```rust
impl SearchExecutor {
    /// 缓存结果
    async fn cache_result(&self, resource: &Resource, lines: Vec<SearchLine>) {
        // 将 Resource 转换为缓存键
        let cache_key = self.resource_to_cache_key(resource);

        simple_cache().put_lines(
            &self.sid,
            &cache_key,
            &lines.iter().map(|l| l.content.clone()).collect::<Vec<_>>(),
            "UTF-8".to_string(),
        ).await;
    }

    /// 资源转缓存键
    fn resource_to_cache_key(&self, resource: &Resource) -> String {
        // 构造 ORL 字符串
        let endpoint = &resource.endpoint;
        let path = resource.primary_path.as_str();

        match (&endpoint.location, &endpoint.backend, &endpoint.access_method) {
            (Location::Local, StorageBackend::FileSystem, AccessMethod::Direct) => {
                format!("orl://local{}", path)
            }
            (Location::Remote { host, port }, StorageBackend::FileSystem, AccessMethod::Proxy) => {
                if let Some(archive_ctx) = &resource.archive_context {
                    format!("orl://{}@agent.{}:{}/{}?entry={}",
                        endpoint.identity,
                        host, port, path,
                        urlencoding::encode(archive_ctx.inner_path.as_str())
                    )
                } else {
                    format!("orl://{}@agent.{}:{}/{}",
                        endpoint.identity, host, port, path
                    )
                }
            }
            (Location::Cloud, StorageBackend::ObjectStorage, AccessMethod::Direct) => {
                if let Some(archive_ctx) = &resource.archive_context {
                    format!("orl://{}@s3/{}?entry={}",
                        endpoint.identity,
                        path,
                        urlencoding::encode(archive_ctx.inner_path.as_str())
                    )
                } else {
                    format!("orl://{}@s3/{}",
                        endpoint.identity, path
                    )
                }
            }
            _ => format!("orl://{}", path)
        }
    }
}
```

---

## 5. 迁移指南

### 5.1 迁移步骤

#### 阶段 1: 准备工作

1. **更新依赖**
   ```toml
   [dependencies]
   opsbox-core = { path = "../opsbox-core", features = ["dfs-v2"] }
   ```

2. **添加兼容层**
   ```rust
   // logseek/src/compat/mod.rs
   pub use opsbox_core::dfs::v1 as odfs;  // 旧接口
   pub use opsbox_core::dfs::v2 as dfs;   // 新接口
   ```

#### 阶段 2: 服务层迁移

1. **ExplorerService**
   ```rust
   // 旧
   pub struct ExplorerService {
       orl_manager: Arc<OrlManager>,
       // ...
   }

   // 新
   pub struct ExplorerService {
       resolver: Arc<ResourceResolver>,
       // ...
   }
   ```

2. **SearchExecutor**
   ```rust
   // 旧
   pub struct SearchExecutor {
       orl_manager: Arc<OrlManager>,
       // ...
   }

   // 新
   pub struct SearchExecutor {
       resolver: Arc<ResourceResolver>,
       // ...
   }
   ```

#### 阶段 3: API 适配

1. **ORL → Resource**
   ```rust
   // 旧
   pub async fn list(&self, orl: &ORL) -> Result<Vec<ResourceItem>>;

   // 新
   pub async fn list(&self, resource: &Resource) -> Result<Vec<ResourceItem>>;

   // 兼容适配
   pub async fn list_orl(&self, orl: &ORL) -> Result<Vec<ResourceItem>> {
       let resource = self.orl_to_resource(orl)?;
       self.list(&resource).await
   }
   ```

2. **结果构造**
   ```rust
   // 旧
   fn build_result_orl(&self, orl: &ORL, path: &str) -> String;

   // 新
   fn build_result_resource(&self, parent: &Resource, entry: &EntryMeta) -> Resource;

   // 兼容适配
   fn build_result_orl_compat(&self, resource: &Resource) -> String {
       self.resource_to_orl_string(resource)
   }
   ```

#### 阶段 4: 测试

1. **单元测试**
   ```rust
   #[cfg(test)]
   mod tests {
       use super::*;

       #[tokio::test]
       async fn test_list_local_files() {
           let service = ExplorerService::new_test();
           let resource = Resource::local("/var/log".into());

           let items = service.list(&resource).await.unwrap();
           assert!(!items.is_empty());
       }

       #[tokio::test]
       async fn test_list_archive_files() {
           let service = ExplorerService::new_test();
           let resource = Resource::local_archive(
               "/data/test.tar".into(),
               "inner/file.log".into()
           );

           let items = service.list(&resource).await.unwrap();
           assert!(!items.is_empty());
       }
   }
   ```

2. **集成测试**
   ```rust
   #[tokio::test]
   async fn test_explorer_integration() {
       // 测试完整的资源浏览流程
   }

   #[tokio::test]
   async fn test_logseek_integration() {
       // 测试完整的搜索流程
   }
   ```

### 5.2 兼容性策略

#### 选项 A: 双轨运行

```rust
pub struct ExplorerService {
    // 保留旧接口
    orl_manager: Arc<OrlManager>,
    // 新接口
    resolver: Arc<ResourceResolver>,
    // 功能开关
    use_new_dfs: bool,
}

impl ExplorerService {
    pub async fn list(&self, orl: &ORL) -> Result<Vec<ResourceItem>> {
        if self.use_new_dfs {
            let resource = self.orl_to_resource(orl)?;
            self.list_v2(&resource).await
        } else {
            self.list_v1(orl).await
        }
    }
}
```

#### 选项 B: 适配器模式

```rust
pub struct OrlToResourceAdapter {
    resolver: Arc<ResourceResolver>,
}

impl OrlToResourceAdapter {
    pub fn list(&self, orl: &ORL) -> Result<Vec<ResourceItem>> {
        let resource = self.convert(orl)?;
        self.resolver.list(&resource).await
    }

    fn convert(&self, orl: &ORL) -> Result<Resource> {
        // ORL → Resource 转换逻辑
    }
}
```

### 5.3 迁移检查清单

- [ ] 更新 Cargo.toml 依赖
- [ ] 添加兼容层模块
- [ ] 重构 ExplorerService
- [ ] 重构 SearchExecutor
- [ ] 更新 API 路由
- [ ] 编写单元测试
- [ ] 编写集成测试
- [ ] 性能基准测试
- [ ] 更新文档
- [ ] 代码审查

---

## 附录

### A. 代码示例对比

| 场景 | 旧代码 | 新代码 |
|------|--------|--------|
| **初始化** | `OrlManager::new()` | `ResourceResolver::new()` |
| **注册 Provider** | `manager.register("key", fs)` | `resolver.register(endpoint, fs)` |
| **获取 Provider** | `manager.get_provider(orl)` | `resolver.resolve(resource)` |
| **判断归档** | `orl.target_type()` | `resource.archive_context.is_some()` |
| **构造结果** | `build_result_orl()` | `build_result_resource()` |

### B. 常见问题

**Q: 如何处理现有的 ORL 字符串？**

A: 使用 `ORL::parse()` 解析后，通过适配器转换为 `Resource`：

```rust
let orl = ORL::parse("orl://local/path/file.tar?entry=inner/log")?;
let resource = orl_to_resource(&orl)?;
```

**Q: 如何保持 API 兼容性？**

A: 提供兼容方法：

```rust
// 新接口
pub async fn list(&self, resource: &Resource) -> Result<...>;

// 兼容旧接口
pub async fn list_orl(&self, orl: &ORL) -> Result<...> {
    self.list(&orl_to_resource(orl)?).await
}
```

**Q: 性能会受影响吗？**

A: 新设计通过以下方式优化性能：
- 类型安全的 Endpoint 键减少查找开销
- Archive 文件系统缓存减少重复解析
- 更好的并发控制

---

**文档版本**: 1.0
**最后更新**: 2026-02-02
