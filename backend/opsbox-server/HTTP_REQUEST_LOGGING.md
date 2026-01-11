# HTTP 请求日志

## 概述

opsbox-server 现在会在 INFO 级别记录所有接收到的 HTTP 请求和响应的详细信息，包括方法、URI、headers、状态码和延迟。

## 日志格式

每个 HTTP 请求会产生两条日志：

1. **请求日志** - 当请求到达时记录
   ```
   INFO tower_http::trace::on_request: HTTP 请求 method=GET uri=/api/v1/agents version=HTTP/1.1 headers={"host": "localhost:3000", "user-agent": "curl/7.79.1", ...}
   ```

2. **响应日志** - 当响应发送时记录，包含状态码和延迟
   ```
   INFO tower_http::trace::on_response: HTTP 响应 status=200 latency_ms=5 headers={"content-type": "application/json", ...}
   ```

## 启用方式

### 方法 1: 环境变量（临时）

```bash
# INFO 级别已足够查看 HTTP 请求日志
RUST_LOG=info ./opsbox-server

# 或者使用 DEBUG 级别查看更多详细信息
RUST_LOG=debug ./opsbox-server
```

### 方法 2: 通过 API 动态调整（推荐）

```bash
# 设置为 INFO 级别（默认，可查看 HTTP 请求日志）
curl -X PUT http://localhost:3000/api/v1/log/config \
  -H "Content-Type: application/json" \
  -d '{"level": "info"}'

# 设置为 WARN 级别（关闭 HTTP 请求日志）
curl -X PUT http://localhost:3000/api/v1/log/config \
  -H "Content-Type: application/json" \
  -d '{"level": "warn"}'
```

### 方法 3: 通过前端界面

访问 Settings -> Server Log Settings，将日志级别设置为 "Info" 或更高级别。

## 日志示例

### 完整示例

```
2026-01-10T23:30:00.123456+08:00 INFO tower_http::trace::on_request: HTTP 请求 method=POST uri=/api/v1/search version=HTTP/1.1 headers={"host": "localhost:3000", "content-type": "application/json", "content-length": "156", "accept": "*/*"}

2026-01-10T23:30:00.125678+08:00 INFO tower_http::trace::on_response: HTTP 响应 status=200 latency_ms=2 headers={"content-type": "application/json", "content-length": "1024"}
```

### 静态资源请求

```
2026-01-10T23:30:01.234567+08:00 INFO tower_http::trace::on_request: HTTP 请求 method=GET uri=/assets/index-abc123.js version=HTTP/1.1 headers={"host": "localhost:3000", "accept": "*/*"}

2026-01-10T23:30:01.235000+08:00 INFO tower_http::trace::on_response: HTTP 响应 status=200 latency_ms=0 headers={"content-type": "application/javascript", "cache-control": "public, max-age=31536000, immutable"}
```

## 记录的信息

### 请求信息
- **method**: HTTP 方法（GET, POST, PUT, DELETE 等）
- **uri**: 请求的完整 URI（包括路径和查询参数）
- **version**: HTTP 协议版本
- **headers**: 所有请求头（以 Debug 格式输出）

### 响应信息
- **status**: HTTP 状态码
- **latency_ms**: 请求处理延迟（毫秒）
- **headers**: 所有响应头（以 Debug 格式输出）

## 性能影响

- INFO 级别的日志会记录所有请求，在高流量场景下会产生大量日志
- 建议根据需要调整日志级别：
  - **开发/调试**: INFO 或 DEBUG
  - **生产环境**: WARN 或 ERROR（关闭 HTTP 请求日志）
  - **性能测试**: WARN（避免日志影响性能测试结果）

## 日志存储

日志文件存储在 `logs/` 目录下：
- `logs/server.log` - 当前日志
- `logs/server.log.YYYY-MM-DD` - 历史日志（按天轮转）

日志保留策略可通过 API 或前端界面配置。

## 隐私和安全

注意：HTTP headers 会被完整记录，可能包含敏感信息（如 Authorization、Cookie 等）。在生产环境中，建议：
1. 使用 WARN 级别关闭 HTTP 请求日志
2. 或者在需要时临时开启，调试完成后立即关闭
3. 确保日志文件的访问权限受到适当限制

