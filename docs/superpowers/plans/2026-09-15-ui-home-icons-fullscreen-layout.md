# 主页/图标/全屏/分区布局 UI优化 实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 为 Collector 新增科技感主页、优化侧边栏图标、实现视频物理全屏、并将文件夹/文件分区展示。

**Architecture:** 纯前端（`src/`）改动 + 一条 Tauri 窗口权限。vanilla-ts + Tauri 2.x，无组件框架。视图为返回 `HTMLElement` 的工厂函数，路由用 `src/lib/router.ts` 的观察者模式。

**Tech Stack:** TypeScript、Vite 8、Tauri 2（`@tauri-apps/api/window`）、内联 SVG 图标、CSS（theme.css / animations.css）。

**验证方式（重要）：** 本项目**无前端单测框架**（历史 plans 亦无单测），且本次全部为 UI/视觉/交互改动。故每个任务的验证循环为：`npm run build`（tsc 类型检查 + vite 构建必须通过）→ 用 `preview_*` 工具在真实 dev server 目视/结构验证 → 提交。**不写 vitest 测试**，与既有工程实践一致。

---

## 文件结构

**新建：**
- `src/views/HomeView.ts` — 主页视图：拉取三类计数、渲染统计卡与背景、显示版本号。

**修改：**
- `src/lib/router.ts` — 新增 `"home"` 路由，默认路由改为 home。
- `src/lib/icons.ts` — 新增 `home`/`film`/`toolbox`/`brand` 图标，`IconName` 扩展。
- `src/components/Sidebar.ts` — 新增主页菜单项、图标映射（video→film、wand→toolbox）、品牌标改用 `brand` 图标。
- `src/main.ts` — `renderRoute` 增加 `home` 分支。
- `vite.config.ts` — `define` 注入 `__APP_VERSION__`。
- `src/vite-env.d.ts` — 声明 `__APP_VERSION__` 常量类型。
- `src/views/PlayerView.ts` — 全屏切换调用 Tauri `setFullscreen`。
- `src-tauri/capabilities/default.json` — 增加窗口全屏权限。
- `src/components/FolderView.ts` — 目录/文件分区渲染、动态区标题、分割线。
- `src/styles/theme.css` — 主页样式、分区样式、图标相关；移除 comic-tree l2 长方形规则。
- `src/styles/animations.css` — 主页背景动画。

---

## Task 1: 注入应用版本号常量

**Files:**
- Modify: `vite.config.ts`
- Modify: `src/vite-env.d.ts`

- [ ] **Step 1: vite.config.ts 读取 package.json version 并注入**

将 `vite.config.ts` 改为：

```ts
import { defineConfig } from "vite";
// @ts-expect-error type error without @types/node package
import process from "node:process";
// @ts-expect-error type error without @types/node package
import { readFileSync } from "node:fs";
const host = process.env.TAURI_DEV_HOST;
const pkg = JSON.parse(readFileSync(new URL("./package.json", import.meta.url), "utf-8"));

// https://vite.dev/config/
export default defineConfig(() => ({
  define: {
    __APP_VERSION__: JSON.stringify(pkg.version),
  },
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host
      ? { protocol: "ws", host, port: 1421 }
      : undefined,
    watch: {
      ignored: ["**/src-tauri/**"],
    },
  },
}));
```

- [ ] **Step 2: 声明常量类型**

在 `src/vite-env.d.ts` 末尾追加：

```ts
declare const __APP_VERSION__: string;
```

- [ ] **Step 3: 构建验证**

Run: `npm run build`
Expected: 构建成功，无 TS 错误。

- [ ] **Step 4: 提交**

```bash
git add vite.config.ts src/vite-env.d.ts
git commit -m "feat: vite 注入 __APP_VERSION__ 常量"
```

---

## Task 2: 新增/替换图标（home / film / toolbox / brand）

**Files:**
- Modify: `src/lib/icons.ts`

- [ ] **Step 1: 扩展 IconName 并新增四个图标路径**

在 `src/lib/icons.ts` 的 `IconName` 联合类型追加 `"home" | "film" | "toolbox" | "brand"`：

```ts
type IconName =
  | "video" | "book" | "gamepad" | "wand" | "settings"
  | "chevronLeft" | "chevronRight" | "folder" | "tree" | "grid"
  | "play" | "pause" | "rewind" | "forward" | "fullscreen"
  | "refresh" | "arrowLeft" | "trash" | "captions" | "archive"
  | "home" | "film" | "toolbox" | "brand";
```

在 `PATHS` 对象中追加四条（放在末尾 `archive` 之后）：

```ts
  home: `<path d="M3 9.5 12 3l9 6.5"/><path d="M5 10v10a1 1 0 0 0 1 1h12a1 1 0 0 0 1-1V10"/><path d="M9 21v-6h6v6"/>`,
  film: `<rect x="2" y="4" width="20" height="16" rx="2"/><path d="M7 4v16M17 4v16M2 9h5M2 15h5M17 9h5M17 15h5M7 12h10"/>`,
  toolbox: `<rect x="2" y="7" width="20" height="14" rx="2"/><path d="M9 7V5a2 2 0 0 1 2-2h2a2 2 0 0 1 2 2v2"/><path d="M2 13h20"/><path d="M10 13v2h4v-2"/>`,
  brand: `<path d="M8.5 7.5a4.5 4.5 0 1 0 4 6.9"/><path d="M15.5 16.5a4.5 4.5 0 1 0-4-6.9"/>`,
```

> `brand` 是手绘的双环/无限环造型，单色线型，呼应 app 图标的环形标志（描摹方案 a）。若后续要更高保真，可装 potrace 精确描摹再替换此 path。

- [ ] **Step 2: 构建验证**

Run: `npm run build`
Expected: 构建成功。

- [ ] **Step 3: 用 preview 目视四个新图标**

启动 dev server（`preview_start`），在浏览器控制台注入临时预览确认图标形状可辨（或等 Task 3 侧边栏接入后一并看）。此步可与 Task 3 合并验证。

- [ ] **Step 4: 提交**

```bash
git add src/lib/icons.ts
git commit -m "feat: 新增 home/film/toolbox/brand 图标"
```

---

## Task 3: 侧边栏接入主页菜单与新图标

**Files:**
- Modify: `src/lib/router.ts`
- Modify: `src/components/Sidebar.ts`

- [ ] **Step 1: router 增加 home 路由并设为默认**

将 `src/lib/router.ts` 改为：

```ts
export type Route = "home" | "video" | "comic" | "game" | "normalize" | "settings";
type Handler = (route: Route) => void;

class Router {
  private handlers: Handler[] = [];
  current: Route = "home";
  on(h: Handler) { this.handlers.push(h); }
  go(route: Route) { this.current = route; this.handlers.forEach(h => h(route)); }
}
export const router = new Router();
```

- [ ] **Step 2: Sidebar 增加主页项、更新图标映射、品牌标改 brand 图标**

将 `src/components/Sidebar.ts` 改为：

```ts
import { router } from "../lib/router";
import type { Route } from "../lib/router";
import { icon } from "../lib/icons";

type IconName = "home" | "film" | "book" | "gamepad" | "toolbox" | "settings";
const TOP: [Route, string, IconName][] = [
  ["home", "主页", "home"],
  ["video", "影视", "film"],
  ["comic", "漫画", "book"],
  ["game", "游戏", "gamepad"],
];
const BOTTOM: [Route, string, IconName][] = [
  ["normalize", "工具箱", "toolbox"],
  ["settings", "设置", "settings"],
];

/** 切换折叠：给 #app 加/去 sidebar-collapsed 类。 */
export function toggleSidebar() {
  document.getElementById("app")!.classList.toggle("sidebar-collapsed");
}

export function Sidebar(): HTMLElement {
  const el = document.createElement("aside");
  el.className = "sidebar glass";
  const render = () => {
    const item = ([r, label, ic]: [Route, string, IconName]) =>
      `<div class="nav-item ${router.current === r ? "active" : ""}" data-route="${r}">
        ${icon(ic)}<span class="nav-label">${label}</span>
      </div>`;
    el.innerHTML =
      `<div class="brand">
         <span class="brand-mark">${icon("brand", 18)}<span class="brand-text">COLLECTOR</span></span>
         <button class="collapse-btn" title="折叠侧边栏">${icon("chevronLeft", 16)}</button>
       </div>
       <nav class="nav-top">${TOP.map(item).join("")}</nav>
       <nav class="nav-bottom">${BOTTOM.map(item).join("")}</nav>`;
    el.querySelectorAll<HTMLElement>(".nav-item").forEach(n =>
      n.onclick = () => router.go(n.dataset.route as Route));
    el.querySelector<HTMLButtonElement>(".collapse-btn")!.onclick = toggleSidebar;
  };
  router.on(render);
  render();
  return el;
}
```

- [ ] **Step 3: 品牌标布局微调样式**

在 `src/styles/theme.css` 的 `.brand-mark` 处（约第 125 行 `.brand-mark{white-space:nowrap}`）改为：

```css
.brand-mark{white-space:nowrap;display:inline-flex;align-items:center;gap:8px}
.brand-mark .icon{color:var(--accent)}
```

- [ ] **Step 4: 构建验证**

Run: `npm run build`
Expected: 构建成功（此时 main.ts 尚无 home 分支，会走 default 占位分支——正常，下个 Task 处理）。

- [ ] **Step 5: preview 目视侧边栏**

`preview_start` 启动，`preview_snapshot` 确认侧边栏出现「主页/影视/漫画/游戏/工具箱/设置」六项，`preview_screenshot` 确认图标形状：主页=房子、影视=胶片条、工具箱=工具箱、品牌标=环形。

- [ ] **Step 6: 提交**

```bash
git add src/lib/router.ts src/components/Sidebar.ts src/styles/theme.css
git commit -m "feat: 侧边栏新增主页菜单，更新影视/工具箱/品牌图标"
```

---

## Task 4: HomeView 视图（统计 + 背景 + 版本号）

**Files:**
- Create: `src/views/HomeView.ts`
- Modify: `src/main.ts`
- Modify: `src/styles/theme.css`
- Modify: `src/styles/animations.css`

- [ ] **Step 1: 创建 HomeView.ts**

新建 `src/views/HomeView.ts`：

```ts
import { api } from "../lib/ipc";
import type { MediaItem } from "../lib/ipc";

/** 主页：拉取三类素材，前端聚合细分统计，渲染科技感概览。 */
export async function HomeView(): Promise<HTMLElement> {
  const el = document.createElement("div");
  el.className = "view-enter home-view";

  // 并发拉取三类；任一失败降级为空数组，保证页面可渲染（计数显示 0）。
  const [videos, comics, games] = await Promise.all([
    api.listMedia("video").catch(() => [] as MediaItem[]),
    api.listMedia("comic").catch(() => [] as MediaItem[]),
    api.listMedia("game").catch(() => [] as MediaItem[]),
  ]);

  const countBy = (arr: MediaItem[], cat: string) =>
    arr.filter((i) => i.category === cat).length;

  const movie = countBy(videos, "电影");
  const anime = countBy(videos, "动漫");
  const tv = countBy(videos, "剧集");
  const comicN = comics.length;
  const gamePlayable = games.filter((g) => g.playable !== false).length;
  const gameDisabled = games.length - gamePlayable;

  const stat = (label: string, n: number, sub: string) => `
    <div class="home-stat">
      <div class="home-stat-label"><span class="home-stat-dot"></span>${label}</div>
      <div class="home-stat-num">${n}</div>
      <div class="home-stat-sub">${sub}</div>
    </div>`;

  el.innerHTML = `
    <div class="home-bg"><div class="home-aurora"></div></div>
    <div class="home-inner">
      <div class="home-stats">
        ${stat("影视", videos.length, `电影 ${movie} · 动漫 ${anime} · 剧集 ${tv}`)}
        ${stat("漫画", comicN, `作品数 ${comicN}`)}
        ${stat("游戏", games.length, `可用 ${gamePlayable} · 不可用 ${gameDisabled}`)}
      </div>
      <div class="home-ver">Collector&nbsp;·&nbsp;版本 <b>v${__APP_VERSION__}</b></div>
    </div>`;
  return el;
}
```

- [ ] **Step 2: main.ts 接入 home 路由分支**

在 `src/main.ts` 顶部 import 区加：

```ts
import { HomeView } from "./views/HomeView";
```

在 `renderRoute` 的 `switch` 中，`case "video"` 之前加：

```ts
    case "home": view = await HomeView(); break;
```

- [ ] **Step 3: 主页样式**

在 `src/styles/theme.css` 末尾追加：

```css
/* ===== 主页 ===== */
.home-view{position:relative;height:100%;overflow:hidden}
.home-bg{position:absolute;inset:0;background:#05070f;z-index:0}
.home-bg::before{content:"";position:absolute;inset:0;background:
  radial-gradient(circle at 20% 15%,rgba(64,120,255,.35),transparent 42%),
  radial-gradient(circle at 82% 28%,rgba(40,150,255,.26),transparent 46%),
  radial-gradient(circle at 60% 95%,rgba(0,220,200,.22),transparent 50%);
  filter:saturate(1.15)}
.home-bg::after{content:"";position:absolute;inset:0;opacity:.35;
  background-image:linear-gradient(rgba(120,160,255,.10) 1px,transparent 1px),
                  linear-gradient(90deg,rgba(120,160,255,.10) 1px,transparent 1px);
  background-size:34px 34px;
  -webkit-mask-image:radial-gradient(circle at 50% 40%,#000,transparent 78%);
  mask-image:radial-gradient(circle at 50% 40%,#000,transparent 78%)}
.home-aurora{position:absolute;top:-40%;left:-10%;width:120%;height:200%;
  background:conic-gradient(from 90deg at 50% 50%,transparent,rgba(91,140,255,.18),transparent,rgba(0,220,200,.16),transparent);
  filter:blur(30px);opacity:.7;animation:home-aurora-spin 40s linear infinite}
.home-inner{position:relative;z-index:1;height:100%;display:flex;flex-direction:column;
  justify-content:center;padding:40px;max-width:900px;margin:0 auto}
.home-stats{display:grid;grid-template-columns:repeat(3,1fr);gap:16px}
.home-stat{background:linear-gradient(160deg,rgba(120,160,255,.14),rgba(120,160,255,.04));
  border:1px solid rgba(120,160,255,.30);border-radius:14px;padding:22px 18px;
  box-shadow:0 8px 30px rgba(30,60,160,.20),inset 0 1px 0 rgba(255,255,255,.06);
  -webkit-backdrop-filter:blur(6px);backdrop-filter:blur(6px)}
.home-stat-label{font-size:13px;color:#c3cdec;letter-spacing:2px;margin-bottom:12px;
  display:flex;align-items:center;gap:8px}
.home-stat-dot{width:7px;height:7px;border-radius:50%;background:var(--accent);
  box-shadow:0 0 10px var(--accent)}
.home-stat-num{font-size:44px;font-weight:800;line-height:1;
  background:linear-gradient(180deg,#cfe0ff,#6f9cff);-webkit-background-clip:text;
  background-clip:text;color:transparent;text-shadow:0 0 24px rgba(91,140,255,.4)}
.home-stat-sub{font-size:11px;color:#8ea0c8;margin-top:12px;letter-spacing:.5px}
.home-ver{margin-top:28px;padding-top:16px;border-top:1px solid rgba(120,160,255,.15);
  text-align:center;font-size:11px;color:#6f7da8;letter-spacing:1px}
.home-ver b{color:#8fb4ff;font-weight:600}
```

- [ ] **Step 4: 背景动画**

在 `src/styles/animations.css` 末尾追加：

```css
@keyframes home-aurora-spin {
  from { transform: rotate(0deg); }
  to   { transform: rotate(360deg); }
}
```

- [ ] **Step 5: 构建验证**

Run: `npm run build`
Expected: 构建成功。

- [ ] **Step 6: preview 验证主页**

`preview_start` 后 `preview_eval` 执行 `window.location.reload()`。`preview_snapshot` 确认三张统计卡文本（影视/漫画/游戏 + 细分数字）、版本号 `v0.1.0` 显示在底部。`preview_screenshot` 确认背景辉光/网格/极光呈现、无紫色、数字发光。`preview_console_logs` 确认无报错。

- [ ] **Step 7: 提交**

```bash
git add src/views/HomeView.ts src/main.ts src/styles/theme.css src/styles/animations.css
git commit -m "feat: 新增科技感主页（细分统计 + 版本号）"
```

---

## Task 5: 视频原生全屏（铺满物理屏幕）

**Files:**
- Modify: `src/views/PlayerView.ts`
- Modify: `src-tauri/capabilities/default.json`

- [ ] **Step 1: 增加窗口全屏权限**

将 `src-tauri/capabilities/default.json` 的 `permissions` 数组改为：

```json
  "permissions": [
    "core:default",
    "core:window:allow-set-fullscreen",
    "core:window:allow-is-fullscreen",
    "opener:default",
    "dialog:default"
  ]
```

- [ ] **Step 2: PlayerView 引入 window API 并改造全屏切换**

在 `src/views/PlayerView.ts` 顶部 import 区加：

```ts
import { getCurrentWindow } from "@tauri-apps/api/window";
```

将现有 `toggleFullscreen` 函数（约在 `.fs` 按钮绑定附近）替换为：

```ts
  const setNativeFullscreen = async (on: boolean) => {
    try { await getCurrentWindow().setFullscreen(on); }
    catch (e) { console.error("[player] setFullscreen failed", e); }
  };
  const enterFullscreen = async () => {
    await setNativeFullscreen(true);
    el.classList.add("fullscreen");
    showControls();
  };
  const exitFullscreen = async () => {
    await setNativeFullscreen(false);
    el.classList.remove("fullscreen");
    showControls();
  };
  const toggleFullscreen = () => {
    if (el.classList.contains("fullscreen")) exitFullscreen();
    else enterFullscreen();
  };
```

- [ ] **Step 3: Esc 退出分支改用 exitFullscreen**

在 `onKey` 中，将原来处理 `Escape` 的分支：

```ts
    } else if (e.key === "Escape" && el.classList.contains("fullscreen")) {
      el.classList.remove("fullscreen");
      showControls();
    }
```

改为：

```ts
    } else if (e.key === "Escape" && el.classList.contains("fullscreen")) {
      e.preventDefault();
      exitFullscreen();
    }
```

- [ ] **Step 4: cleanup 时确保退出原生全屏**

在 `cleanup` 函数体内（`closed = true;` 之后）加：

```ts
    if (el.classList.contains("fullscreen")) {
      getCurrentWindow().setFullscreen(false).catch(() => {});
    }
```

- [ ] **Step 5: 构建验证**

Run: `npm run build`
Expected: 构建成功。

- [ ] **Step 6: 真机验证（需 tauri dev）**

Run: `npm run tauri:dev`
手动验证（此项依赖原生窗口，preview 的浏览器无法验证全屏，需在 Tauri 窗口内操作）：
- 打开任意视频 → 点全屏按钮 → 窗口铺满整个物理屏幕（macOS 菜单栏/Dock 隐藏）。
- 字幕正常显示（若该视频有 .ass）。
- 鼠标移动出现顶栏+播放条，静止 1.5s 隐藏并隐藏光标。
- 按 Esc 退出全屏，窗口恢复。
- 返回列表页，窗口不残留全屏。
Expected: 全部符合。若无法即时真机验证，标注为「待用户在 tauri dev 中验证」。

- [ ] **Step 7: 提交**

```bash
git add src/views/PlayerView.ts src-tauri/capabilities/default.json
git commit -m "feat: 视频全屏改用 Tauri 原生窗口全屏（铺满物理屏幕）"
```

---

## Task 6: FolderView 目录/文件分区展示

**Files:**
- Modify: `src/components/FolderView.ts`
- Modify: `src/styles/theme.css`

- [ ] **Step 1: 分区渲染 + 动态区标题 + 分割线**

在 `src/components/FolderView.ts` 的 `render` 内，将当前拼接 `folders`/`videos` 到单个 `.fv-grid` 的部分改为分区结构。

先在 `render` 顶部（`const node = ...` 之后）计算文件区标题：

```ts
    const fileLabel = kind === "comic" ? "漫画" : kind === "game" ? "游戏" : "视频";
```

保留原 `folders` 和 `videos` 两个 HTML 字符串的生成逻辑不变（仍是 `.fv-cell.fv-folder` 与 `.fv-cell.fv-video`）。将 `el.innerHTML` 的赋值改为：

```ts
    const folderCount = node.children.length;
    const fileCount = node.items.length;
    const bothPresent = folderCount > 0 && fileCount > 0;

    const folderSection = folderCount > 0 ? `
      <div class="fv-section">
        <div class="fv-sec-label">目录 <span class="fv-sec-count">${folderCount}</span></div>
        <div class="fv-grid">${folders}</div>
      </div>` : "";
    const fileSection = fileCount > 0 ? `
      <div class="fv-section">
        <div class="fv-sec-label">${fileLabel} <span class="fv-sec-count">${fileCount}</span></div>
        <div class="fv-grid">${videos}</div>
      </div>` : "";
    const divider = bothPresent ? `<div class="fv-divider"></div>` : "";

    el.innerHTML = `
      <div class="breadcrumb">${crumbs}</div>
      ${folderSection}${divider}${fileSection}`;
```

> 事件绑定（`.crumb`、`.fv-folder`、`.fv-name-editable`、`.fv-video`）保持不变——它们用 `el.querySelectorAll` 全局查找，分区后选择器仍命中。

- [ ] **Step 2: 分区与分割线样式，目录正方形，移除 comic-tree 长方形规则**

在 `src/styles/theme.css`：

移除第 327 行的规则（`comic-tree` 下二级目录竖长方形）：

```css
.comic-tree .fv-folder-l2 .fv-folder-icon{aspect-ratio:2/3}
```

保留 `.comic-tree .fv-folder-l1 .fv-folder-icon{aspect-ratio:1}`（正方形，与目标一致），或直接删除该行——因为基础 `.fv-folder-icon` 已是 `aspect-ratio:1`。为稳妥，两条 comic-tree 规则都删除，让所有目录走基础正方形样式。

在 theme.css 末尾追加分区样式：

```css
/* ===== 目录/文件分区 ===== */
.fv-section{margin-bottom:6px}
.fv-sec-label{font-size:11px;letter-spacing:2px;color:var(--text-dim);
  text-transform:uppercase;margin:6px 6px 10px;display:flex;align-items:center;gap:8px}
.fv-sec-count{background:var(--glass);color:var(--accent);border-radius:10px;
  padding:1px 8px;font-size:10px}
.fv-divider{height:1px;background:var(--border);margin:14px 6px 18px}
```

- [ ] **Step 3: 构建验证**

Run: `npm run build`
Expected: 构建成功。

- [ ] **Step 4: preview 验证分区**

`preview_start` 后进入影视页（`preview_click` 侧边栏影视项，或直接 `preview_eval` 触发路由）。用 `preview_snapshot` 确认：
- 有目录的层级：出现「目录 N」区（正方形图标）。
- 有文件的层级：出现「视频 M」区（长方形封面）。
- 两者并存的层级：中间有分割线。
- `preview_inspect` 检查 `.fv-folder-icon` 的 `aspect-ratio` 为 `1 / 1`，`.fv-video .poster-img` 为 `2 / 3`。
进入漫画页确认文件区标题为「漫画」、游戏页为「游戏」，且漫画/游戏目录图标为正方形。
`preview_screenshot` 记录效果。

- [ ] **Step 5: 提交**

```bash
git add src/components/FolderView.ts src/styles/theme.css
git commit -m "feat: 目录/文件分区展示（正方形目录+长方形文件+分割线+动态标题）"
```

---

## Task 7: 收尾整体回归

**Files:** 无新增改动，仅验证。

- [ ] **Step 1: 全量构建**

Run: `npm run build`
Expected: 通过。

- [ ] **Step 2: preview 全流程目视回归**

`preview_start`，依次验证：
- 启动默认落在主页；统计正确、版本号正确、背景无紫色。
- 侧边栏六项图标正确；折叠/展开正常。
- 影视/漫画/游戏页分区正确；面包屑、进入目录、行内重命名、右键菜单仍工作。
- `preview_console_logs`（level=error）确认无运行时错误。

- [ ] **Step 3: 真机全屏验证（tauri dev）**

Run: `npm run tauri:dev`
在 Tauri 窗口验证视频全屏铺满物理屏幕、字幕、工具栏显隐、Esc、返回不残留。（若前面 Task 5 已验证可跳过。）

- [ ] **Step 4: 无新增提交则跳过；如有微调则提交**

```bash
git add -A && git commit -m "chore: UI优化收尾回归微调"
```

---

## 自查

- **Spec 覆盖**：需求1→Task 1/3/4；需求2→Task 2/3；需求3→Task 5；需求4→Task 6。全覆盖。
- **占位符**：无 TBD/TODO；每个改动步骤均含完整代码。
- **类型一致**：`IconName` 在 icons.ts 与 Sidebar.ts 同步扩展；`Route` 新增 home 在 router.ts 与 main.ts 一致；`__APP_VERSION__` 在 vite define 与 vite-env.d.ts 一致；HomeView 使用的 `MediaItem.category`/`playable` 与现有 ipc.ts 定义一致。
- **无测试框架**：本项目无前端单测，验证以 `npm run build` + `preview_*` + tauri dev 真机（仅全屏）替代，符合既有工程实践。
