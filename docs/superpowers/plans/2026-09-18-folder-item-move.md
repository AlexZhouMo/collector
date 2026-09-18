# 文件夹与单条目移动 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 影视/漫画/游戏三类型支持文件夹移动与单条目跨分类移动（同类型内自由移动，跨类型禁止）；只改库分类/路径 + 字幕跟随，视频文件不动。

**Architecture:** 影视移动用"三分类合一树"（节点 path 带分类前缀 `分类/子路径`，可解析目标 category+category_path）；漫画/游戏用单分类树。前端复用 `MoveDialog` 扩展跨分类；后端单条目复用 `media_update`/`comic_update`/`game_update`，文件夹移动新增 `move_folder` 命令（DB 级联 + 字幕目录跟随）。

**Tech Stack:** Rust (Tauri 2, rusqlite)、TypeScript (Vite)、SQLite。

**验证命令：**
- 后端：`cd /Users/zhoumo/Documents/Claude/collector/src-tauri && cargo test && cargo build`
- 前端：`cd /Users/zhoumo/Documents/Claude/collector && npx tsc --noEmit`
- 运行：`npm run tauri:dev`

**关键结构决策（贯穿全程）：**
- 影视"三分类合一树"：虚拟根 → 三个分类节点（path=分类名，如 `电影`）→ 各自子目录（path=`电影/科幻`、`电影/科幻/诺兰`，即**带分类前缀的完整 path**）。
- 从这种 path 解析目标：第一段是 category，其余是 category_path（`电影/科幻/诺兰` → category=`电影`, category_path=`科幻/诺兰`；`动漫` → category=`动漫`, category_path=``）。
- 漫画/游戏树不变（path 不含分类前缀，如 `单行本`）。
- 后端 media 表 category_path **不含分类名**（现状）——前端解析出的 category_path 传后端时已是去掉分类前缀的。

---

## Task 1: 影视三分类合一树 + path 解析工具

**Files:**
- Modify: `src/lib/videoTree.ts`（加 `buildMergedVideoTree` + `splitCategoryPath`）

- [ ] **Step 1: 写 path 解析工具的失败测试**

在 `src/lib/videoTree.ts` 末尾（或配套测试脚本）验证解析逻辑。因项目前端无测试框架，改为在函数上写明契约并靠 tsc + 运行验证；此处先实现函数。

- [ ] **Step 2: 实现 `splitCategoryPath`**

在 `src/lib/videoTree.ts` 加：
```typescript
/** 影视合一树的 path（带分类前缀）→ {category, categoryPath}。
 * "电影/科幻/诺兰" → {category:"电影", categoryPath:"科幻/诺兰"}
 * "电影" → {category:"电影", categoryPath:""} */
export function splitCategoryPath(prefixedPath: string): { category: string; categoryPath: string } {
  const i = prefixedPath.indexOf("/");
  if (i < 0) return { category: prefixedPath, categoryPath: "" };
  return { category: prefixedPath.slice(0, i), categoryPath: prefixedPath.slice(i + 1) };
}
```

- [ ] **Step 3: 实现 `buildMergedVideoTree`**

把三分类各自的 items 建树，合并到一个虚拟根下，每个分类子树的所有节点 path 前缀加 `分类/`：
```typescript
/** 影视三分类合一树：虚拟根下挂电影/动漫/剧集三个分类子树。
 * 各分类节点 path=分类名；其内部节点 path 加分类前缀（"电影/科幻"）。
 * cats: [[category, items]]，如 [["电影", movieItems],["动漫",animeItems],["剧集",tvItems]] */
export function buildMergedVideoTree(cats: [string, MediaItem[]][]): TreeNode {
  const root: TreeNode = { name: "影视", path: "__root__", children: [], items: [] };
  for (const [cat, items] of cats) {
    const sub = buildVideoTree(cat, items); // 现有：根 path="", 内部 path 不含分类
    // 重写 path 加分类前缀：根→cat，内部→cat/<path>
    const reprefix = (n: TreeNode, isRoot: boolean) => {
      n.path = isRoot ? cat : `${cat}/${n.path}`;
      n.children.forEach(c => reprefix(c, false));
    };
    reprefix(sub, true);
    root.children.push(sub);
  }
  return root;
}
```
（注意：`buildVideoTree` 内部节点 path 形如 "科幻/诺兰"，reprefix 后变 "电影/科幻/诺兰"；根 path "" 变 "电影"。）

- [ ] **Step 4: 类型检查**

Run: `cd /Users/zhoumo/Documents/Claude/collector && npx tsc --noEmit`
Expected: 无输出（新函数类型正确）。

- [ ] **Step 5: 提交**
```bash
cd /Users/zhoumo/Documents/Claude/collector
git add src/lib/videoTree.ts
git commit -m "feat(move): 影视三分类合一树 buildMergedVideoTree + splitCategoryPath 解析

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

## Task 2: MoveDialog 支持跨分类单条目移动

**Files:**
- Modify: `src/components/MoveDialog.ts`

- [ ] **Step 1: 读现状确认改动点**

Run: `sed -n '15,130p' src/components/MoveDialog.ts`
Expected: 确认 `openMoveDialog(item, tree, kind, onMoved)` 签名、确认逻辑（108-125 行按 kind 调 update）、当前位置判断（`p === item.category_path`）。

- [ ] **Step 2: 增加"跨分类"模式**

给 `openMoveDialog` 加一个可选参数 `crossCategory?: boolean`（影视传 true，传入的是合一树）。当 crossCategory 时：
- 虚拟根节点（path=`__root__`，name="影视"）**不可选**（点击无效、不高亮）——它只是三分类的容器，移到它没意义。
- "当前位置"判断改为比对带分类前缀的 path：当前 = `${item.category}${item.category_path ? "/" + item.category_path : ""}`。
- 确认时用 `splitCategoryPath(selected)` 解析出目标 category + categoryPath，调 `api.mediaUpdate(item.id, targetCategory, targetCategoryPath, item.title, item.cover_path, item.description)`。
- 非 crossCategory（漫画/游戏）：保持现状（selected 即 category_path，category 不变）。

- [ ] **Step 3: 引入 splitCategoryPath**

`import { splitCategoryPath } from "../lib/videoTree";` 在确认逻辑里用。

- [ ] **Step 4: 类型检查 + 构建**

Run: `npx tsc --noEmit && echo ok; npm run build 2>&1 | tail -1`
Expected: ok + 构建成功。

- [ ] **Step 5: 提交**
```bash
git add src/components/MoveDialog.ts
git commit -m "feat(move): MoveDialog 支持跨分类模式（影视合一树，selected 解析目标分类）

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

## Task 3: 三视图接入单条目跨分类移动

**Files:**
- Modify: `src/views/MediaLibraryView.ts`、`src/views/ComicView.ts`（开启 supportsMove）

- [ ] **Step 1: 读 MediaLibraryView 的移动菜单构造**

Run: `sed -n '80,95p' src/views/MediaLibraryView.ts`
Expected: 确认 `openMoveDialog(it, tree, cfg.kind, refresh)` 调用处，及 tree 来源（现在是 `buildVideoTree(rootName(), treeItems())`）。

- [ ] **Step 2: 影视用合一树 + crossCategory**

在 MediaLibraryView 里，当 `cfg.cats`（影视）存在时：移动用 `buildMergedVideoTree(cfg.cats.map(c => [c, items.filter(i=>i.category===c)]))` 传入，`openMoveDialog(it, mergedTree, cfg.kind, refresh, true)`（crossCategory=true）。非影视（漫画/游戏单分类）保持 `buildVideoTree(rootName(), treeItems())` + 不传 crossCategory。
（cfg.cats 是 `["电影","动漫","剧集"]`，items 是当前 kind 全部项。）

- [ ] **Step 3: ComicView 开启 supportsMove**

`src/views/ComicView.ts`：`supportsMove: false` → `true`。漫画单分类树，move 走 comicUpdate（改 category_path）。确认 MediaLibraryView 的 onMove 对 comic 用 buildVideoTree("漫画", items)。

- [ ] **Step 4: MoveDialog 对 comic 的 update 调用**

确认 MoveDialog 确认逻辑：kind==="comic" 时调 `api.comicUpdate(item.id, selected, ...)`（现在只有 game/else 分支，需补 comic 分支或让 else 覆盖——检查现状，comic 不能走 mediaUpdate）。

- [ ] **Step 5: 类型检查 + 构建 + 运行验证**

Run: `npx tsc --noEmit && npm run build 2>&1 | tail -1`
`npm run tauri:dev`，实测：影视右键移动 → 弹合一树 → 电影移到动漫成功、列表刷新；漫画右键有移动项、可在漫画内移动；游戏移动正常。

- [ ] **Step 6: 提交**
```bash
git add src/views/MediaLibraryView.ts src/views/ComicView.ts src/components/MoveDialog.ts
git commit -m "feat(move): 三视图单条目移动——影视跨分类合一树、漫画开启移动

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

## Task 4: 后端 move_folder 命令（DB 级联 + 字幕跟随）

**Files:**
- Modify: `src-tauri/src/lib.rs`（新增 `move_folder` command + `move_folder_in_db` 纯函数 + 测试）

- [ ] **Step 1: 读 rename_folder_in_db 级联 SQL 作参考**

Run: `sed -n '127,180p' src-tauri/src/lib.rs`
Expected: 确认级联 UPDATE 模式（`category_path = ?new || substr(category_path, length(?old)+1)` for 子孙 + 精确匹配 self）、has_category 分支（video 用 category 过滤）。

- [ ] **Step 2: 写 move_folder_in_db 纯函数（可测）**

新增（参考 rename_folder_in_db，但支持改 category 且目标是"父路径"）：
```rust
/// 移动文件夹：把 (category, old_path) 整棵子树移到 (target_category, target_parent)/下。
/// 新路径 = target_parent + "/" + old_path 的末段名（folder_name）。
/// video 需同时改 category（跨分类），comic/game 只改 category_path。
/// 校验：目标不能是自身或自身子树；目标位置无同名冲突。表名由 kind 决定（非注入）。
fn move_folder_in_db(db: &Db, kind: MediaKind, category: &str, old_path: &str,
    target_category: &str, target_parent: &str) -> AppResult<()> {
    // folder_name = old_path 末段
    // new_path = if target_parent empty { folder_name } else { target_parent/folder_name }
    // 防移进自身：target 属同 category 且 (new_path==old_path 或 new_path 以 old_path+"/" 开头) → Err
    // 冲突检查：目标 (target_category,new_path) 已存在 → Err
    // 级联：子孙 category_path 前缀 old_path→new_path；video 同时 SET category=target_category
    //   self: category_path=old_path 的那条也更新
}
```
按 rename_folder_in_db 的 SQL 结构实现，video 分支额外 `SET category=?target_category`。self + 子孙两条 UPDATE。

- [ ] **Step 3: 写 move_folder_in_db 单测**

在 lib.rs 的 `#[cfg(test)] mod rename_folder_tests`（或新 mod）加测试：
- video 跨分类移动：电影/科幻 移到 动漫（target_parent=""）→ 该文件夹及子孙 category 变动漫、category_path 变 `科幻`（去掉原前缀重挂）。用内存 db 建几条 media 验证。
- 防移进自身子树：科幻 移到 科幻/子 → Err。
- 同名冲突：目标已有同名文件夹 → Err。

- [ ] **Step 4: 写 move_folder command（含字幕跟随）**

```rust
#[tauri::command(rename_all = "camelCase")]
fn move_folder(app, db, kind: String, category: String, old_path: String,
    target_category: String, target_parent: String) -> AppResult<()> {
    let k = MediaKind::from_kind_str(&kind)?;
    // video：字幕目录跟随 subtitles/<category>/<old_path> → subtitles/<target_category>/<new_path>
    //   （new_path 同 move_folder_in_db 算法：target_parent + folder_name）
    //   视频文件不动（按 spec 决定）。
    // 调 move_folder_in_db 完成 DB 级联。
}
```
字幕跟随参考 rename_folder 的字幕目录 rename（lib.rs:214-225），路径改为跨 category。

- [ ] **Step 5: 注册命令**

lib.rs `invoke_handler` 加 `move_folder,`。

- [ ] **Step 6: 编译 + 测试**

Run: `cd src-tauri && cargo build 2>&1 | grep -iE "error|warning" || echo clean; cargo test 2>&1 | grep "test result:" | head -1`
Expected: clean + 测试全过（含新增 move_folder 测试）。

- [ ] **Step 7: 提交**
```bash
cd /Users/zhoumo/Documents/Claude/collector
git add src-tauri/src/lib.rs
git commit -m "feat(move): 后端 move_folder 命令——文件夹 DB 级联移动 + 字幕目录跟随

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

## Task 5: 前端文件夹移动入口 + ipc

**Files:**
- Modify: `src/lib/ipc.ts`（加 moveFolder）、`src/components/FolderView.ts`（文件夹移动入口）、`src/components/MoveDialog.ts`（支持文件夹移动模式）

- [ ] **Step 1: ipc 加 moveFolder**

`src/lib/ipc.ts` 加：
```typescript
moveFolder: (kind: string, category: string, oldPath: string, targetCategory: string, targetParent: string) =>
  invoke<void>("move_folder", { kind, category, oldPath, targetCategory, targetParent }),
```

- [ ] **Step 2: MoveDialog 支持文件夹移动模式**

扩展 `openMoveDialog` 支持"移动文件夹"：新增一个入口或参数区分 item-move / folder-move。folder-move 时：
- 标题"移动文件夹「X」到…"。
- **排除自身子树**：渲染树时，被移动文件夹自身及其子孙节点标记为禁选（灰显、点击无效）。
- 确认时：影视用 splitCategoryPath(selected) 得 target_category+target_parent，调 `api.moveFolder(kind, sourceCategory, sourceOldPath, targetCategory, targetParent)`；漫画/游戏 target_category=固定分类。
（可新增 `openFolderMoveDialog(folderNode, tree, kind, sourceCategory, crossCategory, onMoved)` 与现有 item 版并列，复用内部渲染。）

- [ ] **Step 3: FolderView 加文件夹移动入口**

`src/components/FolderView.ts`：文件夹格子加操作（右键菜单或 hover 按钮）"移动"，点击弹 folder-move 对话框。传入当前 kind、category、文件夹 path、合一树（影视）或单分类树。参考现有改名入口（FolderView.ts 已有 renameFolder 交互）。

- [ ] **Step 4: 类型检查 + 构建**

Run: `npx tsc --noEmit && npm run build 2>&1 | tail -1`
Expected: ok + 构建成功。

- [ ] **Step 5: 运行验证文件夹移动**

`npm run tauri:dev`，实测：
- 影视：文件夹右键移动 → 合一树 → 移到另一分类下 → 整个文件夹（含视频、子目录）跟随、字幕目录跟随、列表刷新。
- 移进自身子树被禁选。
- 漫画/游戏：文件夹在本分类内移动正常。

- [ ] **Step 6: 提交**
```bash
git add src/lib/ipc.ts src/components/FolderView.ts src/components/MoveDialog.ts
git commit -m "feat(move): 文件夹移动入口（FolderView 菜单 + MoveDialog 文件夹模式 + moveFolder ipc）

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

## Task 6: 端到端验证与收尾

- [ ] **Step 1: 全量后端测试 + 编译**

Run: `cd src-tauri && cargo test 2>&1 | grep "test result:" | head -1; cargo build 2>&1 | grep -iE "error|warning" || echo clean`
Expected: 全过 + clean。

- [ ] **Step 2: 前端类型 + 构建**

Run: `cd /Users/zhoumo/Documents/Claude/collector && npx tsc --noEmit && npm run build 2>&1 | tail -1`
Expected: 无类型错 + 构建成功。

- [ ] **Step 3: 完整场景运行验证**

`npm run tauri:dev`，逐一确认：
- 影视单条目跨分类移动（电影→动漫）：库分类变、字幕跟随、播放正常。
- 影视文件夹跨分类移动：整棵子树迁移、字幕目录跟随。
- 漫画/游戏内部单条目 + 文件夹移动。
- 跨类型隔离：影视移动树里无漫画/游戏节点（选不到）。
- 移进自身子树被拒；同名冲突被拒。

- [ ] **Step 4: 工作树确认**

Run: `git status --short && git log --oneline -6`
Expected: 工作树干净；6 个任务提交在列。
