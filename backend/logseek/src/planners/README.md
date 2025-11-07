# Starlark 存储源规划脚本开发指南

本指南说明如何在用户侧用 Starlark 编写“存储源规划脚本”，由后端解释执行，决定一次搜索要访问的存储源（S3、Agent、本地等）。

已简化为统一的来源模型：Source = Endpoint(在哪里) + Target(查什么) + 可选 filter_glob。

## 放置路径与命名
- 用户脚本路径：`~/.opsbox/planners/<app>.star`
- 选择方式：在搜索 `q` 中使用 `app:<name>`（例如 `app:bbip`）。若未指定，则默认 `bbip`。
- 回退：若用户脚本不存在，则使用内置同名脚本作为回退（例如 `bbip.star`）。

## 运行时注入的变量（只读）
- CLEANED_QUERY: string
  - 已移除了 `dt:/fdt:/tdt:` 日期指令与 `app:xxx` 标记的查询字符串。
- DATE_RANGE: { start: "YYYY-MM-DD", end: "YYYY-MM-DD" }
- TODAY: "YYYY-MM-DD"（北京时间）
- DATES: list[dict]
  - 按范围展开的每日对象，形如：
    - { iso: "YYYY-MM-DD", yyyymmdd: "YYYYMMDD", next_yyyymmdd: "YYYYMMDD" }
- AGENTS: list[dict]
  - 在线 Agent 列表及标签，形如：
    - { id: string, tags: { key: string, value: string, ... } }
- S3_PROFILES: list[dict]
  - 非敏感字段，形如：
    - { profile_name: string, endpoint: string, bucket: string }
  - 不包含 access_key/secret_key；脚本只需返回 profile_name，服务端会用数据库里该 profile 的密钥进行访问。

## 脚本需要导出的变量
- 必须导出：
  - SOURCES: list[Source]，用于告诉后端要搜索的存储源列表。
- 可选导出：
  - CLEANED_QUERY: string（若导出则覆盖运行时注入的值，可用于追加 path 限定等）。

## Source 结构
- Endpoint（在哪里）
  - { kind: "local", root: "/abs/path" }
  - { kind: "agent", agent_id: "agent-01", root: "logs" }  # root 为相对 search_roots 的子路径，"." 表示不限制
  - { kind: "s3", profile: "oss", bucket: "logs-bucket" }
- Target（查什么；所有 path 均相对 endpoint.root）
  - { type: "dir", path: "."|"sub/dir", recursive: true|false }
  - { type: "files", paths: ["a.log", "b.log"] }
  - { type: "targz", path: "archive/app_2025-01-15.tar.gz" }
  - { type: "all" }
- Source 其它字段
  - filter_glob?: string  # 额外路径过滤（与查询中的 path: 规则做 AND）
  - display_name?: string # 可选的 UI 友好名称

## 示例：BBIP 典型策略（可按需调整）
- 需求：
  - 当天（包含 TODAY）使用带标签 `app=bbipapp` 的在线 Agent；
  - 历史（昨天及以前）使用名为 `oss` 的 S3 Profile，并按固定桶集合展开对象键；
  - 如需追加路径限定，覆盖 CLEANED_QUERY。

```python
# 定义业务分桶（如无分桶可修改或留空）
BUCKETS = ['20','21','22','23']

SOURCES = []

# 判断日期范围是否包含今天
has_today = False
for d in DATES:
    if d['iso'] == TODAY:
        has_today = True
        break

# 1) Agent（今天）：按标签 app=bbipapp 筛选
if has_today:
    today_glob = "**/{}/**".format(TODAY)
    for a in AGENTS:
        if a['tags'].get('app') == 'bbipapp':
            SOURCES.append({
                'endpoint': { 'kind': 'agent', 'agent_id': a['id'], 'root': 'logs' },
                'target':   { 'type': 'dir', 'path': '.', 'recursive': True },
                'filter_glob': today_glob,
            })

# 2) S3（历史）：选择名为 oss 的 profile 并展开对象键
oss = None
for p in S3_PROFILES:
    if p['profile_name'] == 'oss':
        oss = p
        break

if oss != None:
    for d in DATES:
        if d['iso'] < TODAY:
            y = d['next_yyyymmdd'][0:4]
            yyyymm = d['next_yyyymmdd'][0:6]
            yyyymmdd = d['next_yyyymmdd']
            file = d['iso']
            for b in BUCKETS:
                key = "bbip/{}/{}/{}/BBIP_{}_APPLOG_{}.tar.gz".format(y, yyyymm, yyyymmdd, b, file)
                SOURCES.append({
                    'endpoint': { 'kind': 's3', 'profile': oss['profile_name'], 'bucket': oss['bucket'] },
                    'target':   { 'type': 'targz', 'path': key },
                })

# 3) 可选：覆盖 CLEANED_QUERY（例如追加路径限定）
# CLEANED_QUERY = CLEANED_QUERY + ' path:logs/*.log'
```

## 提示与约束
- 不要读写外部文件或执行网络操作；脚本应是纯计算（可复现、无副作用）。
- 不要尝试访问密钥；S3_PROFILES 不含敏感字段，服务端会用已保存的密钥执行访问。
- TODAY 以北京时间计算；DATES 为闭区间 [DATE_RANGE.start, DATE_RANGE.end]。
- 语法注意（Starlark 与 Python 差异）：
  - 不支持 f-string，请用 `'...{}'.format(x)`。
  - 不支持生成器表达式与 any()/next() 的生成器用法，请用 for 循环设置标志或查找。
  - 仅内置基本类型与内建函数，无 import。
- 性能：避免构造过于巨大的列表；必要时按日期生成有限对象键或使用前缀+正则。

## 常见错误
- 未导出 `SOURCES` → 后端会报错：未导出 SOURCES。
- `SOURCES` 结构字段拼写错误 → 后端解析失败（中文错误信息）。
- 覆盖的 `CLEANED_QUERY` 非字符串 → 后端解析失败。

## 调试建议
- 在脚本里从小到大逐步生成 SOURCES（先 Agent，再少量 S3），确认搜索能返回结果再扩大范围。
- 后端日志会打印脚本执行错误与解析错误，便于定位问题。

## 参考与官方语法
- 官方语言规范（Language Specification）: https://github.com/bazelbuild/starlark/blob/master/spec.md
- Bazel 官方语言页面（概览与示例）: https://bazel.build/rules/language

如需更多能力（例如 date_* 工具函数），可以反馈需求以便在运行时注入对应内建函数。
