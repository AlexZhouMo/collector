# 漫画分层树形展示（仿影视）设计文档

**日期**：2026-09-11
**状态**：已确认

## 背景与动机

漫画视图当前是平铺网格（一次列出所有漫画）。改为**分层树形导航**，仿影视模块：根层展示各题材
（方形文件夹图标）→ 逐层进入 → 漫画层显示封面卡，点漫画进卷列表。支持文件夹/树形视图切换 + 面包屑。
多子系列（如 `战斗题材/圣斗士星矢`）自然形成中间层文件夹。

## 建树（复用 buildVideoTree）

漫画 `category_path` 已是完整层级路径（如 `战斗题材/圣斗士星矢`），title 是漫画名。直接复用
`buildVideoTree("", comicItems)`：传空字符串作 category（虚拟根，name/path 为空），每部漫画的
category_path 整个作为分类内相对路径逐层建树，末段目录挂该漫画 item。结果：根下是各题材文件夹 →
题材下是漫画或中间子系列目录 → 叶子是漫画 item。`buildVideoTree` 只依赖 category_path 分段 + title，
对漫画完全适用，无需改建树函数。

## 视图重构（ComicView.ts）

仿 VideoView：
- `root = buildVideoTree("", await listMedia("comic"))`。
- 顶部：文件夹/树形视图模式切换（复用 `.vt-btn` + folder/tree 图标）。**不设分类 tab**（题材作为树根层
  的文件夹，逐层进；影视的 tab 是因 category 独立列，漫画无）。
- 主体：`FolderView(root, onOpen, ...)` 或 `TreeView(root, onOpen, ...)`——直接复用影视组件。
- `onOpen(it)` = 进卷列表（现有 openComicVolumes）。
- 维护 folderPath / treeSelected 状态，refresh 保留位置（仿 VideoView）。面包屑由 FolderView 内部提供。

## FolderView 复用适配

FolderView 目录用 `fv-folder-icon`（方形圆角图标），item 用封面（cover_path）。漫画天然适用。
需确认无封面 item 的占位显示（漫画多数 cover_path 为空）——若 FolderView 未处理空封面，加 title 占位
（现有 poster-ph 或等价）。FolderView 的 rename（inline 重命名文件夹）漫画首版保留（改 category_path
级联，后端 rename_folder 逻辑漫画是否支持需确认——若不支持则漫画视图禁用 rename）。

## 路由（main.ts）

`renderRoute("comic")` 仍调 ComicView（内部变树导航）。onOpen→openComicVolumes 不变。卷列表返回 →
renderRoute("comic")（回到树，首版回根，不做位置记忆）。

## 错误处理

- 无漫画：树空，显示空状态。
- 无封面漫画：卡片显示 title 占位。

## 测试

- buildVideoTree 已有逻辑（影视用），漫画复用同函数。
- 树结构、方形文件夹图标、逐层进入、模式切换、面包屑：tsc + 预览 inspect + 真机。
- 项目无前端测试框架，交互观感靠真机。

## 影响与风险

- 仅前端：ComicView.ts 重写（复用 FolderView/TreeView/buildVideoTree）；可能 FolderView 小适配
  （空封面占位、漫画是否启用 rename）。无后端。
- **YAGNI**：漫画不做分类 tab、不做树位置记忆（返回回根）、不做漫画的移动功能（若 FolderView rename
  对漫画 category_path 级联复杂则禁用）。
