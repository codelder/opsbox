# Requirements: OpsBox E2E 测试补充

**Defined:** 2026-03-15
**Core Value:** E2E 测试必须覆盖最终用户的所有关键操作路径

## v1.1 Requirements

基于覆盖分析报告（2026-03-15），补充以下缺口测试。

### Settings CRUD (SETTINGS)

- [ ] **SETTINGS-01**: 用户可以在 S3 对象存储标签页新建 Profile（填写名称、Endpoint、Access Key、Secret Key 后保存）
- [ ] **SETTINGS-02**: 用户可以编辑已有 S3 Profile（修改 Endpoint 等字段后保存）
- [ ] **SETTINGS-03**: 用户可以删除 S3 Profile（点击删除按钮，确认后移除）
- [ ] **SETTINGS-04**: 用户可以在大模型标签页新建 LLM 后端（选择 Provider、填写 Base URL、Model 等）
- [ ] **SETTINGS-05**: 用户可以将某个 LLM 后端设为默认
- [ ] **SETTINGS-06**: 用户可以删除 LLM 后端
- [ ] **SETTINGS-07**: 用户可以在规划脚本标签页新建脚本（填写 App 名称和 Starlark 脚本）
- [ ] **SETTINGS-08**: 用户可以使用「测试」按钮验证规划脚本
- [ ] **SETTINGS-09**: 用户可以在 Agent 标签页为 Agent 添加/删除标签
- [ ] **SETTINGS-10**: 用户可以保存 Agent 的日志级别和保留天数配置

### View Page (VIEW)

- [ ] **VIEW-01**: 用户可以通过按钮增大/减小字体大小（XS/SM/MD/LG/XL 五档）
- [ ] **VIEW-02**: 用户可以点击下载按钮下载当前文件
- [ ] **VIEW-03**: 用户可以使用 Ctrl+G 快捷键跳转到文件底部
- [ ] **VIEW-04**: 用户可以使用 Ctrl+Shift+G 快捷键跳转到文件顶部
- [ ] **VIEW-05**: 用户可以使用 Ctrl+U/D 快捷键上下翻半页
- [ ] **VIEW-06**: 用户可以使用 Ctrl+B/F 快捷键上下翻整页
- [ ] **VIEW-07**: 用户滚动到底部时自动加载更多内容（虚拟滚动）

### Prompt Page (PROMPT)

- [ ] **PROMPT-01**: 用户访问 /prompt 页面可以看到查询语法文档正确渲染（Markdown → HTML）
- [ ] **PROMPT-02**: 页面包含 OR、AND、NOT、短语搜索、正则表达式、路径过滤等语法说明

### Image Viewer Mouse (IMGVIEW)

- [ ] **IMGVIEW-01**: 用户可以使用鼠标拖拽平移图片位置
- [ ] **IMGVIEW-02**: 用户可以使用鼠标滚轮缩放图片（向上放大、向下缩小）
- [ ] **IMGVIEW-03**: 用户可以点击缩略图跳转到对应图片
- [ ] **IMGVIEW-04**: 用户可以点击重置视图按钮恢复默认缩放和旋转
- [ ] **IMGVIEW-05**: 用户可以点击下载按钮下载当前图片
- [ ] **IMGVIEW-06**: 用户可以点击关闭按钮关闭标签页

### Search Details (SEARCH)

- [ ] **SEARCH-01**: 用户可以点击结果卡片上的复制路径按钮复制文件路径
- [ ] **SEARCH-02**: 用户可以点击折叠/展开按钮收起或展开单个结果卡片
- [ ] **SEARCH-03**: 用户鼠标悬停在文件路径上可以看到 ORL 元数据弹窗
- [ ] **SEARCH-04**: 用户可以拖拽侧边栏调整手柄改变侧边栏宽度（200-600px 范围）

### Explorer Details (EXPLORER)

- [ ] **EXPLORER-01**: 用户右键点击文件选择「下载」可以下载该文件
- [ ] **EXPLORER-02**: 用户可以为文件/文件夹设置颜色标签（7 种颜色可选）
- [ ] **EXPLORER-03**: 用户右键点击文件选择「属性」可以查看文件信息
- [ ] **EXPLORER-04**: 用户可以拖拽侧边栏调整手柄改变侧边栏宽度
- [ ] **EXPLORER-05**: 用户在空目录上右键可以点击「刷新」重新加载

### AI Mode Flow (AIMODE)

- [ ] **AIMODE-01**: 用户点击 AI 模式按钮后，输入自然语言查询，系统将其转换为搜索语法并跳转到搜索结果页
- [ ] **AIMODE-02**: AI 转换过程中按钮显示加载动画（彩虹边框）
- [ ] **AIMODE-03**: AI 转换失败时显示错误提示

### Theme Persistence (THEME)

- [ ] **THEME-01**: 用户在首页切换主题后，导航到搜索页主题保持一致
- [ ] **THEME-02**: 用户在搜索页切换主题后，导航到设置页主题保持一致
- [ ] **THEME-03**: 用户刷新页面后主题设置保持不变

## Out of Scope

| Feature | Reason |
|---------|--------|
| 移动端响应式布局测试 | 需要额外工具，v1.0 已排除 |
| 视觉回归测试 | 需要额外工具（如 Percy） |
| 后端 Rust 单元测试 | 本次只关注前端 E2E |
| Agent 管理完整 CRUD | 仅测试标签和日志设置交互，Agent 注册/删除已有集成测试 |

## Traceability

| Requirement | Phase | Status |
|-------------|-------|--------|
| SETTINGS-01 ~ SETTINGS-10 | 待分配 | Pending |
| VIEW-01 ~ VIEW-07 | 待分配 | Pending |
| PROMPT-01 ~ PROMPT-02 | 待分配 | Pending |
| IMGVIEW-01 ~ IMGVIEW-06 | 待分配 | Pending |
| SEARCH-01 ~ SEARCH-04 | 待分配 | Pending |
| EXPLORER-01 ~ EXPLORER-05 | 待分配 | Pending |
| AIMODE-01 ~ AIMODE-03 | 待分配 | Pending |
| THEME-01 ~ THEME-03 | 待分配 | Pending |

**Coverage:**
- v1.1 requirements: 40 total
- Mapped to phases: 0 (pending roadmap)
- Unmapped: 40

---
*Requirements defined: 2026-03-15*
*Last updated: 2026-03-15 after initial definition*
