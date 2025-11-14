#!/usr/bin/env bash
# 将所有包迁移到使用 workspace 版本继承

set -e

echo "🔄 迁移到 workspace 版本继承..."

for cargo_toml in backend/*/Cargo.toml; do
  if [ -f "$cargo_toml" ] && [ "$cargo_toml" != "backend/Cargo.toml" ]; then
    package_name=$(basename $(dirname $cargo_toml))
    echo "  处理: $package_name"
    
    # 使用 sed 替换 version/edition/license 为 workspace 继承
    if [[ "$OSTYPE" == "darwin"* ]]; then
      # macOS
      sed -i '' 's/^version = ".*"$/version.workspace = true/' "$cargo_toml"
      sed -i '' 's/^edition = ".*"$/edition.workspace = true/' "$cargo_toml"
      sed -i '' '/^license.workspace = true$/d' "$cargo_toml"
      sed -i '' '/^authors.workspace = true$/d' "$cargo_toml"
      sed -i '' '/^edition.workspace = true$/a\
license.workspace = true\
authors.workspace = true
' "$cargo_toml"
    else
      # Linux
      sed -i 's/^version = ".*"$/version.workspace = true/' "$cargo_toml"
      sed -i 's/^edition = ".*"$/edition.workspace = true/' "$cargo_toml"
      sed -i '/^license.workspace = true$/d' "$cargo_toml"
      sed -i '/^authors.workspace = true$/d' "$cargo_toml"
      sed -i '/^edition.workspace = true$/a license.workspace = true\nauthors.workspace = true' "$cargo_toml"
    fi
  fi
done

echo "✅ 迁移完成"
echo ""
echo "现在只需要修改 backend/Cargo.toml 中的 [workspace.package] 版本号即可"
