# 主页分割线微调 + 就地新增（视频/漫画/游戏）实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 主页版本分割线下移对齐工具箱下沿；视频/漫画/游戏三类支持「就地新增」——新增默认落在当前文件夹，分类根不显示新增按钮。

**Architecture:** 后端把 `media_create` 泛化为带 `kind` 参数（`library::create_item` 已支持三类）；前端 `openEditDrawer` 增加 `createPath` 落点参数并为 comic/game 补 create 分支；三个列表视图加「+ 新增」按钮，仅文件夹视图非根显示、落点为当前 `folderPath`。主页分割线为纯 CSS 调整。

**Tech Stack:** Rust（Tauri command / rusqlite）、TypeScript（vanilla-ts）、CSS。

**分支：** 实现前先从 master 建特性分支（如 `feat/inplace-add`）。

---

## Task 0: 建特性分支

- [ ] **Step 1: 从最新 master 建分支**

```bash
cd /Users/zhoumo/Documents/Claude/collector
git checkout master && git checkout -b feat/inplace-add
git log --oneline -1
```
Expected: 新分支 `feat/inplace-add`，HEAD 为 master 最新提交（含前述所有已合并工作）。

---

## Task 1: 后端 media_create 泛化为带 kind 参数

**Files:**
- Modify: `src-tauri/src/lib.rs`（`media_create` 函数）

- [ ] **Step 1: 改 `media_create` 签名与实现**

当前 `media_create`（约 373 行）：

```rust
fn media_create(
    app: tauri::AppHandle,
    db: tauri::State<Db>,
    category: String,
    category_path: String,
    title: String,
    cover_path: Option<String>,
    description: Option<String>,
) -> AppResult<i64> {
    let app_data = app.path().app_data_dir().ok()
        .map(|p| p.to_string_lossy().into_owned()).unwrap_or_default();
    let rel_cover = cover_path.map(|c| library::paths::appdata_to_relative(&c, &app_data));
    let it = build_video_item(category, category_path, title, rel_cover, description);
    library::create_item(&db, MediaKind::Video, &it)
}
```

注意：它当前**没有** `#[tauri::command]` 属性行紧贴？先确认——`grep -n "media_create" src-tauri/src/lib.rs` 查看其上方的 `#[tauri::command...]` 行。改后带上 `rename_all = "camelCase"`（因新增了多词参数 `category_path` 已是既有，`kind` 单词无影响，但统一用 camelCase 更稳）。替换整个函数（含其属性行）为：

```rust
#[tauri::command(rename_all = "camelCase")]
fn media_create(
    app: tauri::AppHandle,
    db: tauri::State<Db>,
    kind: String,
    category: String,
    category_path: String,
    title: String,
    cover_path: Option<String>,
    description: Option<String>,
) -> AppResult<i64> {
    let media_kind = MediaKind::from_kind_str(&kind)?;
    let app_data = app.path().app_data_dir().ok()
        .map(|p| p.to_string_lossy().into_owned()).unwrap_or_default();
    let rel_cover = cover_path.map(|c| library::paths::appdata_to_relative(&c, &app_data));
    let it = build_video_item(category, category_path, title, rel_cover, description);
    library::create_item(&db, media_kind, &it)
}
```

`MediaKind::from_kind_str` 已存在于 `library::model`（video/comic/game → 对应 kind，非法值 Err）。确认 `MediaKind` 已在 lib.rs 的 use 范围（现有 `media_create` 用了 `MediaKind::Video`，说明已在作用域）。

- [ ] **Step 2: 编译**

Run: `cd /Users/zhoumo/Documents/Claude/collector/src-tauri && cargo build 2>&1 | tail -12`
Expected: 编译通过。若 `#[tauri::command]` 属性原先不在（重复添加导致），按实际现状只保留一个属性行。

- [ ] **Step 3: 测试**

Run: `cd /Users/zhoumo/Documents/Claude/collector/src-tauri && cargo test --lib 2>&1 | tail -8`
Expected: 全部通过。

- [ ] **Step 4: 提交**

```bash
git add src-tauri/src/lib.rs
git commit -m "feat(be): media_create 泛化为带 kind 参数，覆盖 video/comic/game"
```

---

## Task 2: 前端 ipc mediaCreate 增加 kind 参数

**Files:**
- Modify: `src/lib/ipc.ts`
- Modify: `src/components/EditDrawer.ts`（现有 video create 调用需补 kind，避免编译错）

- [ ] **Step 1: 改 ipc 绑定**

`src/lib/ipc.ts` 当前：

```typescript
  mediaCreate: (category: string, categoryPath: string, title: string,
    coverPath: string | null, description: string | null) =>
    invoke<number>("media_create", { category, categoryPath, title, coverPath, description }),
```

改为：

```typescript
  mediaCreate: (kind: string, category: string, categoryPath: string, title: string,
    coverPath: string | null, description: string | null) =>
    invoke<number>("media_create", { kind, category, categoryPath, title, coverPath, description }),
```

- [ ] **Step 2: 修正 EditDrawer 现有 video create 调用**

`src/components/EditDrawer.ts` 保存段（约 115 行）当前：

```typescript
      } else {
        await api.mediaCreate(category, categoryPath, title,
          coverPath || null, desc || null);
      }
```

改为（补 kind="video"）：

```typescript
      } else {
        await api.mediaCreate("video", category, categoryPath, title,
          coverPath || null, desc || null);
      }
```

- [ ] **Step 3: 类型检查**

Run: `cd /Users/zhoumo/Documents/Claude/collector && npx tsc --noEmit 2>&1 | tail -10`
Expected: 无类型错误（此步仅让现有调用适配新签名；comic/game 的 create 分支在 Task 3 添加）。

- [ ] **Step 4: 提交**

```bash
git add src/lib/ipc.ts src/components/EditDrawer.ts
git commit -m "feat(fe): mediaCreate 增加 kind 参数并适配视频新增调用"
```

---

## Task 3: EditDrawer 支持就地落点 + comic/game 新增分支

**Files:**
- Modify: `src/components/EditDrawer.ts`

- [ ] **Step 1: openEditDrawer 增加 createPath 参数，新增时用作落点**

`EditDrawer.ts` 顶部签名（约 13 行）当前：

```typescript
export function openEditDrawer(item: MediaItem | null, onSaved: () => void, defaultCategory = "电影", kind: "video" | "comic" | "game" = "video"): void {
```

改为增加可选 `createPath`：

```typescript
export function openEditDrawer(item: MediaItem | null, onSaved: () => void, defaultCategory = "电影", kind: "video" | "comic" | "game" = "video", createPath?: string): void {
```

紧随其后的（约 18-19 行）：

```typescript
  const category = item?.category ?? defaultCategory;
  const categoryPath = item?.category_path ?? defaultCategory;
```

改为（新增时 categoryPath 用 createPath，缺省回退 defaultCategory 保持向后兼容）：

```typescript
  const category = item?.category ?? defaultCategory;
  const categoryPath = item?.category_path ?? createPath ?? defaultCategory;
```

- [ ] **Step 2: 给 comic/game 添加新增（create）分支**

保存段当前（约 100-118 行）comic/game 只在 `item?.id` 时 update：

```typescript
      if (kind === "comic") {
        // 漫画只编辑现有条目（item 必有值），保留原分类路径
        if (item?.id) {
          await api.comicUpdate(item.id, categoryPath, title,
            coverPath || null, desc || null);
        }
      } else if (kind === "game") {
        // 游戏只编辑现有条目（item 必有值），保留原分类路径
        if (item?.id) {
          await api.gameUpdate(item.id, categoryPath, title,
            coverPath || null, desc || null);
        }
      } else if (item?.id) {
        await api.mediaUpdate(item.id, category, categoryPath, title,
          coverPath || null, desc || null);
      } else {
        await api.mediaCreate("video", category, categoryPath, title,
          coverPath || null, desc || null);
      }
```

改为（comic/game 无 item 时走 mediaCreate 对应 kind；comic/game 的 category 存库被后端写死，此处传空串占位）：

```typescript
      if (kind === "comic") {
        if (item?.id) {
          await api.comicUpdate(item.id, categoryPath, title,
            coverPath || null, desc || null);
        } else {
          await api.mediaCreate("comic", "", categoryPath, title,
            coverPath || null, desc || null);
        }
      } else if (kind === "game") {
        if (item?.id) {
          await api.gameUpdate(item.id, categoryPath, title,
            coverPath || null, desc || null);
        } else {
          await api.mediaCreate("game", "", categoryPath, title,
            coverPath || null, desc || null);
        }
      } else if (item?.id) {
        await api.mediaUpdate(item.id, category, categoryPath, title,
          coverPath || null, desc || null);
      } else {
        await api.mediaCreate("video", category, categoryPath, title,
          coverPath || null, desc || null);
      }
```

- [ ] **Step 3: 类型检查**

Run: `cd /Users/zhoumo/Documents/Claude/collector && npx tsc --noEmit 2>&1 | tail -10`
Expected: 无类型错误。

- [ ] **Step 4: 提交**

```bash
git add src/components/EditDrawer.ts
git commit -m "feat(fe): EditDrawer 支持就地落点 createPath 与漫画/游戏新增分支"
```

---

## Task 4: VideoView 就地新增 + 按钮显隐规则

**Files:**
- Modify: `src/views/VideoView.ts`

- [ ] **Step 1: 新增按钮点击传当前 folderPath 作落点**

`VideoView.ts` 约 78 行当前：

```typescript
    el.querySelector<HTMLButtonElement>(".add-video-btn")!.onclick = () => openEditDrawer(null, refresh, activeCat);
```

改为：

```typescript
    el.querySelector<HTMLButtonElement>(".add-video-btn")!.onclick = () => openEditDrawer(null, refresh, activeCat, "video", folderPath);
```

- [ ] **Step 2: 按钮显隐——仅文件夹视图 + 非根显示，并随导航更新**

`render` 内构造工具栏后（约 78 行 onclick 绑定附近），加入一个更新按钮显隐的 helper 并初始调用；同时在 folder 视图的 `onNav` 回调里调用它。

在 `render` 里，`add-video-btn` 的 onclick 绑定之后，添加：

```typescript
    const addBtn = el.querySelector<HTMLButtonElement>(".add-video-btn")!;
    const updateAddBtn = () => {
      // 仅文件夹视图且非分类根（folderPath!=="")时显示新增按钮
      addBtn.style.display = (mode === "folder" && folderPath !== "") ? "" : "none";
    };
    updateAddBtn();
```

然后把 folder 视图的 `onNav` 回调（约 82 行 `(p) => { folderPath = p; }`）改为同时更新按钮：

```typescript
      ? FolderView(tree, onOpen, onContext, folderPath, (p) => { folderPath = p; updateAddBtn(); }, (oldPath, newPath) => {
```

（其余 `onRenamed` 回调与参数保持不变。）

注意：`updateAddBtn` 定义需在 `FolderView(...)` 调用之前，以便 `onNav` 闭包能引用它。若现有代码顺序是先绑定 onclick 再 appendChild FolderView，则 `updateAddBtn` 定义位置在两者之间即可（onclick 段之后、body.appendChild 之前）。实现时确保引用顺序正确。

- [ ] **Step 3: 类型检查**

Run: `cd /Users/zhoumo/Documents/Claude/collector && npx tsc --noEmit 2>&1 | tail -10`
Expected: 无类型错误。

- [ ] **Step 4: 提交**

```bash
git add src/views/VideoView.ts
git commit -m "feat(fe): 视频就地新增，新增按钮仅文件夹视图非根显示"
```

---

## Task 5: ComicView 新增按钮 + 就地新增

**Files:**
- Modify: `src/views/ComicView.ts`

- [ ] **Step 1: 在工具栏右侧加「+ 新增漫画」按钮**

读 `ComicView.ts` 的 `render`，找到 `<div class="video-bar-right">`（约 49 行），在 `view-toggle` 之前加入按钮，与 VideoView 结构一致：

```html
          <button class="add-video-btn">+ 新增漫画</button>
```

（复用 `add-video-btn` class 以套用相同样式。）

- [ ] **Step 2: 绑定 onclick + 显隐 helper + onNav 更新**

在 `render` 内 `.vt-btn` 绑定附近，加入：

```typescript
    const addBtn = el.querySelector<HTMLButtonElement>(".add-video-btn")!;
    const updateAddBtn = () => {
      addBtn.style.display = (mode === "folder" && folderPath !== "") ? "" : "none";
    };
    addBtn.onclick = () => openEditDrawer(null, refresh, "", "comic", folderPath);
    updateAddBtn();
```

把 folder 视图 `onNav`（约 61 行 `(p) => { folderPath = p; }`）改为：

```typescript
      ? FolderView(tree, onOpen, onContext, folderPath, (p) => { folderPath = p; updateAddBtn(); }, undefined, false, "comic")
```

（保持 `onRenamed=undefined, enableRename=false, kind="comic"` 不变。）

- [ ] **Step 3: 类型检查**

Run: `cd /Users/zhoumo/Documents/Claude/collector && npx tsc --noEmit 2>&1 | tail -10`
Expected: 无类型错误。

- [ ] **Step 4: 提交**

```bash
git add src/views/ComicView.ts
git commit -m "feat(fe): 漫画就地新增，复用新增按钮与显隐规则"
```

---

## Task 6: GameView 新增按钮 + 就地新增

**Files:**
- Modify: `src/views/GameView.ts`

- [ ] **Step 1: 在工具栏右侧加「+ 新增游戏」按钮**

读 `GameView.ts` 的 `render`，找到 `<div class="video-bar-right">`（约 58 行），在 `view-toggle` 之前加入：

```html
          <button class="add-video-btn">+ 新增游戏</button>
```

- [ ] **Step 2: 绑定 onclick + 显隐 helper + onNav 更新**

在 `render` 内 `.vt-btn` 绑定附近，加入：

```typescript
    const addBtn = el.querySelector<HTMLButtonElement>(".add-video-btn")!;
    const updateAddBtn = () => {
      addBtn.style.display = (mode === "folder" && folderPath !== "") ? "" : "none";
    };
    addBtn.onclick = () => openEditDrawer(null, refresh, "", "game", folderPath);
    updateAddBtn();
```

把 folder 视图 `onNav`（约 70 行 `(p) => { folderPath = p; }`）改为同时 `updateAddBtn()`：

```typescript
      ? FolderView(tree, onOpen, onContext, folderPath, (p) => { folderPath = p; updateAddBtn(); },
          () => { refresh(); }, true, "game")
```

（保持 `onRenamed`、`enableRename=true`、`kind="game"` 等既有参数不变——按实际现状复制。）

- [ ] **Step 3: 类型检查**

Run: `cd /Users/zhoumo/Documents/Claude/collector && npx tsc --noEmit 2>&1 | tail -10`
Expected: 无类型错误。

- [ ] **Step 4: 提交**

```bash
git add src/views/GameView.ts
git commit -m "feat(fe): 游戏就地新增，复用新增按钮与显隐规则"
```

---

## Task 7: 主页分割线下移对齐工具箱下沿

**Files:**
- Modify: `src/styles/theme.css`（`.home-ver`）

- [ ] **Step 1: 改 bottom 值 + 更新注释**

`theme.css` 约 381-383 行当前：

```css
/* 版本区绝对定位在内容区底部：分割线（上边框）对齐左侧菜单「工具箱」项的上沿。
   58px ≈ 侧栏底 padding(16) + 设置项高(约36) + gap(6)，即工具箱上沿到窗口底的距离。 */
.home-ver{position:absolute;left:clamp(32px,5vw,72px);right:clamp(32px,5vw,72px);bottom:58px;
```

改为（对齐工具箱下沿 = 设置项上沿，约减去设置项高+gap ≈ 42px）：

```css
/* 版本区绝对定位在内容区底部：分割线（上边框）对齐左侧菜单「工具箱」项的下沿
   （即「设置」项的上沿）。20px ≈ 侧栏底 padding(16) + 少量间隙，即设置项上沿到窗口底的距离。 */
.home-ver{position:absolute;left:clamp(32px,5vw,72px);right:clamp(32px,5vw,72px);bottom:20px;
```

- [ ] **Step 2: 类型检查（确保 CSS 改动未误伤 ts 引用，形式性）**

Run: `cd /Users/zhoumo/Documents/Claude/collector && npx tsc --noEmit 2>&1 | tail -5`
Expected: 无错误。

- [ ] **Step 3: 提交**

```bash
git add src/styles/theme.css
git commit -m "style(home): 版本分割线下移对齐工具箱下沿"
```

---

## Task 8: 整体验证

**Files:** 无（验证任务）

- [ ] **Step 1: 全量构建**

Run: `cd /Users/zhoumo/Documents/Claude/collector && npx tsc --noEmit && (cd src-tauri && cargo build 2>&1 | tail -3 && cargo test --lib 2>&1 | tail -6)`
Expected: tsc 无错误；cargo build 完成；cargo test --lib 全绿。

- [ ] **Step 2: 手动验证清单（运行应用中确认）**

- 主页版本分割线位置比之前略低，与左侧「工具箱」项下边缘对齐。
- 影视：分类根无「新增视频」按钮；进子文件夹后按钮出现；点击填写后新增条目落在该文件夹（返回可见）；返回根按钮消失；树视图下不显示按钮。
- 漫画：子文件夹出现「新增漫画」按钮、根隐藏；新增落当前文件夹、刷新可见。
- 游戏：子文件夹出现「新增游戏」按钮、根隐藏；新增落当前文件夹、刷新可见。

- [ ] **Step 3: 前序任务已各自提交，本任务无额外提交**

---

## 自审记录

- **Spec 覆盖**：需求1→Task7；需求2（就地落点）→Task1(后端)+Task2(ipc)+Task3(drawer)+Task4(video)；需求2（根隐藏按钮）→Task4 Step2；需求3→Task1(kind 泛化)+Task3(comic/game create 分支)+Task5(comic 按钮)+Task6(game 按钮)。全覆盖。
- **依赖顺序**：Task1(后端 kind)→Task2(ipc 适配，含修正现有 video 调用避免编译错)→Task3(drawer create 分支)→Task4/5/6(视图按钮)→Task7(CSS,独立)。顺序保证每步都能编译通过。
- **类型/命名一致**：`mediaCreate(kind, category, categoryPath, title, coverPath, description)` 签名在 ipc 与所有调用点一致；`openEditDrawer(item, onSaved, defaultCategory, kind, createPath?)` 新增参数位置一致；`updateAddBtn`/`addBtn` 在三视图同名同义；显隐条件三视图统一 `mode==="folder" && folderPath!==""`。
- **路径语义已核实**：`folderPath` == 存库 `category_path`（分类内相对路径、不含分类名），直接传递无需拼前缀。
- **无占位符**：每步含确切文件位置、完整代码、确切命令与预期。bottom:20px 为初值，Task8 目视校准属正常范畴（spec 已声明）。
