#!/usr/bin/env bash
set -euo pipefail

# 中文注释：以“更积极回收”的配置运行 opsbox-server（二进制已使用 mimalloc 作为全局分配器）
# 注意：当前项目已切换为 mimalloc，此脚本中的 MALLOC_CONF 设置针对 jemalloc，不再生效。
# 请直接运行 opsbox-server。
export MALLOC_CONF="background_thread:true,dirty_decay_ms:0,muzzy_decay_ms:0"

# 中文注释：以工作区 manifest 路径运行 opsbox-server，透传所有参数
exec cargo run --manifest-path backend/Cargo.toml -p opsbox-server -- "$@"
