#!/usr/bin/env bash
# 统一设置项目版本号
# 用法: ./scripts/release/set-version.sh 0.1.0-rc1

set -e

VERSION=$1

if [ -z "$VERSION" ]; then
  echo "错误: 请提供版本号"
  echo "用法: $0 <version>"
  echo "示例: $0 0.1.0-rc1"
  exit 1
fi

echo "📦 设置版本号为: $VERSION"
echo ""

# 1. 更新所有 backend Cargo.toml
echo "🦀 更新 Rust 包版本..."
for cargo_toml in backend/*/Cargo.toml; do
  if [ -f "$cargo_toml" ]; then
    # 使用 sed 替换 version 行
    if [[ "$OSTYPE" == "darwin"* ]]; then
      # macOS
      sed -i '' "s/^version = \".*\"$/version = \"$VERSION\"/" "$cargo_toml"
    else
      # Linux
      sed -i "s/^version = \".*\"$/version = \"$VERSION\"/" "$cargo_toml"
    fi
    echo "  ✓ $(basename $(dirname $cargo_toml))"
  fi
done

# 2. 更新前端 package.json
echo ""
echo "🌐 更新前端版本..."
if [ -f "web/package.json" ]; then
  if [[ "$OSTYPE" == "darwin"* ]]; then
    sed -i '' "s/\"version\": \".*\"/\"version\": \"$VERSION\"/" web/package.json
  else
    sed -i "s/\"version\": \".*\"/\"version\": \"$VERSION\"/" web/package.json
  fi
  echo "  ✓ web/package.json"
fi

# 3. 更新 Cargo.lock
echo ""
echo "🔒 更新 Cargo.lock..."
cd backend
cargo update --workspace --quiet
cd ..
echo "  ✓ Cargo.lock"

# 4. 显示更改
echo ""
echo "📝 版本号已更新，请检查以下文件:"
git diff --name-only | grep -E "(Cargo.toml|package.json|Cargo.lock)" || echo "  (无更改)"

echo ""
echo "✅ 版本号设置完成: $VERSION"
echo ""
echo "下一步:"
echo "  1. 检查更改: git diff"
echo "  2. 提交更改: git add -A && git commit -m \"chore: bump version to $VERSION\""
echo "  3. 创建标签: git tag v$VERSION"
echo "  4. 推送标签: git push origin v$VERSION"
