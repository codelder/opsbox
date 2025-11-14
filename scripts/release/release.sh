#!/usr/bin/env bash
# 自动化发布流程
# 用法: ./scripts/release/release.sh 0.1.0-rc1

set -e

VERSION=$1

if [ -z "$VERSION" ]; then
  echo "错误: 请提供版本号"
  echo "用法: $0 <version>"
  exit 1
fi

echo "🚀 开始发布流程: v$VERSION"
echo ""

# 1. 检查工作区是否干净
if [ -n "$(git status --porcelain)" ]; then
  echo "❌ 错误: 工作区有未提交的更改"
  echo "请先提交或暂存更改"
  exit 1
fi

# 2. 设置版本号
echo "📦 设置版本号..."
./scripts/release/set-version.sh "$VERSION"

# 3. 提交更改
echo ""
echo "💾 提交版本更改..."
git add -A
git commit -m "chore: bump version to $VERSION"

# 4. 创建标签
echo ""
echo "🏷️  创建 Git 标签..."
git tag -a "v$VERSION" -m "Release v$VERSION"

# 5. 推送
echo ""
echo "📤 推送到远程仓库..."
read -p "是否推送到远程? (y/N) " -n 1 -r
echo
if [[ $REPLY =~ ^[Yy]$ ]]; then
  git push origin HEAD
  git push origin "v$VERSION"
  echo "✅ 已推送到远程"
else
  echo "⏭️  跳过推送"
fi

echo ""
echo "🎉 发布完成: v$VERSION"
