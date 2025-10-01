#!/usr/bin/env bash
# 中文注释：NDJSON 流式检索一键压测脚本
# 功能：
# 1) 重启 api-gateway 并设置 CPU 并发上限
# 2) 执行 LONG_SECS(默认120) 秒的 CPU=16 压测并导出自适应护栏日志为 CSV
# 3) 对 CPU=8、12、16 分别执行 SHORT_SECS(默认30) 秒对比压测并打印吞吐汇总（Markdown 表）
#
# 使用：
#   bash scripts/bench-ndjson.sh
# 可选环境变量：
#   QUERY_JSON     NDJSON 查询 JSON 字符串（默认：{"q":"error fdt:20250816 tdt:20250819"}）
#   ADDR           服务地址（默认：127.0.0.1:4000）
#   WORKER_THREADS Tokio 工作线程数（默认：16）
#   S3_MAX_CONC    S3/MinIO IO 并发（默认：12）
#   STREAM_CH_CAP  输出通道容量（默认：256）
#   MINIO_TIMEOUT  MinIO 超时秒数（默认：60）
#   MINIO_RETRIES  MinIO 最大重试次数（默认：5）
#   CPU_SERIES     对比压测的 CPU 并发列表（逗号分隔，默认：8,12,16）
#   LONG_SECS      长测时长（默认：120）
#   SHORT_SECS     短测时长（默认：30）
#   BIN_PATH       api-gateway 二进制路径（默认：server/target/release/api-gateway）
#   LOG_PATH       日志文件路径（默认：~/.opsbox/api-gateway.log）
#   JEMALLOC_AGGRESSIVE 若为 1/true/yes，则为进程设置更积极回收的 MALLOC_CONF
#   MALLOC_CONF    如已事先设置，则优先使用该值（覆盖 aggressive 预设）

set -euo pipefail

ROOT_DIR=$(cd "$(dirname "$0")/.." && pwd)
BIN_DEFAULT="$ROOT_DIR/server/target/release/api-gateway"
LOG_DEFAULT="$HOME/.opsbox/api-gateway.log"

BIN="${BIN_PATH:-$BIN_DEFAULT}"
LOG="${LOG_PATH:-$LOG_DEFAULT}"
ADDR="${ADDR:-127.0.0.1:4000}"
WORKER_THREADS="${WORKER_THREADS:-16}"
S3_MAX_CONC="${S3_MAX_CONC:-12}"
STREAM_CH_CAP="${STREAM_CH_CAP:-256}"
MINIO_TIMEOUT="${MINIO_TIMEOUT:-60}"
MINIO_RETRIES="${MINIO_RETRIES:-5}"
CPU_SERIES="${CPU_SERIES:-8,12,16}"
LONG_SECS="${LONG_SECS:-120}"
SHORT_SECS="${SHORT_SECS:-30}"
QUERY_JSON_DEFAULT='{"q":"error fdt:20250816 tdt:20250822"}'
QUERY_JSON="${QUERY_JSON:-$QUERY_JSON_DEFAULT}"

# 中文注释：可选启用 jemalloc 的“积极回收”配置
# 触发条件：JEMALLOC_AGGRESSIVE=1|true|yes，且未显式设置 MALLOC_CONF
AGG="${JEMALLOC_AGGRESSIVE:-}"
if [[ "$AGG" == "1" || "$AGG" == "true" || "$AGG" == "TRUE" || "$AGG" == "yes" || "$AGG" == "YES" ]]; then
  if [[ -z "${MALLOC_CONF:-}" ]]; then
    export MALLOC_CONF="background_thread:true,dirty_decay_ms:0,muzzy_decay_ms:0"
    echo "[bench-ndjson] jemalloc aggressive enabled; MALLOC_CONF=${MALLOC_CONF}"
  else
    echo "[bench-ndjson] JEMALLOC_AGGRESSIVE set but MALLOC_CONF already present; respecting MALLOC_CONF=${MALLOC_CONF}"
  fi
fi

BASE_ARGS=(
  --addr "$ADDR"
  --worker-threads "$WORKER_THREADS"
  --s3-max-concurrency "$S3_MAX_CONC"
  --stream-ch-cap "$STREAM_CH_CAP"
  --minio-timeout-sec "$MINIO_TIMEOUT"
  --minio-max-attempts "$MINIO_RETRIES"
  -V
)

# 中文注释：优雅重启 api-gateway 并使用指定 CPU 并发
restart_with_cpu() {
  local cpu="$1"
  local pids
  pids=$(pgrep -f "$BIN" || true)
  if [ -n "$pids" ]; then
    kill -TERM $pids || true
    for i in {1..50}; do
      sleep 0.1
      alive=$(ps -o pid= -p $pids 2>/dev/null | tr -d " " || true)
      [ -z "$alive" ] && break
    done
    alive=$(ps -o pid= -p $pids 2>/dev/null | tr -d " " || true)
    [ -n "$alive" ] && kill -KILL $alive || true
  fi
  nohup "$BIN" "${BASE_ARGS[@]}" --cpu-concurrency "$cpu" >> "$LOG" 2>&1 &
  local newpid=$!
  for i in {1..50}; do
    sleep 0.2
    # 中文注释：在 set -euo pipefail 下，使用 if 包裹避免因 grep 返回非零而退出
    if curl -sS "http://$ADDR/healthy" | grep -q "ok"; then
      break
    fi
  done
  echo "restarted pid=$newpid cpu=$cpu"
}

# 中文注释：执行流式检索压测
run_stream_test() {
  local seconds="$1"; local label="$2"; local cpu="$3"
  local tmp
  tmp=$(mktemp) && printf "%s" "$QUERY_JSON" > "$tmp"
  local before_lines t0 t1 lines dur
  before_lines=$(wc -l < "$LOG" | tr -d " ")
  t0=$(date +%s)
  # 中文注释：在 set -euo pipefail 下，允许 curl 因 --max-time 返回非零但仍统计已有输出
  lines=$(( $( (curl -sS -N --max-time "$seconds" \
    -H "Accept: application/x-ndjson" -H "Content-Type: application/json" \
    --data-binary @"$tmp" "http://$ADDR/api/v1/logseek/stream.s3.ndjson" || true) | wc -l | tr -d " ") ))
  t1=$(date +%s); dur=$((t1 - t0)); rm -f "$tmp"

  # 中文注释：导出自适应护栏日志（仅 label 包含 csv 的情况）
  if [[ "$label" == *csv* ]]; then
    local out="$HOME/adaptive_${seconds}s_cpu${cpu}.csv"
    # 中文注释：先写表头，再追加匹配到的 adaptive 行；若无匹配，不影响生成
    printf "%s\n" "time_iso,target,effective,err_rate_percent,tp_per_s" > "$out"
    tail -n +$((before_lines+1)) "$LOG" | \
      grep -E "adaptive: cpu target=" | \
      sed -E 's/^\[([^]]+)\].*cpu target=([0-9]+) effective=([0-9]+) err_rate=([0-9.]+)% tp=([0-9.]+)\/s.*/\1,\2,\3,\4,\5/' >> "$out" || true
    echo "csv=$out"
  fi

  # 输出单行结果，供汇总
  awk -v L=$lines -v D=$dur -v C=$cpu -v LBL=$label \
    'BEGIN{tp=(D>0?L/D:0); printf "__RESULT__ label=%s cpu=%d lines=%d duration_s=%d avg_tput=%.2f\n", LBL, C, L, D, tp}'
}

main() {
  local results=""

  # 1) CPU=16，长测并导出 CSV
  restart_with_cpu 16
  local r1; r1=$(run_stream_test "$LONG_SECS" csv 16); echo "$r1"; results+=$'\n'; results+="$r1"

  # 2) CPU 系列短测（默认：8、12、16）
  IFS=',' read -r -a CPUS <<< "$CPU_SERIES"
  for c in "${CPUS[@]}"; do
    restart_with_cpu "$c"
    local rr; rr=$(run_stream_test "$SHORT_SECS" short "$c"); echo "$rr"; results+=$'\n'; results+="$rr"
  done

  # 打印 Markdown 汇总表
  echo
  echo "Summary (Markdown)"
  echo "| label | cpu | duration_s | lines | avg_tput (/s) |"
  echo "|-------|-----|------------|-------|----------------|"
  printf "%s\n" "$results" | awk -F'[ =]' '/^__RESULT__/ { printf "| %s | %s | %s | %s | %s |\n", $3, $5, $9, $7, $11 }'
}

main "$@"
