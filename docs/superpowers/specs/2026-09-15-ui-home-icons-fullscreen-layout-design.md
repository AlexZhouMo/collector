# Collector UI 优化设计（主页 / 图标 / 全屏 / 分区布局）

日期：2026-09-15
状态：设计已与用户逐条确认，待审阅

## 概述

本设计覆盖四项前端 UI 优化，均为渲染层 / 交互层改动，仅需前端（`src/`）与少量 Tauri 权限配置改动，**不改数据库结构**：

1. 新增「主页」菜单与首页（科技感背景 + 各类目细分统计 + 版本号）。
2. 侧边栏图标评估与替换（影视、工具箱、品牌标，新增主页图标）。
3. 视频全屏改为铺满整个物理屏幕（Tauri 原生窗口全屏），保留字幕与工具栏鼠标显隐。
4. 文件夹 / 文件分区展示（目录=正方形、文件=长方形，同宽对齐，中间分割线，区标题动态）。

---

## 需求 1 · 主页

### 路由与菜单
- `Route` 类型新增 `"home"`（`src/lib/router.ts`）。
- 路由默认值从 `"video"` 改为 `"home"`，应用启动即进入主页。
- 侧边栏 `TOP` 菜单最顶部新增 `["home", "主页", "home"]`（`src/components/Sidebar.ts`）。
- `main.ts` 的 `renderRoute` 增加 `case "home": view = await HomeView(); break;`。

### 新建视图 `src/views/HomeView.ts`
- 异步拉取三类素材：`api.listMedia("video" | "comic" | "game")`，前端聚合计数。
- 统计口径（全部来自现有字段，无需改后端）：
  - **影视**：总数 = video 条目数；细分按 `category` 分组：电影 / 动漫 / 剧集各自计数。
  - **漫画**：总数 = comic 条目数，标注「作品数」。**不统计卷数**（卷数需扫 zip，代价大）。
  - **游戏**：总数 = game 条目数；按 `playable` 分：可用 / 不可用计数。
  - 任何计数为空时显示 `0`（不留白）。
- 版本号：从 `package.json` 的 `version` 注入。做法：在 `vite.config.ts` 用 `define` 注入 `__APP_VERSION__`（读 package.json version），HomeView 引用该常量。展示在页面最底部居中，形如 `Collector · 版本 v0.1.0`。

### 视觉（已确认方向）
布局 = A 方案「三张统计卡横排」，**不显示品牌名与副标题**。
- 背景：深空底 `#05070f` + 多层径向辉光（蓝 `rgba(64,120,255)` + 青 `rgba(0,220,200)`，**无紫色**）+ 淡科技网格（radial mask 渐隐）+ 缓慢旋转的 conic 极光带。
- 统计卡：玻璃拟态（半透明蓝底 + 蓝色描边 + 内高光 + 投影 + backdrop-blur），大号数字用蓝色线性渐变文字 + 发光。
- 版本号：底部居中，上方一条分隔线。
- 复用现有 `--accent` 等主题变量；新样式集中写在 `theme.css` 的 `.home-*` 命名空间下。
- 背景动画走 `animations.css`，遵循克制原则（低速、低透明度），避免喧宾夺主。

---

## 需求 2 · 图标

图标定义集中在 `src/lib/icons.ts` 的 `PATHS`。改动如下：

| 菜单/位置 | 现状 | 处理 |
|---|---|---|
| 主页（新增） | — | 新增 `home` 图标：标准「房子」（含门），线型 |
| 影视 | 纯播放三角（与 `play` 撞） | 新增/替换 `film`（胶片/影片条），菜单改用 `film` |
| 漫画 | 书本 | 保留 `book` |
| 游戏 | 手柄 | 保留 `gamepad` |
| 工具箱 | 魔杖 `wand` | 改用新增 `toolbox`：经典手提工具箱（提手 + 开合缝），线型 |
| 设置 | 齿轮 | 保留 `settings` |
| 品牌标 | `◈` 字符 | 换成 app 图标的**单色矢量 SVG** |

### 品牌标（单色矢量）实现说明
- 来源：`src-tauri/icons/icon.png`（蓝黄双色环形标志）。
- 目标：单色矢量 SVG，用 `currentColor` 渲染，与其它菜单图标同一线型体系。
- **注意**：系统当前无 potrace / imagemagick，无法即时自动描摹。落地方案（实现阶段二选一）：
  - **方案 a（推荐）**：手工绘制一个匹配该 logo 造型（双环 / 无限环）的 SVG 路径，纳入 `icons.ts` 作为 `brand` 图标。可控、无外部依赖、单色统一。
  - **方案 b**：安装 potrace（`brew install potrace`）+ 先将 png 转 pbm，描摹出 SVG path 后清理，再纳入 `icons.ts`。保真度高但引入构建外的手工步骤。
  - 实现时先按 a 出一版，若造型还原度不满意再走 b。
- 侧边栏 `brand-mark` 区：`◈ COLLECTOR` 改为 `<brand svg> COLLECTOR`。

### 类型与调用
- `icons.ts` 的 `IconName` 增加 `"home" | "film" | "toolbox" | "brand"`。
- `Sidebar.ts` 的 `IconName` 局部类型与 `TOP`/`BOTTOM` 映射同步更新（video→film、wand→toolbox、新增 home、brand）。

---

## 需求 3 · 视频全屏铺满物理屏幕

### 方案：Tauri 原生窗口全屏
- 现状：`PlayerView.ts` 的 `toggleFullscreen` 只切换 `.player-view` 的 `fullscreen` CSS 类（`position:fixed;inset:0`），仅铺满**应用窗口**。
- 改为：在切换 CSS 类的同时，调用 Tauri `getCurrentWindow().setFullscreen(true/false)`，使整个应用窗口进入 OS 级全屏（macOS 隐藏菜单栏/Dock，Windows 隐藏任务栏），从而 CSS 铺满窗口 = 铺满物理屏幕。

### 具体改动
- `src/lib/ipc.ts`（或 PlayerView 内直接 import）：引入 `import { getCurrentWindow } from "@tauri-apps/api/window"`。
- `toggleFullscreen`：
  - 进入：`await getCurrentWindow().setFullscreen(true)` → 加 `fullscreen` 类 → `showControls()`（启动 1.5s 自动隐藏）。
  - 退出：`await getCurrentWindow().setFullscreen(false)` → 去 `fullscreen` 类 → 常显控件。
- `Esc` 退出全屏分支：同样调用 `setFullscreen(false)`。
- 离开播放器 / `cleanup`：若仍处于全屏，确保 `setFullscreen(false)`，避免退回列表页仍是全屏窗口。
- 字幕：libass canvas 覆盖在 `.player-video` 上，随 video 尺寸定位；窗口全屏后 video 铺满，字幕自动跟随，无需额外处理。**保留现有 `SubtitleRenderer` 逻辑不动。**
- 工具栏鼠标显隐：完全复用现有 `showControls` / `onMouseMove` / `controls-hidden` 逻辑（鼠标动即现、静止 1.5s 隐 + 隐藏光标）。

### 权限
- `src-tauri/capabilities/default.json` 的 `permissions` 增加窗口全屏权限：`core:window:allow-set-fullscreen`（如需读取状态再加 `core:window:allow-is-fullscreen`）。

### 边界
- WKWebView 元素级 Fullscreen API 不稳定（代码原注释已述），故**不采用** `requestFullscreen()`；原生窗口全屏是更稳的跨平台方案。

---

## 需求 4 · 文件夹 / 文件分区布局

### 目标（已确认）
参考 OS「按类型分类」：目录区在上、文件区在下，中间分割线；**目录格与文件格同列宽对齐**（目录正方形、文件竖长方形）。区标题：目录区固定「目录」，文件区随类型动态。

### 改动 `src/components/FolderView.ts`
当前把 `folders + videos` 拼进单个 `.fv-grid`。改为渲染两个分区：

```
<div class="breadcrumb">…</div>
<div class="fv-section fv-folders">
  <div class="fv-sec-label">目录 <span class="fv-sec-count">N</span></div>
  <div class="fv-grid">…目录格（正方形）…</div>
</div>
<div class="fv-divider"></div>            <!-- 仅当两区都非空时渲染 -->
<div class="fv-section fv-files">
  <div class="fv-sec-label">{文件区标题} <span class="fv-sec-count">M</span></div>
  <div class="fv-grid">…文件格（长方形封面）…</div>
</div>
```

- **分割线条件**：仅当「目录数 > 0 且 文件数 > 0」时渲染 `.fv-divider`；只有一种时不显示分割线（但仍显示对应区标题）。
- **区标题**：
  - 目录区：固定「目录」。
  - 文件区标题按 `kind` 动态：`video → "视频"`、`comic → "漫画"`、`game → "游戏"`。`FolderView` 已有 `kind` 参数，无需改调用方签名。
- **计数**：各区标题后带数量徽标（目录 N、文件 M）。

### 样式 `src/styles/theme.css`
- 两区 `.fv-grid` 共用同一 `grid-template-columns:repeat(auto-fill,minmax(140px,1fr))`（现有值），保证列宽一致、上下对齐。
- 目录格 `.fv-folder-icon`：`aspect-ratio:1`（正方形），保留现有发光/hover。
- 文件格 `.fv-video .poster-img`：`aspect-ratio:2/3`（竖长方形），保持现状。
- **统一漫画/游戏文件夹为正方形**：移除 `comic-tree` 下 `.fv-folder-l2 .fv-folder-icon{aspect-ratio:2/3}` 规则（第 327 行），使 comic/game 的下级目录也用正方形。`fv-folder-l1/l2` 分级 class 可保留或简化，但目录图标一律正方形。
- 新增 `.fv-section`、`.fv-sec-label`、`.fv-sec-count`、`.fv-divider` 样式：分割线用**纯色实线**（如 `1px` 的 `var(--border)` 或低透明度描边色，不用渐变），标题小号大写间距、计数徽标玻璃胶囊。

### 影响面
- `VideoView` / `ComicView` / `GameView` 三处都通过同一个 `FolderView` 渲染，改一处三处生效。
- `TreeView`（树形视图）不在本次改动范围。
- 面包屑、进入文件夹、行内重命名、右键菜单等现有逻辑保持不变。

---

## 不做（YAGNI）
- 不统计漫画卷数、不统计占用空间 / 最近播放（数据库无此数据，成本高）。
- 不改树形视图布局。
- 不引入图标字体库（继续用内联 SVG，保持零依赖）。
- 不改后端表结构 / 扫描逻辑。

## 测试与验证
- 主页：三类计数正确（含 0 值）、版本号显示、背景动画不卡顿。
- 图标：侧边栏六项 + 品牌标渲染正确、折叠态正常。
- 全屏：进入铺满物理屏幕、字幕可见、工具栏鼠标显隐正常、Esc 退出、返回列表不残留全屏；macOS 与 Windows 均验证。
- 分区：仅目录 / 仅文件 / 两者并存三种情况分区与分割线正确；目录正方形、文件长方形、列宽对齐；漫画/游戏目录也为正方形。
- 用 `preview_*` 在真实 dev server 中逐项验证。

## 涉及文件清单
- `src/lib/router.ts`（新增 home 路由、默认路由）
- `src/components/Sidebar.ts`（新增主页菜单、图标映射）
- `src/lib/icons.ts`（新增 home/film/toolbox/brand 图标）
- `src/views/HomeView.ts`（新建）
- `src/main.ts`（home 路由分支）
- `vite.config.ts`（注入版本号常量）
- `src/views/PlayerView.ts`（原生全屏）
- `src/lib/ipc.ts` 或 PlayerView（引入 window API）
- `src-tauri/capabilities/default.json`（全屏权限）
- `src/components/FolderView.ts`（分区渲染）
- `src/styles/theme.css`（主页、分区、图标相关样式；移除 comic-tree l2 长方形规则）
- `src/styles/animations.css`（主页背景动画）
