#!/bin/bash
# 调试 Ollama 原始输出的脚本

echo "=== Ollama 原始输出调试工具 ==="
echo

# 检查 Ollama 是否运行
if ! curl -s http://127.0.0.1:11434/api/tags > /dev/null; then
    echo "错误: Ollama 服务未运行或无法访问 http://127.0.0.1:11434"
    echo "请确保 Ollama 已启动并运行在默认端口"
    exit 1
fi

echo "✓ Ollama 服务正在运行"
echo

# 设置环境变量（可选）
export RUST_LOG=debug

# 运行调试脚本
echo "开始调试..."
cd "$(dirname "$0")/.."

# 使用 cargo run 运行调试脚本
cargo run --bin debug_ollama 2>/dev/null || {
    echo "尝试使用 rust-script 运行..."
    if command -v rust-script >/dev/null 2>&1; then
        rust-script scripts/debug_ollama.rs
    else
        echo "请安装 rust-script: cargo install rust-script"
        echo "或者手动运行: cargo run --bin debug_ollama"
    fi
}
