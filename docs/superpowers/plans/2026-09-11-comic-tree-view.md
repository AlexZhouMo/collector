# 漫画分层树形展示 实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 漫画视图改为仿影视的分层树形导航（题材作根层方形文件夹→逐层进→漫画卡→卷列表），文件夹/树形切换+面包屑，复用 FolderView/TreeView/buildVideoTree。

**Architecture:** ComicView 重写为仿 VideoView（无分类 tab），root=buildVideoTree("", comicItems)，FolderView/TreeView 复用；FolderView 加可选 enableRename 开关（漫画禁用视频专用 rename）。纯前端。

**Tech Stack:** vanilla-ts，复用现有组件。

---

### Task 1: FolderView 加 enableRename 开关 + ComicView 重写为树形

**Files:**
- Modify: `src/components/FolderView.ts`
- Modify: `src/views/ComicView.ts`

- [ ] **Step 1: FolderView 加 enableRename 参数**

FolderView 内置了 `attachInlineRename` 调 `api.renameFolder(root.name, ...)`（视频专用，漫画不适用）。加一个可选 `enableRename` 参数（默认 true，不影响视频；漫画传 false 跳过 rename 绑定）。

在 FolderView 签名末尾加参数：

```ts
export function FolderView(
  root: TreeNode,
  onOpen: (it: MediaItem) => void,
  onContext?: (it: MediaItem, x: number, y: number) => void,
  initialPath?: string,
  onNav?: (path: string) => void,
  onRenamed?: (oldPath: string, newPath: string) => void,
  enableRename: boolean = true
): HTMLElement {
```

把绑定 rename 的那段（`el.querySelectorAll<HTMLElement>(".fv-folder").forEach(...) 里 attachInlineRename(...)`）用 `if (enableRename) { ... }` 包起来——即仅当 enableRename 时才绑定 inline 重命名；否则文件夹名只读（文件夹点击进入的绑定保留，不受影响）。

> 读 FolderView.ts 找到 attachInlineRename 所在的 forEach 块（约 65-83 行），精确地只把 rename 绑定包进 `if (enableRename)`，不要影响 `.fv-folder` 的点击进入（`data-folder` 的 onclick）与 `.fv-video` 的 onOpen。

- [ ] **Step 2: 重写 ComicView 为树形导航（仿 VideoView，无 tab）**

`src/views/ComicView.ts` 重写：

```ts
import { api } from "../lib/ipc";
import type { MediaItem } from "../lib/ipc";
import { buildVideoTree } from "../lib/videoTree";
import { FolderView } from "../components/FolderView";
import { TreeView } from "../components/TreeView";
import { icon } from "../lib/icons";

type ViewMode = "folder" | "tree";

export async function ComicView(onOpen: (it: MediaItem) => void): Promise<HTMLElement> {
  const el = document.createElement("div");
  el.className = "view-enter video-view"; // 复用 video-view 布局样式
  const items = await api.listMedia("comic");
  let mode: ViewMode = "folder";
  let folderPath = "";
  let treeSelected = "";

  const render = () => {
    // 漫画树：虚拟空根，category_path 即完整层级路径
    const tree = buildVideoTree("", items);
    el.innerHTML = `
      <div class="video-bar">
        <div class="tabs"><span class="tab active">漫画</span></div>
        <div class="video-bar-right">
          <div class="view-toggle">
            <button class="vt-btn ${mode === "folder" ? "active" : ""}" data-mode="folder" title="文件夹视图">${icon("folder", 16)}</button>
            <button class="vt-btn ${mode === "tree" ? "active" : ""}" data-mode="tree" title="树形视图">${icon("tree", 16)}</button>
          </div>
        </div>
      </div>
      <div class="video-body"></div>`;
    el.querySelectorAll<HTMLButtonElement>(".vt-btn").forEach(b =>
      b.onclick = () => { mode = b.dataset.mode as ViewMode; render(); });
    const body = el.querySelector<HTMLElement>(".video-body")!;
    body.appendChild(mode === "folder"
      ? FolderView(tree, onOpen, undefined, folderPath, (p) => { folderPath = p; }, undefined, false)
      : TreeView(tree, onOpen, undefined, treeSelected, (p) => { treeSelected = p; }));
  };
  render();
  return el;
}
```

> 说明：`buildVideoTree("", items)` 空根名——根 path 为空串，各题材是根的 children。FolderView 传
> `enableRename=false`（漫画禁用重命名）、onContext=undefined（漫画无右键菜单，若 FolderView 要求
> onContext 可选则 undefined 即可）。`.tabs` 放一个静态"漫画"标签占位（保持与影视一致的顶栏布局，不可点
> 切换）——或去掉 tabs 只留视图切换；倾向保留静态标签使布局一致。`video-view`/`video-body`/`video-bar`
> 样式复用影视，无需新增 CSS。

- [ ] **Step 3: 类型检查**

Run: `npx tsc --noEmit`
Expected: 无输出。修到干净（FolderView enableRename 默认值不破坏 VideoView 现有调用——VideoView 未传该参数，用默认 true，行为不变；TreeView 签名 `(root,onOpen,onContext?,selected?,onNav?)` 与调用匹配）。

- [ ] **Step 4: 预览验证**

起预览进漫画页，inspect：`.video-bar`/视图切换按钮存在、`.folder-view` 渲染根层题材文件夹（`.fv-folder` + `fv-folder-icon` 方形图标）；点题材文件夹进入下一层；切树形视图；面包屑存在。截图看分层效果。（预览无 IPC，listMedia 返回空——需注入 mock comic items 验证树渲染，或靠真机。inspect 结构 + 真机为准。）

- [ ] **Step 5: 提交**

```bash
git add src/components/FolderView.ts src/views/ComicView.ts
git commit -m "feat(comic): 漫画视图改为分层树形(仿影视，题材根层文件夹，复用FolderView/TreeView)

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

## 自查

- **规格覆盖**：题材作根层文件夹（Step 2 buildVideoTree("",items) 空根，题材为 children）、文件夹/树形切换（Step 2 vt-btn + mode）、方形文件夹图标（复用 FolderView fv-folder-icon）、点漫画进卷列表（Step 2 onOpen=openComicVolumes via 路由）、无分类 tab（Step 2 静态"漫画"标签不可切）、空封面占位（FolderView 已有 poster-ph）、禁用 rename（Step 1 enableRename=false）——规格各条均有对应实现。
- **占位符扫描**：无 TBD；Step 1/2 给出完整代码。Step 1 的 rename 块位置需实施时读 FolderView 精确定位（已注明）。
- **一致性**：FolderView 新增 enableRename 默认 true→VideoView 调用不变（行为不变）；ComicView 的 onOpen 由 main.ts 传入（openComicVolumes，不变）；buildVideoTree/TreeView/FolderView 接口与调用匹配。
- **待实施确认点**：FolderView rename 块的精确行范围（Step 1）；TreeView 是否要求 onContext 必填（Step 2 传 undefined，若必填则调整）；main.ts 的 `renderRoute("comic")` 调 ComicView 的 onOpen 仍是 openComicVolumes（不用改 main.ts）。
