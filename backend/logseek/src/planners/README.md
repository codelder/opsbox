# Starlark 存储源规划脚本指南（Local/Agent/S3，统一 Source 模型）

本文面向需要用 Starlark 编写"存储源规划脚本"的开发者，讲解如何构造统一 Source（Endpoint+Target）、如何组合 Local/Agent/S3/tar.gz 等来源，并给出可直接复制的脚本片段与最佳实践。

## 快速上手：最小可行脚本（Local/Agent/S3）

- 目标：扫描服务器本地某个目录，递归搜索所有文件。

```python
# 最小化示例：本地目录递归搜索
SOURCES = [
    {
        'endpoint': { 'kind': 'local', 'root': '/var/log/myapp' },   # 绝对路径
        'target':   { 'type': 'dir', 'path': '.', 'recursive': True } # '.' 表示 root 自身
    }
]
```

- 目标：按 Agent 标签选择在线 Agent，在其 subpath 下递归搜索。

```python
# 最小化示例：按标签选择 Agent，递归目录
SOURCES = []
for a in AGENTS:
    if a.get('tags', {}).get('env') == 'prod':
        SOURCES.append({
            'endpoint': { 'kind': 'agent', 'agent_id': a['id'], 'subpath': 'logs' },
            'target':   { 'type': 'dir', 'path': '.', 'recursive': True },
        })
```

- 目标：从 S3 读取单个 tar.gz 并在归档内匹配。

```python
# 最小化示例：S3 单个 tar.gz 对象
SOURCES = [{
    'endpoint': { 'kind': 's3', 'profile': 'oss', 'bucket': 'logs-bucket' },
    'target':   { 'type': 'archive', 'path': 'archive/app_2025-01-15.tar.gz' },  # path 为对象 Key（相对 bucket）
    'filter_glob': '**/*.log',  # 可选
}]
```

## 核心数据模型（统一 Source 模型）

- Endpoint（在哪里）
  - 本地：`{ kind: 'local', root: '/abs/path' }`（`root` 为服务器上的绝对路径）
  - Agent：`{ kind: 'agent', agent_id: '<id>', subpath: 'subdir' }`（`subpath` 为相对该 Agent `search_roots` 的子路径；`subpath='.'` 表示不限制，使用所有 `search_roots`）
  - S3：`{ kind: 's3', profile: 'name', bucket: 'bucket' }`（通过 `profile` 选择已配置的对象存储，`bucket` 指定桶）
- Target（查什么；所有 path 相对 endpoint.root（Local）或 endpoint.subpath（Agent））
  - 目录：`{ type: 'dir', path: '.', recursive: True }`
  - 文件清单：`{ type: 'files', paths: ['a.log','b.log'] }`
  - 归档（tar/tar.gz/gz）：`{ type: 'archive', path: 'backup_2025-01-15.tar.gz' }`（S3 场景下 `path` 为对象 Key，相对 `bucket`，不要写 `s3://...`；zip 暂不支持）
- 其它字段（可选）
  - `filter_glob?: string` 额外路径过滤（与查询中的 path: 规则做 AND）
    - **适用于所有源类型**：Local、Agent、S3 都支持
    - **AND 语义**：路径必须同时满足 `filter_glob` 和用户查询的 `path:` 限定词
    - 示例：`filter_glob: "**/*.log"` + 用户查询 `path:error` → 只匹配包含 "error" 的 `.log` 文件
  - `display_name?: string` UI 友好展示名
    - 用于在搜索结果中显示更友好的来源名称
    - 示例：`display_name: "生产环境日志"`

提示：
- Local 的 `root` 必须是服务器上的绝对路径。
- Agent 的 `subpath` 与 `search_roots` 的关系：
  - Agent 启动时可配置多个 `search_roots`（逗号分隔的绝对路径列表，如 `"/var/log,/opt/logs"`）
  - 脚本中的 `subpath` 是相对路径，相对于 Agent 的每个 `search_roots`
  - Agent 会在每个 `search_roots` 下查找 `subpath` 指定的子路径
  - 例如：`search_roots = ["/var/log", "/opt/logs"]`，`subpath = "logs"` → Agent 会在 `/var/log/logs` 和 `/opt/logs/logs` 下查找
  - `subpath = "."` 表示不限制，使用所有 `search_roots` 下的内容
  - 所有路径必须在某个 `search_roots` 下（白名单校验，保证安全性）
- S3：`path` 填对象 Key（相对 `bucket`），当前仅支持 `target='archive'`（tar/tar.gz/gz；zip 暂不支持）。

## 运行时注入变量（只读）

- **CLEANED_QUERY**: string（移除了 `app:`/`dt:`/`fdt:`/`tdt:` 指令）
  - 已清理的查询字符串，可直接用于搜索
  - 脚本可以覆盖此变量以追加额外的限定词

- **DATE_RANGE**: { start: "YYYY-MM-DD", end: "YYYY-MM-DD" }
  - 根据查询中的日期指令解析出的日期范围

- **TODAY**: "YYYY-MM-DD"（北京时间）
  - 当前日期，用于判断是否为当日数据

- **DATES**: list[dict]，按日期范围展开的每日对象：
  - 每项包含：`{ iso: "YYYY-MM-DD", yyyymmdd: "YYYYMMDD", next_yyyymmdd: "YYYYMMDD" }`
  - `iso`: ISO 格式日期（用于字符串比较和显示）
  - `yyyymmdd`: 当前日期的 8 位数字格式
  - `next_yyyymmdd`: 下一天的 8 位数字格式（用于生成文件名等）

- **AGENTS**: list[dict]，在线 Agent 及标签：
  - 每项包含：`{ id: string, tags: { key: value } }`
  - 仅包含当前在线的 Agent
  - 可通过标签筛选，例如：`agent.get('tags', {}).get('env') == 'prod'`

- **S3_PROFILES**: list[dict]（非敏感字段）：
  - 每项包含：`{ profile_name: str, endpoint: str, bucket: str }`
  - **注意**：密钥等敏感信息不会暴露，后端会用数据库中的配置访问
  - 用途：在脚本中选择要使用的 `profile_name` 与对应 `bucket`
  - 选择示例：
    ```python
    oss = None
    for p in S3_PROFILES:
        if p['profile_name'] == 'oss':
            oss = p
            break
    # 之后可用 oss['profile_name'] 和 oss['bucket']
    ```

## 查询指令说明

### app: 指令

- **格式**：`app:<app_name>`
- **作用**：选择要使用的规划脚本
- **示例**：`app:bbip error` → 使用 bbip 脚本，查询为 "error"
- **默认值**：未指定时使用 `app:bbip`
- **注意**：该指令会被移除，不会传递给规划脚本

### 日期指令（dt:/fdt:/tdt:）

- **格式**：`dt:YYYYMMDD`、`fdt:YYYYMMDD`、`tdt:YYYYMMDD`
  - 必须是 8 位数字（YYYYMMDD）
  - 例如：`dt:20250115` 表示 2025-01-15

- **指令说明**：
  - `dt:YYYYMMDD`：指定单个日期（起始和结束都是该日期）
  - `fdt:YYYYMMDD`：起始日期（from date）
  - `tdt:YYYYMMDD`：结束日期（to date）

- **组合逻辑**：
  - 如果指定了 `dt:`，则忽略 `fdt:` 和 `tdt:`
  - 如果只指定了 `fdt:`，则结束日期等于起始日期
  - 如果只指定了 `tdt:`，则起始日期等于结束日期
  - 如果都未指定，则默认为今天

- **示例**：
  - `dt:20250115 error` → 查询 2025-01-15 的 "error"
  - `fdt:20250110 tdt:20250115 error` → 查询 2025-01-10 到 2025-01-15 的 "error"
  - `fdt:20250110 error` → 查询 2025-01-10 的 "error"（单日）

- **注意**：这些指令会被移除，不会传递给规划脚本，但会影响 `DATE_RANGE` 和 `DATES` 变量的值

脚本需导出：
- 必须：`SOURCES: list[Source]`
- 可选：`CLEANED_QUERY: string`（覆盖注入值，例如追加路径限定）

## 常用模式（Local/Agent/S3）

1) 本地目录递归搜索（可选 filter_glob）

```python
SOURCES = [{
    'endpoint': { 'kind': 'local', 'root': '/var/log/myapp' },
    'target':   { 'type': 'dir', 'path': '.', 'recursive': True },
    'filter_glob': '**/*.log',  # 可选：仅 .log
}]
```

2) 本地归档内搜索（tar/tar.gz/gz）

```python
SOURCES = [{
    'endpoint': { 'kind': 'local', 'root': '/archive' },
    'target':   { 'type': 'archive', 'path': 'logs_2025-01-15.tar.gz' },
    'filter_glob': '**/*.log',
}]
```

3) 多个本地目录合并

```python
SOURCES = []
for root in ['/var/log/app', '/var/log/system', '/opt/logs/service']:
    SOURCES.append({
        'endpoint': { 'kind': 'local', 'root': root },
        'target':   { 'type': 'dir', 'path': '.', 'recursive': True },
        'filter_glob': '**/*.log',
    })
```

4) S3：单个归档对象（tar/tar.gz/gz）

```python
SOURCES = [{
    'endpoint': { 'kind': 's3', 'profile': 'oss', 'bucket': 'logs-bucket' },
    'target':   { 'type': 'archive', 'path': 'archive/app_2025-01-15.tar.gz' },  # path 是对象 Key，相对 bucket
    'filter_glob': '**/*.log',
}]
```

5) S3：按日期/分桶批量生成对象 Key

```python
SOURCES = []
BUCKETS = ['20','21','22','23']

# 选择名为 oss 的 profile
oss = None
for p in S3_PROFILES:
    if p['profile_name'] == 'oss':
        oss = p
        break

if oss != None:
    for d in DATES:
        if d['iso'] < TODAY:  # 仅历史
            y = d['next_yyyymmdd'][0:4]
            yyyymm = d['next_yyyymmdd'][0:6]
            yyyymmdd = d['next_yyyymmdd']
            file = d['iso']
            for b in BUCKETS:
                key = 'bbip/{}/{}/{}/BBIP_{}_APPLOG_{}.tar.gz'.format(y, yyyymm, yyyymmdd, b, file)
                SOURCES.append({
                    'endpoint': { 'kind': 's3', 'profile': oss['profile_name'], 'bucket': oss['bucket'] },
                    'target':   { 'type': 'archive', 'path': key },
                    'filter_glob': '**/*.log',
                })
```

## Agent 场景（tar.gz 与目录/文件清单）

1) 按标签筛选 Agent，生成按日期/小时的 tar.gz 目标

```python
SOURCES = []

APPS = { 'app_a': 'applog_a', 'app_b': 'applog_b', 'app_c': 'applog_c' }

for agent in AGENTS:
    if agent.get('tags', {}).get('env') == 'prod':
        for d in DATES:
            date_iso = d['iso']
            for app_name, tar_prefix in APPS.items():
                for hour in range(24):
                    tar_name = '{}_{}_{:02d}.tar.gz'.format(tar_prefix, date_iso, hour)
                    SOURCES.append({
                        'endpoint': { 'kind': 'agent', 'agent_id': agent['id'], 'subpath': 'logs' },
                        'target':   { 'type': 'archive', 'path': tar_name },
                        'filter_glob': '**/*.log',
                    })
```

2) 其他目标类型（目录 / 文件清单 / 全量）

```python
SOURCES = []

# 目录
SOURCES.append({
    'endpoint': { 'kind': 'agent', 'agent_id': 'server-01', 'subpath': 'logs' },
    'target':   { 'type': 'dir', 'path': 'web', 'recursive': True },
    'filter_glob': '**/*error*.log',
})

# 文件清单
SOURCES.append({
    'endpoint': { 'kind': 'agent', 'agent_id': 'server-01', 'subpath': '.' },
    'target':   { 'type': 'files', 'paths': ['logs/access.log', 'logs/error.log'] },
})
```

## 组合示例：混合 Local/S3 + Agent

```python
SOURCES = []

# 方案 A：本地作为历史归档
for d in DATES:
    if d['iso'] < TODAY:
        SOURCES.append({
            'endpoint': { 'kind': 'local', 'root': '/archive' },
            'target':   { 'type': 'archive', 'path': 'logs_{}.tar.gz'.format(d['iso']) },
            'filter_glob': '**/*.log',
        })

# 方案 B：S3 作为历史归档（选择 oss profile）
oss = None
for p in S3_PROFILES:
    if p['profile_name'] == 'oss':
        oss = p
        break
if oss != None:
    for d in DATES:
        if d['iso'] < TODAY:
            key = 'archive/logs_{}.tar.gz'.format(d['iso'])
            SOURCES.append({
                'endpoint': { 'kind': 's3', 'profile': oss['profile_name'], 'bucket': oss['bucket'] },
                'target':   { 'type': 'archive', 'path': key },
                'filter_glob': '**/*.log',
            })

# Agent：当日实时
for a in AGENTS:
    if a.get('tags', {}).get('type') == 'prod':
        SOURCES.append({
            'endpoint': { 'kind': 'agent', 'agent_id': a['id'], 'subpath': 'logs' },
            'target':   { 'type': 'dir', 'path': '.', 'recursive': True },
            'filter_glob': '**/{}/**'.format(TODAY),  # 仅当日
        })
```

## 覆盖 CLEANED_QUERY（可选）

你可以基于系统注入的 `CLEANED_QUERY` 做追加。例如限制文件名或路径：

```python
# 将查询限定到特定路径/扩展名（与 UI path: 规则做 AND）
CLEANED_QUERY = CLEANED_QUERY + ' path:**/*.log'
```

## 脚本放置与选择

### 脚本加载优先级（从高到低）

1. **数据库存储**：通过 API 保存到数据库的脚本（优先级最高）
2. **用户目录**：`~/.opsbox/planners/<app>.star`
3. **内置脚本**：`backend/logseek/src/planners/<app>.star`（作为回退）

### 选择脚本

- 在搜索框查询中加上 `app:<app>` 来选择脚本；未指定时默认 `app:bbip`
- 例如：`app:bbip error` 会使用 `bbip` 脚本
- `app:` 限定词会被自动移除，不会传递给规划脚本

## 常见错误与排查

- 未导出 `SOURCES`：后端会提示“未导出 SOURCES”。
- 字段拼写错误或大小写不一致：
  - Endpoint 用 `kind`，取值 `'local'|'agent'|'s3'`
  - Target 用 `type`，取值 `'dir'|'files'|'archive'`
- 路径语义：
  - 所有 Target 路径（含 files/archive）均相对 `endpoint.root`（Local）或 `endpoint.subpath`（Agent）。
  - Agent 的 `subpath='.'` 表示不限制，使用所有 `search_roots` 下的内容。
  - S3 的 `target.archive.path` 必须是对象 Key，相对 `bucket`，不要写成 `s3://bucket/key`。
- Profile/存储配置：
  - `profile` 名称未配置或拼写错误会导致后端取对象失败（查看问题详情/服务端日志）。
- 规模控制：
  - 生成对象数量过多（长日期区间 × 多桶）会显著增加 IO；先缩小范围或分批执行。
- 能力边界：
  - 目前 S3 仅支持 `target='archive'`（tar/tar.gz/gz；zip 暂不支持）；如需前缀列举并进行“目录式”搜索，请反馈需求。
- 诊断日志：
  - 规划阶段后端会记录每个 Source 的 RAW JSON 与解析结果，便于定位字段问题。

## 最佳实践

- 面向“来源（Endpoint）+ 目标（Target）”思维构造 SOURCES；相对路径统一相对 `root`。
- 用 `filter_glob` 精确约束（与查询中的 `path:` 一起形成更小的交集）。
- 按日期/标签/应用维度拆解，尽量避免一次性生成过多对象键。
- 覆盖 `CLEANED_QUERY` 时仅做增量追加，保持原查询语义清晰。
- 在本地/Agent 混合场景：通常“当日走 Agent，历史走归档（本地/S3）”可获得更佳吞吐。
- S3 建议：
  - 控制单次生成的对象数量（尽量按日/小时打包成较大粒度归档）。
  - 显式指定 `profile` 与 `bucket`，避免环境切换时混淆。
  - `target.archive.path` 仅写对象 Key，配合 `filter_glob` 在归档内进一步约束匹配。

## 脚本骨架模板

```python
# 1) 读取上下文（TODAY / DATES / AGENTS / S3_PROFILES 等）
SOURCES = []

# 2) 业务条件 → 生成 Local / Agent / S3 来源
# 示例：本地目录
SOURCES.append({
    'endpoint': { 'kind': 'local', 'root': '/var/log/app' },
    'target':   { 'type': 'dir', 'path': '.', 'recursive': True },
    'filter_glob': '**/*.log',
})

# 示例：按标签选择 Agent
for a in AGENTS:
    if a.get('tags', {}).get('env') == 'prod':
        SOURCES.append({
            'endpoint': { 'kind': 'agent', 'agent_id': a['id'], 'subpath': 'logs' },
            'target':   { 'type': 'dir', 'path': '.', 'recursive': True },
        })

# 示例：S3 单个 tar.gz（从 S3_PROFILES 选择 profile/bucket）
oss = None
for p in S3_PROFILES:
    if p['profile_name'] == 'oss':
        oss = p
        break
if oss != None:
    SOURCES.append({
        'endpoint': { 'kind': 's3', 'profile': oss['profile_name'], 'bucket': oss['bucket'] },
        'target':   { 'type': 'archive', 'path': 'archive/app_2025-01-15.tar.gz' },
        'filter_glob': '**/*.log',
        'display_name': 'OSS 归档日志',  # 可选：UI 友好名称
    })

# 3) 可选：覆盖 CLEANED_QUERY（例如增加 path 约束）
# CLEANED_QUERY = CLEANED_QUERY + ' path:**/*.log'
```
