# 日志配置指南

本指南介绍 OpsBox 日志系统的配置和管理方法。

## 概述

OpsBox 使用 `tracing` 框架提供结构化日志功能，支持：

- 滚动日志文件（按日期自动滚动）
- 动态日志级别调整（无需重启）
- 自定义日志路径
- 日志保留策略
- Web UI 管理界面

## 日志级别说明

OpsBox 支持五个日志级别，从低到高依次为：

| 级别 | 说明 | 使用场景 |
|------|------|----------|
| **ERROR** | 错误信息 | 仅记录系统错误和异常情况 |
| **WARN** | 警告信息 | 记录警告和潜在问题 |
| **INFO** | 信息日志 | 记录关键操作和状态变化（推荐用于生产环境） |
| **DEBUG** | 调试信息 | 记录详细的调试信息（用于开发和问题排查） |
| **TRACE** | 追踪信息 | 记录最详细的执行追踪（仅用于深度调试） |

**推荐设置：**
- 生产环境：`INFO`
- 开发环境：`DEBUG`
- 问题排查：`DEBUG` 或 `TRACE`

⚠️ **注意**：`DEBUG` 和 `TRACE` 级别会产生大量日志，可能影响性能和磁盘空间，建议仅在需要时临时启用。

## 启动参数配置

### Server 启动参数

```bash
# 基本启动（使用默认配置）
./opsbox-server

# 指定日志目录
./opsbox-server --log-dir /var/log/opsbox

# 指定日志保留天数
./opsbox-server --log-retention 30

# 组合使用
./opsbox-server --log-dir /var/log/opsbox --log-retention 30
```

**默认配置：**
- 日志目录：`~/.opsbox/logs`
- 日志级别：`INFO`
- 日志保留：7 天

### Agent 启动参数

```bash
# 基本启动（使用默认配置）
./opsbox-agent

# 指定日志目录
./opsbox-agent --log-dir /var/log/opsbox-agent

# 指定日志保留天数
./opsbox-agent --log-retention 14

# 组合使用
./opsbox-agent --log-dir /var/log/opsbox-agent --log-retention 14
```

**默认配置：**
- 日志目录：`~/.opsbox-agent/logs`
- 日志级别：`INFO`
- 日志保留：7 天

### 环境变量配置

除了命令行参数，还可以使用 `RUST_LOG` 环境变量设置日志级别：

```bash
# 设置全局日志级别
RUST_LOG=debug ./opsbox-server

# 设置特定模块的日志级别
RUST_LOG=opsbox_server=debug,logseek=info ./opsbox-server

# 组合使用
RUST_LOG=debug ./opsbox-server --log-dir /var/log/opsbox
```

### 检索日志分层（LogSeek）

检索链路的日志按“粒度”分层，便于在生产环境默认保持低噪音、在排障时逐步打开细节：

- **任务级（INFO）**：一次搜索请求的开始/结束、每个数据源的开始/完成、关键计数与耗时
- **文件级（DEBUG）**：单文件是否命中/跳过的原因、匹配行数、编码判定/跳过等
- **细节级（TRACE）**：逐文件的缓存/管道细节、编码探测细节、逐事件/逐条目明细

常用 `RUST_LOG` 示例：

```bash
# 默认：只看任务级（推荐生产环境）
RUST_LOG=info ./opsbox-server

# 排查检索结果：打开文件级日志
RUST_LOG=info,logseek=debug ./opsbox-server

# 深度排查：打开更细的追踪（会非常多）
RUST_LOG=info,logseek=debug,logseek::service::search=trace,logseek::service::entry_stream=trace ./opsbox-server
```

## 日志文件管理

### 日志文件命名规则

日志文件按日期自动滚动，命名格式如下：

**Server 日志：**
```
~/.opsbox/logs/
├── opsbox-server.log              # 当天日志（符号链接）
├── opsbox-server.2024-01-15.log   # 历史日志
├── opsbox-server.2024-01-14.log
└── opsbox-server.2024-01-13.log
```

**Agent 日志：**
```
~/.opsbox-agent/logs/
├── opsbox-agent.log               # 当天日志（符号链接）
├── opsbox-agent.2024-01-15.log    # 历史日志
├── opsbox-agent.2024-01-14.log
└── opsbox-agent.2024-01-13.log
```

### 日志滚动策略

- **滚动时机**：每天午夜自动创建新的日志文件
- **保留策略**：自动删除超过保留天数的旧日志文件
- **文件大小**：单个日志文件大小不受限制（由日期滚动控制）

### 日志保留配置

日志保留天数决定了系统保留多少天的历史日志：

```bash
# 保留 7 天（默认）
./opsbox-server --log-retention 7

# 保留 30 天
./opsbox-server --log-retention 30

# 保留 90 天
./opsbox-server --log-retention 90
```

⚠️ **注意**：修改日志保留天数后，会在下次日志滚动时生效（即第二天午夜）。

## Web UI 管理界面

OpsBox 提供了 Web UI 来管理日志配置，无需重启服务即可动态调整。

### 访问日志管理界面

1. 打开 OpsBox Web UI：http://localhost:4000
2. 进入"设置"页面
3. 选择"Server 日志"或"Agent 管理"标签

### Server 日志设置

在"设置 > Server 日志"页面，可以配置：

- **日志级别**：从下拉菜单选择 ERROR/WARN/INFO/DEBUG/TRACE
- **日志保留**：输入保留天数（1-365）
- **日志路径**：显示当前日志目录（只读，启动时指定）

点击"保存"按钮后，日志级别会立即生效。

### Agent 日志设置

在"设置 > Agent 管理"页面，展开某个 Agent 的"日志设置"区域：

- **日志级别**：从下拉菜单选择 ERROR/WARN/INFO/DEBUG/TRACE
- **日志保留**：输入保留天数（1-365）

点击"保存"按钮后，配置会通过 Server 代理发送到 Agent。

⚠️ **注意**：
- 仅在 Agent 在线时可以修改配置
- Agent 离线时，日志设置区域会被禁用

## 动态日志级别调整

### 使用 Web UI 调整

最简单的方法是通过 Web UI 调整日志级别（见上一节）。

### 使用 API 调整

也可以直接调用 API 来调整日志级别：

**Server 日志级别：**
```bash
# 获取当前配置
curl http://localhost:4000/api/v1/log/config

# 更新日志级别
curl -X PUT http://localhost:4000/api/v1/log/level \
  -H "Content-Type: application/json" \
  -d '{"level": "debug"}'

# 更新日志保留天数
curl -X PUT http://localhost:4000/api/v1/log/retention \
  -H "Content-Type: application/json" \
  -d '{"retention_count": 30}'
```

**Agent 日志级别（通过 Server 代理）：**
```bash
# 获取 Agent 配置
curl http://localhost:4000/api/v1/agents/{agent_id}/log/config

# 更新 Agent 日志级别
curl -X PUT http://localhost:4000/api/v1/agents/{agent_id}/log/level \
  -H "Content-Type: application/json" \
  -d '{"level": "debug"}'

# 更新 Agent 日志保留天数
curl -X PUT http://localhost:4000/api/v1/agents/{agent_id}/log/retention \
  -H "Content-Type: application/json" \
  -d '{"retention_count": 14}'
```

## 日志格式

### 控制台输出格式

控制台日志使用彩色格式（如果终端支持）：

```
2024-01-15T10:30:45.123Z  INFO opsbox_server::server: Server started on 127.0.0.1:4000
2024-01-15T10:30:46.456Z DEBUG logseek::service::search: Executing search query="error" limit=100
2024-01-15T10:30:47.789Z  WARN opsbox_server::daemon: Failed to connect to agent agent_id="agent-1"
2024-01-15T10:30:48.012Z ERROR opsbox_server::api: Request failed error="Database connection timeout"
```

格式说明：
- 时间戳（ISO 8601 格式）
- 日志级别（带颜色）
- 模块路径
- 日志消息
- 结构化字段（key=value）

### 文件输出格式

文件日志使用纯文本格式（无颜色）：

```
2024-01-15T10:30:45.123Z  INFO opsbox_server::server: Server started on 127.0.0.1:4000
2024-01-15T10:30:46.456Z DEBUG logseek::service::search: Executing search query="error" limit=100
```

## 故障排查指南

### 问题：日志文件未创建

**症状**：启动后没有生成日志文件

**可能原因：**
1. 日志目录不存在或无写入权限
2. 磁盘空间不足

**解决方法：**
```bash
# 检查日志目录权限
ls -la ~/.opsbox/logs

# 创建日志目录
mkdir -p ~/.opsbox/logs

# 检查磁盘空间
df -h ~/.opsbox/logs

# 使用自定义日志目录
./opsbox-server --log-dir /tmp/opsbox-logs
```

### 问题：日志级别修改不生效

**症状**：通过 Web UI 修改日志级别后，仍然看不到 DEBUG 日志

**可能原因：**
1. 浏览器缓存
2. API 请求失败
3. 数据库连接问题

**解决方法：**
```bash
# 1. 检查 API 响应
curl http://localhost:4000/api/v1/log/config

# 2. 查看 Server 日志确认是否有错误
tail -f ~/.opsbox/logs/opsbox-server.log

# 3. 重启服务（最后手段）
./opsbox-server stop
./opsbox-server start
```

### 问题：日志文件过大

**症状**：单个日志文件占用大量磁盘空间

**可能原因：**
1. 日志级别设置为 DEBUG 或 TRACE
2. 高频日志输出
3. 日志保留天数过多

**解决方法：**
```bash
# 1. 降低日志级别
curl -X PUT http://localhost:4000/api/v1/log/level \
  -H "Content-Type: application/json" \
  -d '{"level": "info"}'

# 2. 减少日志保留天数
curl -X PUT http://localhost:4000/api/v1/log/retention \
  -H "Content-Type: application/json" \
  -d '{"retention_count": 7}'

# 3. 手动清理旧日志
rm ~/.opsbox/logs/opsbox-server.2024-01-*.log
```

### 问题：Agent 日志配置无法修改

**症状**：在 Web UI 中无法修改 Agent 日志配置

**可能原因：**
1. Agent 离线
2. Agent 网络不可达
3. Agent 版本不兼容

**解决方法：**
```bash
# 1. 检查 Agent 状态
curl http://localhost:4000/api/v1/agents

# 2. 直接访问 Agent API（如果可达）
curl http://<agent-host>:<agent-port>/api/v1/log/config

# 3. 检查 Agent 日志
ssh <agent-host>
tail -f ~/.opsbox-agent/logs/opsbox-agent.log

# 4. 重启 Agent
./opsbox-agent stop
./opsbox-agent start
```

### 问题：日志中出现大量 WARN 或 ERROR

**症状**：日志文件中频繁出现警告或错误信息

**排查步骤：**

1. **查看具体错误信息**
   ```bash
   # 过滤 ERROR 日志
   grep "ERROR" ~/.opsbox/logs/opsbox-server.log
   
   # 过滤 WARN 日志
   grep "WARN" ~/.opsbox/logs/opsbox-server.log
   ```

2. **启用 DEBUG 日志获取更多信息**
   ```bash
   curl -X PUT http://localhost:4000/api/v1/log/level \
     -H "Content-Type: application/json" \
     -d '{"level": "debug"}'
   ```

3. **检查系统资源**
   ```bash
   # 检查磁盘空间
   df -h
   
   # 检查内存使用
   free -h
   
   # 检查进程状态
   ps aux | grep opsbox
   ```

4. **查看完整日志上下文**
   ```bash
   # 实时查看日志
   tail -f ~/.opsbox/logs/opsbox-server.log
   
   # 查看最近 100 行
   tail -n 100 ~/.opsbox/logs/opsbox-server.log
   ```

### 问题：日志输出到控制台但不输出到文件

**症状**：控制台能看到日志，但日志文件为空或不存在

**可能原因：**
1. 日志目录权限问题
2. 磁盘空间不足
3. 文件系统错误

**解决方法：**
```bash
# 检查日志目录
ls -la ~/.opsbox/logs

# 检查磁盘空间
df -h ~/.opsbox

# 检查文件系统错误
dmesg | grep -i error

# 使用 strace 追踪文件操作（高级）
strace -e trace=open,write ./opsbox-server 2>&1 | grep log
```

## 最佳实践

### 生产环境配置建议

```bash
# 推荐配置
./opsbox-server \
  --log-dir /var/log/opsbox \
  --log-retention 30

# 使用 systemd 管理（推荐）
# 见下一节
```

### 日志级别使用建议

- **生产环境**：使用 `INFO` 级别，记录关键操作和状态变化
- **预发布环境**：使用 `DEBUG` 级别，便于发现潜在问题
- **开发环境**：使用 `DEBUG` 或 `TRACE` 级别，获取详细信息
- **问题排查**：临时启用 `DEBUG` 或 `TRACE`，排查完成后恢复 `INFO`

### 日志保留策略建议

- **开发环境**：7 天（默认）
- **生产环境**：30-90 天（根据合规要求）
- **高负载系统**：7-14 天（避免磁盘占用过多）

### 日志监控建议

1. **定期检查日志文件大小**
   ```bash
   du -sh ~/.opsbox/logs
   ```

2. **监控磁盘空间**
   ```bash
   df -h ~/.opsbox
   ```

3. **设置日志告警**（使用外部工具）
   - 监控 ERROR 日志数量
   - 监控磁盘空间使用率
   - 监控日志文件大小

4. **定期审查日志内容**
   - 检查是否有异常错误
   - 检查是否有性能问题
   - 检查是否有安全问题

## 与 systemd 集成

如果使用 systemd 管理 OpsBox 服务，可以配置日志输出到 journald：

```ini
[Unit]
Description=OpsBox Server
After=network.target

[Service]
Type=simple
User=opsbox
Group=opsbox
ExecStart=/usr/local/bin/opsbox-server --log-dir /var/log/opsbox --log-retention 30
Restart=on-failure
RestartSec=5s

# 日志配置
StandardOutput=journal
StandardError=journal
SyslogIdentifier=opsbox-server

[Install]
WantedBy=multi-user.target
```

查看 systemd 日志：
```bash
# 查看实时日志
journalctl -u opsbox-server -f

# 查看最近 100 行
journalctl -u opsbox-server -n 100

# 查看特定时间范围
journalctl -u opsbox-server --since "2024-01-15 10:00:00"
```

## 相关文档

- [日志系统架构](../architecture/logging-architecture.md) - 日志系统设计文档
- [Tracing 使用指南](./tracing-usage.md) - 开发者日志使用指南
- [API 文档](../api/logging-api.md) - 日志配置 API 参考
