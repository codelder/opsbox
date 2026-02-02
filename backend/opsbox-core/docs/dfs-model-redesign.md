# DFS 领域模型重构方案

## 问题分析

当前设计的 `EndpointType` 存在概念混淆：

```rust
pub enum EndpointType {
  Local,   // 本地文件系统
  Agent,   // 远程文件系统的代理
  S3,      // 云端对象存储
}
```

**核心问题**：Local、Agent、S3 不在同一抽象层次上
- **Agent** 在其运行的服务器上就是 **Local**
- Agent 是 Local 的 **远程代理**，而非独立的存储类型
- 这混淆了 **位置** 与 **访问方式** 两个正交的概念

---

## 方案一：正交维度模型 (推荐)

### 核心思想
将 **位置** (Location) 和 **存储后端** (StorageBackend) 分离为两个正交维度

### 领域模型

```rust
/// 位置维度：资源在哪里
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Location {
    /// 本机
    Local,
    /// 远程主机（通过网络访问）
    Remote { host: String, port: u16 },
    /// 云服务
    Cloud,
}

/// 存储后端维度：如何存储数据
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StorageBackend {
    /// 文件系统 (块存储)
    FileSystem,
    /// 对象存储 (键值存储)
    ObjectStorage,
}

/// 访问方式：如何连接到存储
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AccessMethod {
    /// 直接访问（本地文件系统、S3 SDK）
    Direct,
    /// 代理访问（通过 Agent）
    Proxy,
}

/// 端点描述：组合三个维度
pub struct Endpoint {
    pub location: Location,
    pub backend: StorageBackend,
    pub access_method: AccessMethod,
    pub identity: String,  // profile名、agent名等
}

impl Endpoint {
    /// 创建本地文件系统端点
    pub fn local_fs() -> Self {
        Endpoint {
            location: Location::Local,
            backend: StorageBackend::FileSystem,
            access_method: AccessMethod::Direct,
            identity: "localhost".to_string(),
        }
    }

    /// 创建Agent端点（远程文件系统代理）
    pub fn agent(host: String, port: u16, agent_name: String) -> Self {
        Endpoint {
            location: Location::Remote { host, port },
            backend: StorageBackend::FileSystem,
            access_method: AccessMethod::Proxy,
            identity: agent_name,
        }
    }

    /// 创建S3端点（云端对象存储）
    pub fn s3(profile: String) -> Self {
        Endpoint {
            location: Location::Cloud,
            backend: StorageBackend::ObjectStorage,
            access_method: AccessMethod::Direct,
            identity: profile,
        }
    }
}
```

### 对应关系表

| 配置 | Location | Backend | AccessMethod |
|------|----------|---------|--------------|
| `orl://local/var/log` | Local | FileSystem | Direct |
| `orl://web-01@agent.192.168.1.100:4001/app/logs` | Remote {192.168.1.100, 4001} | FileSystem | Proxy |
| `orl://prod@s3/bucket/path` | Cloud | ObjectStorage | Direct |

### ORL 格式调整

```rust
/// ORL 格式调整以反映新模型
///
/// 原有格式保持兼容，但语义更清晰：
///
/// - `orl://local/path`
///   -> Location=Local, Backend=FileSystem, Access=Direct
///
/// - `orl://[agent_name@]agent.[host][:port]/path`
///   -> Location=Remote, Backend=FileSystem, Access=Proxy
///
/// - `orl://[profile@]s3/bucket/path`
///   -> Location=Cloud, Backend=ObjectStorage, Access=Direct

pub struct ORL {
    inner: String,
    endpoint: Endpoint,
    path: String,
    archive_entry: Option<String>,
}

impl ORL {
    pub fn parse(s: impl Into<String>) -> Result<Self, OrlError> {
        let s = s.into();
        let uri = Uri::parse(&s)?;

        let endpoint = Self::parse_endpoint(&uri)?;
        let path = uri.path().as_str().to_string();
        let archive_entry = Self::extract_entry_param(&uri);

        Ok(Self { inner: s, endpoint, path, archive_entry })
    }

    fn parse_endpoint(uri: &Uri<&str>) -> Result<Endpoint, OrlError> {
        let auth = uri.authority().ok_or(OrlError::MissingAuthority)?;
        let host = auth.host();

        // 根据 host 判断端点类型
        match host.split('.').next().unwrap_or(host) {
            "local" => Ok(Endpoint::local_fs()),
            "agent" => {
                // 解析 `agent.[host][:port]` 格式
                let parts: Vec<&str> = host.split('.').collect();
                let host = parts.get(1).map(|s| s.to_string()).unwrap_or_else(||
                    auth.userinfo().and_then(|u| u.split(':').next()).map(String::from)
                        .unwrap_or_else(|| "localhost".to_string())
                );
                let port = parts.get(2).and_then(|p| p.parse().ok()).unwrap_or(4001);
                let agent_name = auth.userinfo()
                    .and_then(|u| u.split(':').next())
                    .map(String::from)
                    .unwrap_or_else(|| "root".to_string());

                Ok(Endpoint::agent(host, port, agent_name))
            }
            "s3" => {
                let profile = auth.userinfo()
                    .map(|u| u.as_str().to_string())
                    .unwrap_or_else(|| "default".to_string());
                Ok(Endpoint::s3(profile))
            }
            unknown => Err(OrlError::InvalidEndpointType(unknown.to_string()))
        }
    }
}
```

### FileSystem Trait 重构

```rust
/// 基础文件系统 trait - 所有后端都实现
#[async_trait]
pub trait FileSystem: Send + Sync {
    /// 获取元数据
    async fn metadata(&self, path: &OpsPath) -> Result<FileMetadata, FsError>;

    /// 列出目录
    async fn read_dir(&self, path: &OpsPath) -> Result<Vec<DirEntry>, FsError>;

    /// 打开文件读取
    async fn open_read(&self, path: &OpsPath) -> Result<Box<dyn AsyncRead + Send + Unpin>, FsError>;

    /// 获取端点信息
    fn endpoint(&self) -> &Endpoint;
}

/// ObjectStorage 特定操作
#[async_trait]
pub trait ObjectStorage: FileSystem {
    /// 列出对象（支持分页）
    async fn list_objects(&self, prefix: &str, limit: Option<usize>)
        -> Result<ObjectList, FsError>;

    /// 获取预签名URL
    async fn presigned_url(&self, key: &str, ttl: Duration) -> Result<String, FsError>;
}

/// 具体实现
pub struct LocalFileSystem {
    endpoint: Endpoint,
}

impl FileSystem for LocalFileSystem { /* ... */ }

pub struct AgentProxyClient {
    endpoint: Endpoint,
    client: HttpClient,
}

impl FileSystem for AgentProxyClient { /* ... */ }

pub struct S3Storage {
    endpoint: Endpoint,
    client: S3Client,
}

impl FileSystem for S3Storage { /* ... */ }
impl ObjectStorage for S3Storage { /* ... */ }
```

### 优势

1. **概念清晰**：位置、存储后端、访问方式三个维度正交
2. **易于扩展**：未来添加 EFS（云文件系统）只需 `Location::Cloud + Backend::FileSystem`
3. **类型安全**：编译时保证组合的合法性
4. **消除混淆**：Agent 明确表达为"远程的文件系统代理"

---

## 方案二：分层模型

### 核心思想
通过访问方式的分层来表达关系

```rust
/// 访问通道
pub enum AccessChannel {
    /// 直接访问通道
    Direct(Box<dyn DirectAccess>),
    /// 代理访问通道
    Proxy(Box<dyn ProxyAccess>),
}

/// 直接访问（本地文件系统、S3 SDK）
#[async_trait]
pub trait DirectAccess: Send + Sync {
    async fn read(&self, path: &str) -> Result<Vec<u8>>;
    async fn list(&self, path: &str) -> Result<Vec<DirEntry>>;
}

/// 代理访问（通过 Agent）
#[async_trait]
pub trait ProxyAccess: Send + Sync {
    /// 目标端点的信息
    fn target_endpoint(&self) -> &Endpoint;

    /// 代理转发请求
    async fn proxy_read(&self, path: &str) -> Result<Vec<u8>>;
    async fn proxy_list(&self, path: &str) -> Result<Vec<DirEntry>>;
}

/// Agent 的实现：通过 HTTP 代理到远程的 LocalFileSystem
pub struct AgentProxy {
    agent_info: AgentInfo,
    http_client: HttpClient,
}

impl ProxyAccess for AgentProxy {
    fn target_endpoint(&self) -> &Endpoint {
        &self.agent_info.endpoint
    }

    async fn proxy_read(&self, path: &str) -> Result<Vec<u8>> {
        // 转发 HTTP 请求到 Agent
        self.http_client.get(format!("{}/read{}", self.agent_info.url, path)).await?
    }
}
```

### 优势
1. Agent 明确表达为"代理模式"
2. 符合 GoF 的代理设计模式
3. 易于理解 Agent 的本质：转发请求到远程 Local

---

## 方案三：类型状态模型

### 核心思想
使用 Rust 的类型系统强制区分不同访问方式的端点

```rust
/// 端点的访问状态
pub trait AccessState {}

/// 直接访问状态
pub struct DirectAccess;
impl AccessState for DirectAccess {}

/// 代理访问状态
pub struct ProxyAccess;
impl AccessState for ProxyAccess {}

/// 带访问状态的端点
pub struct Endpoint<S: AccessState> {
    location: Location,
    backend: StorageBackend,
    _state: PhantomData<S>,
}

impl Endpoint<DirectAccess> {
    /// 创建本地文件系统端点
    pub fn local_fs() -> Self {
        Endpoint {
            location: Location::Local,
            backend: StorageBackend::FileSystem,
            _state: PhantomData,
        }
    }

    /// 创建 S3 端点
    pub fn s3(profile: String) -> Self {
        Endpoint {
            location: Location::Cloud,
            backend: StorageBackend::ObjectStorage,
            _state: PhantomData,
        }
    }
}

impl Endpoint<ProxyAccess> {
    /// 创建 Agent 端点
    pub fn agent(host: String, port: u16) -> Self {
        Endpoint {
            location: Location::Remote { host, port },
            backend: StorageBackend::FileSystem,
            _state: PhantomData,
        }
    }
}

/// 只有直接访问的端点才能直接打开文件
pub fn open_read(endpoint: &Endpoint<DirectAccess>, path: &str) -> Result<FileHandle> {
    // 编译时保证只有 DirectAccess 端点能调用
}

/// 代理访问端点需要通过代理方法
pub fn proxy_read(endpoint: &Endpoint<ProxyAccess>, path: &str) -> Result<Vec<u8>> {
    // Agent 特定的代理逻辑
}
```

### 优势
1. 编译时保证访问方式的正确性
2. 防止混淆 Direct 和 Proxy 操作
3. 零成本抽象

---

## 迁移建议

### 阶段1：保持兼容，引入新概念
```rust
// 保留旧类型作为别名
pub use EndpointType as LegacyEndpointType;

// 引入新的正交模型
pub mod v2 {
    pub use {Location, StorageBackend, AccessMethod, Endpoint};
}
```

### 阶段2：逐步迁移
1. 新代码使用新的模型
2. 旧代码通过适配器模式兼容
3. ORL 解析器同时支持两种格式

### 阶段3：完全切换
1. 移除旧的 `EndpointType`
2. 统一使用新的维度模型

---

## 总结

| 方案 | 优势 | 劣势 | 推荐度 |
|------|------|------|--------|
| 方案一：正交维度 | 概念清晰、易扩展 | 需要重构现有代码 | ⭐⭐⭐⭐⭐ |
| 方案二：分层模型 | 符合代理模式 | 仍有一定混淆 | ⭐⭐⭐⭐ |
| 方案三：类型状态 | 编译时安全 | 复杂度高、学习曲线陡 | ⭐⭐⭐ |

**推荐方案一**：正交维度模型最准确地表达了领域的本质，且为未来扩展（如 EFS、MinIO）提供了清晰的路径。

---

**文档版本**: 1.0
**创建日期**: 2026-02-02
