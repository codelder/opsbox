# Starlark 存储源规划脚本指南 (ORL 统一模型)

本文面向需要编写 Starlark 脚本来规划搜索来源的开发者，讲解如何构造 **ORL (OpsBox Resource Locator)** 字符串，以及如何组合 Local、Agent 和 S3 资源。

---

## 脚本如何被调用

在开始编写脚本之前，了解脚本是如何被系统调用的非常重要：

### 1. 触发方式

在搜索查询中使用 `app:<应用名>` 限定词来指定使用哪个规划脚本：

```
app:nginx error          # 使用名为 "nginx" 的规划脚本
app:myapp dt:20240101    # 使用名为 "myapp" 的规划脚本
```

### 2. 默认脚本

若未指定 `app:`，系统会尝试使用已配置的默认规划脚本。若无默认脚本，搜索将报错并提示用户指定应用名。

### 3. 脚本加载顺序

脚本按以下优先级查找：
1. **数据库**（通过 UI/API 保存的脚本）
2. **用户目录** `$HOME/.opsbox/planners/<app>.star`

若都找不到，搜索将报错。

### 4. 完整工作流

1. 在 **设置 → 规划脚本管理** 页面创建脚本
2. 使用"测试"功能验证脚本输出是否符合预期
3. 可选：将脚本设为默认，这样不指定 `app:` 时也会使用它

**测试功能说明**：测试接口与真实搜索入口保持一致。`app` 由测试表单字段指定（不必写在查询中），查询中的 `encoding:/path:` 等限定词会被自动剥离，行为与线上搜索完全相同。

### 5. 限定词处理边界

以下限定词由**搜索层**处理，不会传递给规划脚本：
- `app:<应用名>` — 选择规划脚本
- `encoding:<编码>` — 指定文件编码（如 `encoding:gbk`）
- `path:<模式>` — 包含路径过滤
- `-path:<模式>` — 排除路径过滤
- `dt:/fdt:/tdt:` — 日期范围（会转换为 `DATE_RANGE` 和 `DATES` 变量）

规划脚本接收的 `CLEANED_QUERY` 是移除上述限定词后的纯查询文本。

---

## 什么是 ORL？

ORL 是 OpsBox 统一的资源定位符，通过类似 URL 的字符串精确描述一个资源的物理位置和访问方式。

### 基本格式

```
orl://{endpoint}/{path}?entry={entry_path}&glob={glob_pattern}
```

### Endpoint 类型

| 类型 | 格式 | 示例 |
|------|------|------|
| **本地文件** | `orl://local/path` | `orl://local/var/log/app.log` |
| **Agent（基本）** | `orl://{id}@agent/path` | `orl://web-01@agent/app/logs/` |
| **Agent（带地址）** | `orl://{id}@agent.{host}:{port}/path` | `orl://web-01@agent.192.168.1.100:4001/app/logs/` |
| **S3（bucket 在 endpoint）** | `orl://{profile}:{bucket}@s3/path` | `orl://oss:my-bucket@s3/logs/2024/` |
| **S3（bucket 在路径）** | `orl://{profile}@s3/{bucket}/path` | `orl://oss@s3/my-bucket/logs/2024/` |

### 查询参数

- **entry** (可选): 归档文件内部的路径。支持的归档格式：`tar`、`tar.gz`、`tgz`、`gz`、`zip`
- **glob** (可选): 过滤文件名的通配符模式，如 `**/*.log`

---

## 快速上手：最小可行脚本

脚本的核心任务是根据当前的查询环境，导出一个名为 `SOURCES` 的**字符串列表**（ORL 列表）。

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
# 注意：f-string 内只能用简单标识符，不能直接用 a.get('id') 或 a['id']
SOURCES = []
for a in AGENTS:
    if a['tags'].get('env') == 'prod':
        agent_id = a.get('id')  # 先提取变量
        SOURCES.append(f"orl://{agent_id}@agent/app/logs/")
```

### 3. S3 归档文件查阅
```python
# 直接读取 S3 上的某个归档包内的特定文件
# 注意：bucket 名称需要您自己知道，S3_PROFILES 不包含 bucket 信息
SOURCES = [
    "orl://my-profile:my-bucket@s3/archive/2025/01/data.tar.gz?entry=server_error.log"
]
```

---

## 运行时注入变量 (只读)

规划脚本运行前，后端会自动注入以下变量供查询使用：

- **CLEANED_QUERY**: `str`。移除了 `dt:`、`app:`、`encoding:`、`path:` 等特殊指令后的纯查询文本。
- **TODAY**: `str`。当前日期（北京时区），格式 `YYYY-MM-DD`。
- **DATE_RANGE**: `dict`。当前查询的时间范围 `{'start': 'YYYY-MM-DD', 'end': 'YYYY-MM-DD'}`。
- **DATES**: `list[dict]`。日期范围内的逐日对象列表。
  - 属性：`iso` (YYYY-MM-DD), `yyyymmdd` (YYYYMMDD), `next_yyyymmdd` (次日的 YYYYMMDD)。
- **AGENTS**: `list[dict]`。当前在线的 Agent 列表（心跳超时 90 秒内）。
  - 属性：`id` (Agent 标识), `tags` (标签字典)。
- **S3_PROFILES**: `list[dict]`。已配置的 S3 访问配置列表（仅非敏感字段）。
  - 属性：`profile_name` (配置名称), `endpoint` (服务端点)。
  - **注意**：不包含 `bucket` 字段，bucket 需要在 ORL 中手动指定。

---

## 常用脚本模式

### 模式 A：按日期轮转的本地日志
当日志按 `YYYYMMDD` 分目录存储时：
```python
SOURCES = []
for d in DATES:
    ymd = d['yyyymmdd']
    SOURCES.append(f"orl://local/data/logs/{ymd}/?glob=*.log")
```

### 模式 B：分片存储的 S3 历史数据
假设数据按小时打包存储在 S3，**bucket 名称需要手动指定**：
```python
SOURCES = []

# 检查是否有 S3 配置可用
if S3_PROFILES:
    first_profile = S3_PROFILES[0]
    profile_name = first_profile['profile_name']
    bucket = "my-bucket"  # 替换为实际的 bucket 名称

    for d in DATES:
        ymd = d['yyyymmdd']
        for hour in range(24):
            key = f"MYAPP/ARCHIVE/{ymd}/log_{hour:02d}.tar.gz"
            SOURCES.append(f"orl://{profile_name}:{bucket}@s3/{key}")
```

### 模式 C：分层搜索 (本地实时 + 远程备份 + 节点调试)
场景：优先搜索本地当前日志，如果时间跨度包含历史，则自动加入 S3 归档；同时根据标签抓取特定 Agent 的日志。
```python
SOURCES = [
    "orl://local/var/log/app/current.log"  # 本地实时日志
]

# 1. 自动追加历史日期对应的 S3 归档
# 注意：bucket 名称需要替换为实际值
for d in DATES:
    iso = d['iso']
    ymd = d['yyyymmdd']
    if iso < TODAY:
        SOURCES.append(f"orl://oss:my-bucket@s3/history/{ymd}.tar.gz")

# 2. 如果包含特定标签的 Agent 在线，也拉取其临时日志
for a in AGENTS:
    if a['tags'].get('role') == 'worker':
        agent_id = a.get('id')  # 提取变量用于 f-string
        SOURCES.append(f"orl://{agent_id}@agent/var/log/worker.log")
```

---

## 开发建议

1. **善用 f-string**: Starlark 运行时已启用 `f-string` 支持，拼接 ORL 更加直观。
2. **测试脚本**: 在管理界面中使用"测试"功能，可以即时看到脚本生成的 `SOURCES` (ORL 列表) 和调试日志。
3. **性能注意**: 避免在单次查询中生成数千个 ORL，这会显著增加后端的压力。尽量利用 `glob` 模式合并同目录下的多个文件。
4. **调试**: 可以使用 `print()` 函数输出日志，日志会直接显示在前端的测试反馈区域。

---

## Starlark 语法限制

Starlark 是 Python 的子集，本项目使用 `starlark-rust` 实现（v0.13），已启用 f-string 扩展。

### f-string 中的表达式限制

**关键限制**：f-string 内部**只支持简单标识符**，不支持任何表达式（包括 `.get()` 方法调用和 `[]` 索引）。

```python
# ❌ 错误：f-string 内不支持方法调用
f"orl://{a.get('id')}@agent/logs/"

# ❌ 错误：f-string 内不支持索引表达式
f"orl://{a['id']}@agent/logs/"

# ✅ 正确：先提取变量，再用 f-string
agent_id = a.get('id')  # 或 a['id']
f"orl://{agent_id}@agent/logs/"

# ✅ 也可以用字符串拼接（普通表达式中 a['id'] 和 a.get('id') 都支持）
"orl://" + a['id'] + "@agent/logs/"
"orl://" + a.get('id') + "@agent/logs/"
```

### 普通表达式（非 f-string）

在普通表达式中，字典方括号访问和方法调用都是**完全支持**的：

```python
# ✅ 赋值语句
name = a['id']
name = a.get('id')

# ✅ 条件判断
if a['tags'].get('env') == 'prod':
    ...

# ✅ 字符串拼接
"orl://" + a['id'] + "@agent/logs/"
"orl://" + a.get('id') + "@agent/logs/"
```

### 推荐风格

```python
# 条件判断：使用简洁的方括号访问
if a['tags'].get('env') == 'prod':
    ...

# f-string：必须先提取变量
agent_id = a.get('id')  # 或 a['id']
f"orl://{agent_id}@agent/logs/"

# 或使用字符串拼接（无需提前提取）
"orl://" + a.get('id') + "@agent/logs/"
```

---

## 常见问题

- **Q: 为什么生成的 SOURCES 不生效？**
  - 请确保 `SOURCES` 是一个**字符串列表**（ORL 字符串），不是字典或其他类型。

- **Q: 路径包含中文字符怎么办？**
  - Starlark 支持 Unicode，但为了保险起见，复杂的路径建议进行 URL 编码。

- **Q: 什么时候用 glob vs 什么时候增加多个 ORL？**
  - 如果文件处于同一父目录下，建议使用一个带 `glob` 的 ORL。
  - 如果是物理分散的（如不同 Bucket 或不同服务器），则必须使用多个 ORL。

- **Q: S3_PROFILES 为什么没有 bucket 字段？**
  - S3 配置是访问凭证，bucket 是运行时指定的。一个 S3 配置可以访问多个 bucket，因此 bucket 需要在 ORL 中手动指定。

- **Q: 如何指定 Agent 的连接地址？**
  - 如果 Agent 已注册到系统，使用 `orl://{id}@agent/path` 即可，系统会自动解析地址。
  - 如果需要显式指定地址，使用 `orl://{id}@agent.{host}:{port}/path` 格式。

- **Q: 脚本执行出错会怎样？**
  - 脚本语法错误或运行时错误会导致搜索失败，错误信息会返回给用户。建议使用"测试"功能先验证脚本。
