# 主页分割线微调 + 就地新增（视频/漫画/游戏）· 设计

日期：2026-09-15
分支：master（本次在 master 上新建特性分支实现）

## 背景与目标

三项需求：
1. **主页版本分割线下移**：当前分割线对齐左侧菜单「工具箱」项的**上沿**，改为对齐「工具箱」项的**下沿**（略往下移）。
2. **新增视频改为「就地新增」**：新增的视频默认落在**当前浏览的文件夹**，而非分类根；**根目录（分类根）不支持新增，故不显示「新增」按钮**。
3. **同一「就地新增」能力复用到漫画、游戏模块**。

## 现状与关键约束

- 主页版本区 `.home-ver` 用 `position:absolute; bottom:58px` 对齐工具箱上沿（`theme.css` 注释已记录 58px 的推导）。
- `openEditDrawer(item, onSaved, defaultCategory, kind)`：新增时把 `category_path` 设为 `defaultCategory`（分类根，如「电影」），**未使用当前文件夹路径**。
- **只有 VideoView 有「+ 新增视频」按钮，且常显**；ComicView/GameView **无新增入口**。
- EditDrawer 对 comic/game **只有编辑分支（`if (item?.id)`），无 create 分支**。
- 后端 `library::create_item(db, kind, item)` **已支持 video/comic/game 三类 INSERT**；但命令层只有 `media_create`（硬编码 Video），**无 comic/game create 命令**。
- `FolderView` 的当前路径经 `onNav(p)` 回传外层 `folderPath`，**不触发重渲染**；分类根路径为空串 `""`。

## 涉及文件

- `src/styles/theme.css`（`.home-ver` bottom）
- `src/components/EditDrawer.ts`（新增落点 + comic/game create 分支）
- `src/lib/ipc.ts`（comic/game create 绑定）
- `src-tauri/src/lib.rs`（create 命令泛化/新增）
- `src/views/VideoView.ts`、`src/views/ComicView.ts`、`src/views/GameView.ts`（新增按钮 + 就地落点 + 显隐规则）

---

## 需求 1 · 主页分割线下移对齐工具箱下沿

「工具箱」是侧栏底部倒数第 2 项，其**下沿** = 「设置」项的**上沿**。当前 `bottom:58px` ≈ 工具箱上沿到窗口底的距离（含设置项高约 36 + gap 6 + 底 padding 16）。下移到工具箱下沿即减去一个「设置项高 + gap」≈ 42px，得 `bottom ≈ 16~20px`。

**改法**：`.home-ver` 的 `bottom` 从 `58px` 改为约 `20px`（对齐工具箱下沿 = 设置项上沿），并更新其上方注释说明新的对齐目标与推导。实现时在应用中目视校准，微调到分割线与工具箱项下边缘齐平。

**验收**：主页版本分割线的横线与左侧菜单「工具箱」项的下边缘对齐（比之前略低）。

---

## 需求 2 · 视频就地新增 + 根目录隐藏按钮

### 落点：当前文件夹
新增按钮点击从 `openEditDrawer(null, refresh, activeCat)` 改为传入**当前文件夹的分类内相对路径**作为落点。

`openEditDrawer` 当前签名的 `defaultCategory` 同时被用作 `category` 和 `category_path`（`categoryPath = item?.category_path ?? defaultCategory`）。就地新增需要 `category`（分类名，如「电影」）与 `category_path`（当前文件夹完整相对路径，如「电影/谍战」）**分别指定**。

**改法**：给 `openEditDrawer` 增加一个可选参数 `createPath?: string`（新增时的落点分类路径）。新增（`item===null`）时：
- `category` = `defaultCategory`（分类名，用于视频 media 表的 category 列）。
- `categoryPath` = `createPath ?? defaultCategory`（缺省仍落分类根，向后兼容）。

VideoView 调用改为 `openEditDrawer(null, refresh, activeCat, "video", folderPath)`。

> 路径语义已核实一致：视频 `category_path` 的存库格式即「分类内相对路径、不含分类名」（`videoTree.ts` 注释与 `buildVideoTree` 确认，如 "科幻/星战"，根为空串 `""`）。而 `folderPath` 就是树节点的 `path`，语义完全相同。因此就地新增时 `categoryPath` **直接传 `folderPath`，无需拼分类前缀**。按显隐规则新增按钮只在非根出现，故 `folderPath` 总是具体子路径（非空），落点即当前文件夹。`category`（分类名，如「电影」）仍传 `activeCat`，用于 video media 表的 category 列。

### 显隐：根目录不显示，仅文件夹视图显示
- 新增按钮**仅在**「文件夹视图（mode==="folder"）且当前不在分类根（folderPath !== ""）」时显示；树视图或分类根时隐藏（树视图无「当前文件夹」落点语义）。
- 因 `FolderView` 的 `onNav` 不触发外层重渲染，需在 `onNav` 回调里**同步更新按钮显隐**（直接切换按钮的 `display`，不整体重渲染以免打断浏览）。切换类目/视图模式时经 `render()` 重建，按钮初始显隐按当前 `folderPath`/`mode` 计算。

**验收**：在分类根看不到「新增视频」按钮；进入任一子文件夹后按钮出现；点击后新增条目落在该文件夹；返回根按钮再次消失；树视图下不显示该按钮。

---

## 需求 3 · 漫画 / 游戏复用就地新增

### 后端：create 命令覆盖三类
`library::create_item` 已支持三类，只需命令层暴露。**改法**：将 `media_create` 泛化为接受 `kind` 参数：

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

（`build_video_item` 只是组装 `ScannedItem`，与 kind 无关，名字可保留或改为 `build_scanned_item`；本次沿用以减小改动面。comic/game 的 category 列由 `create_item` 内部写死「漫画」「游戏」，故传入的 `category` 对 comic/game 不影响存库，仅 video 用。）

### 前端 ipc
`mediaCreate` 增加 `kind` 参数：
```typescript
mediaCreate: (kind: string, category: string, categoryPath: string, title: string,
  coverPath: string | null, description: string | null) =>
  invoke<number>("media_create", { kind, category, categoryPath, title, coverPath, description }),
```

### EditDrawer：comic/game 的 create 分支
当前 comic/game 只在 `item?.id` 时 update。新增（`item===null`）时应调用 `api.mediaCreate(kind, ...)`：
- comic：`api.mediaCreate("comic", category, categoryPath, title, cover, desc)`（category 传占位如空串或分类名，存库被写死「漫画」）。
- game：`api.mediaCreate("game", ...)`。
- video：`api.mediaCreate("video", ...)`（替换现有 `mediaCreate` 调用，补上 kind）。

### ComicView / GameView：新增按钮
在两视图的 `video-bar-right`（工具栏右侧）加与视频一致的「+ 新增」按钮（漫画标签「+ 新增漫画」、游戏「+ 新增游戏」），套用同样的显隐规则（文件夹视图 + 非根显示）与就地落点（传当前 `folderPath`）。调用 `openEditDrawer(null, refresh, "", "comic", folderPath)` / `("game", folderPath)`。

**验收**：漫画、游戏视图在子文件夹下出现「新增」按钮、根目录隐藏；新增条目落在当前文件夹；新增后列表刷新可见。

---

## 测试策略

- 后端：`media_create` 泛化后，现有 `cargo test --lib` 保持通过；`from_kind_str` 已有测试覆盖非法 kind。可加一条测试验证 `create_item` 对 comic/game 落到对应表（若既有测试未覆盖）。
- 前端：无单测，靠 `tsc --noEmit` + 应用手动验证三类新增的落点与按钮显隐。
- 集成：三个视图在根/子文件夹/树视图下的按钮显隐；三类新增后重扫或刷新的归类正确。

## 非目标（YAGNI）

- 不给新增抽屉增加「可编辑所在目录」字段（用户采用 A 方案：进目录再新增）。
- 不改树视图的新增语义（树视图不提供就地新增）。
- 不改 `category_path` 的既有存库格式与扫描逻辑。
- 不动本轮之外的其它 UI。
