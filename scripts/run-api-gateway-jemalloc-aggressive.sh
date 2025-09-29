#!/usr/bin/env bash
set -euo pipefail

# 中文注释：以“更积极回收”的配置运行 api-gateway（二进制已使用 jemalloc 作为全局分配器）
# 说明：
# - background_thread:true  启用 jemalloc 后台线程，周期性清理空闲页
# - dirty_decay_ms:0        立即清理 dirty 页（不延迟）
# - muzzy_decay_ms:0        立即清理 muzzy 页（macOS 上常用 MADV_FREE 标记，可在压力下被系统回收）
# 注：RSS 可能不会瞬时下降，但在压力或后台清理后会逐步回落；请以压力测试/长期观测为准。
export MALLOC_CONF="background_thread:true,dirty_decay_ms:0,muzzy_decay_ms:0"

# 中文注释：以工作区 manifest 路径运行 api-gateway，透传所有参数
exec cargo run --manifest-path server/Cargo.toml -p api-gateway -- "$@"
