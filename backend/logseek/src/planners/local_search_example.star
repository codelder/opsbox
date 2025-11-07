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
# Use case: Search logs in a local mounted directory on the server
SOURCES.append({
    "type": "local",
    "path": "/var/log/myapp",
    "recursive": True,
})

# Example 2: Search local tar.gz archive
# Use case: Search in a compressed archive of historical logs
SOURCES.append({
    "type": "local",
    "path": "/archive/logs_2025-01-15.tar.gz",
    "recursive": True,
    "path_filter_glob": "**/*.log",
})

# Example 3: Search with explicit scope - Directory
# Use case: Explicitly specify searching a directory
SOURCES.append({
    "type": "local",
    "path": "/var/log/nginx",
    "recursive": True,
    "scope": {
        "Directory": {
            "path": "/var/log/nginx",
            "recursive": True
        }
    },
})

# Example 4: Search with explicit scope - TarGz
# Use case: Explicitly specify searching a tar.gz file
SOURCES.append({
    "type": "local",
    "path": "/backup/app_logs_2025-01-15.tar.gz",
    "recursive": True,
    "scope": {
        "TarGz": {
            "path": "/backup/app_logs_2025-01-15.tar.gz"
        }
    },
    "path_filter_glob": "**/*.log",
})

# Example 5: Mixed sources - combine local and Agent
# Use case: Search today's logs from local archives and live Agent nodes
if len(AGENTS) > 0:
    for agent in AGENTS:
        if "type" in agent["tags"] and agent["tags"]["type"] == "prod":
            SOURCES.append({
                "type": "agent",
                "agent_id": agent["id"],
                "scope": {
                    "Directory": {
                        "path": "logs",
                        "recursive": True
                    }
                },
            })

# Search historical archives from local storage
for d in DATES:
    if d["iso"] < TODAY:
        archive_name = "logs_archive_{}.tar.gz".format(d["iso"])
        SOURCES.append({
            "type": "local",
            "path": "/archive/{}".format(archive_name),
            "recursive": True,
            "scope": {
                "TarGz": {
                    "path": "/archive/{}".format(archive_name)
                }
            },
            "path_filter_glob": "**/*.log",
        })

# Example 6: Multiple local directories
# Use case: Search across multiple log directories
log_dirs = [
    "/var/log/app",
    "/var/log/system",
    "/opt/logs/service",
]

for log_dir in log_dirs:
    SOURCES.append({
        "type": "local",
        "path": log_dir,
        "recursive": True,
        "path_filter_glob": "**/*.log",
    })

# Optional: Override CLEANED_QUERY if needed
# CLEANED_QUERY = CLEANED_QUERY + " level:ERROR"
