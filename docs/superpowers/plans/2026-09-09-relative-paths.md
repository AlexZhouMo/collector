# 存储路径相对化重构 实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development or superpowers:executing-plans. Steps use checkbox (`- [ ]`).

**Goal:** 数据库存相对路径（视频相对分类根、字幕/封面相对 app_data、category_path 去分类名），后端 list_media 读时拼绝对，前端/播放器零改动；迁移存量数据并重置 ID。

**Architecture:** 后端加纯函数做相对↔绝对转换；写库处转相对，list_media 命令层读时拼绝对；前端 videoTree 去 slice(1)；一次性迁移命令重写库。

**设计文档:** docs/superpowers/specs/2026-09-09-relative-paths-design.md

**既有形态:**
- MediaItem { id, kind, category, category_path, title, path, subtitle_path:Option, cover_path:Option, description, platform_ok, exec_path }
- media_create/update(db, category, category_path, title, path, subtitle_path, cover_path, description) → build_video_item → library::create_item/update_item
- list_items(db, kind) → Vec<MediaItem>（纯读）
- settings::get(db, "video_movie_root"|"video_anime_root"|"video_tv_root")
- app_data: app.path().app_data_dir()；covers = app_data/covers；新增 subtitles = app_data/subtitles
- 前端 buildVideoTree: category_path.split("/").slice(1) 去分类名首段（line 21）
- import_cover/import_cover_cropped/save_cover 现返回绝对；poster save_cover 存绝对

---

### Task 1: 路径转换纯函数 + 单测（src-tauri/src/library/paths.rs 新建）

**Files:** Create `src-tauri/src/library/paths.rs`；Modify `src-tauri/src/library/mod.rs`（加 `pub mod paths;`）

- [ ] **Step 1: 写函数 + 测试**

创建 paths.rs：
```rust
//! 路径相对化/绝对化转换。数据库存相对，读取拼绝对。
use std::path::Path;

/// 分类名 → settings 中的 video root key。
pub fn video_root_key(category: &str) -> &'static str {
    match category {
        "电影" => "video_movie_root",
        "动漫" => "video_anime_root",
        "剧集" => "video_tv_root",
        _ => "video_movie_root",
    }
}

/// 绝对视频路径 → 分类根下相对（去掉 root 前缀）。root 为空或不匹配则原样返回。
pub fn video_to_relative(abs: &str, root: &str) -> String {
    if root.is_empty() { return abs.to_string(); }
    match Path::new(abs).strip_prefix(root) {
        Ok(rel) => rel.to_string_lossy().replace('\\', "/"),
        Err(_) => abs.to_string(),
    }
}

/// 相对视频路径 + root → 绝对。root 为空则原样返回相对。
pub fn video_to_absolute(rel: &str, root: &str) -> String {
    if root.is_empty() || rel.is_empty() { return rel.to_string(); }
    if Path::new(rel).is_absolute() { return rel.to_string(); }
    format!("{}/{}", root.trim_end_matches('/'), rel)
}

/// 绝对 app_data 内路径 → 相对 app_data（保留 covers/ 或 subtitles/ 段）。
pub fn appdata_to_relative(abs: &str, app_data: &str) -> String {
    match Path::new(abs).strip_prefix(app_data) {
        Ok(rel) => rel.to_string_lossy().replace('\\', "/"),
        Err(_) => abs.to_string(),
    }
}

/// 相对 app_data 路径 + app_data → 绝对。空则空。
pub fn appdata_to_absolute(rel: &str, app_data: &str) -> String {
    if rel.is_empty() { return String::new(); }
    if Path::new(rel).is_absolute() { return rel.to_string(); }
    format!("{}/{}", app_data.trim_end_matches('/'), rel)
}

/// 去掉 category_path 首段分类名：电影/科幻/星战 → 科幻/星战；仅分类名 → 空串。
pub fn strip_category(category_path: &str) -> String {
    let mut it = category_path.splitn(2, '/');
    it.next(); // 丢弃首段
    it.next().unwrap_or("").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn video_rel_abs_roundtrip() {
        let abs = "/media/电影根/科幻/星战/x.mkv";
        let rel = video_to_relative(abs, "/media/电影根");
        assert_eq!(rel, "科幻/星战/x.mkv");
        assert_eq!(video_to_absolute(&rel, "/media/电影根"), abs);
    }
    #[test]
    fn video_empty_root_passthrough() {
        assert_eq!(video_to_relative("/a/b.mkv", ""), "/a/b.mkv");
        assert_eq!(video_to_absolute("科幻/x.mkv", ""), "科幻/x.mkv");
    }
    #[test]
    fn appdata_rel_abs() {
        assert_eq!(appdata_to_relative("/app/covers/x.jpg", "/app"), "covers/x.jpg");
        assert_eq!(appdata_to_absolute("covers/x.jpg", "/app"), "/app/covers/x.jpg");
        assert_eq!(appdata_to_absolute("", "/app"), "");
    }
    #[test]
    fn strip_category_works() {
        assert_eq!(strip_category("电影/科幻/星战"), "科幻/星战");
        assert_eq!(strip_category("电影"), "");
    }
}
```
mod.rs 顶部加 `pub mod paths;`

- [ ] **Step 2: 测试**

Run: `cd src-tauri && cargo test library::paths 2>&1 | tail -8`
Expected: 4 测试 PASS。

- [ ] **Step 3: Commit**
```bash
git add src-tauri/src/library/paths.rs src-tauri/src/library/mod.rs
git commit -m "feat(paths): 路径相对/绝对转换纯函数+单测"
```

---

### Task 2: 新建 subtitles 目录

**Files:** Modify `src-tauri/src/lib.rs`（setup）

- [ ] **Step 1:** setup 里 covers 创建旁边加 subtitles：
在 setup 中（app_data_dir 后）加：
```rust
let ad = app.path().app_data_dir().expect("app data dir");
std::fs::create_dir_all(ad.join("covers")).ok();
std::fs::create_dir_all(ad.join("subtitles")).ok();
```
（若已有 covers create，则补 subtitles 一行。）

- [ ] **Step 2:** build 通过。Commit：`feat: setup 创建 subtitles 目录（与 covers 同级）`

---

### Task 3: list_media 读时拼绝对（lib.rs 命令层）

**Files:** Modify `src-tauri/src/lib.rs`（list_media 命令）

- [ ] **Step 1:** list_media 命令改为拿 app + db，调 list_items 后拼绝对：
```rust
#[tauri::command]
fn list_media(app: tauri::AppHandle, db: tauri::State<Db>, kind: String) -> AppResult<Vec<MediaItem>> {
    use tauri::Manager;
    let k = MediaKind::from_kind_str(&kind)?;
    let mut items = library::list_items(&db, k)?;
    if matches!(k, MediaKind::Video) {
        let app_data = app.path().app_data_dir().ok()
            .map(|p| p.to_string_lossy().into_owned()).unwrap_or_default();
        for it in &mut items {
            // 视频 path：按 category 取 root 拼
            let root = settings::get(&db, library::paths::video_root_key(&it.category))?.unwrap_or_default();
            it.path = library::paths::video_to_absolute(&it.path, &root);
            if let Some(s) = &it.subtitle_path {
                it.subtitle_path = Some(library::paths::appdata_to_absolute(s, &app_data));
            }
            if let Some(c) = &it.cover_path {
                it.cover_path = Some(library::paths::appdata_to_absolute(c, &app_data));
            }
        }
    }
    Ok(items)
}
```
（原 list_media 若只有 db 参数，加 app 参数；Tauri 自动注入。前端 invoke 不变——多注入的 app 参数由框架提供。）

- [ ] **Step 2:** build 通过；真机 list 正常（暂时数据仍是绝对，拼接对绝对是幂等的：video_to_absolute 判 is_absolute 原样返回，appdata_to_absolute 同理）。这保证**迁移前不破坏现状**。

- [ ] **Step 3:** Commit：`feat(paths): list_media 读时拼绝对路径`

---

### Task 4: 写库处转相对（lib.rs CRUD + cover + 抓取 save_cover）

**Files:** Modify `src-tauri/src/lib.rs`（media_create/update、import_cover*、fetch_posters 的 save）；`src-tauri/src/poster/image_proc.rs`（save_cover 返回相对可选）

- [ ] **Step 1:** media_create/media_update 加 app 参数，存前转相对：
在 build_video_item 前把 path/subtitle_path/cover_path 转相对：
```rust
// media_update / media_create 内，构造 it 前：
use tauri::Manager;
let app_data = app.path().app_data_dir().ok().map(|p| p.to_string_lossy().into_owned()).unwrap_or_default();
let root = settings::get(&db, library::paths::video_root_key(&category))?.unwrap_or_default();
let rel_path = library::paths::video_to_relative(&path, &root);
let rel_sub = subtitle_path.map(|s| library::paths::appdata_to_relative(&s, &app_data));
let rel_cover = cover_path.map(|c| library::paths::appdata_to_relative(&c, &app_data));
let it = build_video_item(category, category_path, title, rel_path, rel_sub, rel_cover, description);
```
（两个命令都加 `app: tauri::AppHandle` 参数。）

- [ ] **Step 2:** import_cover/import_cover_cropped：现返回绝对 covers 路径。改为返回相对 `covers/xxx.jpg`。save_cover 保持写绝对文件，但返回相对——或在命令层把返回值 appdata_to_relative。最简：命令返回前 `paths::appdata_to_relative(&abs, &app_data)`。
- fetch_posters 的 save_cover 回填 cover_path：存前也转相对（appdata_to_relative）。

- [ ] **Step 3:** build + cargo test 通过。真机：新增/编辑视频存的是相对，list 读回拼绝对显示正常。

- [ ] **Step 4:** Commit：`feat(paths): 写库处路径转相对（CRUD/封面/抓取）`

---

### Task 5: 前端树去分类名连锁（videoTree.ts + VideoView.ts）

**Files:** Modify `src/lib/videoTree.ts`、`src/views/VideoView.ts`

- [ ] **Step 1:** buildVideoTree 去 slice(1)：
category_path 现为分类内相对（如「科幻/星战」）。改：
```ts
export function buildVideoTree(category: string, items: MediaItem[]): TreeNode {
  const root: TreeNode = { name: category, path: "", children: [], items: [] };
  for (const it of items) {
    const inner = it.category_path ? it.category_path.split("/").filter(Boolean) : [];
    let node = root; let prefix = "";
    for (const seg of inner) {
      prefix = prefix ? `${prefix}/${seg}` : seg;
      let child = node.children.find(c => c.name === seg);
      if (!child) { child = { name: seg, path: prefix, children: [], items: [] }; node.children.push(child); }
      node = child;
    }
    node.items.push(it);
  }
  sortTree(root); return root;
}
```
（根 path 空串；节点 path 为分类内相对路径，不含分类名。collectFolderPaths/findNode 语义随之为分类内相对。）

- [ ] **Step 2:** VideoView 移动功能：fp 现为分类内相对路径（含空串=根），mediaUpdate 存 category_path 直接用 fp。collectFolderPaths 返回含空串（根）——移动菜单「根」项 label 处理空串显示为「分类名（根）」。面包屑等如需分类名前端补 activeCat。

- [ ] **Step 3:** tsc 通过。真机：树展示/进文件夹/移动/编辑正常，category_path 存分类内相对。

- [ ] **Step 4:** Commit：`feat(paths): 前端树用分类内相对 category_path（去分类名）`

---

### Task 6: 重写初始化迁移命令

**Files:** Modify `src-tauri/src/lib.rs`（新命令 migrate_to_relative 或改造）

- [ ] **Step 1:** 新命令 migrate_to_relative(app, db)：
- 提示：执行前用户应手动备份 collector.sqlite。
- 读全部 video 行（含绝对 path/category_path/subtitle/cover）。
- 按 category（电影>动漫>剧集 固定序）、再 category_path 排序。
- 逐条转相对：
  - path: video_to_relative(abs, root_of_category)
  - category_path: strip_category(category_path)
  - subtitle/cover: appdata_to_relative
- 清库（DELETE FROM media_item; DELETE FROM sqlite_sequence WHERE name='media_item'）。
- 按排序重插，ID 从 1。
- 复用 replace_items 或 insert_one_tx 模式。

- [ ] **Step 2:** 注册命令；前端设置页加「迁移为相对路径」按钮（一次性），或直接脚本触发。加确认提示。

- [ ] **Step 3:** build + test。真机：备份后执行迁移 → 库变相对 → list 拼回绝对 → 播放/字幕/封面/树/移动全部正常。

- [ ] **Step 4:** Commit：`feat(paths): 重写初始化迁移（转相对+排序+ID重置）`

---

### Task 7: 回归脚本适配 + 全后端测试

**Files:** Modify `scripts/poster_regression.py`（cover_path 相对适配）

- [ ] **Step 1:** 脚本读 cover_path 现为相对；基准对比逻辑用相对值统一（load_items 的 cover_path 判断改为含 `covers/` 相对）。重建基准。
- [ ] **Step 2:** `cd src-tauri && cargo test` 全通过；`npx tsc --noEmit` 通过。
- [ ] **Step 3:** Commit：`chore: 回归脚本适配相对 cover_path + 重建基准`

---

## Self-Review

- Spec 覆盖：视频相对分类根✓(T1/T3/T4)、字幕封面相对 app_data✓、subtitles 目录✓(T2)、category_path 去分类名✓(T1 strip_category/T5)、读时拼✓(T3)、写时转✓(T4)、迁移排序+ID重置✓(T6)、前端树连锁✓(T5)、回归适配✓(T7)。
- 幂等性保障：video_to_absolute/appdata_to_absolute 对绝对路径原样返回（is_absolute 判断）——所以 T3 上线后、T6 迁移前，库里仍是绝对，拼接不重复破坏，平滑过渡。
- 类型一致：paths.rs 函数签名 T1 定义，T3/T4/T6 调用一致；buildVideoTree 根 path 空串语义 T5 定义、VideoView 移动 fp 一致。
- 顺序：T1→T2→T3（读拼，幂等安全）→T4（写转）→T5（前端树）→T6（迁移，此后库变相对）→T7。迁移放最后，前面改动对绝对数据幂等，安全。
