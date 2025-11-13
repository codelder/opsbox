#!/usr/bin/env python3
import subprocess
import os

# Set environment to disable pager
env = os.environ.copy()
env['GIT_PAGER'] = ''
env['PAGER'] = 'cat'

def run_git(args):
    """Run git command and return output"""
    result = subprocess.run(
        ['git'] + args,
        capture_output=True,
        text=True,
        env=env
    )
    return result.stdout, result.stderr, result.returncode

# Create new branch
print("Creating new branch...")
stdout, stderr, code = run_git(['checkout', '-b', 'refactor/error-handling-improvements'])
if code != 0:
    print(f"Branch creation output: {stdout}{stderr}")

# Stage files
files = [
    'backend/logseek/src/domain/source_planner/starlark_runtime.rs',
    'backend/logseek/src/lib.rs',
    'backend/logseek/src/routes/nl2q.rs',
    'backend/logseek/src/routes/planners.rs',
    'backend/logseek/src/routes/search.rs',
    'backend/logseek/src/routes/view.rs'
]

print("Staging files...")
for f in files:
    stdout, stderr, code = run_git(['add', f])
    if code != 0:
        print(f"Error adding {f}: {stderr}")

# Commit
commit_msg = """refactor: improve error handling with ServiceError abstraction

- Replace direct AppError usage with ServiceError in starlark runtime
- Add ServiceError to AppError conversion in lib.rs
- Update error handling in nl2q, planners, search, and view routes
- Improve error context with ConfigError and ProcessingError variants
- Enhance error messages for better debugging"""

print("Creating commit...")
stdout, stderr, code = run_git(['commit', '-m', commit_msg])
print(stdout)
if stderr:
    print(stderr)

# Show result
print("\n=== Commit created successfully ===")
stdout, stderr, _ = run_git(['log', '-1', '--oneline'])
print(stdout)

print("=== Current branch ===")
stdout, stderr, _ = run_git(['branch', '--show-current'])
print(stdout)
