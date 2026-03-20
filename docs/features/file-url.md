# ORL（统一资源定位）

**文档版本**: v1.1  
**最后更新**: 2026-03-20

虽然文件名仍然叫 `file-url.md`，但当前实现已经以 ORL 为统一资源定位协议。

## 概述

ORL 全称为 `OpsBox Resource Locator`，用于统一表示：

- 本地文件
- Agent 远程文件
- S3 对象
- 归档内条目
- 带内置 glob 过滤的目录资源

核心解析实现位于：

- `backend/opsbox-core/src/dfs/orl_parser.rs`
- `backend/opsbox-core/src/dfs/resource.rs`
- `backend/opsbox-core/src/dfs/endpoint.rs`

## 基本格式

```text
orl://<endpoint>/<path>?<query>
```

## 端点类型

### 1. 本地文件系统

```text
orl://local/var/log/app.log
orl://local/C:/logs/app.log
```

### 2. Agent

两种常用形式：

```text
orl://web-01@agent/var/log/app.log
orl://web-01@10.0.0.8:3976@agent/var/log/app.log
```

说明：

- `web-01@agent/...` 使用默认 Agent 端口
- `web-01@10.0.0.8:3976@agent/...` 显式指定远端地址和端口

### 3. S3

当前实现兼容两种 bucket 表达方式：

```text
orl://default@s3/my-bucket/path/to/file.log
orl://prod:my-bucket@s3/path/to/file.log
```

说明：

- `default@s3/my-bucket/...`：bucket 放在 path 第一段
- `prod:my-bucket@s3/...`：bucket 放在 endpoint identity

### 4. 发现根节点

Explorer 会使用两个虚拟根：

```text
orl://agent/
orl://s3/
```

分别用于列出：

- 所有可用 Agent
- 所有可用 S3 profiles

## 查询参数

### 归档条目

```text
orl://local/data/archive.tar.gz?entry=inner/file.log
orl://web-01@agent/logs/backup.zip?entry=2024/01/app.log
```

### 内置路径过滤

```text
orl://local/var/log/?glob=*.log
```

`glob` 会被解析为资源级附加过滤条件。

## 资源模型

ORL 解析后会变成：

- `Endpoint`
- `primary_path`
- `archive_context`
- `filter_glob`

其中：

- `Endpoint` 描述位置、访问方式和后端类型
- `primary_path` 是主路径
- `archive_context` 描述归档内条目
- `filter_glob` 来自 `?glob=`

## S3 bucket 提取规则

当前实现的优先级：

1. 如果 endpoint 本身带 bucket，优先使用 endpoint.bucket
2. 否则从 path 第一段提取 bucket

这就是为什么两种 S3 ORL 写法都能工作。

## 前端中的使用方式

前端已经广泛使用 ORL：

- 搜索结果中的 `path`
- `/search` 页资源树构建
- `/explorer` 页导航
- `/view` 和 `/image-view` 页查询参数

相关文件：

- `web/src/lib/utils/orl.ts`
- `web/src/routes/search/+page.svelte`
- `web/src/routes/explorer/+page.svelte`

## 与旧命名的关系

仓库中仍能看到一些历史命名：

- `file-url.md`
- 旧注释中的 FileUrl / ODFI 字样

但当前对外协议和主要实现都应以 ORL 为准。
