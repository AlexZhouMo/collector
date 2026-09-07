# UI 调整设计：窗口最大化 / 侧边栏折叠 / 按钮图标 / 视频双视图

- 日期：2026-09-07
- 状态：设计已确认，待用户复审
- 范围：纯前端 + tauri.conf.json（不改 Rust 后端逻辑）

## Context

Collector 基础功能已完成（视频/漫画/游戏查看 + 标准化工具台）。本轮针对使用体验做四项 UI 调整，让界面更专业、内容区更高效：默认最大化窗口、侧边栏可折叠且图标更贴切、按钮统一字体并加图标、视频内容区支持树形/文件夹两种浏览方式（参考百度网盘）。这些改动不涉及后端数据模型——视频层级所需的 `category_path` 字段已存在于 `MediaItem`。

## 1. 默认最大化窗口

**文件**：`src-tauri/tauri.conf.json`

`app.windows[0]` 增加 `"maximized": true` 和 `"resizable": true`（resizable 默认即 true，显式写明）。保留现有 `width`/`height` 作为取消最大化后的还原尺寸。效果：应用启动即最大化，用户仍可手动还原/缩放。

## 2. 侧边栏折叠 + 贴切 SVG 图标

**文件**：`src/components/Sidebar.ts`、`src/lib/icons.ts`（新建）、`src/styles/theme.css`、`src/main.ts`

- **图标模块** `src/lib/icons.ts`：导出一组 Lucide/Feather 风格的内联 SVG 线性图标字符串（统一 `stroke="currentColor"`、`width/height` 由 CSS 控制），供侧边栏和按钮复用。侧边栏图标：视频=播放三角、漫画=书本、游戏=手柄、标准化=魔棒、设置=齿轮。另含折叠/展开箭头、文件夹、文件夹树、播放、刷新、返回、树形、网格等按钮图标。
- **折叠交互**：侧边栏顶部（brand 旁）放一个折叠按钮。折叠时侧边栏宽度过渡到 0（`width:0; overflow:hidden`，带 CSS transition），完全隐藏；主内容区自动占满。折叠后在窗口左上角显示一个小的展开按钮/热区（fixed 定位、玻璃拟态小圆钮）点击恢复。折叠状态用一个模块级变量 + body/根容器的 class 控制（如 `.sidebar-collapsed`），无需持久化（本轮 YAGNI，不存偏好）。
- **导航项**：每个 nav-item 渲染为「SVG 图标 + 文字」；文字用 `<span class="nav-label">` 包裹，便于折叠时的样式处理（虽然本方案折叠是整体隐藏，label span 仍利于对齐图标）。

## 3. 按钮字体统一 + 图标

**文件**：`src/lib/icons.ts`、`src/styles/theme.css`、各含按钮的视图（SettingsView / NormalizeView / ComicReaderView / PlayerView 等）

- **字体统一**：全局 `button` 规则已有 `font: inherit`。本轮核查所有按钮（含 `.player-bar button`、阅读器 `.reader-bar` 按钮等）确保未被局部样式覆盖字体，保持与正文一致的 system-ui/PingFang。
- **按钮加图标**：功能按钮文字前加贴切 SVG 图标，图标与文字用 `display:inline-flex; gap` 对齐。约定：
  - 「选择目录」→ 文件夹图标；「扫描/扫描视频库」→ 刷新图标；标准化「开始」→ 播放图标；阅读器/播放器「返回」→ 左箭头；播放器 播放/暂停/快进快退/全屏 → 对应控制图标（替换现有的 ⏪⏸ 等字符为 SVG，风格统一）。
- 提供一个小的按钮内容 helper（可放 icons.ts 或 escape 同级的小工具），生成 `图标 + 文字` 的 innerHTML 片段，减少重复。

## 4. 视频内容区：树形 / 文件夹双视图

**文件**：`src/views/VideoView.ts`（重构）、新建 `src/lib/videoTree.ts`（树构建）、`src/components/FolderView.ts`、`src/components/TreeView.ts`、`src/styles/theme.css`

保留顶部电影/动漫/电视剧分类 tab。选中分类后，内容区用「树形」或「文件夹」展示该分类内部层级。右上角切换按钮在两种模式间切换（同一时刻只显示一种）。纯前端，数据来自 `api.listMedia("video")` 返回的扁平 `MediaItem[]`，按 `category_path` 构建层级。

### 4.1 树构建 `src/lib/videoTree.ts`
- 输入：某分类下的 `MediaItem[]`。每个 item 的 `category_path` 形如 `电影/科幻/星球大战`（分类名为首段）。
- 把 `category_path` 去掉首段（分类名）后剩余的路径段作为该分类内的目录层级；最末层目录下挂载视频条目（叶子）。
- 输出一个树结构：`interface TreeNode { name: string; path: string; children: TreeNode[]; items: MediaItem[] }`——`children` 为子目录节点，`items` 为该目录直接包含的视频。
- 提供按 path 取节点的辅助函数，供文件夹视图定位当前层。

### 4.2 文件夹视图 `src/components/FolderView.ts`
- 输入：树 + 当前路径 + 回调（进入子目录 / 打开视频）。
- 顶部面包屑：分类名 ／ 子目录 ／ …，每段可点击跳转到该层。
- 内容区网格：先渲染当前节点的子文件夹（SVG 文件夹图标 + 名称，方形卡片），再渲染当前节点直接包含的视频海报（复用 PosterGrid 的海报样式）。两者混排在同一网格（草图已确认）。
- 点文件夹卡片 → 进入该子目录（更新当前路径重渲染）；点视频海报 → onOpen(item) 打开播放器。

### 4.3 树视图 `src/components/TreeView.ts`
- 左侧目录树：递归渲染 TreeNode，可展开/折叠（▸/▾），点节点选中。
- 右侧内容：显示选中节点的 `items`（视频海报网格，复用海报样式）+ 可选地把子目录也在右侧列出（本方案右侧只显示选中节点的视频海报，子目录靠左树导航，保持清爽）。
- 展开状态用组件内的 Set<path> 记录。

### 4.4 VideoView 重构
- 保留分类 tab + 新增右上角视图切换按钮（树形/文件夹，SVG 图标）。
- 维护当前分类、当前视图模式、（文件夹模式的）当前路径三个状态。
- 切分类 / 切视图模式时重建对应组件。VideoView 变成「状态协调 + tab/切换按钮」外壳，具体展示委托给 FolderView / TreeView，保持各文件聚焦。

## 组件边界

- `icons.ts`：纯数据（SVG 字符串字典）+ 可选 helper，无状态。
- `videoTree.ts`：纯函数（MediaItem[] → TreeNode），无 DOM，可独立单测。
- `FolderView` / `TreeView`：接收树 + 回调，返回 DOM，不直接调 IPC（数据由 VideoView 传入），可独立理解。
- `VideoView`：状态协调外壳。
- `Sidebar`：加折叠状态 + 图标，接口不变（仍导出 Sidebar()）。

## 明确不做（YAGNI）

- 不持久化侧边栏折叠状态、视图模式偏好（本轮不加设置项）。
- 不改后端 / 数据模型（`category_path` 已够用）。
- 树视图右侧不重复列子文件夹（靠左树导航）。
- 不做拖拽、多选、右键菜单等文件管理器高级交互。

## 验证

1. `npm run build` 前端编译通过；`videoTree.ts` 若加单测则 `cargo test` 无关、用前端测试或手动验证树构建正确。
2. 应用 dev 模式热重载：
   - 启动即最大化窗口，可手动还原/缩放。
   - 侧边栏图标贴切；点折叠按钮侧边栏平滑收起、内容区变宽；点展开钮恢复。
   - 各按钮字体与正文一致、文字前有贴切图标；播放器控制条图标为统一 SVG 风格。
   - 视频菜单：选分类后，右上角切换树形/文件夹；文件夹视图可面包屑导航、进入子目录、点海报播放；树视图可展开目录树、右侧看海报。
