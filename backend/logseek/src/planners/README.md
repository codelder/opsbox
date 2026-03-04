# Starlark 存储源规划脚本指南 (ORL 统一模型)

本文面向需要编写 Starlark 脚本来规划搜索来源的开发者，讲解如何构造 **ORL (OpsBox Resource Locator)** 字符串，以及如何组合 Local、Agent 和 S3 资源。

## 什么是 ORL？

ORL 是 OpsBox 统一的资源定位符，通过类似 URL 的字符串精确描述一个资源的物理位置和访问方式。其标准格式如下：

`orl://{endpoint_id}@{endpoint_type}/{path}?entry={entry_path}&glob={glob_pattern}`

- **endpoint_type**: `local` (本地)、`agent` (远程代理)、`s3` (对象存储)。
- **endpoint_id**:
  - `local`: 留空 (例如 `orl://local/`)。
  - `agent`: Agent ID (例如 `orl://web-01@agent/`)。
  - `s3`: 配置名称:存储桶 (例如 `orl://oss:logs-bucket@s3/`)。
- **path**: 资源所在的绝对路径或相对根路径。
- **entry** (可选): 归档文件 (tar/tar.gz/gz) 内部的路径。
- **glob** (可选): 进一步过滤文件名的通配符。

---

## 快速上手：最小可行脚本

脚本的核心任务是根据当前的查询环境，导出一个名为 `SOURCES` 的字符串列表。

### 1. 本地目录搜索
```python
# 递归搜索本地 /var/log/myapp 下的所有 .log 文件
SOURCES = [
    "orl://local/var/log/myapp/?glob=**/*.log"
]
```

### 2. 多 Agent 混合搜索
```python
# 在所有在线生产环境 Agent 的固定目录下搜索
SOURCES = [
    f"orl://{a['id']}@agent/app/logs/"
    for a in AGENTS if a['tags'].get('env') == 'prod'
]
```

### 3. S3 归档文件查阅
```python
# 直接读取 S3 上的某个归档包内的特定文件
SOURCES = [
    f"orl://oss:prod-bucket@s3/archive/2025/01/data.tar.gz?entry=server_error.log"
]
```

---

## 运行时注入变量 (只读)

规划脚本运行前，后端会自动注入以下变量供查询使用：

- **CLEANED_QUERY**: `str`。移除了 `dt:`、`app:` 等特殊指令后的纯查询文本。
- **TODAY**: `str`。当前日期，格式 `YYYY-MM-DD`。
- **DATE_RANGE**: `dict`。当前查询的时间范围 `{'start': '...', 'end': '...'}`。
- **DATES**: `list[dict]`。日期范围内的逐日对象列表。
  - 属性：`iso` (YYYY-MM-DD), `yyyymmdd` (YYYYMMDD), `next_yyyymmdd`。
- **AGENTS**: `list[dict]`。当前在线的 Agent 列表。
  - 属性：`id`, `tags` (字典格式)。
- **S3_PROFILES**: `list[dict]`。已配置的 S3 访问偏好名称。
  - 属性：`profile_name`, `endpoint`。

---

## 常用脚本模式

### 模式 A：按日期轮转的本地日志
当日志按 `YYYYMMDD` 分目录存储时：
```python
SOURCES = []
for d in DATES:
    SOURCES.append(f"orl://local/data/logs/{d['yyyymmdd']}/?glob=*.log")
```

### 模式 B：分片存储的 S3 历史数据
假设数据按小时打包存储在 S3：
```python
SOURCES = []
# 从变量中找到名为 'oss' 的配置
oss = next((p for p in S3_PROFILES if p['profile_name'] == 'oss'), None)

if oss:
    for d in DATES:
        for hour in range(24):
            key = f"BBIP/ARCHIVE/{d['yyyymmdd']}/log_{hour:02d}.tar.gz"
            SOURCES.append(f"orl://{oss['profile_name']}:{oss['bucket']}@s3/{key}")
```

### 模式 C：分层搜索 (本地实时 + 远程备份 + 节点调试)
场景：优先搜索本地当前日志，如果时间跨度包含历史，则自动加入 S3 归档；同时根据标签抓取特定 Agent 的日志。
```python
SOURCES = [
    "orl://local/var/log/app/current.log" # 本地实时日志
]

# 1. 自动追加历史日期对应的 S3 归档
for d in DATES:
    if d['iso'] < TODAY:
        SOURCES.append(f"orl://oss:backup-bucket@s3/history/{d['yyyymmdd']}.tar.gz")

# 2. 如果包含特定标签的 Agent 在线，也拉取其临时日志
for a in AGENTS:
    if a['tags'].get('role') == 'worker':
        SOURCES.append(f"orl://{a['id']}@agent/var/log/worker.log")
```

---

## 开发建议

1. **善用 f-string**: Starlark 运行时已启用 `f-string` 支持，拼接 ORL 更加直观。
2. **测试脚本**: 在管理界面中使用“测试”功能，可以即时看到脚本生成的 `SOURCES` (ORL 列表) 是否符合预期。
3. **性能注意**: 避免在单次查询中生成数千个 ORL，这会显著增加后端的压力。尽量利用 `glob` 模式。
4. **调试**: 可以使用 Python 的 `print()` 函数输出日志，日志会直接显示在前端的测试反馈区域。

## 常见问题

- **Q: 为什么生成的 SOURCES 不生效？**
  - 请确保 `SOURCES` 是一个字符串列表。如果列表包含字典或其他类型，后端将报错。
- **Q: 路径包含中文字符怎么办？**
  - Starlark 支持 Unicode，但为了保险起见，复杂的路径建议进行 URL 编码。
- **Q: 什么时候用 glob vs 什么时候增加多个 ORL？**
  - 如果文件处于同一父目录下，建议使用一个带 `glob` 的 ORL。如果是物理分散的（如不同 Bucket 或不同服务器），则必须使用多个 ORL。
