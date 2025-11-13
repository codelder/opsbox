#!/bin/bash

# Disable git pager
export GIT_PAGER=cat
export PAGER=cat

# Create new branch
git checkout -b refactor/error-handling-improvements

# Stage the changes
git add backend/logseek/src/domain/source_planner/starlark_runtime.rs
git add backend/logseek/src/lib.rs
git add backend/logseek/src/routes/nl2q.rs
git add backend/logseek/src/routes/planners.rs
git add backend/logseek/src/routes/search.rs
git add backend/logseek/src/routes/view.rs

# Commit with message
git commit -m "refactor: improve error handling with ServiceError abstraction

- Replace direct AppError usage with ServiceError in starlark runtime
- Add ServiceError to AppError conversion in lib.rs
- Update error handling in nl2q, planners, search, and view routes
- Improve error context with ConfigError and ProcessingError variants
- Enhance error messages for better debugging"

# Show the result
echo "=== Commit created successfully ==="
git log -1 --oneline
echo "=== Current branch ==="
git branch --show-current
