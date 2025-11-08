# Starlark 存储源规划脚本指南（Local/Agent/S3，统一 Source 模型）

本文面向需要用 Starlark 编写“存储源规划脚本”的开发者，结合内置示例 `local_search_example.star` 与 `multi_tar_agent_example.star`，并补充 S3 场景，讲解如何构造统一 Source（Endpoint+Target）、如何组合 Local/Agent/S3/tar.gz 等来源，并给出可直接复制的脚本片段与最佳实践。

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

- 目标：按 Agent 标签选择在线 Agent，在其 root 下递归搜索。

```python
# 最小化示例：按标签选择 Agent，递归目录
SOURCES = []
for a in AGENTS:
    if a.get('tags', {}).get('env') == 'prod':
        SOURCES.append({
            'endpoint': { 'kind': 'agent', 'agent_id': a['id'], 'root': 'logs' },
            'target':   { 'type': 'dir', 'path': '.', 'recursive': True },
        })
```

- 目标：从 S3 读取单个 tar.gz 并在归档内匹配。

```python
# 最小化示例：S3 单个 tar.gz 对象
SOURCES = [{
    'endpoint': { 'kind': 's3', 'profile': 'oss', 'bucket': 'logs-bucket' },
    'target':   { 'type': 'targz', 'path': 'archive/app_2025-01-15.tar.gz' },  # path 为对象 Key（相对 bucket）
    'filter_glob': '**/*.log',  # 可选
}]
```

## 核心数据模型（统一 Source 模型）

- Endpoint（在哪里）
  - 本地：`{ kind: 'local', root: '/abs/path' }`
  - Agent：`{ kind: 'agent', agent_id: '<id>', root: 'subdir' }`（`root='.'` 表示不限制）
  - S3：`{ kind: 's3', profile: 'name', bucket: 'bucket' }`（通过 `profile` 选择已配置的对象存储，`bucket` 指定桶）
- Target（查什么；所有 path 相对 endpoint.root）
  - 目录：`{ type: 'dir', path: '.', recursive: True }`
  - 文件清单：`{ type: 'files', paths: ['a.log','b.log'] }`
  - 单个 tar.gz：`{ type: 'targz', path: 'backup_2025-01-15.tar.gz' }`（S3 场景下 `path` 为对象 Key，相对 `bucket`，不要写成 `s3://...`）
  - 全量：`{ type: 'all' }`
- 其它字段（可选）
  - `filter_glob?: string` 额外路径过滤（与查询中的 path: 规则做 AND）
  - `display_name?: string` UI 友好展示名

提示：
- Local 的 `root` 必须是服务器上的绝对路径。
- Agent 的所有相对路径都以 `root` 为基准拼接。
- S3：`path` 填对象 Key（相对 `bucket`），当前仅支持 `target='targz'` 组合（目录/文件清单形态可按后续能力扩展）。

## 运行时注入变量（只读）

- CLEANED_QUERY: string（移除了 `app:`/`dt:`/`fdt:`/`tdt:` 指令）
- DATE_RANGE: { start: "YYYY-MM-DD", end: "YYYY-MM-DD" }
- TODAY: "YYYY-MM-DD"（北京时间）
- DATES: list[dict]，按日期范围展开的每日对象：
  - { iso, yyyymmdd, next_yyyymmdd }
- AGENTS: list[dict]，在线 Agent 及标签：
  - { id: string, tags: { key: value } }
- S3_PROFILES: list[dict]（非敏感字段）：
  - { profile_name, endpoint, bucket }
  - 用途：在脚本中选择要使用的 `profile_name` 与对应 `bucket`，密钥不会暴露，后端会用数据库中的配置访问。
  - 选择示例：
    ```python
    oss = None
    for p in S3_PROFILES:
        if p['profile_name'] == 'oss':
            oss = p
            break
    # 之后可用 oss['profile_name'] 和 oss['bucket']
    ```

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

2) 本地 tar.gz 归档内搜索

```python
SOURCES = [{
    'endpoint': { 'kind': 'local', 'root': '/archive' },
    'target':   { 'type': 'targz', 'path': 'logs_2025-01-15.tar.gz' },
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

4) S3：单个 tar.gz 对象

```python
SOURCES = [{
    'endpoint': { 'kind': 's3', 'profile': 'oss', 'bucket': 'logs-bucket' },
    'target':   { 'type': 'targz', 'path': 'archive/app_2025-01-15.tar.gz' },  # path 是对象 Key，相对 bucket
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
                    'target':   { 'type': 'targz', 'path': key },
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
                        'endpoint': { 'kind': 'agent', 'agent_id': agent['id'], 'root': 'logs' },
                        'target':   { 'type': 'targz', 'path': tar_name },
                        'filter_glob': '**/*.log',
                    })
```

2) 其他目标类型（目录 / 文件清单 / 全量）

```python
SOURCES = []

# 目录
SOURCES.append({
    'endpoint': { 'kind': 'agent', 'agent_id': 'server-01', 'root': 'logs' },
    'target':   { 'type': 'dir', 'path': 'web', 'recursive': True },
    'filter_glob': '**/*error*.log',
})

# 文件清单
SOURCES.append({
    'endpoint': { 'kind': 'agent', 'agent_id': 'server-01', 'root': '.' },
    'target':   { 'type': 'files', 'paths': ['logs/access.log', 'logs/error.log'] },
})

# 全量（谨慎使用）
SOURCES.append({
    'endpoint': { 'kind': 'agent', 'agent_id': 'server-01', 'root': '.' },
    'target':   { 'type': 'all' },
    'filter_glob': '**/*.log',
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
            'target':   { 'type': 'targz', 'path': 'logs_{}.tar.gz'.format(d['iso']) },
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
                'target':   { 'type': 'targz', 'path': key },
                'filter_glob': '**/*.log',
            })

# Agent：当日实时
for a in AGENTS:
    if a.get('tags', {}).get('type') == 'prod':
        SOURCES.append({
            'endpoint': { 'kind': 'agent', 'agent_id': a['id'], 'root': 'logs' },
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

- 文件位置：`~/.opsbox/planners/<app>.star`
- 选择脚本：在搜索框查询中加上 `app:<app>`；未指定时默认 `app:bbip`
- 若用户脚本不存在，会回退到内置同名脚本

## 常见错误与排查

- 未导出 `SOURCES`：后端会提示“未导出 SOURCES”。
- 字段拼写错误或大小写不一致：
  - Endpoint 用 `kind`，取值 `'local'|'agent'|'s3'`
  - Target 用 `type`，取值 `'dir'|'files'|'targz'|'all'`
- 路径语义：
  - 所有 Target 路径（含 files/targz）均相对 `endpoint.root`。
  - Agent 的 `root='.'` 表示不限根（由后端解释为服务端默认根）。
  - S3 的 `target.targz.path` 必须是对象 Key，相对 `bucket`，不要写成 `s3://bucket/key`。
- Profile/存储配置：
  - `profile` 名称未配置或拼写错误会导致后端取对象失败（查看问题详情/服务端日志）。
- 规模控制：
  - 生成对象数量过多（长日期区间 × 多桶）会显著增加 IO；先缩小范围或分批执行。
- 能力边界：
  - 目前 S3 仅支持 `target='targz'` 组合；如需前缀列举并进行“目录式”搜索，请反馈需求。
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
  - `target.targz.path` 仅写对象 Key，配合 `filter_glob` 在归档内进一步约束匹配。

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
            'endpoint': { 'kind': 'agent', 'agent_id': a['id'], 'root': 'logs' },
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
        'target':   { 'type': 'targz', 'path': 'archive/app_2025-01-15.tar.gz' },
        'filter_glob': '**/*.log',
    })

# 3) 可选：覆盖 CLEANED_QUERY（例如增加 path 约束）
# CLEANED_QUERY = CLEANED_QUERY + ' path:**/*.log'
```
