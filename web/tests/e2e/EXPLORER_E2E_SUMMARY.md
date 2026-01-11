# Explorer E2E 测试总结

## 测试改进

将 Explorer 的集成测试从直接 API 调用改为真正的端到端测试，从浏览器发起，测试完整的用户流程。

## 测试用例

### ✅ 通过的测试（6个）

1. **should list local files with correct API field name (orl not odfi)**
   - 验证前端使用正确的 API 字段名 `orl`
   - 捕获的 bug：前端曾使用错误的字段名 `odfi`

2. **should list agent root (discovery) with correct provider registration**
   - 验证 `orl://agent/` 能正确列出所有 agent
   - 捕获的 bug：OrlManager 使用 `effective_id()` 导致 key 为 `agent.localhost` 而不是 `agent.root`

3. **should list agent root directory (empty path)**
   - 验证 `orl://agent-id@agent/` 能列出 agent 的 search roots
   - 捕获的 bug：Agent 对空路径返回 404

4. **should list agent files**
   - 验证能列出 agent 的文件

5. **should download local file by clicking**
   - 验证下载本地文件功能

6. **should verify API requests use correct field names**
   - 验证所有 API 请求都使用正确的字段名

### ❌ 失败的测试（1个）

7. **should navigate through agent files by clicking**
   - 测试通过双击导航到子目录
   - **当前 bug**：点击进入 agent 的 search-roots 后，再双击进入下级目录时，agent 返回 404
   - 错误信息：`Agent 返回错误状态: 404 Not Found`
   - 这是一个真实的用户报告的 bug

## 修复的 Bug

### 1. 前端 API 字段名错误

- **文件**：`web/src/lib/modules/explorer/api.ts`
- **问题**：使用 `odfi` 而不是 `orl`
- **修复**：改为 `orl`

### 2. OrlManager key 生成错误

- **文件**：`backend/opsbox-core/src/odfs/manager.rs`
- **问题**：使用 `effective_id()` 将空 ID 映射为 "localhost"
- **修复**：使用 `endpoint_id()` 直接处理，空 ID 映射为 `.root`

### 3. ExplorerModule 未调用 with_agent_manager

- **文件**：`backend/explorer/src/lib.rs`
- **问题**：创建 ExplorerService 时未配置 AgentManager
- **修复**：在 `router()` 方法中调用 `with_agent_manager`

### 4. Agent 空路径返回 404

- **文件**：`backend/agent/src/routes.rs`
- **问题**：`handle_list_files` 不处理空路径
- **修复**：当路径为空或 `/` 时，列出所有 search roots 的内容

## 待修复的 Bug

### Agent 子目录导航 404

- **现象**：进入 agent 的 search-roots 后，双击进入下级目录时返回 404
- **测试**：`should navigate through agent files by clicking`
- **需要调查**：
  1. 前端双击后发送的 ORL 是什么
  2. Agent 收到的路径参数是什么
  3. 为什么 `resolve_directory_path` 返回错误

## 测试策略

现在的测试是真正的端到端测试：

- ✅ 从浏览器发起
- ✅ 测试真实的用户流程
- ✅ 报文由前端代码生成
- ✅ 调用真实的后端 API
- ✅ 启动真实的 agent

这比之前的 API 集成测试更接近真实用户体验，能捕获更多前后端集成问题。
