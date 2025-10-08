# S3 Profile 管理功能

## 概述

S3 Profile 管理功能允许您管理多个 S3 对象存储连接配置，每个 Profile 包含独立的 Endpoint、Access Key 和 Secret Key。

## 功能特性

### 1. Profile 管理

- ✅ **列出所有 Profiles** - 查看已配置的所有 S3 连接
- ✅ **创建 Profile** - 添加新的 S3 连接配置
- ✅ **编辑 Profile** - 修改现有 Profile 的连接信息
- ✅ **删除 Profile** - 删除不需要的 Profile（`default` profile 不可删除）

### 2. 数据迁移

- ✅ **自动迁移旧配置** - 首次启动时，旧的单一 S3 配置会自动迁移到 `default` profile
- ✅ **向后兼容** - 保留原有的 `/settings/s3` API 以支持旧版本前端
- ✅ **统一管理界面** - 前端已移除旧的“存储设置”选项卡，统一使用 Profile 管理

### 3. 前端界面

- ✅ **统一管理界面** - 设置页面使用 Profile 管理作为首页
- ✅ **可视化编辑** - 直观的列表视图和表单编辑
- ✅ **验证反馈** - 清晰的错误提示和成功消息
- ✅ **多选项卡布局** - 预留告警、通知、团队等未来功能

## API 接口

### 列出所有 Profiles

```http
GET /api/v1/logseek/profiles
```

**响应示例：**

```json
{
  "profiles": [
    {
      "profile_name": "production-logs",
      "endpoint": "http://minio.example.com:9000",
      "bucket": "app-logs",
      "access_key": "minioadmin",
      "secret_key": "minioadmin"
    },
    {
      "profile_name": "production-backups",
      "endpoint": "http://minio.example.com:9000",
      "bucket": "backups",
      "access_key": "minioadmin",
      "secret_key": "minioadmin"
    }
  ]
}
```

### 创建或更新 Profile

```http
POST /api/v1/logseek/profiles
Content-Type: application/json

{
  "profile_name": "staging-logs",
  "endpoint": "http://minio-staging.example.com:9000",
  "bucket": "app-logs",
  "access_key": "staging_key",
  "secret_key": "staging_secret"
}
```

**响应：** `204 No Content`

### 删除 Profile

```http
DELETE /api/v1/logseek/profiles/{profile_name}
```

**响应：** `204 No Content`

**限制：** `default` profile 不能被删除

## 数据库结构

### s3_profiles 表

```sql
CREATE TABLE IF NOT EXISTS s3_profiles (
    profile_name TEXT PRIMARY KEY,
    endpoint TEXT NOT NULL,
    bucket TEXT NOT NULL,
    access_key TEXT NOT NULL,
    secret_key TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);
```

### logseek_settings 表（迁移标记）

```sql
CREATE TABLE IF NOT EXISTS logseek_settings (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL,
    updated_at INTEGER NOT NULL
);
```

## 使用指南

### 1. 访问 Profile 管理页面

1. 打开浏览器访问 Opsboard
2. 点击设置图标或访问 `/settings`
3. 默认就是 **对象存储**（Profile 管理）界面

### 2. 添加新 Profile

1. 点击 **新建 Profile** 按钮
2. 填写 Profile 信息：
   - **Profile 名称**: 用于标识配置的唯一名称（如 `production-logs`, `staging-backups`）
   - **Endpoint**: S3 服务地址（如 `http://minio.example.com:9000`）
   - **Bucket**: S3 存储桶名称（如 `app-logs`, `backups`）
   - **Access Key**: S3 访问密钥
   - **Secret Key**: S3 密钥
3. 点击 **保存 Profile**

### 3. 编辑 Profile

1. 在 Profile 列表中找到要编辑的配置
2. 点击 **编辑** 按钮
3. 修改 Endpoint、Bucket、Access Key 或 Secret Key
4. 点击 **保存 Profile**

**注意：** Profile 名称不可修改

### 4. 删除 Profile

1. 在 Profile 列表中找到要删除的配置
2. 点击 **删除** 按钮
3. 确认删除操作

**注意：** `default` profile 是系统保留配置，不能删除

## FILE_URL 格式支持

Profile 功能已集成到 FILE_URL 系统中。**每个 Profile 现在包含完整的访问配置（Endpoint + Bucket + Credentials）**，使用更简洁：

### 格式说明

- **使用指定 Profile**: `s3://profile_name/path/to/object`

### 示例

```
# 使用 production-logs profile（已包含 endpoint + bucket）
s3://production-logs/2025/01/app.log

# 使用 staging-backups profile
s3://staging-backups/2025/01/backup.tar.gz

# Tar 包内文件（使用 production-logs profile）
tar.gz+s3://production-logs/archive.tar.gz:logs/app.log
```

### 设计优势

✅ **更符合实际场景**：一个 Profile = 一个具体的业务场景（如 production-logs, staging-backups）  
✅ **URL 更简洁**：不需要同时指定 profile 和 bucket  
✅ **权限隔离更清晰**：每个 Profile 对应不同的访问权限  
✅ **使用更方便**：选择 Profile = 同时确定了所有访问信息

## 未来计划

### 跨 Profile 搜索

当前搜索功能仅支持使用默认配置（原有的单一 S3 配置）。未来版本将支持：

- 🔲 搜索时选择特定 Profile
- 🔲 跨多个 Profile 并行搜索
- 🔲 搜索结果显示数据来源 Profile

## 重要改进

### 统一管理界面

**问题：**原有设计同时存在“存储设置”和“Profile 管理”两个选项卡，会导致：
- 功能重复，用户困惑
- 数据不一致风险（操作不同数据表）

**解决方案：**
- 移除了旧的“存储设置”选项卡
- 将“Profile 管理”重命名为“对象存储”
- 设置页面默认显示 Profile 管理
- 保留多选项卡布局，方便未来扩展告警、通知、团队等功能

**优势：**
- ✅ 避免功能重复和用户困惑
- ✅ 统一数据来源，避免不一致
- ✅ Profile 功能更强大，支持多配置
- ✅ 简化维护和代码复杂度

## 故障排除

### Profile 创建失败

**问题：** 保存 Profile 时提示错误

**可能原因：**
1. Profile 名称已存在
2. Endpoint 格式不正确
3. 凭证无效

**解决方案：**
- 使用唯一的 Profile 名称
- 确保 Endpoint 包含协议和端口（如 `http://host:9000`）
- 验证 Access Key 和 Secret Key 的正确性

### 旧配置未迁移

**问题：** 升级后看不到 `default` profile

**解决方案：**
1. 检查数据库中是否有 `logseek_s3_config` 表的数据
2. 查看日志确认迁移是否成功
3. 如果迁移失败，手动创建 `default` profile

## 技术细节

### 后端实现

- **语言**: Rust
- **数据库**: SQLite
- **模块**: `server/logseek/src/repository/settings.rs`
- **API 路由**: `server/logseek/src/routes.rs`

### 前端实现

- **框架**: SvelteKit 5 (Runes API)
- **状态管理**: `useProfiles` composable
- **API 客户端**: `ui/src/lib/modules/logseek/api/profiles.ts`
- **UI 组件**: `ui/src/routes/settings/ProfileManagement.svelte`

### 核心文件

#### 后端
- `server/logseek/src/repository/settings.rs` - Profile CRUD 操作
- `server/logseek/src/api/models.rs` - API 数据模型
- `server/logseek/src/routes.rs` - HTTP 路由处理
- `server/logseek/src/domain/file_url.rs` - FILE_URL 格式支持

#### 前端
- `ui/src/lib/modules/logseek/composables/useProfiles.svelte.ts` - 状态管理
- `ui/src/lib/modules/logseek/api/profiles.ts` - API 客户端
- `ui/src/lib/modules/logseek/types/index.ts` - TypeScript 类型定义
- `ui/src/routes/settings/ProfileManagement.svelte` - Profile 管理 UI
- `ui/src/routes/settings/+page.svelte` - 设置页面（集成 Profile 管理）

## 贡献

如有问题或建议，欢迎提交 Issue 或 Pull Request。
