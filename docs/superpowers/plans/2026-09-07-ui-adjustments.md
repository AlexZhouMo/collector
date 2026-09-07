# UI 调整实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 四项前端 UI 调整——默认最大化窗口、侧边栏折叠+SVG 图标、按钮统一字体+图标、视频内容区树形/文件夹双视图。

**Architecture:** 纯前端 + tauri.conf.json，不改 Rust 后端。新建图标模块（SVG 字典）供全局复用；侧边栏加折叠状态；视频区按现有 `category_path` 在前端构建层级树，拆成纯函数树构建 + FolderView/TreeView 两个展示组件 + VideoView 状态外壳。

**Tech Stack:** Tauri 2.x、vanilla-ts + Vite、原生 CSS（玻璃拟态）。

**参考设计：** `docs/superpowers/specs/2026-09-07-ui-adjustments-design.md`

**关键现状：**
- `src/main.ts`：`#app`（flex）→ Sidebar() + content(main)。renderRoute switch 各视图；openPlayer/openComicReader 用 detach 事件清理。
- `src/components/Sidebar.ts`：TOP/BOTTOM 数组，nav-item 用 Unicode 符号（▶▤◉⚙），render + router.on。
- `src/views/VideoView.ts`：分类 tab（电影/动漫/电视剧）+ PosterGrid。
- `MediaItem.category_path`：形如 `电影/科幻/星球大战`（首段=分类名）。
- `PosterGrid(items, onOpen)`：海报网格，可复用。CSS 类 `.poster/.poster-img/.poster-title/.poster-grid` 已在 theme.css。
- 全局 `button{font:inherit;...}` 玻璃拟态样式已存在；`.player-bar button{background:transparent;border:none;...}` 覆盖播放条按钮。
- TS 严格模式（isolatedModules，纯类型用 `import type`）。

**执行：** 单份计划，7 个任务，前 3 个独立（窗口/图标/侧边栏/按钮），后面视频双视图递进。前端无现成单测框架——纯函数 `buildVideoTree` 用一个临时的 node 断言脚本验证，其余靠 `npm run build`（tsc 类型检查）+ 应用热重载手动验证。

---

## 文件结构

```
src-tauri/tauri.conf.json        # 窗口 maximized
src/lib/icons.ts                 # 新建：SVG 图标字典 + iconText helper
src/lib/videoTree.ts             # 新建：category_path → TreeNode 纯函数
src/components/Sidebar.ts        # 折叠 + SVG 图标
src/components/FolderView.ts     # 新建：文件夹视图（面包屑+进入式）
src/components/TreeView.ts       # 新建：树视图（左树+右海报）
src/views/VideoView.ts           # 重构：分类 tab + 视图切换 + 委托
src/main.ts                      # 折叠展开钮挂载 + 折叠 class 容器
src/styles/theme.css             # 折叠/图标/文件夹/树 相关样式
各含按钮视图                      # 按钮加图标（SettingsView/NormalizeView/PlayerView/ComicReaderView）
```

---

## Task 1: 默认最大化窗口

**Files:**
- Modify: `src-tauri/tauri.conf.json`

- [ ] **Step 1: 改窗口配置**

把 `app.windows[0]` 改为（保留 width/height 作还原尺寸，加 maximized/resizable）：
```json
{
  "title": "Collector",
  "width": 1200,
  "height": 800,
  "maximized": true,
  "resizable": true
}
```
（width/height 从 800×600 提到 1200×800，作为取消最大化后的合理还原尺寸。）

- [ ] **Step 2: 验证**

Run: `cd src-tauri && cargo build 2>&1 | tail -3`（确认 conf 合法、能编译）。
Expected: 编译通过。运行时应用启动即最大化（dev 已跑则会重启窗口）。

- [ ] **Step 3: 提交**

```bash
git add src-tauri/tauri.conf.json && git commit -m "feat: launch window maximized, keep resizable"
```

---

## Task 2: SVG 图标模块

**Files:**
- Create: `src/lib/icons.ts`

- [ ] **Step 1: 写图标字典 + helper**

Create `src/lib/icons.ts`（Lucide 风格线性图标，viewBox 0 0 24 24，stroke=currentColor；尺寸由 CSS/内联 width 控制。这里给出实际 path）：
```ts
// Lucide 风格内联 SVG 图标。stroke 用 currentColor，随文字颜色。
type IconName =
  | "video" | "book" | "gamepad" | "wand" | "settings"
  | "chevronLeft" | "chevronRight" | "folder" | "tree" | "grid"
  | "play" | "pause" | "rewind" | "forward" | "fullscreen"
  | "refresh" | "arrowLeft";

const PATHS: Record<IconName, string> = {
  video: `<polygon points="5 3 19 12 5 21 5 3"/>`,
  book: `<path d="M4 19.5A2.5 2.5 0 0 1 6.5 17H20"/><path d="M6.5 2H20v20H6.5A2.5 2.5 0 0 1 4 19.5v-15A2.5 2.5 0 0 1 6.5 2z"/>`,
  gamepad: `<line x1="6" y1="12" x2="10" y2="12"/><line x1="8" y1="10" x2="8" y2="14"/><line x1="15" y1="13" x2="15.01" y2="13"/><line x1="18" y1="11" x2="18.01" y2="11"/><rect x="2" y="6" width="20" height="12" rx="2"/>`,
  wand: `<path d="M15 4V2"/><path d="M15 16v-2"/><path d="M8 9h2"/><path d="M20 9h2"/><path d="M17.8 11.8 19 13"/><path d="M15 9h.01"/><path d="M17.8 6.2 19 5"/><path d="m3 21 9-9"/><path d="M12.2 6.2 11 5"/>`,
  settings: `<circle cx="12" cy="12" r="3"/><path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 0 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-4 0v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 0 1-2.83-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1 0-4h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 0 1 2.83-2.83l.06.06a1.65 1.65 0 0 0 1.82.33H9a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 4 0v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 0 1 2.83 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82V9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1z"/>`,
  chevronLeft: `<polyline points="15 18 9 12 15 6"/>`,
  chevronRight: `<polyline points="9 18 15 12 9 6"/>`,
  folder: `<path d="M4 20h16a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-7.9a2 2 0 0 1-1.69-.9L9.6 3.9A2 2 0 0 0 7.93 3H4a2 2 0 0 0-2 2v13c0 1.1.9 2 2 2Z"/>`,
  tree: `<rect x="3" y="3" width="6" height="6" rx="1"/><rect x="15" y="15" width="6" height="6" rx="1"/><path d="M6 9v3a3 3 0 0 0 3 3h6"/><path d="M15 18H9"/>`,
  grid: `<rect x="3" y="3" width="7" height="7"/><rect x="14" y="3" width="7" height="7"/><rect x="14" y="14" width="7" height="7"/><rect x="3" y="14" width="7" height="7"/>`,
  play: `<polygon points="5 3 19 12 5 21 5 3"/>`,
  pause: `<rect x="6" y="4" width="4" height="16"/><rect x="14" y="4" width="4" height="16"/>`,
  rewind: `<polygon points="11 19 2 12 11 5 11 19"/><polygon points="22 19 13 12 22 5 22 19"/>`,
  forward: `<polygon points="13 19 22 12 13 5 13 19"/><polygon points="2 19 11 12 2 5 2 19"/>`,
  fullscreen: `<path d="M8 3H5a2 2 0 0 0-2 2v3"/><path d="M21 8V5a2 2 0 0 0-2-2h-3"/><path d="M3 16v3a2 2 0 0 0 2 2h3"/><path d="M16 21h3a2 2 0 0 0 2-2v-3"/>`,
  refresh: `<path d="M3 12a9 9 0 0 1 9-9 9.75 9.75 0 0 1 6.74 2.74L21 8"/><path d="M21 3v5h-5"/><path d="M21 12a9 9 0 0 1-9 9 9.75 9.75 0 0 1-6.74-2.74L3 16"/><path d="M8 16H3v5"/>`,
  arrowLeft: `<line x1="19" y1="12" x2="5" y2="12"/><polyline points="12 19 5 12 12 5"/>`,
};

/** 返回一个内联 SVG 字符串。size 默认 18。 */
export function icon(name: IconName, size = 18): string {
  return `<svg class="icon" width="${size}" height="${size}" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">${PATHS[name]}</svg>`;
}

/** 图标 + 文字，用于按钮/导航项内容。text 已假定为可信静态文案（不转义）。 */
export function iconText(name: IconName, text: string, size = 16): string {
  return `<span class="icon-text">${icon(name, size)}<span>${text}</span></span>`;
}
```

- [ ] **Step 2: 加图标基础 CSS**

在 `src/styles/theme.css` 末尾追加：
```css
.icon{display:inline-block;vertical-align:middle;flex-shrink:0}
.icon-text{display:inline-flex;align-items:center;gap:7px}
```

- [ ] **Step 3: 验证编译**

Run: `npm run build 2>&1 | tail -5`
Expected: tsc + vite 通过（icons.ts 目前未被引用不报错，因为 export 的东西即使没用 tsc 不报未使用的 export）。

- [ ] **Step 4: 提交**

```bash
git add src/lib/icons.ts src/styles/theme.css && git commit -m "feat: add inline SVG icon module"
```

---

## Task 3: 侧边栏图标 + 折叠

**Files:**
- Modify: `src/components/Sidebar.ts`, `src/main.ts`, `src/styles/theme.css`

- [ ] **Step 1: 重写 Sidebar 用 SVG 图标 + 折叠按钮**

Rewrite `src/components/Sidebar.ts`:
```ts
import { router } from "../lib/router";
import type { Route } from "../lib/router";
import { icon } from "../lib/icons";

type IconName = "video" | "book" | "gamepad" | "wand" | "settings";
const TOP: [Route, string, IconName][] = [
  ["video", "视频", "video"],
  ["comic", "漫画", "book"],
  ["game", "游戏", "gamepad"],
];
const BOTTOM: [Route, string, IconName][] = [
  ["normalize", "标准化", "wand"],
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
         <span class="brand-mark">◈ COLLECTOR</span>
         <button class="collapse-btn" title="折叠侧边栏">${icon("chevronLeft" as any, 16)}</button>
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
（注：`icon("chevronLeft" as any,...)` 里 chevronLeft 已在 IconName 联合内，实际不需要 as any——写成 `icon("chevronLeft", 16)`。此处直接用 `icon("chevronLeft", 16)`。）

修正 collapse 按钮那行为：`<button class="collapse-btn" title="折叠侧边栏">${icon("chevronLeft", 16)}</button>`

- [ ] **Step 2: main.ts 加展开钮 + 挂载**

Modify `src/main.ts`：给 `#app` 一个 id（当前用 querySelector 拿到但没确保 id；index.html 里是 `<div id="app">`，已有 id）。在 `app.appendChild(Sidebar())` 之后、content 之前，追加一个折叠时显示的展开钮：
```ts
import { Sidebar, toggleSidebar } from "./components/Sidebar";
import { icon } from "./lib/icons";
// ... 在 app.appendChild(Sidebar()); 之后：
const expandBtn = document.createElement("button");
expandBtn.className = "expand-btn glass";
expandBtn.title = "展开侧边栏";
expandBtn.innerHTML = icon("chevronRight", 18);
expandBtn.onclick = toggleSidebar;
app.appendChild(expandBtn);
```

- [ ] **Step 3: 折叠 + 图标 CSS**

在 `src/styles/theme.css` 追加/调整。先确认已有的 `.sidebar`/`.nav-item` 规则（在 theme.css 里），追加：
```css
.sidebar{transition:width .22s ease, padding .22s ease, opacity .18s ease; overflow:hidden}
.brand{display:flex;align-items:center;justify-content:space-between}
.brand-mark{white-space:nowrap}
.collapse-btn{padding:4px;border-radius:8px;background:transparent;border:none;color:var(--text-dim)}
.collapse-btn:hover{color:var(--text);background:var(--glass);box-shadow:none}
.nav-item{display:flex;align-items:center;gap:10px}
.nav-label{white-space:nowrap}
/* 折叠态：#app.sidebar-collapsed 时侧边栏宽度归零 */
#app.sidebar-collapsed .sidebar{width:0;padding-left:0;padding-right:0;opacity:0;border:none}
.expand-btn{position:fixed;left:10px;top:14px;z-index:50;padding:6px;border-radius:10px;display:none;color:var(--text)}
#app.sidebar-collapsed .expand-btn{display:inline-flex}
```
（`.sidebar` 原有 `width:180px`，折叠时被 `#app.sidebar-collapsed .sidebar` 覆盖为 0。expand-btn 默认隐藏，折叠时显示。）

- [ ] **Step 4: 验证**

Run: `npm run build 2>&1 | tail -5`（tsc 通过）。
应用热重载后手动看：侧边栏显示 SVG 图标 + 文字；点 brand 旁折叠钮 → 侧边栏平滑收起、左上角出现展开钮；点展开钮恢复。

- [ ] **Step 5: 提交**

```bash
git add src/components/Sidebar.ts src/main.ts src/styles/theme.css
git commit -m "feat: sidebar SVG icons and collapse toggle"
```

---

## Task 4: 按钮加图标 + 字体核查

**Files:**
- Modify: `src/views/SettingsView.ts`, `src/views/NormalizeView.ts`, `src/views/ComicReaderView.ts`, `src/views/PlayerView.ts`, `src/styles/theme.css`

- [ ] **Step 1: SettingsView 按钮加图标**

Modify `src/views/SettingsView.ts`：import `{ icon }` from `../lib/icons`。把「选择目录」按钮内容改为 `${icon("folder",15)}<span>选择目录</span>` 并给 button 加 class `icon-text`（或直接在 button 内用 icon-text span）。「扫描」「扫描视频库」按钮内容前加 `${icon("refresh",15)}`。
具体：模板里 `<button data-pick="${k}">选择目录</button>` → `<button class="icon-text" data-pick="${k}">${icon("folder",15)}<span>选择目录</span></button>`；扫描按钮 `<button class="btn-primary icon-text" data-scan="${k}">${icon("refresh",15)}<span>扫描</span></button>`；视频扫描钮同理加 refresh 图标。
注意：扫描逻辑里有 `b.textContent = "扫描中…"` / 恢复 `original`——现在按钮内含 SVG，用 textContent 会抹掉图标。改为：保存 `const original = b.innerHTML`，禁用时 `b.innerHTML = icon("refresh",15) + "<span>扫描中…</span>"`（或仅改文字 span）。最简做法：给文字套 `<span class="btn-label">`，改 `b.querySelector(".btn-label").textContent`。请用 btn-label span 方式，避免破坏图标。

- [ ] **Step 2: NormalizeView 按钮加图标**

Modify `src/views/NormalizeView.ts`：「选择输入目录/输出目录/图片目录」加 folder 图标、「选择输出zip」加 folder 图标、「开始」加 play 图标。同样注意「处理中…」不要用 textContent 抹掉图标——用 btn-label span 或 innerHTML 重设含图标。import icon。

- [ ] **Step 3: ComicReaderView / PlayerView 按钮图标统一**

Modify `src/views/ComicReaderView.ts`：「← 返回」改为 `${icon("arrowLeft",16)}<span>返回</span>`（button 加 icon-text class），「切换：长条/翻页」保留文字（可选加 grid/tree 图标，非必须）。
Modify `src/views/PlayerView.ts`：把控制条里的字符图标替换为 SVG——`.back`→arrowLeft、`.rw`→rewind、`.pp`→play/pause（切换时用 `el.querySelector(".pp")!.innerHTML = icon(paused?"play":"pause",18)`）、`.ff`→forward、`.fs`→fullscreen。初始 `.pp` 显示 pause 图标（播放中）。这些 button 在 `.player-bar` 下，字体已被 `.player-bar button` 管，图标随 currentColor。

- [ ] **Step 4: 验证**

Run: `npm run build 2>&1 | tail -5`（tsc 通过）。
热重载后看：各按钮文字前有贴切图标、字体与正文一致；播放器控制条为统一 SVG 图标；扫描中/处理中状态切换不丢图标。

- [ ] **Step 5: 提交**

```bash
git add src/views/ src/styles/theme.css
git commit -m "feat: add icons to action buttons, unify button content"
```

---

## Task 5: 视频层级树构建（纯函数）

**Files:**
- Create: `src/lib/videoTree.ts`

- [ ] **Step 1: 写树构建 + helper**

Create `src/lib/videoTree.ts`:
```ts
import type { MediaItem } from "./ipc";

export interface TreeNode {
  name: string;          // 该层目录名（根为分类名）
  path: string;          // 从分类根到本节点的完整 category_path 前缀，如 "电影/科幻"
  children: TreeNode[];  // 子目录
  items: MediaItem[];    // 本目录直接包含的视频
}

/**
 * 把某分类下的扁平 items 按 category_path 构建为一棵树。
 * category 为分类名（如 "电影"），作为树根 name。
 * 每个 item 的 category_path 形如 "电影/科幻/星球大战"，
 * 首段=分类名，其后为分类内的目录层级；末段目录下挂该视频。
 * 若 category_path 只有分类名一段（视频直放分类根），挂到根的 items。
 */
export function buildVideoTree(category: string, items: MediaItem[]): TreeNode {
  const root: TreeNode = { name: category, path: category, children: [], items: [] };
  for (const it of items) {
    const segs = it.category_path.split("/");
    // 去掉首段分类名，剩余为分类内层级
    const inner = segs.slice(1);
    if (inner.length === 0) {
      root.items.push(it);
      continue;
    }
    let node = root;
    let prefix = category;
    for (const seg of inner) {
      prefix = `${prefix}/${seg}`;
      let child = node.children.find(c => c.name === seg);
      if (!child) {
        child = { name: seg, path: prefix, children: [], items: [] };
        node.children.push(child);
      }
      node = child;
    }
    node.items.push(it);
  }
  sortTree(root);
  return root;
}

function sortTree(node: TreeNode) {
  node.children.sort((a, b) => a.name.localeCompare(b.name, "zh"));
  node.items.sort((a, b) => a.title.localeCompare(b.title, "zh"));
  node.children.forEach(sortTree);
}

/** 按完整 path 在树中查找节点，找不到返回 null。 */
export function findNode(root: TreeNode, path: string): TreeNode | null {
  if (root.path === path) return root;
  for (const c of root.children) {
    const found = findNode(c, path);
    if (found) return found;
  }
  return null;
}
```

- [ ] **Step 2: 验证树构建正确（临时 node 断言脚本）**

因前端无单测框架，用临时脚本验证纯函数逻辑。创建临时 `/tmp/tree-test.mjs`（跑完删除，不提交）：
```js
// 复制 buildVideoTree 逻辑做等价验证，或用 tsx 直接跑。这里用最小手动断言：
// 模拟 items
const items = [
  { title: "星球大战", category_path: "电影/科幻/星球大战" },
  { title: "异形", category_path: "电影/科幻/异形" },
  { title: "教父", category_path: "电影/动作/教父" },
  { title: "裸片", category_path: "电影" },
];
// 期望：root(电影).items=[裸片]; children=[科幻(items:星球大战,异形), 动作(items:教父)]
```
更实际的验证：用 `npx tsx` 直接 import 编译。Run:
```bash
cd /Users/zhoumo/Documents/Claude/collector && cat > /tmp/tree-test.ts <<'EOF'
import { buildVideoTree, findNode } from "./src/lib/videoTree";
const items:any = [
  { title:"星球大战", category_path:"电影/科幻/星球大战", id:1 },
  { title:"异形", category_path:"电影/科幻/异形", id:2 },
  { title:"教父", category_path:"电影/动作/教父", id:3 },
  { title:"裸片", category_path:"电影", id:4 },
];
const t = buildVideoTree("电影", items);
console.assert(t.items.length===1 && t.items[0].title==="裸片", "root items");
console.assert(t.children.length===2, "two subdirs");
const sci = findNode(t, "电影/科幻");
console.assert(sci && sci.items.length===2, "科幻 has 2");
console.log("TREE_OK");
EOF
npx tsx /tmp/tree-test.ts; rm -f /tmp/tree-test.ts
```
Expected: 输出 `TREE_OK`，无 assert 报错。（若 npx tsx 不可用，改用 `npm run build` 确保类型正确，逻辑靠 Task 7 集成后手动验证。）

- [ ] **Step 3: 提交**

```bash
git add src/lib/videoTree.ts && git commit -m "feat: build video hierarchy tree from category_path"
```

---

## Task 6: FolderView 与 TreeView 组件

**Files:**
- Create: `src/components/FolderView.ts`, `src/components/TreeView.ts`
- Modify: `src/styles/theme.css`

- [ ] **Step 1: FolderView（面包屑 + 进入式，文件夹与海报混排）**

Create `src/components/FolderView.ts`:
```ts
import { convertFileSrc } from "@tauri-apps/api/core";
import type { MediaItem } from "../lib/ipc";
import type { TreeNode } from "../lib/videoTree";
import { findNode } from "../lib/videoTree";
import { icon } from "../lib/icons";
import { esc } from "../lib/escape";

/**
 * 文件夹视图：进入式浏览一棵 TreeNode。
 * root: 分类树根；onOpen: 打开视频。内部维护当前路径。
 */
export function FolderView(root: TreeNode, onOpen: (it: MediaItem) => void): HTMLElement {
  const el = document.createElement("div");
  el.className = "folder-view";
  let currentPath = root.path;

  const render = () => {
    const node = findNode(root, currentPath) ?? root;
    // 面包屑：从 root 到 currentPath 逐段
    const segs = currentPath.split("/");
    const crumbs = segs.map((seg, i) => {
      const p = segs.slice(0, i + 1).join("/");
      const last = i === segs.length - 1;
      return `<span class="crumb ${last ? "crumb-cur" : ""}" data-path="${esc(p)}">${esc(seg)}</span>`;
    }).join('<span class="crumb-sep">／</span>');

    const folders = node.children.map(c => `
      <div class="fv-cell fv-folder" data-folder="${esc(c.path)}">
        <div class="fv-folder-icon">${icon("folder", 40)}</div>
        <span class="fv-name">${esc(c.name)}</span>
      </div>`).join("");
    const videos = node.items.map((it, i) => `
      <div class="fv-cell fv-video" data-i="${i}">
        <div class="poster-img">${it.cover_path ? `<img src="${convertFileSrc(it.cover_path)}"/>` : `<div class="poster-ph">${esc(it.title)}</div>`}</div>
        <span class="fv-name">${esc(it.title)}</span>
      </div>`).join("");

    el.innerHTML = `
      <div class="breadcrumb">${crumbs}</div>
      <div class="fv-grid">${folders}${videos}</div>`;

    el.querySelectorAll<HTMLElement>(".crumb").forEach(c =>
      c.onclick = () => { currentPath = c.dataset.path!; render(); });
    el.querySelectorAll<HTMLElement>(".fv-folder").forEach(f =>
      f.onclick = () => { currentPath = f.dataset.folder!; render(); });
    el.querySelectorAll<HTMLElement>(".fv-video").forEach(v =>
      v.onclick = () => onOpen(node.items[Number(v.dataset.i)]));
  };
  render();
  return el;
}
```

- [ ] **Step 2: TreeView（左树 + 右海报）**

Create `src/components/TreeView.ts`:
```ts
import { convertFileSrc } from "@tauri-apps/api/core";
import type { MediaItem } from "../lib/ipc";
import type { TreeNode } from "../lib/videoTree";
import { findNode } from "../lib/videoTree";
import { esc } from "../lib/escape";

/**
 * 树视图：左侧可展开目录树，右侧显示选中节点的视频海报。
 */
export function TreeView(root: TreeNode, onOpen: (it: MediaItem) => void): HTMLElement {
  const el = document.createElement("div");
  el.className = "tree-view";
  const expanded = new Set<string>([root.path]); // 默认展开根
  let selected = root.path;

  const renderTreeNodes = (node: TreeNode, depth: number): string => {
    const isOpen = expanded.has(node.path);
    const hasChildren = node.children.length > 0;
    const arrow = hasChildren ? (isOpen ? "▾" : "▸") : "　";
    const row = `<div class="tree-node ${selected === node.path ? "sel" : ""}" data-path="${esc(node.path)}" style="padding-left:${depth * 14 + 4}px">
      <span class="tree-arrow" data-toggle="${esc(node.path)}">${arrow}</span>
      <span class="tree-name">${esc(node.name)}</span>
    </div>`;
    const childrenHtml = isOpen ? node.children.map(c => renderTreeNodes(c, depth + 1)).join("") : "";
    return row + childrenHtml;
  };

  const render = () => {
    const node = findNode(root, selected) ?? root;
    const posters = node.items.map((it, i) => `
      <div class="poster card-hover" data-i="${i}">
        <div class="poster-img">${it.cover_path ? `<img src="${convertFileSrc(it.cover_path)}"/>` : `<div class="poster-ph">${esc(it.title)}</div>`}</div>
        <div class="poster-title">${esc(it.title)}</div>
      </div>`).join("");
    el.innerHTML = `
      <div class="tree-pane">${renderTreeNodes(root, 0)}</div>
      <div class="tree-content"><div class="poster-grid">${posters || '<div class="tree-empty">此目录下无直接视频，请展开子目录</div>'}</div></div>`;

    el.querySelectorAll<HTMLElement>(".tree-arrow").forEach(a =>
      a.onclick = (e) => { e.stopPropagation(); const p = a.dataset.toggle!; expanded.has(p) ? expanded.delete(p) : expanded.add(p); render(); });
    el.querySelectorAll<HTMLElement>(".tree-node").forEach(n =>
      n.onclick = () => { selected = n.dataset.path!; render(); });
    el.querySelectorAll<HTMLElement>(".tree-content .poster").forEach(p =>
      p.onclick = () => onOpen(node.items[Number(p.dataset.i)]));
  };
  render();
  return el;
}
```

- [ ] **Step 3: FolderView/TreeView CSS**

在 `src/styles/theme.css` 追加：
```css
/* 面包屑 */
.breadcrumb{font-size:12px;color:var(--text-dim);margin-bottom:14px}
.crumb{color:var(--accent);cursor:pointer}
.crumb:hover{text-decoration:underline}
.crumb-cur{color:var(--text);cursor:default}
.crumb-cur:hover{text-decoration:none}
.crumb-sep{margin:0 6px;color:var(--text-dim)}
/* 文件夹视图网格：文件夹与海报混排 */
.fv-grid{display:grid;grid-template-columns:repeat(auto-fill,minmax(140px,1fr));gap:14px;align-items:start}
.fv-cell{cursor:pointer;display:flex;flex-direction:column;gap:6px}
.fv-folder-icon{width:100%;aspect-ratio:1;border-radius:10px;background:rgba(91,140,255,.12);border:1px solid rgba(91,140,255,.3);display:flex;align-items:center;justify-content:center;color:var(--accent);transition:box-shadow .18s}
.fv-folder:hover .fv-folder-icon{box-shadow:0 6px 20px var(--accent-glow)}
.fv-video .poster-img{aspect-ratio:2/3;border-radius:10px;overflow:hidden;background:var(--glass);border:1px solid var(--border);display:flex;align-items:center;justify-content:center}
.fv-video .poster-img img{width:100%;height:100%;object-fit:cover}
.fv-name{font-size:12px;color:var(--text);text-align:center;word-break:break-all}
/* 树视图 */
.tree-view{display:flex;gap:14px;min-height:0}
.tree-pane{width:200px;flex-shrink:0;border-right:1px solid var(--border);padding-right:8px;overflow:auto;max-height:calc(100vh - 160px)}
.tree-node{display:flex;align-items:center;gap:4px;padding:4px 0;border-radius:6px;cursor:pointer;font-size:13px;color:var(--text-dim);white-space:nowrap}
.tree-node:hover{background:var(--glass)}
.tree-node.sel{background:var(--accent-glow);color:var(--text)}
.tree-arrow{width:14px;display:inline-block;text-align:center;color:var(--text-dim)}
.tree-content{flex:1;min-width:0;overflow:auto}
.tree-empty{color:var(--text-dim);font-size:12px;padding:20px}
```

- [ ] **Step 4: 验证编译**

Run: `npm run build 2>&1 | tail -5`
Expected: tsc + vite 通过（组件此时未被 VideoView 引用，export 不报错）。

- [ ] **Step 5: 提交**

```bash
git add src/components/FolderView.ts src/components/TreeView.ts src/styles/theme.css
git commit -m "feat: folder view and tree view components for video"
```

---

## Task 7: VideoView 重构（分类 tab + 视图切换 + 委托）

**Files:**
- Modify: `src/views/VideoView.ts`, `src/styles/theme.css`

- [ ] **Step 1: 重写 VideoView**

Rewrite `src/views/VideoView.ts`:
```ts
import { api } from "../lib/ipc";
import type { MediaItem } from "../lib/ipc";
import { buildVideoTree } from "../lib/videoTree";
import { FolderView } from "../components/FolderView";
import { TreeView } from "../components/TreeView";
import { icon } from "../lib/icons";

type ViewMode = "folder" | "tree";

export async function VideoView(onOpen: (it: MediaItem) => void): Promise<HTMLElement> {
  const el = document.createElement("div");
  el.className = "view-enter video-view";
  const items = await api.listMedia("video");
  const cats = ["电影", "动漫", "电视剧"];
  let activeCat = cats[0];
  let mode: ViewMode = "folder";

  const render = () => {
    const catItems = items.filter(i => i.category === activeCat);
    const tree = buildVideoTree(activeCat, catItems);
    el.innerHTML = `
      <div class="video-bar">
        <div class="tabs">${cats.map(c =>
          `<span class="tab ${c === activeCat ? "active" : ""}" data-c="${c}">${c}</span>`).join("")}</div>
        <div class="view-toggle">
          <button class="vt-btn ${mode === "folder" ? "active" : ""}" data-mode="folder" title="文件夹视图">${icon("folder", 16)}</button>
          <button class="vt-btn ${mode === "tree" ? "active" : ""}" data-mode="tree" title="树形视图">${icon("tree", 16)}</button>
        </div>
      </div>
      <div class="video-body"></div>`;

    el.querySelectorAll<HTMLElement>(".tab").forEach(t =>
      t.onclick = () => { activeCat = t.dataset.c!; render(); });
    el.querySelectorAll<HTMLButtonElement>(".vt-btn").forEach(b =>
      b.onclick = () => { mode = b.dataset.mode as ViewMode; render(); });

    const body = el.querySelector<HTMLElement>(".video-body")!;
    body.appendChild(mode === "folder" ? FolderView(tree, onOpen) : TreeView(tree, onOpen));
  };
  render();
  return el;
}
```

- [ ] **Step 2: video-bar / view-toggle CSS**

在 `src/styles/theme.css` 追加：
```css
.video-bar{display:flex;align-items:center;margin-bottom:16px}
.video-bar .tabs{margin-bottom:0}
.view-toggle{margin-left:auto;display:flex;gap:6px}
.vt-btn{padding:6px 10px;border-radius:8px;color:var(--text-dim)}
.vt-btn.active{background:var(--accent-glow);border-color:var(--accent);color:var(--text)}
```

- [ ] **Step 3: 验证编译 + 集成**

Run: `npm run build 2>&1 | tail -5`
Expected: tsc + vite 通过。
应用热重载后手动验证（此任务完成后可全面测第 4 项）：视频菜单 → 选分类 → 默认文件夹视图，面包屑 + 子文件夹（蓝色文件夹图标）与视频海报混排；点文件夹进入下层、点面包屑回退；右上角切树形视图 → 左树可展开/折叠、点节点右侧显示海报；点海报打开播放器。

- [ ] **Step 4: 提交**

```bash
git add src/views/VideoView.ts src/styles/theme.css
git commit -m "feat: video view with folder/tree mode switch"
```

---

## 自查（Self-Review）

**1. Spec 覆盖：**
- 默认最大化窗口（可缩放）→ Task 1 ✓
- 侧边栏折叠（完全隐藏+展开钮）→ Task 3 ✓
- 图标更贴切（SVG，视频/漫画/游戏/标准化/设置各专属）→ Task 2 图标 + Task 3 侧边栏 ✓
- 按钮字体一致 + 加图标 → Task 4 ✓
- 视频树形视图 → Task 5 树构建 + Task 6 TreeView + Task 7 切换 ✓
- 视频文件夹视图（面包屑/进入式/混排，百度网盘风）→ Task 6 FolderView + Task 7 ✓
- 保留分类 tab、分类内分层 → Task 7 ✓
- 组件边界（icons/videoTree 纯、FolderView/TreeView 接树+回调、VideoView 外壳）→ Task 5/6/7 ✓
- 明确不做（不持久化偏好、不改后端）→ 计划未引入持久化/后端改动 ✓

**2. 占位符扫描：** 无 TODO/TBD；每个代码步骤给出完整可用代码；Task 3 Step 1 note 里修正了 collapse 按钮的 icon 调用（去掉误写的 as any，用 `icon("chevronLeft",16)`）。

**3. 类型/命名一致性：**
- `TreeNode {name,path,children,items}`、`buildVideoTree(category, items)`、`findNode(root, path)` 在 Task 5 定义，Task 6/7 引用一致。
- `icon(name, size)` / `iconText` 签名 Task 2 定义，后续一致调用。
- `toggleSidebar` Task 3 定义并在 Sidebar 与 main.ts（展开钮）共用。
- IconName 联合含所有用到的图标名（video/book/gamepad/wand/settings/chevronLeft/chevronRight/folder/tree/grid/play/pause/rewind/forward/fullscreen/refresh/arrowLeft）；Task 3/4/6/7 用到的 folder/tree/refresh/play/arrowLeft/rewind/forward/pause/fullscreen/chevron* 均在其中 ✓
- ViewMode "folder"|"tree" 在 VideoView 内一致。
- 按钮「处理中」状态：Task 4 明确用 btn-label span 或 innerHTML 重设，避免 textContent 抹掉 SVG（与 Task 4 的 SettingsView/NormalizeView 现有 textContent 逻辑冲突点已指出修法）。

自查通过。
