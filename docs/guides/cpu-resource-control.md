# CPU资源控制指南

**文档版本**: v1.0  
**最后更新**: 2025-11-10

## 🎯 背景

OpsBox Agent 作为运维工具，通常与业务系统部署在同一台服务器上。为了**避免影响业务系统性能**，Agent 现在支持CPU资源控制。

## ⚠️ 问题分析

### **不限制CPU核数的风险**
- **CPU资源竞争**：Agent 使用所有CPU核心，可能抢占业务系统资源
- **内存压力**：大量并发任务可能导致内存不足
- **I/O竞争**：频繁的日志文件读取影响业务系统磁盘I/O
- **响应延迟**：业务系统响应时间增加

## 🔧 解决方案

### **1. 保守的默认策略**

Agent 现在使用**保守的CPU使用策略**：

| CPU核心数 | 工作线程数 | 说明 |
|----------|-----------|------|
| 1核 | 1个线程 | 单核系统，最小资源使用 |
| 2-4核 | 2个线程 | 中小型系统，适度使用 |
| 5-7核 | 3个线程 | 中型系统，平衡性能与资源 |
| 8核以上 | CPU核心数的一半（向上取整，最大16） | 大型系统，使用一半资源 |

**具体示例**：
| CPU核心数 | 默认线程数 | CPU使用率 |
|----------|-----------|----------|
| 1核 | 1个 | ~12% |
| 2核 | 2个 | ~25% |
| 4核 | 2个 | ~12% |
| 8核 | 3个 | ~9% |
| 16核 | 8个 | ~12% |
| 32核 | 16个 | ~12% |

### **2. 环境变量控制**

通过 `AGENT_WORKER_THREADS` 环境变量精确控制：

```bash
# 使用默认保守策略（推荐）
export AGENT_WORKER_THREADS=""  # 空值

# 手动指定线程数
export AGENT_WORKER_THREADS=2    # 使用2个工作线程
export AGENT_WORKER_THREADS=1    # 最小资源使用
export AGENT_WORKER_THREADS=8    # 高资源使用（最大16个）
```

## 📋 使用示例

### **生产环境部署**

```bash
# 保守配置（推荐）
export AGENT_WORKER_THREADS=2
export SEARCH_ROOTS="/var/log,/opt/app/logs"
export SERVER_ENDPOINT="http://opsbox-server:4000"

./opsbox-agent
```

### **资源受限环境**

```bash
# 最小资源使用
export AGENT_WORKER_THREADS=1
export SEARCH_ROOTS="/var/log"  # 只搜索系统日志

./opsbox-agent
```

### **高性能环境**

```bash
# 允许更多资源使用（8核以上系统会自动使用一半核心数）
export AGENT_WORKER_THREADS=""  # 使用默认策略
export SEARCH_ROOTS="/var/log,/opt/app/logs,/home/user/logs"

./opsbox-agent
```

## 🚀 启动脚本

### **使用提供的启动脚本**

```bash
# 使用默认保守策略
./scripts/run/run-agent.sh

# 自定义线程数
AGENT_WORKER_THREADS=2 ./scripts/run/run-agent.sh
```

### **Docker部署**

```dockerfile
# Dockerfile
FROM rust:1.90 as builder
WORKDIR /app
COPY . .
RUN cargo build --release -p opsbox-agent

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/backend/target/release/opsbox-agent /usr/local/bin/
EXPOSE 8090

# 保守的CPU使用策略
ENV AGENT_WORKER_THREADS=2
CMD ["opsbox-agent"]
```

## 📊 性能影响评估

### **资源使用对比**

| 配置 | CPU使用 | 内存使用 | 搜索性能 | 业务影响 |
|------|---------|----------|----------|----------|
| **无限制** | 100% | 高 | 最快 | ⚠️ 高影响 |
| **一半核心** | ~50% | 中 | 快 | ✅ 低影响 |
| **2线程** | ~25% | 低 | 中等 | ✅ 最小影响 |
| **1线程** | ~12% | 最低 | 慢 | ✅ 无影响 |

### **推荐配置**

| 环境类型 | 推荐线程数 | 说明 |
|----------|-----------|------|
| **生产环境** | 默认策略 | 自动适配CPU核心数 |
| **测试环境** | 2个 | 足够测试需求 |
| **资源受限** | 1个 | 最小资源占用 |
| **专用服务器** | 默认策略 | 使用一半CPU核心 |

## 🔍 监控建议

### **系统监控指标**

```bash
# CPU使用率监控
top -p $(pgrep opsbox-agent)

# 内存使用监控  
ps aux | grep opsbox-agent

# 线程数确认
ps -T -p $(pgrep opsbox-agent) | wc -l
```

### **业务系统影响监控**

- **响应时间**：监控业务系统API响应时间
- **CPU使用率**：确保业务系统CPU使用率正常
- **内存使用**：避免内存不足导致OOM
- **磁盘I/O**：监控日志读取对磁盘的影响

## ⚡ 最佳实践

### **1. 渐进式调优**

```bash
# 第一步：使用默认保守策略
export AGENT_WORKER_THREADS=""

# 第二步：监控业务系统影响
# 如果无影响，可以适当增加
export AGENT_WORKER_THREADS=2

# 第三步：根据实际需求调整
export AGENT_WORKER_THREADS=3
```

### **2. 环境隔离**

```bash
# 使用cgroups限制资源
echo $$ > /sys/fs/cgroup/cpu/agent/cgroup.procs
echo 200000 > /sys/fs/cgroup/cpu/agent/cpu.cfs_quota_us  # 限制20% CPU
```

### **3. 时间窗口控制**

```bash
# 在业务低峰期进行大量搜索
export AGENT_WORKER_THREADS=4  # 夜间使用更多资源

# 业务高峰期降低资源使用
export AGENT_WORKER_THREADS=1  # 白天最小资源使用
```

## 🎯 总结

通过CPU资源控制，OpsBox Agent 现在可以：

✅ **避免影响业务系统**：保守的默认策略  
✅ **灵活配置**：环境变量精确控制  
✅ **渐进调优**：根据实际需求调整  
✅ **监控友好**：清晰的资源使用情况  

**推荐**：生产环境使用 `AGENT_WORKER_THREADS=2`，既能保证搜索性能，又能最小化对业务系统的影响。
