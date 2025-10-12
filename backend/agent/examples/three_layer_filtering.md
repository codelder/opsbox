# 三层过滤架构使用示例

## 概述

`opsbox-agent` 现在支持三层过滤架构，提供从粗粒度到细粒度的渐进式过滤：

1. **第一层：SearchScope** - 定义搜索范围（目录、文件、压缩包）
2. **第二层：path_filter** - 应用路径模式过滤
3. **第三层：query path:** - 最终的文件匹配

## 使用示例

### 1. 启动 Agent

```bash
# 启动 Agent，配置搜索根目录
./opsbox-agent start \
  --search-roots "/var/log,/opt/app/logs,/tmp/debug" \
  --agent-id "prod-web-01" \
  --listen-port 4001
```

### 2. 查询可用路径

```bash
# 查询 Agent 可用的子目录
curl http://localhost:4001/api/v1/paths

# 返回示例：
# ["app", "nginx", "system", "web"]
```

### 3. 三层过滤搜索示例

#### 示例1：应用日志搜索

```bash
curl -X POST http://localhost:4001/api/v1/search \
  -H "Content-Type: application/json" \
  -d '{
    "task_id": "search-001",
    "query": "error path:error.log",
    "context_lines": 3,
    "path_filter": "**/*.log",
    "scope": {
      "Directory": {
        "path": "app",
        "recursive": true
      }
    }
  }'
```

**执行流程：**
1. **SearchScope::Directory("app")** → 搜索 `/var/log/app`, `/opt/app/logs/app`, `/tmp/debug/app`
2. **path_filter("**/*.log")** → 过滤出所有 `.log` 文件
3. **query("error path:error.log")** → 最终只搜索 `error.log` 文件中的 "error" 内容

#### 示例2：特定文件搜索

```bash
curl -X POST http://localhost:4001/api/v1/search \
  -H "Content-Type: application/json" \
  -d '{
    "task_id": "search-002",
    "query": "500",
    "context_lines": 2,
    "path_filter": "**/*error*",
    "scope": {
      "Files": {
        "paths": ["app/error.log", "nginx/access.log"]
      }
    }
  }'
```

**执行流程：**
1. **SearchScope::Files** → 搜索 `/var/log/app/error.log`, `/var/log/nginx/access.log`
2. **path_filter("**/*error*")** → 只保留包含 "error" 的文件
3. **query("500")** → 搜索包含 "500" 的内容

#### 示例3：压缩包搜索

```bash
curl -X POST http://localhost:4001/api/v1/search \
  -H "Content-Type: application/json" \
  -d '{
    "task_id": "search-003",
    "query": "exception path:error.log",
    "context_lines": 5,
    "path_filter": "**/*.log",
    "scope": {
      "TarGz": {
        "path": "backup/app-2024-01-01.tar.gz"
      }
    }
  }'
```

**执行流程：**
1. **SearchScope::TarGz** → 搜索 `/var/log/backup/app-2024-01-01.tar.gz`
2. **path_filter("**/*.log")** → 在压缩包中过滤 `.log` 文件
3. **query("exception path:error.log")** → 最终只搜索 `error.log` 文件

#### 示例4：全范围搜索

```bash
curl -X POST http://localhost:4001/api/v1/search \
  -H "Content-Type: application/json" \
  -d '{
    "task_id": "search-004",
    "query": "critical",
    "context_lines": 1,
    "path_filter": "**/*.log",
    "scope": "All"
  }'
```

**执行流程：**
1. **SearchScope::All** → 搜索所有配置的根目录
2. **path_filter("**/*.log")** → 过滤出所有 `.log` 文件
3. **query("critical")** → 搜索包含 "critical" 的内容

## 路径解析策略

### 相对路径映射

Agent 会将相对路径映射到 `search_roots` 的子目录：

```bash
# 配置
--search-roots "/var/log,/opt/app/logs"

# 请求
"scope": {"Directory": {"path": "app", "recursive": true}}

# 实际搜索路径
# - /var/log/app (如果存在)
# - /opt/app/logs/app (如果存在)
```

### 智能路径解析

如果直接路径不存在，Agent 会尝试在子目录中查找：

```bash
# 如果 /var/log/app 不存在
# Agent 会查找 /var/log/*/app
# 例如：/var/log/web/app, /var/log/api/app
```

## 错误处理

### 友好的错误信息

```bash
# 如果路径不存在
curl -X POST http://localhost:4001/api/v1/search \
  -d '{"scope": {"Directory": {"path": "nonexistent", "recursive": true}}}'

# 返回错误：
# "SearchScope 解析失败: 未找到目录: nonexistent。可用的子目录: [\"app\", \"nginx\", \"system\"]"
```

### 路径过滤器错误

```bash
# 无效的 glob 模式
curl -X POST http://localhost:4001/api/v1/search \
  -d '{"path_filter": "[invalid", "scope": "All"}'

# 返回错误：
# "路径过滤器应用失败: 路径过滤器语法错误: ..."
```

## API 端点

### 1. 健康检查
```bash
curl http://localhost:4001/health
# 返回: OK
```

### 2. Agent 信息
```bash
curl http://localhost:4001/api/v1/info
# 返回 Agent 的详细信息
```

### 3. 可用路径
```bash
curl http://localhost:4001/api/v1/paths
# 返回可用的子目录列表
```

### 4. 搜索请求
```bash
curl -X POST http://localhost:4001/api/v1/search \
  -H "Content-Type: application/json" \
  -d '{...}'
```

### 5. 进度查询
```bash
curl http://localhost:4001/api/v1/progress/{task_id}
# 返回搜索进度
```

### 6. 取消搜索
```bash
curl -X POST http://localhost:4001/api/v1/cancel/{task_id}
# 取消指定的搜索任务
```

## 最佳实践

### 1. 路径配置
- 使用相对路径，环境无关
- 配置多个 `search_roots` 提高容错性
- 定期检查可用路径

### 2. 过滤策略
- 先用 `SearchScope` 缩小范围
- 再用 `path_filter` 过滤文件类型
- 最后用 `query path:` 精确匹配

### 3. 性能优化
- 避免过于宽泛的搜索范围
- 使用合适的 `path_filter` 减少文件数量
- 合理设置 `context_lines` 控制输出量

### 4. 错误处理
- 先查询可用路径再搜索
- 处理路径不存在的错误
- 监控搜索进度和状态

## 总结

三层过滤架构提供了：

✅ **清晰的分层**：每层都有明确的过滤职责
✅ **灵活的组合**：可以单独或组合使用各层过滤
✅ **用户友好**：相对路径，环境无关
✅ **错误友好**：清晰的错误信息和可用选项提示
✅ **性能优化**：渐进式过滤，减少不必要的文件处理

这种设计让搜索功能既强大又易用，是一个很好的架构设计！
