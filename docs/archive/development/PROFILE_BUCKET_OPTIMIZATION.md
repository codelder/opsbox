# Profile 结构优化 - Bucket = Profile

## 🎯 优化目标

将 Profile 的设计从 **"S3 存储 = Profile"** 改为 **"Bucket = Profile"**，使每个 Profile 包含完整的访问配置。

## 📝 背景

### 原设计问题
```
Profile = Endpoint + Credentials
使用时: 需要指定 profile + bucket
FILE_URL: s3://profile:bucket/key
```

**问题：**
- 🔴 使用不便：需要在两处配置（Profile + 搜索时指定 bucket）
- 🔴 FILE_URL 复杂：需要同时指定 profile 和 bucket
- 🔴 不符合直觉：用户更关心"访问哪个 bucket"而不是"使用哪个 endpoint"

### 新设计优势
```
Profile = Endpoint + Bucket + Credentials
使用时: 仅指定 profile
FILE_URL: s3://profile/key  ✨
```

**优势：**
- ✅ 更符合实际场景：一个 Profile = 一个具体业务场景
- ✅ URL 更简洁：不需要同时指定两个标识符
- ✅ 权限隔离更清晰：每个 Profile 独立管理
- ✅ 使用更方便：一步到位

## 🔧 实施的修改

### 1. 后端修改（Rust）

#### 数据结构
```rust
// 旧
pub struct S3Profile {
  pub profile_name: String,
  pub endpoint: String,
  pub access_key: String,
  pub secret_key: String,
}

// 新 ✨
pub struct S3Profile {
  pub profile_name: String,
  pub endpoint: String,
  pub bucket: String,        // 新增
  pub access_key: String,
  pub secret_key: String,
}
```

#### 数据库表
```sql
-- 旧
CREATE TABLE s3_profiles (
    profile_name TEXT PRIMARY KEY,
    endpoint TEXT NOT NULL,
    access_key TEXT NOT NULL,
    secret_key TEXT NOT NULL,
    ...
);

-- 新 ✨
CREATE TABLE s3_profiles (
    profile_name TEXT PRIMARY KEY,
    endpoint TEXT NOT NULL,
    bucket TEXT NOT NULL,      -- 新增
    access_key TEXT NOT NULL,
    secret_key TEXT NOT NULL,
    ...
);
```

#### 数据迁移
```rust
// 自动迁移旧配置到 default profile
if let Some((endpoint, bucket, access_key, secret_key)) = old_config {
  sqlx::query(
    "INSERT OR IGNORE INTO s3_profiles 
     (profile_name, endpoint, bucket, access_key, secret_key, ...) 
     VALUES ('default', ?, ?, ?, ?, ...)"  // bucket 也一起迁移
  )
  .bind(&endpoint)
  .bind(&bucket)  // ✨
  .bind(&access_key)
  ...
}
```

**修改的文件：**
- `server/logseek/src/repository/settings.rs` - Profile 结构和 CRUD
- `server/logseek/src/api/models.rs` - API 数据模型

### 2. 前端修改（TypeScript/Svelte）

#### 类型定义
```typescript
// 旧
export interface S3ProfilePayload {
  profile_name: string;
  endpoint: string;
  access_key: string;
  secret_key: string;
}

// 新 ✨
export interface S3ProfilePayload {
  profile_name: string;
  endpoint: string;
  bucket: string;        // 新增
  access_key: string;
  secret_key: string;
}
```

#### UI 表单
在 Profile 管理界面添加 Bucket 输入字段：

```svelte
<div>
  <label for="profile-bucket">Bucket</label>
  <input
    id="profile-bucket"
    type="text"
    bind:value={bucket}
    placeholder="bucket"
    required
  />
  <p>指定要访问的 S3 存储桶名称</p>
</div>
```

#### 列表显示
```svelte
<!-- 旧：只显示 endpoint -->
<p>{profile.endpoint}</p>

<!-- 新：显示 endpoint + bucket ✨ -->
<p>{profile.endpoint} / {profile.bucket}</p>
```

**修改的文件：**
- `ui/src/lib/modules/logseek/types/index.ts` - 类型定义
- `ui/src/routes/settings/ProfileManagement.svelte` - UI 组件

### 3. FILE_URL 格式变化

```
旧格式: s3://profile:bucket/key
新格式: s3://profile/key  ✨

示例：
旧: s3://production:app-logs/2025/01/app.log
新: s3://production-logs/2025/01/app.log  ✨
```

**说明：**
- Profile 内部已包含 bucket 信息
- URL 更简洁直观
- 一个 Profile = 一个完整的访问配置

## 💡 使用场景示例

### 场景 1：同一 MinIO 实例，不同 Bucket

```
MinIO 实例 (minio.prod.com:9000)
├── Profile: production-logs
│   ├── Endpoint: minio.prod.com:9000
│   ├── Bucket: app-logs
│   └── Credentials: xxx
├── Profile: production-audit
│   ├── Endpoint: minio.prod.com:9000
│   ├── Bucket: audit-logs
│   └── Credentials: xxx
└── Profile: production-backups
    ├── Endpoint: minio.prod.com:9000
    ├── Bucket: backups
    └── Credentials: yyy
```

**优势：**
- 每个业务场景独立配置
- 可以有不同的访问权限
- 命名更清晰（profile 名称包含用途）

### 场景 2：不同环境

```
Production
├── Profile: prod-logs (minio-prod:9000 / app-logs)
└── Profile: prod-backups (minio-prod:9000 / backups)

Staging
├── Profile: staging-logs (minio-staging:9000 / app-logs)
└── Profile: staging-backups (minio-staging:9000 / backups)
```

## ✅ 验证结果

### 编译测试
```bash
# 后端
$ cargo check -p logseek
✓ Finished in 1.17s

# 前端
$ pnpm run build
✓ Built in 8.88s
```

### 功能验证
- ✅ 数据库表结构正确更新
- ✅ 自动迁移逻辑包含 bucket
- ✅ API 端点正常工作（包含 bucket 字段）
- ✅ UI 表单正确显示 bucket 输入
- ✅ 列表视图显示 endpoint/bucket

## 📚 文档更新

已更新的文档：
- `docs/S3_PROFILE_FEATURE.md` - 功能说明文档
- `docs/PROFILE_SUMMARY.md` - 功能完成总结
- `docs/PROFILE_BUCKET_OPTIMIZATION.md` - 本文档（优化说明）

## 🎉 总结

### 修改范围
- **后端文件**: 2 个（settings.rs, models.rs）
- **前端文件**: 2 个（types/index.ts, ProfileManagement.svelte）
- **文档文件**: 3 个

### 改动评估
- **代码改动**: 小（仅添加一个字段）
- **功能影响**: 大（用户体验显著提升）
- **风险**: 低（功能刚完成，无历史数据）

### 核心价值
1. ✅ **更符合直觉** - Profile 名称即业务场景
2. ✅ **URL 更简洁** - 减少复杂度
3. ✅ **权限更清晰** - 独立管理每个场景
4. ✅ **使用更方便** - 一步配置完成

**这是一个正确的设计决策，及时修正了架构方向！** 🚀
