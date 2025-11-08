# Local directory and tar.gz file search planner (Starlark)
# Description:
# - Backend-injected variables (see starlark_runtime.rs):
#   CLEANED_QUERY, DATE_RANGE, TODAY, DATES, S3_PROFILES, AGENTS
# - Export:
#   SOURCES: list[dict]
#   Optional override CLEANED_QUERY
# - Features:
#   Server-side local file system search
#   Support for local directories and tar.gz archives
#   Automatic file type detection based on extension

SOURCES = []

# Example 1: Search local directory
SOURCES.append({
    "endpoint": { "kind": "local", "root": "/var/log/myapp" },
    "target":   { "type": "dir", "path": ".", "recursive": True },
})

# Example 2: Search local archive (tar/tar.gz/gz auto-detected)
SOURCES.append({
    "endpoint": { "kind": "local", "root": "/archive" },
    "target":   { "type": "archive", "path": "logs_2025-01-15.tar.gz" },
    "filter_glob": "**/*.log",
})

# Example 3: Explicit directory under root
SOURCES.append({
    "endpoint": { "kind": "local", "root": "/var/log" },
    "target":   { "type": "dir", "path": "nginx", "recursive": True },
})

# Example 4: Explicit archive under root
SOURCES.append({
    "endpoint": { "kind": "local", "root": "/backup" },
    "target":   { "type": "archive", "path": "app_logs_2025-01-15.tar.gz" },
    "filter_glob": "**/*.log",
})

# Example 5: Mixed local and Agent
if len(AGENTS) > 0:
    for agent in AGENTS:
        if agent.get("tags", {}).get("type") == "prod":
            SOURCES.append({
                "endpoint": { "kind": "agent", "agent_id": agent["id"], "root": "logs" },
                "target":   { "type": "dir", "path": ".", "recursive": True },
            })

# Search historical archives from local storage
for d in DATES:
    if d["iso"] < TODAY:
        archive_name = "logs_archive_{}.tar.gz".format(d["iso"])
        SOURCES.append({
            "endpoint": { "kind": "local", "root": "/archive" },
    "target":   { "type": "archive", "path": archive_name },
            "filter_glob": "**/*.log",
        })

# Example 6: Multiple local directories
log_dirs = [
    "/var/log/app",
    "/var/log/system",
    "/opt/logs/service",
]
for log_dir in log_dirs:
    SOURCES.append({
        "endpoint": { "kind": "local", "root": log_dir },
        "target":   { "type": "dir", "path": ".", "recursive": True },
        "filter_glob": "**/*.log",
    })

# Optional: Override CLEANED_QUERY if needed
# CLEANED_QUERY = CLEANED_QUERY + " level:ERROR"
