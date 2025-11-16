# Logging Level Audit and Optimization Recommendations

This document provides an audit of current logging usage and recommendations for optimization.

## Logging Level Guidelines

### ERROR
- **Use for**: Unrecoverable errors that prevent operation
- **Examples**: Database connection failures, critical service errors, data corruption
- **Action**: Requires immediate attention

### WARN
- **Use for**: Recoverable errors, degraded functionality, potential issues
- **Examples**: Retry attempts, fallback to defaults, deprecated API usage
- **Action**: Should be monitored but not critical

### INFO
- **Use for**: Important state changes, key operations, lifecycle events
- **Examples**: Service startup/shutdown, configuration changes, major operations
- **Action**: Normal operational visibility

### DEBUG
- **Use for**: Detailed diagnostic information useful for troubleshooting
- **Examples**: Request/response details, intermediate calculations, flow control
- **Action**: Enabled during development or troubleshooting

### TRACE
- **Use for**: Very detailed diagnostic information, step-by-step execution
- **Examples**: Loop iterations, detailed state dumps, every function call
- **Action**: Rarely needed, only for deep debugging

---

## Current Logging Audit

### 1. Database Operations (opsbox-core/src/database.rs)

**Current:**
```rust
tracing::info!("初始化数据库连接池: {}", config.url);
tracing::info!("数据库连接池初始化成功，最大连接数: {}", config.max_connections);
tracing::info!("执行 {} 模块的数据库迁移", module);
tracing::info!("{} 模块数据库迁移完成", module);
```

**Recommendation:** ✅ **Keep as INFO**
- Database initialization is a critical lifecycle event
- Migration execution is important for operational visibility
- These happen infrequently (startup only)

---

### 2. Search Operations (logseek/src/routes/search.rs, search_executor.rs)

**Current:**
```rust
tracing::info!("[Search] 开始搜索: q={}", body.q);
tracing::info!("[SearchExecutor] 开始搜索: q={}", query);
tracing::info!("[SearchExecutor] 获取到 {} 个存储源配置", sources.len());
```

**Recommendation:** ⚠️ **Move to DEBUG**
- Search operations happen frequently
- Detailed search parameters are diagnostic information
- Keep only high-level metrics at INFO

**Suggested Changes:**
```rust
// Keep at INFO for high-level metrics
tracing::info!("[Search] 搜索完成: query={}, sources={}, results={}, duration={:?}", 
    query, source_count, result_count, duration);

// Move to DEBUG for detailed parameters
tracing::debug!("[Search] 开始搜索: q={}", body.q);
tracing::debug!("[SearchExecutor] 获取到 {} 个存储源配置", sources.len());
```

---

### 3. NL2Q Operations (logseek/src/service/nl2q.rs, routes/nl2q.rs)

**Current:**
```rust
tracing::info!("NL2Q API请求: {}", body.nl);
tracing::info!("NL2Q API成功: {} -> '{}', 耗时: {:?}", body.nl, q, start.elapsed());
info!("NL2Q请求: '{}'", nl);
info!("LLM 响应耗时: {:?}，模型: {}", duration, resp.model);
info!("LLM 内容输出: '{}'", &q);
info!("LLM 内容输出: '{:?}'", resp);
info!("NL2Q生成成功: '{}'", &q);
```

**Recommendation:** ⚠️ **Consolidate and optimize**
- Too many INFO logs for a single operation
- Duplicate logging at different layers
- LLM response details should be DEBUG

**Suggested Changes:**
```rust
// Single INFO log with summary
tracing::info!("NL2Q: '{}' -> '{}' (model={}, duration={:?})", 
    body.nl, q, resp.model, duration);

// Move details to DEBUG
tracing::debug!("LLM 内容输出: '{}'", &q);
tracing::debug!("LLM 响应详情: {:?}", resp);
```

---

### 4. LLM Operations (opsbox-core/src/llm/mod.rs)

**Current:**
```rust
info!("Ollama 原始响应: {}", response_text);
info!("OpenAI 原始响应: {}", response_text);
debug!("Ollama 请求 URL: {}", self.base_chat_url);
debug!("Ollama 请求体: {}", serde_json::to_string_pretty(&body)...);
debug!("Ollama 响应状态: {}", resp.status());
```

**Recommendation:** ⚠️ **Move raw responses to DEBUG**
- Raw response text is diagnostic information
- Current DEBUG usage is appropriate
- INFO should only show high-level metrics

**Suggested Changes:**
```rust
// Move to DEBUG
tracing::debug!("Ollama 原始响应: {}", response_text);
tracing::debug!("OpenAI 原始响应: {}", response_text);

// Add INFO for summary
tracing::info!("LLM 请求完成: provider={}, model={}, tokens={}, duration={:?}",
    provider, model, token_count, duration);
```

---

### 5. Agent Operations (agent/src/lib.rs, main.rs)

**Current:**
```rust
tracing::info!("日志级别已更新为: {}", level);
tracing::info!("日志保留数量已更新为: {} 天（重启后失效）", req.retention_count);
debug!("[Wire] ← /api/v1/search 请求体: {}", s);
debug!("搜索参数: ctx={}, path_filter_present={}, scope=...", ...);
debug!("搜索已被取消，停止处理");
debug!("向 Server 注册: {}", url);
debug!("心跳发送成功");
```

**Recommendation:** ✅ **Mostly appropriate**
- Configuration changes at INFO is correct
- Wire protocol details at DEBUG is correct
- Consider adding INFO for registration success

**Suggested Changes:**
```rust
// Add INFO for important events
tracing::info!("Agent 注册成功: server={}, agent_id={}", server, agent_id);

// Keep existing DEBUG logs
```

---

### 6. Windows Service (agent/src/daemon_windows.rs)

**Current:**
```rust
tracing::info!("收到 Windows 服务停止请求");
tracing::info!("开始注册 Windows 服务控制处理器...");
tracing::info!("服务控制处理器注册成功");
tracing::info!("设置服务状态为启动中...");
tracing::info!("启动主逻辑线程...");
tracing::info!("主逻辑线程已启动，开始执行初始化...");
tracing::info!("主逻辑正常退出");
tracing::info!("主逻辑初始化成功（线程运行中），设置服务状态为运行中");
tracing::info!("Windows 服务已成功启动并运行");
tracing::info!("服务运行中，等待停止信号...");
tracing::info!("收到停止信号，开始停止 Windows 服务...");
tracing::info!("Windows 服务已停止");
tracing::info!("OpsBox Agent Windows 服务启动中...");
tracing::info!("Agent ID: {}", config.agent_id);
tracing::info!("Agent Name: {}", config.agent_name);
tracing::info!("Server: {}", config.server_endpoint);
tracing::info!("Listen Port: {}", config.listen_port);
tracing::info!("使用 {} 个工作线程", worker_threads);
tracing::info!("收到停止信号，开始优雅关闭...");
```

**Recommendation:** ⚠️ **Reduce verbosity**
- Too many INFO logs for service lifecycle
- Some intermediate steps should be DEBUG
- Keep only key state transitions at INFO

**Suggested Changes:**
```rust
// Keep at INFO - key state transitions
tracing::info!("Windows 服务启动: agent_id={}, server={}, port={}", 
    config.agent_id, config.server_endpoint, config.listen_port);
tracing::info!("Windows 服务运行中");
tracing::info!("收到停止信号，开始优雅关闭...");
tracing::info!("Windows 服务已停止");

// Move to DEBUG - intermediate steps
tracing::debug!("开始注册 Windows 服务控制处理器...");
tracing::debug!("服务控制处理器注册成功");
tracing::debug!("设置服务状态为启动中...");
tracing::debug!("启动主逻辑线程...");
tracing::debug!("使用 {} 个工作线程", worker_threads);
```

---

### 7. Logging Configuration (opsbox-core/src/logging.rs, logging/repository.rs)

**Current:**
```rust
tracing::info!("日志配置数据库迁移完成");
tracing::info!("已更新 {} 的日志级别为 {}", component, level_str);
tracing::info!("已更新 {} 的日志保留数量为 {} 天", component, count);
tracing::info!("已为 {} 创建默认日志配置", component);
```

**Recommendation:** ✅ **Keep as INFO**
- Configuration changes are important operational events
- Happen infrequently
- Should be visible in normal operation

---

### 8. Source Planner (logseek/src/domain/source_planner/starlark_runtime.rs)

**Current:**
```rust
tracing::info!("[Planner] RAW SOURCE[{}] JSON: {}", i, j);
tracing::info!("[Planner] 脚本生成来源总数: {}", sources.len());
tracing::info!("[Planner] 来源[{}] s3 profile={} bucket={} archive={}", ...);
tracing::info!("[Planner] 来源[{}] agent id={} subpath={} scope={} filter_glob={}", ...);
tracing::info!("[Planner] 来源[{}] local root={} scope={} filter_glob={}", ...);
```

**Recommendation:** ⚠️ **Move to DEBUG**
- Detailed source configuration is diagnostic information
- Raw JSON output is definitely DEBUG level
- Only summary should be INFO

**Suggested Changes:**
```rust
// Keep at INFO - summary only
tracing::info!("[Planner] 脚本生成来源总数: {}", sources.len());

// Move to DEBUG - details
tracing::debug!("[Planner] RAW SOURCE[{}] JSON: {}", i, j);
tracing::debug!("[Planner] 来源[{}] s3 profile={} bucket={} archive={}", ...);
tracing::debug!("[Planner] 来源[{}] agent id={} subpath={} scope={} filter_glob={}", ...);
```

---

### 9. Search Service (logseek/src/service/search.rs, entry_stream.rs)

**Current:**
```rust
debug!("找到匹配: {} ({} 行)", path, merged.len());
debug!("检测到 UTF-16 LE BOM");
debug!("检测到 UTF-16 BE BOM");
debug!("检测到 UTF-8 BOM");
debug!("chardetng 检测到编码: {} ({})", ...);
debug!("开始文本搜索，上下文行数: {}, 搜索条件数: {}", ...);
debug!("文件不是文本格式，跳过搜索");
debug!("使用指定的编码: {} ({})", enc_name, enc.name());
debug!("自动检测到编码: {}", name);
debug!("无法确定文件编码，跳过搜索");
debug!("读取完成，共{}'行，开始执行搜索逻辑", lines.len());
debug!("执行文件级布尔计算，关键字出现状态: {:?}", occurs);
debug!("文件级布尔求值不满足，跳过文件");
debug!("无匹配行，跳过文件");
debug!("找到{}行匹配结果，开始生成上下文区间", matched_lines.len());
debug!("路径不匹配，跳过: {}", &meta.path);
```

**Recommendation:** ✅ **Appropriate DEBUG usage**
- All these are diagnostic details
- Useful for troubleshooting search issues
- Should remain at DEBUG level

---

### 10. S3 Repository (logseek/src/repository/s3.rs)

**Current:**
```rust
debug!("加载 S3 配置（default profile）");
debug!("验证 S3 配置有效性");
debug!("将 S3 配置写入 s3_profiles(default)");
debug!("开始验证S3连接: endpoint={}, bucket={}", ...);
debug!("加载 S3 Profile: {}", profile_name);
debug!("加载所有 S3 Profiles");
```

**Recommendation:** ✅ **Appropriate DEBUG usage**
- Repository operations are diagnostic details
- Should remain at DEBUG level
- Consider adding INFO for S3 connection success/failure

**Suggested Addition:**
```rust
// Add INFO for important events
tracing::info!("S3 连接验证成功: endpoint={}, bucket={}", endpoint, bucket);
tracing::warn!("S3 连接验证失败: endpoint={}, error={}", endpoint, error);
```

---

## Summary of Recommendations

### High Priority Changes

1. **Search Operations**: Move detailed search logs from INFO to DEBUG
   - Files: `logseek/src/routes/search.rs`, `logseek/src/service/search_executor.rs`
   - Impact: Reduce INFO log volume by ~60%

2. **NL2Q Operations**: Consolidate multiple INFO logs into single summary
   - Files: `logseek/src/service/nl2q.rs`, `logseek/src/routes/nl2q.rs`
   - Impact: Reduce INFO log volume by ~70% for NL2Q operations

3. **LLM Operations**: Move raw responses from INFO to DEBUG
   - Files: `opsbox-core/src/llm/mod.rs`
   - Impact: Reduce INFO log volume and improve readability

4. **Windows Service**: Reduce verbosity of service lifecycle logs
   - Files: `agent/src/daemon_windows.rs`
   - Impact: Reduce INFO log volume by ~50% for Windows service

5. **Source Planner**: Move detailed source info from INFO to DEBUG
   - Files: `logseek/src/domain/source_planner/starlark_runtime.rs`
   - Impact: Reduce INFO log volume by ~80% for planner operations

### Medium Priority Changes

6. **Agent Operations**: Add INFO logs for important events (registration success)
   - Files: `agent/src/main.rs`
   - Impact: Improve operational visibility

7. **S3 Repository**: Add INFO logs for connection success/failure
   - Files: `logseek/src/repository/s3.rs`
   - Impact: Improve operational visibility

### Low Priority / Already Optimal

- Database operations (opsbox-core/src/database.rs) ✅
- Logging configuration (opsbox-core/src/logging.rs) ✅
- Agent configuration changes (agent/src/lib.rs) ✅
- Search service DEBUG logs (logseek/src/service/search.rs) ✅
- S3 repository DEBUG logs (logseek/src/repository/s3.rs) ✅

---

## Expected Impact

### Before Optimization
- **INFO logs per search**: ~10-15 lines
- **INFO logs per NL2Q**: ~7-8 lines
- **INFO logs per service start**: ~15-20 lines (Windows)

### After Optimization
- **INFO logs per search**: ~1-2 lines (summary only)
- **INFO logs per NL2Q**: ~1 line (summary only)
- **INFO logs per service start**: ~4-5 lines (key transitions only)

### Overall Reduction
- **Estimated INFO log volume reduction**: 60-70%
- **Improved signal-to-noise ratio**: High
- **Better operational visibility**: Key events stand out
- **Easier troubleshooting**: DEBUG logs still available when needed

---

## Implementation Checklist

- [ ] Update search operation logs (search.rs, search_executor.rs)
- [ ] Consolidate NL2Q logs (nl2q.rs)
- [ ] Move LLM raw responses to DEBUG (llm/mod.rs)
- [ ] Reduce Windows service verbosity (daemon_windows.rs)
- [ ] Move source planner details to DEBUG (starlark_runtime.rs)
- [ ] Add INFO logs for agent registration (main.rs)
- [ ] Add INFO logs for S3 connection events (s3.rs)
- [ ] Test with different log levels
- [ ] Update documentation
- [ ] Verify no critical information lost

---

## Testing Recommendations

1. **Run with INFO level**: Verify only key operations are logged
2. **Run with DEBUG level**: Verify detailed information is available
3. **Run under load**: Verify log volume is manageable
4. **Check for missing information**: Ensure no critical events are lost
5. **Review log readability**: Ensure logs tell a clear story

---

## References

- Requirements: `.kiro/specs/tracing-logging-system/requirements.md` (需求 4.3-4.7)
- Design: `.kiro/specs/tracing-logging-system/design.md`
- Rust tracing documentation: https://docs.rs/tracing/
