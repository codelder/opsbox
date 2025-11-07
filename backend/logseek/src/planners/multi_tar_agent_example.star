# 多tar.gz Agent 规划脚本示例（Starlark）
# 说明：
# - 依赖后端注入的变量（见后端 starlark_runtime.rs）：
#   CLEANED_QUERY, DATE_RANGE, TODAY, DATES, S3_PROFILES, AGENTS
# - 导出：
#   SOURCES: list[dict]
#   可选覆盖 CLEANED_QUERY
# - 新增支持：
#   SourceConfig::Agent 现在支持 scope 字段
#   可指定搜索范围为 TarGz、Directory、Files 等类型

SOURCES = []

# 应用配置
APPS = {
    "app_a": "applog_a",
    "app_b": "applog_b",
    "app_c": "applog_c",
}

# 方案 1: 按标签筛选 Agent，为其指定 tar.gz 搜索范围
if len(AGENTS) > 0:
    # 筛选生产环境的 Agent（标签 env=prod）
    for agent in AGENTS:
        if "env" in agent["tags"] and agent["tags"]["env"] == "prod":
            # 为每个日期生成 tar.gz 归档文件源
            for d in DATES:
                date_iso = d["iso"]
                
                for app_name, tar_prefix in APPS.items():
                    # 为每个应用的每小时生成一个 tar.gz 源
                    for hour in range(24):
                        tar_filename = "{}_{}_{:02d}.tar.gz".format(tar_prefix, date_iso, hour)
                        
                        SOURCES.append({
                            "type": "agent",
                            "agent_id": agent["id"],
                            "scope_root": "logs",
                            # 新增：指定搜索范围为 TarGz 类型
                            # Agent 将自动识别这是归档文件并解压搜索
                            "scope": {
                                "TarGz": {
                                    "path": tar_filename
                                }
                            },
                            # 可选：在 tar.gz 内过滤文件
                            "path_filter_glob": "**/*.log",
                        })

# 方案 2: 混合搜索范围示例
# SOURCES = []
# for agent in AGENTS:
#     if "app" in agent["tags"] and agent["tags"]["app"] == "web":
#         # 示例1: 搜索特定目录
#         SOURCES.append({
#             "type": "agent",
#             "agent_id": agent["id"],
#             "scope": {
#                 "Directory": {
#                     "path": "logs/web",
#                     "recursive": True
#                 }
#             },
#             "path_filter_glob": "**/*error*.log",
#         })
#         
#         # 示例2: 搜索特定 tar.gz 文件
#         SOURCES.append({
#             "type": "agent",
#             "agent_id": agent["id"],
#             "scope": {
#                 "TarGz": {
#                     "path": "backup/web_2025-01-15.tar.gz"
#                 }
#             },
#             "path_filter_glob": "**/*.log",
#         })
#         
#         # 示例3: 搜索特定文件列表
#         SOURCES.append({
#             "type": "agent",
#             "agent_id": agent["id"],
#             "scope": {
#                 "Files": {
#                     "paths": ["logs/access.log", "logs/error.log"]
#                 }
#             },
#         })
#         
#         # 示例4: 搜索所有范围
#         SOURCES.append({
#             "type": "agent",
#             "agent_id": agent["id"],
#             "scope": "All",
#             "path_filter_glob": "**/*.log",
#         })

# 方案 3: 按日期范围和 Agent 标签分配 tar.gz
# for d in DATES:
#     date_iso = d["iso"]
#     
#     # 当日日志 -> 从 prod Agent 的 tar.gz 中查找
#     if d["iso"] == TODAY:
#         for agent in AGENTS:
#             if "env" in agent["tags"] and agent["tags"]["env"] == "prod":
#                 SOURCES.append({
#                     "type": "agent",
#                     "agent_id": agent["id"],
#                     "scope": {
#                         "TarGz": {
#                             "path": "logs/today_{}.tar.gz".format(date_iso)
#                         }
#                     },
#                 })
#     # 历史日志 -> 从 archive Agent 的 tar.gz 中查找
#     else:
#         for agent in AGENTS:
#             if "app" in agent["tags"] and agent["tags"]["app"] == "archiver":
#                 SOURCES.append({
#                     "type": "agent",
#                     "agent_id": agent["id"],
#                     "scope": {
#                         "TarGz": {
#                             "path": "archive/logs_{}.tar.gz".format(date_iso)
#                         }
#                     },
#                 })

# 可选：覆写 CLEANED_QUERY
# CLEANED_QUERY = CLEANED_QUERY + " app:(app_a OR app_b OR app_c)"
