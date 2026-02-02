# Archive 概念分析与建模

## 问题本质

**核心问题**：Archive 在当前模型中被当作 `TargetType`，但它实际上与 Endpoint 是完全正交的概念：

```
混淆的关系：
EndpointType { Local, Agent, S3 }  ← 位置/后端
TargetType { Dir, Archive }        ← ??? 这是资源类型，不是端点类型
```

---

## Archive 的本质

### 1. Archive 是一种容器格式

Archive（如 .tar, .zip）是一个**文件**，但它内部包含了一个完整的目录结构：

```
物理视角：                    逻辑视角：
/data/logs/archive.tar       /data/logs/archive.tar/
├── metadata.json           ├── metadata.json
├── 2024/                   ├── 2024/
│   ├── 01/                 │   ├── 01/
│   │   ├── app.log         │   │   ├── app.log
│   │   └── error.log       │   │   └── error.log
│   └── 02/                 │   └── 02/
└── config/                 └── config/
```

### 2. Archive 与 Endpoint 的正交关系

Archive 可以存在于**任何**存储后端上：

| 存储位置 | Archive 示例 |
|---------|-------------|
| Local | `/var/log/backup.tar.gz` |
| S3 | `s3://backup-bucket/2024.tar` |
| Agent | `orl://web-01@agent/data/archive.zip` |

**结论**：Archive 是文件的**属性/格式**，与存储位置无关。

### 3. Archive 是一个虚拟文件系统

当我们要访问 Archive 内部时，它表现得像一个文件系统：

```
orl://local/data/archive.tar?entry=inner/file.log

物理链路：
1. LocalFS 读取 /data/archive.tar
2. ArchiveFS 解压 archive.tar
3. 返回 inner/file.log 的内容
```

---

## 正确的概念模型

### 核心维度分离

```
┌─────────────────────────────────────────────────────────┐
│                    Resource (资源)                      │
│  ┌─────────────┐  ┌──────────────┐  ┌────────────────┐ │
│  │  Endpoint   │  │    Path      │  │ Archive Context│ │
│  │ (存储位置)   │  │   (路径)     │  │  (可选的容器)   │ │
│  └─────────────┘  └──────────────┘  └────────────────┘ │
│         │                │                   │         │
│         ▼                ▼                   ▼         │
│  Location        │data/archive.tar│  │inner/file.log   │
│  Backend         │                 │  │                 │
│  AccessMethod                     │                   │
└─────────────────────────────────────────────────────────┘
```

### 领域模型设计

```rust
/// 资源描述 = 端点 + 路径 + 可选的归档上下文
pub struct Resource {
    /// 存储端点
    pub endpoint: Endpoint,

    /// 主路径（指向归档文件或普通文件/目录）
    pub primary_path: ResourcePath,

    /// 归档上下文（如果访问归档内部）
    pub archive_context: Option<ArchiveContext>,
}

/// 资源路径
#[derive(Clone, PartialEq, Eq)]
pub struct ResourcePath {
    /// 路径片段
    segments: Vec<String>,

    /// 是否为绝对路径
    is_absolute: bool,
}

/// 归档上下文
#[derive(Clone, PartialEq, Eq)]
pub struct ArchiveContext {
    /// 归档内的路径
    inner_path: ResourcePath,

    /// 归档类型（可选，用于性能优化）
    archive_type: Option<ArchiveType>,
}

/// 归档类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ArchiveType {
    Tar,
    TarGz,
    Tgz,
    Zip,
    Gz,
    // 未来可扩展
    Rar,
    SevenZ,
}

impl ArchiveType {
    /// 从文件扩展名检测
    pub fn from_extension(path: &str) -> Option<Self> {
        if path.ends_with(".tar") {
            Some(Self::Tar)
        } else if path.ends_with(".tar.gz") {
            Some(Self::TarGz)
        } else if path.ends_with(".tgz") {
            Some(Self::Tgz)
        } else if path.ends_with(".zip") {
            Some(Self::Zip)
        } else if path.ends_with(".gz") {
            Some(Self::Gz)
        } else {
            None
        }
    }

    /// 从 Magic Bytes 检测（更可靠）
    pub fn from_magic_bytes(data: &[u8]) -> Option<Self> {
        if data.len() < 4 {
            return None;
        }

        // ZIP: PK\x03\x04
        if &data[0..2] == b"PK" {
            return Some(Self::Zip);
        }

        // TAR: 空块或特定签名（较难检测）
        // 这里可以添加更精确的检测逻辑

        None
    }
}
```

---

## FileSystem 的分层实现

### 核心思想：组合模式 + 装饰器模式

```rust
/// 基础文件系统 trait
#[async_trait]
pub trait FileSystem: Send + Sync + AsAny {
    /// 获取元数据
    async fn metadata(&self, path: &ResourcePath) -> Result<FileMetadata, FsError>;

    /// 列出目录
    async fn read_dir(&self, path: &ResourcePath) -> Result<Vec<DirEntry>, FsError>;

    /// 打开文件读取
    async fn open_read(&self, path: &ResourcePath) -> Result<Box<dyn AsyncRead + Send + Unpin>, FsError>;

    /// 获取端点信息
    fn endpoint(&self) -> &Endpoint;
}

/// Archive 文件系统 - 装饰器模式
///
/// 包装一个底层 FS，提供归档内的访问能力
pub struct ArchiveFileSystem<F: FileSystem> {
    /// 底层文件系统
    underlying: F,

    /// 归档文件路径
    archive_path: ResourcePath,

    /// 归档类型
    archive_type: ArchiveType,

    /// 归档缓存（用于已打开的归档）
    cache: Arc<Mutex<LruCache<ResourcePath, Archive>>>,
}

impl<F: FileSystem> ArchiveFileSystem<F> {
    /// 创建归档文件系统
    pub fn new(underlying: F, archive_path: ResourcePath) -> Result<Self, FsError> {
        let archive_type = ArchiveType::from_extension(archive_path.as_str())
            .ok_or_else(|| FsError::UnknownArchiveFormat)?;

        Ok(Self {
            underlying,
            archive_path,
            archive_type,
            cache: Arc::new(Mutex::new(LruCache::new(16))),
        })
    }

    /// 获取或打开归档
    async fn get_archive(&self) -> Result<Archive, FsError> {
        // 检查缓存
        {
            let mut cache = self.cache.lock().await;
            if let Some(archive) = cache.get(&self.archive_path) {
                return Ok(archive.clone());
            }
        }

        // 打开归档文件
        let mut file = self.underlying.open_read(&self.archive_path).await?;

        // 检测归档类型（如果还没确定）
        let archive_type = if self.archive_type == ArchiveType::Tar {
            // 尝试从内容检测
            let mut buffer = [0u8; 512];
            file.read_exact(&mut buffer).await?;
            ArchiveType::from_magic_bytes(&buffer).unwrap_or(self.archive_type)
        } else {
            self.archive_type
        };

        // 解析归档
        let archive = match archive_type {
            ArchiveType::Tar => Archive::from_tar(file).await?,
            ArchiveType::TarGz | ArchiveType::Tgz => Archive::from_tar_gz(file).await?,
            ArchiveType::Zip => Archive::from_zip(file).await?,
            ArchiveType::Gz => Archive::from_gz(file).await?,
            _ => return Err(FsError::UnknownArchiveFormat),
        };

        // 缓存归档
        {
            let mut cache = self.cache.lock().await;
            cache.put(self.archive_path.clone(), archive.clone());
        }

        Ok(archive)
    }
}

#[async_trait]
impl<F: FileSystem> FileSystem for ArchiveFileSystem<F> {
    /// 归档内的元数据
    async fn metadata(&self, inner_path: &ResourcePath) -> Result<FileMetadata, FsError> {
        let archive = self.get_archive().await?;
        archive.entry_metadata(inner_path).ok_or(FsError::NotFound)
    }

    /// 列出归档内目录
    async fn read_dir(&self, inner_path: &ResourcePath) -> Result<Vec<DirEntry>, FsError> {
        let archive = self.get_archive().await?;
        archive.list_entries(inner_path)
    }

    /// 读取归档内文件
    async fn open_read(&self, inner_path: &ResourcePath) -> Result<Box<dyn AsyncRead + Send + Unpin>, FsError> {
        let archive = self.get_archive().await?;
        archive.open_entry(inner_path).ok_or(FsError::NotFound)?
    }

    fn endpoint(&self) -> &Endpoint {
        self.underlying.endpoint()
    }
}
```

---

## OrlManager 的重构

### 静态类型系统表达层次关系

```rust
/// 文件系统层次
pub enum FileSystemLayer {
    /// 直接访问底层文件系统
    Direct(Arc<dyn FileSystem>),

    /// 归档文件系统（多层嵌套）
    Archive {
        /// 底层文件系统
        base: Arc<dyn FileSystem>,
        /// 归档路径链（支持嵌套归档：tar.gz?entry=inner.zip?entry=deeper/file.txt）
        archive_chain: Vec<(ResourcePath, ArchiveContext)>,
    },
}

pub struct OrlManager {
    providers: HashMap<String, Arc<dyn FileSystem>>,
    archive_cache: Arc<Mutex<LruCache<ResourcePath, Arc<ArchiveFileSystem>>>>,
}

impl OrlManager {
    /// 解析资源并创建适当的文件系统层次
    pub async fn resolve(&self, orl: &ORL) -> Result<Arc<dyn FileSystem>, OrlError> {
        // 1. 获取底层文件系统
        let base_fs = self.get_provider(&orl.endpoint()).await?;

        // 2. 检查是否有归档上下文
        if let Some(archive_ctx) = orl.archive_context() {
            // 创建归档文件系统
            let archive_path = orl.primary_path();

            // 检查缓存
            let cache_key = ResourcePath::from(orl.as_str());
            if let Some(cached) = self.get_cached_archive(&cache_key).await {
                return Ok(cached);
            }

            // 创建新的归档文件系统
            let archive_fs = Arc::new(ArchiveFileSystem::new(base_fs, archive_path)?);

            // 缓存
            self.cache_archive(cache_key, archive_fs.clone()).await;

            Ok(archive_fs)
        } else {
            // 直接访问底层文件系统
            Ok(base_fs)
        }
    }

    /// 支持嵌套归档
    pub async fn resolve_nested(&self, orl: &ORL) -> Result<FileSystemLayer, OrlError> {
        let base_fs = self.get_provider(&orl.endpoint()).await?;
        let mut current = FileSystemLayer::Direct(base_fs);
        let mut archive_chain = Vec::new();

        // 解析归档链
        let mut remaining_orl = orl.clone();
        while let Some(archive_ctx) = remaining_orl.archive_context() {
            let archive_path = remaining_orl.primary_path();

            archive_chain.push((archive_path.clone(), archive_ctx.clone()));

            // 创建下一层 ORL（如果归档内还有归档）
            // 这是一个高级特性，暂时可以简化
            break;
        }

        if !archive_chain.is_empty() {
            let base = match current {
                FileSystemLayer::Direct(fs) => fs,
                _ => return Err(OrlError::NestedArchiveNotSupported),
            };

            current = FileSystemLayer::Archive { base, archive_chain };
        }

        Ok(current)
    }
}
```

---

## ORL 格式调整

### 当前格式问题

```
orl://local/archive.tar?entry=inner/file.log
```

**问题**：`entry` 参数只是个字符串，没有语义

### 改进方案

```rust
/// ORL 支持归档的嵌套访问
pub struct ORL {
    /// 端点信息
    endpoint: Endpoint,

    /// 主资源路径
    primary_path: ResourcePath,

    /// 归档上下文（支持嵌套）
    archive_stack: Vec<ArchiveContext>,
}

impl ORL {
    /// 解析 ORL
    ///
    /// 支持格式：
    /// - `orl://local/path/file.log`
    /// - `orl://local/path/archive.tar?entry=inner/file.log`
    /// - `orl://local/path/nested.tar.gz?entry=inner.zip?entry=deep/file.txt`
    pub fn parse(s: impl Into<String>) -> Result<Self, OrlError> {
        let s = s.into();
        let uri = Uri::parse(&s)?;

        let endpoint = Self::parse_endpoint(&uri)?;
        let path_str = uri.path().as_str();

        // 解析归档栈
        let mut archive_stack = Vec::new();

        // 检查是否有 entry 参数
        if let Some(query) = uri.query() {
            let mut current_path = ResourcePath::from(path_str);

            // 解析嵌套的 entry 参数（高级特性）
            // entry=inner/file.txt@inner.zip@deeper/file.txt
            for entry_part in query.as_str().split('@') {
                if let Some(inner_path) = entry_part.strip_prefix("entry=") {
                    archive_stack.push(ArchiveContext {
                        inner_path: ResourcePath::from(inner_path),
                        archive_type: ArchiveType::from_extension(current_path.as_str()),
                    });
                }
            }
        }

        Ok(Self {
            endpoint,
            primary_path: ResourcePath::from(path_str),
            archive_stack,
        })
    }

    /// 获取归档上下文（最外层）
    pub fn archive_context(&self) -> Option<&ArchiveContext> {
        self.archive_stack.first()
    }

    /// 判断是否有归档上下文
    pub fn has_archive(&self) -> bool {
        !self.archive_stack.is_empty()
    }
}
```

---

## 概念关系图

```
┌─────────────────────────────────────────────────────────────────┐
│                        DFS 领域模型                              │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  ┌──────────────────────────────────────────────────────────┐  │
│  │                     Resource (资源)                      │  │
│  │                                                           │  │
│  │  ┌─────────────┐  ┌──────────────┐  ┌────────────────┐  │  │
│  │  │  Endpoint   │  │  PrimaryPath │  │ Archive Stack  │  │  │
│  │  │  (存储位置)  │  │  (主路径)    │  │  (可选容器)    │  │  │
│  │  └─────────────┘  └──────────────┘  └────────────────┘  │  │
│  │         │                                     │          │  │
│  │         ▼                                     ▼          │  │
│  │  Location                             ┌──────────────┐   │  │
│  │  Backend                             │ArchiveContext│   │  │
│  │  AccessMethod                        │- inner_path  │   │  │
│  │                                       │- type        │   │  │
│  │                                       └──────────────┘   │  │
│  └──────────────────────────────────────────────────────────┘  │
│                          │                                      │
│                          ▼                                      │
│  ┌──────────────────────────────────────────────────────────┐  │
│  │                   FileSystem (接口)                      │  │
│  │                                                           │  │
│  │  metadata()   read_dir()   open_read()                   │  │
│  └──────────────────────────────────────────────────────────┘  │
│                          │                                      │
│         ┌────────────────┼────────────────┐                    │
│         ▼                ▼                ▼                    │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐            │
│  │LocalFileSystem│  │S3Storage    │  │AgentProxyFS │            │
│  └─────────────┘  └─────────────┘  └─────────────┘            │
│         │                                        │             │
│         └────────────────┬───────────────────────┘             │
│                          ▼                                     │
│                 ┌─────────────────────┐                        │
│                 │ ArchiveFileSystem   │                        │
│                 │ (Decorator Pattern) │                        │
│                 └─────────────────────┘                        │
│                          │                                     │
│                          ▼                                     │
│                 ┌─────────────────────┐                        │
│                 │   NestedArchiveFS   │                        │
│                 │  (recursive)        │                        │
│                 └─────────────────────┘                        │
└─────────────────────────────────────────────────────────────────┘
```

---

## 总结：概念正交性矩阵

| 维度 | 选项 | 说明 |
|------|------|------|
| **Location** | Local / Remote / Cloud | 资源在哪里 |
| **Backend** | FileSystem / ObjectStorage | 如何存储 |
| **AccessMethod** | Direct / Proxy | 如何连接 |
| **Container** | Plain / Archive | 是否在容器内 |
| **ArchiveType** | Tar / Zip / Gz | 容器格式类型 |

**关键洞察**：
1. **Archive 不是 Endpoint**，而是文件的**容器属性**
2. **Archive 是虚拟文件系统**，可以嵌套在任何存储上
3. **访问 Archive** = 底层FS + Archive装饰器

---

**文档版本**: 1.0
**创建日期**: 2026-02-02
