# 面板内视频 CRUD 设计文档

- 日期：2026-09-08
- 状态：设计待复审
- 范围：子项目 2/2（前项目：初始化导入，已完成）

## Context

初始化导入子项目已把 demo 视频库结构灌进数据库。本子项目让用户在内容面板内直接管理视频条目：增、删、改、设置展示图。日常靠面板 CRUD 维护；「初始化示例库」是一次性重置操作（点了会清掉手动改动，属预期，不做合并）。

## 已确认的设计决策

1. **四项操作**：增（新增条目）、删（删除条目，带确认）、改（编辑条目）、设置展示图。
2. **交互形态**：右键卡片弹菜单（编辑 / 设置展示图 / 删除）；编辑在**右侧抽屉**表单；新增用一个「+ 新增视频」入口打开同款抽屉（空表单）。
3. **可编辑字段（全部）**：标题、分类（电影/动漫/剧集）、category_path（层级路径）、展示图、视频文件路径、字幕路径、简介。
4. **文件选择**：视频/字幕/展示图用系统文件对话框（dialog open）选。
5. **展示图存储**：选完**拷贝到应用数据目录**（如 `<app_data>/covers/`），cover_path 存拷贝后路径（原图移动/删除不影响）。
6. **CRUD 直改数据库**（按 id update/delete/insert）；与「初始化示例库」重置独立。

## 架构

新增后端 CRUD command + 前端右键菜单/抽屉组件。复用现有 media_item 表、MediaItem 类型、list_media。

### 后端 `src-tauri/src/library/mod.rs`（新增 CRUD 函数）
- `update_item(db, id, fields) -> AppResult<()>`：按 id UPDATE media_item 的可编辑列（title/category/category_path/path/subtitle_path/cover_path/description）。
- `delete_item(db, id) -> AppResult<()>`：DELETE media_item WHERE id（watch_state 随 CASCADE）。
- `create_item(db, item) -> AppResult<i64>`：INSERT 一条 video，返回新 id。path 需唯一（若用户没选视频文件，用一个唯一占位如 subtitle 或时间戳）。
- 单测：update 改字段后 list 反映；delete 后消失；create 返回递增 id。

### 后端 `src-tauri/src/library/cover.rs`（新增，展示图拷贝）
- `import_cover(app_data: &Path, src_image: &str) -> AppResult<String>`：把 src_image 拷到 `<app_data>/covers/<hash>.<ext>`，返回目标路径。hash 用源路径或内容避免重名。
- 单测：拷贝后目标存在、返回路径在 covers 目录。

### 后端 command `src-tauri/src/lib.rs`
- `media_update(id, title, category, categoryPath, path, subtitlePath, coverPath, description)` — rename_all camelCase。
- `media_delete(id)`。
- `media_create(...)` 同 update 字段（无 id），返回新 id。
- `import_cover(srcImage)` — 取 app_data 拷贝，返回新 cover 路径（前端编辑/新增时先调它拿路径，再填进表单/提交）。
- 注册进 generate_handler!。

### 前端
- `src/lib/ipc.ts`：加 mediaUpdate/mediaDelete/mediaCreate/importCover。
- `src/components/ContextMenu.ts`（新增）：通用右键菜单（传菜单项 + 位置），点项回调。
- `src/components/EditDrawer.ts`（新增）：右侧抽屉表单，字段 = 标题/分类下拉/category_path/视频文件(选)/字幕(选)/展示图(选+预览)/简介；保存调 mediaUpdate 或 mediaCreate。
- `src/views/VideoView.ts` + `TreeView.ts`/`FolderView.ts`：卡片绑右键 → ContextMenu（编辑/设置展示图/删除）；编辑/新增 → EditDrawer；删除 → confirm 后 mediaDelete；操作后刷新列表（重新 listMedia + 重渲染）。顶部加「+ 新增视频」按钮开空抽屉。
- 「设置展示图」菜单项：直接 dialog 选图 → importCover → mediaUpdate 只更新 cover_path → 刷新。

## 关键文件
- `src-tauri/src/library/mod.rs`（update/delete/create_item）
- `src-tauri/src/library/cover.rs`（新增，import_cover）
- `src-tauri/src/lib.rs`（4 command）
- `src/lib/ipc.ts`（4 IPC）
- `src/components/ContextMenu.ts`、`EditDrawer.ts`（新增）
- `src/views/VideoView.ts`、`src/components/TreeView.ts`、`FolderView.ts`（右键/新增/刷新接入）
- `src/styles/theme.css`（菜单/抽屉样式）

## 明确不做（YAGNI）
- 不做批量操作/多选
- 不做拖拽排序（ID 顺序由初始化导入定）
- 不做撤销/回收站（删除即删，带确认）
- 不改漫画/游戏的管理（仅视频）
- 不做与初始化的合并（初始化是重置）

## 验证
1. 后端单测：update/delete/create、import_cover 拷贝。
2. 真机：右键视频卡片 → 菜单出现；编辑 → 抽屉改标题/分类/展示图 → 保存 → 卡片更新；删除 → 确认 → 消失；新增 → 空抽屉填信息 → 出现新卡片（id 递增）；设置展示图 → 选图 → 封面变。
3. 编辑分类/category_path 后条目移到对应 tab/层级。
4. 展示图拷到 app_data/covers，原图删除后封面仍在。

## 风险
- 低。标准 CRUD + 文件拷贝。注意 path 唯一性（新增无视频文件时的占位）、编辑 category/category_path 后前端刷新到正确 tab/层级。
