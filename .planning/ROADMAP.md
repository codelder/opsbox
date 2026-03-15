# Roadmap: OpsBox E2E Testing

## Milestones

- ✅ **v1.0 E2E 测试断言收紧** — Phases 1-6 (shipped 2026-03-14)
- 📋 **v1.1 全面补充测试覆盖** — Phases 7-13 (current)

## Phases

<details>
<summary>✅ v1.0 E2E 测试断言收紧 (Phases 1-6) — SHIPPED 2026-03-14</summary>

- [x] Phase 1: 收紧 settings.spec.ts 断言 (1/1 plan) — completed 2026-03-14
- [x] Phase 2: 收紧 search.spec.ts 和 search_ux.spec.ts (1/1 plan) — completed 2026-03-14
- [x] Phase 3: 收紧 integration_explorer.spec.ts (1/1 plan) — completed 2026-03-14
- [x] Phase 4: 添加错误处理测试 (1/1 plan) — completed 2026-03-14
- [x] Phase 5: 添加加载状态测试 (1/1 plan) — completed 2026-03-14
- [x] Phase 6: 添加边界情况和无障碍测试 (1/1 plan) — completed 2026-03-14

</details>

- [ ] **Phase 7: Settings — S3 Profiles & LLM Backends** — 测试 S3 Profile 和 LLM 后端的完整 CRUD 操作
- [ ] **Phase 8: Settings — Planner Scripts & Agent Config** — 测试规划脚本管理和 Agent 配置交互
- [ ] **Phase 9: View Page — Controls & Keyboard Navigation** — 测试字体调节、下载、键盘快捷键和虚拟滚动
- [ ] **Phase 10: Image Viewer — Mouse Interactions** — 测试图片拖拽平移、滚轮缩放、缩略图导航和工具栏操作
- [ ] **Phase 11: Search & Explorer — Interaction Details** — 测试结果卡片交互、侧边栏调整、右键菜单和颜色标签
- [ ] **Phase 12: AI Mode — Query Conversion Flow** — 测试自然语言转搜索语法的完整流程
- [ ] **Phase 13: Theme — Cross-Page Persistence** — 测试主题切换在页面导航和刷新后保持一致

## Phase Details

### Phase 7: Settings — S3 Profiles & LLM Backends

**Goal**: Users can fully manage S3 storage profiles and LLM backend configurations through the Settings page

**Depends on**: Nothing (first v1.1 phase)

**Requirements**: SETTINGS-01, SETTINGS-02, SETTINGS-03, SETTINGS-04, SETTINGS-05, SETTINGS-06

**Success Criteria** (what must be TRUE):
1. User can create a new S3 Profile by filling name, endpoint, access key, secret key and saving
2. User can edit an existing S3 Profile and persist changes after save
3. User can delete an S3 Profile with confirmation and it is removed from the list
4. User can create a new LLM backend by selecting provider, entering base URL, model and saving
5. User can set an LLM backend as the default and it is visually marked as default
6. User can delete an LLM backend and it is removed from the list

**Plans**: TBD

### Phase 8: Settings — Planner Scripts & Agent Config

**Goal**: Users can manage planner scripts and configure agent settings through the Settings page

**Depends on**: Nothing

**Requirements**: SETTINGS-07, SETTINGS-08, SETTINGS-09, SETTINGS-10

**Success Criteria** (what must be TRUE):
1. User can create a new planner script by filling app name and Starlark script content
2. User can click the "Test" button on a planner script and see validation feedback
3. User can add and remove tags on an agent in the Agent settings tab
4. User can set and save agent log level and retention days configuration

**Plans**: TBD

### Phase 9: View Page — Controls & Keyboard Navigation

**Goal**: Users can control font size, download files, navigate via keyboard shortcuts, and auto-load content in the View page

**Depends on**: Nothing

**Requirements**: VIEW-01, VIEW-02, VIEW-03, VIEW-04, VIEW-05, VIEW-06, VIEW-07, PROMPT-01, PROMPT-02

**Success Criteria** (what must be TRUE):
1. User can increase and decrease font size using buttons cycling through five levels (XS/SM/MD/LG/XL)
2. User can click the download button to download the currently viewed file
3. User can press Ctrl+G to jump to the bottom of the file
4. User can press Ctrl+Shift+G to jump to the top of the file
5. User can press Ctrl+U/D to scroll half a page up/down
6. User can press Ctrl+B/F to scroll a full page up/down
7. User can scroll to the bottom and observe additional content auto-loading (virtual scroll)
8. User can visit /prompt and see query syntax documentation rendered as formatted HTML
9. The prompt page contains syntax explanations for OR, AND, NOT, phrase search, regex, and path filters

**Plans**: TBD

### Phase 10: Image Viewer — Mouse Interactions

**Goal**: Users can pan, zoom, navigate thumbnails, and use toolbar controls in the Image Viewer

**Depends on**: Nothing

**Requirements**: IMGVIEW-01, IMGVIEW-02, IMGVIEW-03, IMGVIEW-04, IMGVIEW-05, IMGVIEW-06

**Success Criteria** (what must be TRUE):
1. User can drag with mouse to pan the image position
2. User can scroll mouse wheel up to zoom in and down to zoom out
3. User can click a thumbnail to navigate to the corresponding image
4. User can click the reset view button to restore default zoom and rotation
5. User can click the download button to download the current image
6. User can click the close button to close the image viewer tab

**Plans**: TBD

### Phase 11: Search & Explorer — Interaction Details

**Goal**: Users can interact with search result cards, adjust sidebar width, use context menus, and manage file metadata in Explorer

**Depends on**: Nothing

**Requirements**: SEARCH-01, SEARCH-02, SEARCH-03, SEARCH-04, EXPLORER-01, EXPLORER-02, EXPLORER-03, EXPLORER-04, EXPLORER-05

**Success Criteria** (what must be TRUE):
1. User can click the copy path button on a search result card and the file path is copied
2. User can click the collapse/expand button to hide or show a single result card
3. User can hover over a file path and see an ORL metadata popup
4. User can drag the sidebar resize handle to adjust sidebar width within 200-600px range
5. User can right-click a file in Explorer and select "Download" to download it
6. User can assign a color label (from 7 options) to a file or folder
7. User can right-click a file and select "Properties" to view file information
8. User can drag the Explorer sidebar resize handle to adjust its width
9. User can right-click on an empty directory and select "Refresh" to reload content

**Plans**: TBD

### Phase 12: AI Mode — Query Conversion Flow

**Goal**: Users can enter natural language queries and have them converted to search syntax, with proper loading and error states

**Depends on**: Phase 7 (LLM backend must be configurable)

**Requirements**: AIMODE-01, AIMODE-02, AIMODE-03

**Success Criteria** (what must be TRUE):
1. User can click the AI mode button, enter a natural language query, and be redirected to search results with converted syntax
2. The AI mode button shows a loading animation (rainbow border) while converting the query
3. User sees an error message when AI query conversion fails

**Plans**: TBD

### Phase 13: Theme — Cross-Page Persistence

**Goal**: Theme selection persists across page navigation and page refreshes

**Depends on**: Nothing

**Requirements**: THEME-01, THEME-02, THEME-03

**Success Criteria** (what must be TRUE):
1. User switches theme on the home page, navigates to the search page, and the theme remains the same
2. User switches theme on the search page, navigates to the settings page, and the theme remains the same
3. User switches theme and refreshes the page, and the theme setting is preserved

**Plans**: TBD

## Progress

| Phase | Plans Complete | Status | Completed |
|-------|----------------|--------|-----------|
| 7. Settings — S3 & LLM | 0/1 | Not started | - |
| 8. Settings — Planner & Agent | 0/1 | Not started | - |
| 9. View Page — Controls & Keys | 0/1 | Not started | - |
| 10. Image Viewer — Mouse | 0/1 | Not started | - |
| 11. Search & Explorer — Details | 0/1 | Not started | - |
| 12. AI Mode — Flow | 0/1 | Not started | - |
| 13. Theme — Persistence | 0/1 | Not started | - |

---

*For milestone archive details, see `.planning/milestones/v1.0-ROADMAP.md`*
