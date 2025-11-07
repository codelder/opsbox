# 内置 bbip 存储源规划脚本（Starlark）
# 说明：
# - 依赖后端注入的变量（见后端 starlark_runtime.rs）：
#   CLEANED_QUERY, DATE_RANGE, TODAY, DATES, S3_PROFILES, AGENTS
# - 导出：
#   SOURCES: list[dict]
#   可选覆盖 CLEANED_QUERY

SOURCES = []
# 业务自定义：BBIP 的分桶集合（如无分桶可留空或删除 S3 生成段）
BUCKETS = ['20','21','22','23']

# 计算当日是否参与（不预设意义，按 BBIP 约定：若范围包含 TODAY，则 Agent 用于当天）
has_today = False
for d in DATES:
    if d["iso"] == TODAY:
        has_today = True
        break

# Agent（今天）：在脚本内按标签 app=bbipapp 自行筛选
if has_today and len(AGENTS) > 0:
    today_glob = "**/{}/**".format(TODAY)
    for a in AGENTS:
        if "app" in a["tags"] and a["tags"]["app"] == "bbipapp":
            SOURCES.append({
                "endpoint": {"kind": "agent", "agent_id": a["id"], "root": "logs"},
                "target": {"type": "dir", "path": ".", "recursive": True},
                "filter_glob": today_glob,
            })

# S3（昨天及以前）
oss = None
for prof in S3_PROFILES:
    if prof["profile_name"] == "oss":
        oss = prof
        break

if oss != None:
    for d in DATES:
        if d["iso"] < TODAY:  # 字符串比较对 YYYY-MM-DD 等价于日期顺序
            y = d["next_yyyymmdd"][0:4]
            yyyymm = d["next_yyyymmdd"][0:6]
            yyyymmdd = d["next_yyyymmdd"]
            file = d["iso"]
            for b in BUCKETS:
                key = "bbip/{}/{}/{}/BBIP_{}_APPLOG_{}.tar.gz".format(
                    y, yyyymm, yyyymmdd, b, file,
                )
                SOURCES.append({
                    "endpoint": {"kind": "s3", "profile": oss["profile_name"], "bucket": oss["bucket"]},
                    "target": {"type": "targz", "path": key},
                })

# 可选：覆写 CLEANED_QUERY，例如追加路径限定
# CLEANED_QUERY = CLEANED_QUERY
