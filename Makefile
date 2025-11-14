.PHONY: help version release

help:
	@echo "OpsBox 项目管理命令"
	@echo ""
	@echo "版本管理:"
	@echo "  make version VERSION=0.1.0-rc1  - 设置版本号"
	@echo "  make release VERSION=0.1.0-rc1  - 完整发布流程"
	@echo ""
	@echo "构建:"
	@echo "  make build                      - 构建所有组件（本地）"
	@echo "  make build-backend              - 构建后端（本地）"
	@echo "  make build-frontend             - 构建前端"
	@echo "  make build-linux                - 交叉编译到 Linux (x86_64-musl)"
	@echo "  make build-linux-debug          - 交叉编译到 Linux (debug)"
	@echo "  make package                    - 打包发布版本"
	@echo ""
	@echo "测试:"
	@echo "  make test                       - 运行所有测试"
	@echo "  make test-backend               - 运行后端测试"

version:
	@if [ -z "$(VERSION)" ]; then \
		echo "错误: 请指定版本号"; \
		echo "用法: make version VERSION=0.1.0-rc1"; \
		exit 1; \
	fi
	@./scripts/release/set-version.sh $(VERSION)

release:
	@if [ -z "$(VERSION)" ]; then \
		echo "错误: 请指定版本号"; \
		echo "用法: make release VERSION=0.1.0-rc1"; \
		exit 1; \
	fi
	@./scripts/release/release.sh $(VERSION)

build: build-backend build-frontend

build-backend:
	cd backend && cargo build --release

build-frontend:
	cd web && pnpm install && pnpm build

build-linux:
	@echo "🐧 交叉编译到 Linux (x86_64-musl)..."
	@./scripts/build/cross-build-linux.sh release

build-linux-debug:
	@echo "🐧 交叉编译到 Linux (x86_64-musl, debug)..."
	@./scripts/build/cross-build-linux.sh debug

package:
	@echo "📦 打包发布版本..."
	@./scripts/build/package-release.sh

test: test-backend

test-backend:
	cd backend && cargo test
