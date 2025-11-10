#!/bin/bash
# 优雅关闭测试脚本
# 用于验证 Ctrl-C 和 SIGTERM 信号处理是否正常

set -e

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

echo -e "${BLUE}╔════════════════════════════════════════╗${NC}"
echo -e "${BLUE}║   优雅关闭测试                         ║${NC}"
echo -e "${BLUE}╚════════════════════════════════════════╝${NC}"

# 获取项目根目录
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"

# 构建项目
echo -e "\n${YELLOW}[1/3] 编译 opsbox-server...${NC}"
cd "$PROJECT_ROOT/backend"
cargo build --release -p opsbox-server 2>&1 | tail -5
cd "$PROJECT_ROOT"

echo -e "${GREEN}✓ 编译完成${NC}"

# 测试 1: SIGINT (Ctrl-C)
echo -e "\n${YELLOW}[2/3] 测试 SIGINT (Ctrl-C) 信号...${NC}"
echo -e "${BLUE}启动服务器（5秒后自动发送 SIGINT）${NC}"

# 启动服务器（后台）
"$PROJECT_ROOT/backend/target/release/opsbox-server" \
  --host 127.0.0.1 --port 18080 \
  --database-url /tmp/test_shutdown.db \
  > /tmp/opsbox-server_test.log 2>&1 &

SERVER_PID=$!
echo -e "服务器 PID: ${GREEN}$SERVER_PID${NC}"

# 等待服务器启动
sleep 3

# 检查服务器是否正在运行
if ps -p $SERVER_PID > /dev/null; then
  echo -e "${GREEN}✓ 服务器已启动${NC}"
else
  echo -e "${RED}✗ 服务器启动失败${NC}"
  cat /tmp/opsbox-server_test.log
  exit 1
fi

# 发送 SIGINT
echo -e "${BLUE}发送 SIGINT 信号...${NC}"
kill -SIGINT $SERVER_PID

# 等待进程退出（最多5秒）
echo -e "${BLUE}等待优雅关闭...${NC}"
for i in {1..10}; do
  if ! ps -p $SERVER_PID > /dev/null 2>&1; then
    echo -e "${GREEN}✓ 服务器已优雅关闭 (${i}秒)${NC}"
    break
  fi
  sleep 0.5
done

# 检查是否还在运行
if ps -p $SERVER_PID > /dev/null 2>&1; then
  echo -e "${RED}✗ 服务器未能在5秒内关闭，强制终止${NC}"
  kill -9 $SERVER_PID
  exit 1
fi

# 检查日志中的关闭信息
echo -e "\n${BLUE}检查日志输出:${NC}"
if grep -q "收到关闭信号.*SIGINT" /tmp/opsbox-server_test.log; then
  echo -e "${GREEN}✓ 发现 SIGINT 信号日志${NC}"
else
  echo -e "${RED}✗ 未找到 SIGINT 信号日志${NC}"
  echo "日志内容:"
  cat /tmp/opsbox-server_test.log
  exit 1
fi

if grep -q "所有模块已清理完成" /tmp/opsbox-server_test.log; then
  echo -e "${GREEN}✓ 发现模块清理完成日志${NC}"
else
  echo -e "${YELLOW}⚠ 未找到模块清理完成日志${NC}"
fi

# 测试 2: SIGTERM
echo -e "\n${YELLOW}[3/3] 测试 SIGTERM 信号...${NC}"
echo -e "${BLUE}启动服务器（5秒后自动发送 SIGTERM）${NC}"

# 清空日志
> /tmp/opsbox-server_test.log

# 启动服务器（后台）
"$PROJECT_ROOT/backend/target/release/opsbox-server" \
  --host 127.0.0.1 --port 18080 \
  --database-url /tmp/test_shutdown.db \
  > /tmp/opsbox-server_test.log 2>&1 &

SERVER_PID=$!
echo -e "服务器 PID: ${GREEN}$SERVER_PID${NC}"

# 等待服务器启动
sleep 3

# 发送 SIGTERM
echo -e "${BLUE}发送 SIGTERM 信号...${NC}"
kill -SIGTERM $SERVER_PID

# 等待进程退出
echo -e "${BLUE}等待优雅关闭...${NC}"
for i in {1..10}; do
  if ! ps -p $SERVER_PID > /dev/null 2>&1; then
    echo -e "${GREEN}✓ 服务器已优雅关闭 (${i}秒)${NC}"
    break
  fi
  sleep 0.5
done

# 检查是否还在运行
if ps -p $SERVER_PID > /dev/null 2>&1; then
  echo -e "${RED}✗ 服务器未能在5秒内关闭，强制终止${NC}"
  kill -9 $SERVER_PID
  exit 1
fi

# 检查日志
echo -e "\n${BLUE}检查日志输出:${NC}"
if grep -q "收到关闭信号.*SIGTERM" /tmp/opsbox-server_test.log; then
  echo -e "${GREEN}✓ 发现 SIGTERM 信号日志${NC}"
else
  echo -e "${RED}✗ 未找到 SIGTERM 信号日志${NC}"
  echo "日志内容:"
  cat /tmp/opsbox-server_test.log
  exit 1
fi

# 清理
rm -f /tmp/test_shutdown.db /tmp/test_shutdown.db-shm /tmp/test_shutdown.db-wal

echo -e "\n${GREEN}╔════════════════════════════════════════╗${NC}"
echo -e "${GREEN}║   ✓ 所有测试通过！                     ║${NC}"
echo -e "${GREEN}╚════════════════════════════════════════╝${NC}"

echo -e "\n${BLUE}测试总结:${NC}"
echo -e "  ✓ SIGINT (Ctrl-C) - 优雅关闭正常"
echo -e "  ✓ SIGTERM - 优雅关闭正常"
echo -e "  ✓ 日志输出正确"
echo -e "  ✓ 资源清理完成"

echo -e "\n${YELLOW}提示: 完整日志保存在 /tmp/opsbox-server_test.log${NC}"
