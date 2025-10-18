# Local 文件系统支持完善总结

## ✅ 完成情况

**时间**: 2025-10-08  
**状态**: ✅ 已完成

## 📊 新增功能

### 1. 文件名模式过滤 ✅

**功能**: 支持正则表达式过滤文件名

**API**:
```rust
let source = LocalFileSystem::new(PathBuf::from("/var/log"))
    .with_pattern(r".*\.log$".to_string())?; // 只匹配 .log 文件
```

**特点**:
- 使用 `regex` crate 提供强大的正则表达式支持
- 在文件名层面过滤，性能开销小
- 支持复杂的匹配规则

**使用场景**:
- 只搜索特定类型的日志文件（如 `*.log`, `*.txt`）
- 排除特定模式的文件（如 `.*\.tmp$`, `.*\.bak$`）
- 按日期模式过滤（如 `.*-2024-.*\.log$`）

---

### 2. 大目录优化 ✅

**功能**: 限制每个目录最大扫描文件数

**API**:
```rust
let source = LocalFileSystem::new(PathBuf::from("/var/log"))
    .with_max_files_per_dir(10000); // 每目录最多10000个文件
```

**特点**:
- 防止单个大目录导致内存溢出
- 避免长时间阻塞
- 早期失败，及时反馈

**默认值**: 10000 文件/目录

**使用场景**:
- 扫描可能包含大量文件的目录
- 资源受限的环境
- 需要快速响应的搜索

---

### 3. 递归深度限制 ✅

**功能**: 限制目录树遍历的最大深度

**API**:
```rust
let source = LocalFileSystem::new(PathBuf::from("/"))
    .with_max_depth(20); // 最大深度20层
```

**特点**:
- 防止目录树过深导致栈溢出
- 控制搜索范围
- 提高性能

**默认值**: 20 层

**使用场景**:
- 从根目录开始搜索时限制范围
- 避免意外进入过深的目录结构
- 防止恶意构造的深层目录攻击

---

### 4. 符号链接循环检测 ✅

**功能**: 检测并避免符号链接循环

**实现**:
- 使用 inode 号跟踪已访问的目录
- 自动检测循环并跳过
- 只在启用 `follow_symlinks` 时生效

**特点**:
- 防止无限循环
- 基于 Unix 系统的 inode 机制（`#[cfg(unix)]`）
- 零配置，自动工作

**使用场景**:
- 遍历可能包含循环链接的目录
- 系统目录搜索（`/var`, `/usr` 等）

---

### 5. 权限检查 ✅

**功能**: 自动检测并跳过无权限读取的文件

**实现**:
- 尝试打开文件来检测权限
- 自动跳过权限被拒绝的文件
- 记录调试日志

**特点**:
- 优雅处理权限错误
- 不会因为单个文件权限问题而失败
- 提供详细的调试信息

**使用场景**:
- 非 root 用户搜索系统目录
- 多用户环境
- 包含受保护文件的目录

---

## 🔧 技术实现

### 核心数据结构

```rust
pub struct LocalFileSystem {
  root_path: PathBuf,
  recursive: bool,
  follow_symlinks: bool,
  pattern: Option<Regex>,              // 新增：文件名过滤
  max_files_per_dir: Option<usize>,    // 新增：目录文件数限制
  max_depth: Option<usize>,            // 新增：递归深度限制
}
```

### 关键优化

1. **inode 跟踪集合**
   ```rust
   let mut visited_inodes = HashSet::new();
   ```
   - 用于检测符号链接循环
   - O(1) 查找和插入性能

2. **深度追踪栈**
   ```rust
   let mut stack = vec![(root, 0)]; // (path, depth)
   ```
   - 同时跟踪路径和深度
   - 支持深度限制判断

3. **早期退出机制**
   - 达到文件数限制立即停止扫描该目录
   - 达到深度限制跳过子目录
   - 权限错误跳过文件

---

## 🧪 测试覆盖

### 新增测试用例

1. ✅ `test_pattern_filtering` - 文件名模式过滤
2. ✅ `test_max_files_per_dir` - 大目录文件数限制
3. ✅ `test_max_depth` - 递归深度限制
4. ✅ `test_symlink_loop_detection` - 符号链接循环检测

### 现有测试保持通过

- ✅ `test_list_files_empty_dir`
- ✅ `test_list_files_with_files`
- ✅ `test_list_files_recursive`
- ✅ `test_list_files_non_recursive`
- ✅ `test_open_file`
- ✅ `test_open_nonexistent_file`

---

## 📖 使用示例

### 基本用法

```rust
use logseek::storage::local::LocalFileSystem;
use std::path::PathBuf;

// 创建基本的本地文件系统源
let source = LocalFileSystem::new(PathBuf::from("/var/log"));
```

### 只搜索日志文件

```rust
let source = LocalFileSystem::new(PathBuf::from("/var/log"))
    .with_pattern(r".*\.log$".to_string())?;
```

### 限制大目录性能影响

```rust
let source = LocalFileSystem::new(PathBuf::from("/data"))
    .with_max_files_per_dir(5000)  // 每目录最多5000文件
    .with_max_depth(10);            // 最大深度10层
```

### 安全遍历符号链接

```rust
let source = LocalFileSystem::new(PathBuf::from("/"))
    .with_follow_symlinks(true)  // 启用符号链接跟随
    .with_max_depth(15);          // 限制深度防止过深
// 循环链接会自动检测并跳过
```

### 完整配置示例

```rust
let source = LocalFileSystem::new(PathBuf::from("/var/log"))
    .with_recursive(true)                        // 递归搜索
    .with_follow_symlinks(false)                 // 不跟随符号链接
    .with_pattern(r".*\.(log|txt)$".to_string())? // 只匹配 .log 和 .txt
    .with_max_files_per_dir(10000)               // 每目录最多10000文件
    .with_max_depth(20);                         // 最大深度20层
```

---

## 📊 性能对比

### 优化前

| 场景 | 文件数 | 耗时 | 内存 |
|-----|-------|-----|------|
| 大目录 | 100K+ | 长时间阻塞 | 可能OOM |
| 深层目录 | 深度50+ | 栈溢出风险 | 高 |
| 循环链接 | N/A | 无限循环 | N/A |

### 优化后

| 场景 | 文件数 | 耗时 | 内存 | 说明 |
|-----|-------|-----|------|------|
| 大目录 | 限制10K | 可控 | 稳定 | 早期退出 |
| 深层目录 | 限制20层 | 可控 | 低 | 跳过深层 |
| 循环链接 | N/A | 正常 | 正常 | 自动检测 |

---

## 🔮 后续优化建议

### 1. 性能监控

**添加指标收集**:
```rust
pub struct ScanStats {
    pub total_files: usize,
    pub skipped_dirs: usize,
    pub permission_denied: usize,
    pub pattern_filtered: usize,
    pub scan_duration: Duration,
}
```

### 2. 增量扫描

**支持文件变更监听**:
```rust
impl LocalFileSystem {
    pub fn with_change_detection(mut self, enable: bool) -> Self {
        // 使用 notify crate 监听文件变更
    }
}
```

### 3. 并行扫描

**支持多线程目录遍历**:
```rust
impl LocalFileSystem {
    pub fn with_parallelism(mut self, num_workers: usize) -> Self {
        // 并行扫描多个子目录
    }
}
```

### 4. 缓存优化

**缓存目录结构**:
```rust
// 缓存最近扫描的目录结构
// 避免重复扫描
```

---

## 📝 API 文档

### 构造器

```rust
pub fn new(root_path: PathBuf) -> Self
```
创建本地文件系统存储源。

### 配置方法

```rust
pub fn with_recursive(self, recursive: bool) -> Self
```
设置是否递归搜索子目录（默认：true）。

```rust
pub fn with_follow_symlinks(self, follow: bool) -> Self
```
设置是否跟随符号链接（默认：false）。

```rust
pub fn with_pattern(self, pattern: String) -> Result<Self, StorageError>
```
设置文件名过滤正则表达式。

```rust
pub fn with_max_files_per_dir(self, max: usize) -> Self
```
设置每目录最大文件数（默认：10000）。

```rust
pub fn with_max_depth(self, max: usize) -> Self
```
设置最大递归深度（默认：20）。

---

## 🎯 总结

Local 文件系统支持已完善！

**关键成果**:
1. ✅ 文件名模式过滤 - 支持复杂的正则表达式
2. ✅ 大目录优化 - 防止内存溢出和长时间阻塞
3. ✅ 递归深度限制 - 防止栈溢出
4. ✅ 符号链接循环检测 - 自动防止无限循环
5. ✅ 权限检查 - 优雅处理权限错误
6. ✅ 完整的测试覆盖

**收益**:
- ⭐⭐⭐⭐⭐ 安全性提升（防止循环、溢出）
- ⭐⭐⭐⭐⭐ 性能可控（限制文件数、深度）
- ⭐⭐⭐⭐⭐ 灵活性增强（模式过滤）
- ⭐⭐⭐⭐ 用户体验改善（优雅错误处理）

**生产就绪**: 这些优化使 LocalFileSystem 具备了生产环境的鲁棒性和可控性！ 🚀
