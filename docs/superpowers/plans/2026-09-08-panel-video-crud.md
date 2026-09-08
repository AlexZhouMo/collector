# 面板内视频 CRUD 实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development or superpowers:executing-plans. Steps use checkbox (`- [ ]`).

**Goal:** 在视频内容面板里右键卡片增删改视频条目、设置展示图，编辑用右侧抽屉表单，展示图拷到应用目录，CRUD 直改数据库。

**Architecture:** 后端加 update/delete/create_item + import_cover（拷贝图到 app_data/covers）+ 4 command；前端加 ContextMenu、EditDrawer 组件，三个视图组件（PosterGrid/TreeView/FolderView）加 onContext 回调，VideoView 统一处理菜单/抽屉/刷新。

**Tech Stack:** Rust rusqlite/fs、Tauri command、vanilla-ts + dialog 插件。

**参考设计：** docs/superpowers/specs/2026-09-08-panel-video-crud-design.md

**当前代码事实：** PosterGrid(items, onOpen)、TreeView(root, onOpen)、FolderView(root, onOpen) 卡片 data-i 点击 onOpen。MediaItem 字段 id/kind/category/category_path/title/path/subtitle_path/cover_path/description/platform_ok/exec_path。media_item.path UNIQUE。library/mod.rs 有 list_items/replace_items/insert_one_tx。

---

## Task 1: 后端 CRUD 函数（update/delete/create_item）

**Files:** Modify src-tauri/src/library/mod.rs

- [ ] Step 1 写测试：update_item 改 title 后 list 反映；delete_item 后条目消失；create_item 返回新 id 且 list 含之。
- [ ] Step 2 跑测试确认失败。
- [ ] Step 3 实现：
  - `update_item(db, id, item: &ScannedItem)`: UPDATE media_item SET category,category_path,title,path,subtitle_path,cover_path,description WHERE id=?（用 ScannedItem 承载字段，kind 固定 video）。
  - `delete_item(db, id)`: DELETE FROM media_item WHERE id=?。
  - `create_item(db, item: &ScannedItem) -> i64`: INSERT（复用 insert_one_tx 逻辑，单独连接），返回 last_insert_rowid。
- [ ] Step 4 跑测试 PASS。
- [ ] Step 5 提交 `feat: media_item update/delete/create`

## Task 2: 展示图拷贝 cover.rs

**Files:** Create src-tauri/src/library/cover.rs; Modify src-tauri/src/library/mod.rs (mod cover)

- [ ] Step 1 写测试：import_cover 把临时图拷到 covers 目录，返回路径在 covers 下、文件存在。
- [ ] Step 2 确认失败。
- [ ] Step 3 实现 `import_cover(covers_dir: &Path, src: &str) -> AppResult<String>`：create_dir_all；目标名用 src 路径 hash + 原扩展名；std::fs::copy；返回目标绝对路径。
- [ ] Step 4 测试 PASS。
- [ ] Step 5 提交 `feat: import_cover copies image into app covers dir`

## Task 3: 后端 command + IPC

**Files:** Modify src-tauri/src/lib.rs, src/lib/ipc.ts

- [ ] Step 1 lib.rs 加 command（rename_all camelCase）：
  - `media_update(db, id, title, category, categoryPath, path, subtitlePath, coverPath, description)` → 组 ScannedItem 调 update_item。
  - `media_delete(db, id)`。
  - `media_create(db, ...同字段无id)` → create_item，返回 i64。
  - `import_cover(app, srcImage)` → 取 app_data_dir/covers 调 cover::import_cover 返回路径。
  注册进 generate_handler!。
- [ ] Step 2 ipc.ts 加 mediaUpdate/mediaDelete/mediaCreate/importCover。
- [ ] Step 3 前后端编译通过。
- [ ] Step 4 提交 `feat: media CRUD commands + import_cover`

## Task 4: ContextMenu 组件

**Files:** Create src/components/ContextMenu.ts; Modify src/styles/theme.css

- [ ] Step 1 实现 `showContextMenu(x, y, items: {label, danger?, onClick}[])`：在 (x,y) 建一个绝对定位菜单 div（glass），点项调 onClick 后关闭；点外部/Esc 关闭；同一时刻只一个。
- [ ] Step 2 CSS .context-menu/.context-item/.context-item.danger。
- [ ] Step 3 编译通过。
- [ ] Step 4 提交 `feat: ContextMenu component`

## Task 5: EditDrawer 组件

**Files:** Create src/components/EditDrawer.ts; Modify src/styles/theme.css

- [ ] Step 1 实现 `openEditDrawer(item | null, onSaved)`：右侧滑入抽屉表单，字段=标题输入/分类下拉(电影/动漫/剧集)/category_path输入/视频文件(显示+选按钮 dialog open)/字幕(选)/展示图(选→importCover→预览)/简介 textarea；「保存」→ item 有 id 调 mediaUpdate 否则 mediaCreate → onSaved()；「取消」关闭。item=null 为新增（空表单）。
- [ ] Step 2 CSS .edit-drawer（fixed right、滑入动画）/表单样式。
- [ ] Step 3 编译通过。
- [ ] Step 4 提交 `feat: EditDrawer component`

## Task 6: 视图接入右键 + 新增 + 刷新

**Files:** Modify src/components/PosterGrid.ts, TreeView.ts, FolderView.ts, src/views/VideoView.ts

- [ ] Step 1 三组件加 `onContext?: (it: MediaItem, x: number, y: number) => void` 参数；卡片 `oncontextmenu = e => { e.preventDefault(); onContext?.(item, e.clientX, e.clientY); }`。
- [ ] Step 2 VideoView：传 onContext = (it,x,y) => showContextMenu(x,y,[编辑→openEditDrawer(it,refresh)、设置展示图→dialog选图→importCover→mediaUpdate(仅cover)→refresh、删除→confirm→mediaDelete→refresh])；顶部加「+ 新增视频」按钮→openEditDrawer(null, refresh)；refresh=重新 listMedia + 重渲染当前视图。
- [ ] Step 3 前后端编译通过。
- [ ] Step 4 提交 `feat: wire context menu + add + refresh in video views`

## Task 7: 真机验证

- [ ] 重启应用。验证：右键卡片出菜单；编辑抽屉改标题/分类/展示图保存→更新；删除确认→消失；新增→空抽屉→新卡片(id递增)；设置展示图→选图→封面变、拷到 app_data/covers；改分类后移到对应 tab。

---

## 自查
1. Spec 覆盖：增(create/Task5-6)、删(delete/confirm)、改(update/抽屉)、设置展示图(import_cover+update cover)、右键菜单(Task4)、抽屉(Task5)、全字段(Task5 表单)、展示图拷贝(Task2)、CRUD直改库(Task1)——全覆盖。
2. 占位符：无。
3. 类型一致：ScannedItem 承载 update/create 字段；command camelCase(categoryPath/subtitlePath/coverPath/srcImage) ↔ IPC；MediaItem 前后端一致；onContext 回调签名 (it,x,y) 三组件一致。
