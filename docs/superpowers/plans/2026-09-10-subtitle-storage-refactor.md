# 字幕存储重构 + 「字幕批量校准」工具 实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 字幕改为按 category/category_path/title 推导的明文+目录结构存储（去掉 subtitle_path 列），播放器与校准工具共用同一文件；启动自动迁移旧 hash 字幕；工具箱「字幕标准化」改名「字幕批量校准」移到海报生成下、样式统一、输入可配输出固定。

**Architecture:** 新增字幕路径推导纯函数（三处共用：播放/校准输出/迁移）。去 subtitle_path 列牵动 model/scanner/library-CRUD/lib.rs/player/前端。启动时一次性迁移（备份 sqlite→归位 hash 字幕→DROP COLUMN）。重命名/移动文件夹时同步 rename 磁盘字幕目录。

**Tech Stack:** Tauri 2 + vanilla-ts + Vite；Rust + rusqlite 0.31 bundled（SQLite 3.44+，支持 ALTER TABLE DROP COLUMN）。`cargo test` / `npx tsc --noEmit` / `npm run build`；真机 `npm run tauri dev`。

**规范约定（务必遵守）:**
- 字幕相对路径：`subtitles/<category>/<category_path>/<title>.ass`，category_path 为空则省略中段。绝对：`<app_data>/` + 该相对路径。**迁移写入与运行推导必须用同一函数**。
- category_path 语义：分类内相对路径，不含分类名。故字幕路径显式带 category 段以区分分类。
- 不做 sanitize：直接用原始 category/category_path/title 拼；迁移遇非法字符/失败记 `eprintln!` 跳过、不中断。
- `MediaItem` 同时用于 serde 给前端；去 subtitle_path 后前端 ipc 类型同步去掉。
- 现有：`Db(pub Mutex<Connection>)`；`AppError::{Db,Other,Invalid}`；`settings::get/set(&db,key)`；`paths::video_root_key`；`ScannedItem` 与 `MediaItem` 均含 `subtitle_path: Option<String>`（待删）。
- setup 内 `Db::open` 在 lib.rs:469，`app.manage(db)` 在 470；`dir` 是 app_data_dir。

---

### Task 1: 字幕路径推导纯函数

**Files:** Modify `src-tauri/src/library/paths.rs`（加函数 + 单测）

- [ ] **Step 1: 写失败的单测**（追加到 paths.rs 的 `#[cfg(test)] mod tests`）:
```rust
    #[test]
    fn subtitle_paths() {
        assert_eq!(subtitle_rel_path("电影", "动作/古墓丽影", "[2001].古墓丽影"),
            "subtitles/电影/动作/古墓丽影/[2001].古墓丽影.ass");
        assert_eq!(subtitle_rel_path("电影", "", "沙丘"), "subtitles/电影/沙丘.ass");
        assert_eq!(subtitle_abs_path("/app", "电影", "科幻", "星战"),
            "/app/subtitles/电影/科幻/星战.ass");
    }
```

- [ ] **Step 2: 运行验证失败**

Run: `cd /Users/zhoumo/Documents/Claude/collector/src-tauri && cargo test --lib paths::tests::subtitle_paths 2>&1 | tail -10`
Expected: 编译失败——函数未定义。

- [ ] **Step 3: 实现**（加到 `src-tauri/src/library/paths.rs`）:
```rust
/// 字幕在 app_data 下的相对路径：subtitles/<category>/<category_path>/<title>.ass
/// category_path 为空则省略中段。播放推导、校准输出、迁移归位三处共用。
pub fn subtitle_rel_path(category: &str, category_path: &str, title: &str) -> String {
    if category_path.is_empty() {
        format!("subtitles/{category}/{title}.ass")
    } else {
        format!("subtitles/{category}/{category_path}/{title}.ass")
    }
}

/// 字幕绝对路径：<app_data>/subtitles/...
pub fn subtitle_abs_path(app_data: &str, category: &str, category_path: &str, title: &str) -> String {
    format!("{}/{}", app_data.trim_end_matches('/'),
        subtitle_rel_path(category, category_path, title))
}
```

- [ ] **Step 4: 运行验证通过**

Run: `cd /Users/zhoumo/Documents/Claude/collector/src-tauri && cargo test --lib paths::tests::subtitle_paths 2>&1 | tail -10`
Expected: ok。

- [ ] **Step 5: 提交**
```bash
cd /Users/zhoumo/Documents/Claude/collector
git add src-tauri/src/library/paths.rs
git commit -m "feat(paths): 字幕路径推导纯函数 subtitle_rel_path/subtitle_abs_path

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

### Task 2: 去掉 subtitle_path 列（model / scanner / library CRUD）

**Files:** Modify `src-tauri/src/library/model.rs`、`src-tauri/src/library/scanner.rs`、`src-tauri/src/library/mod.rs`

- [ ] **Step 1: model.rs 去字段**

删除 `MediaItem` 的 `pub subtitle_path: Option<String>,` 一行。

- [ ] **Step 2: scanner.rs 去字段**

`ScannedItem` 结构删 `pub subtitle_path: Option<String>,`。scanner 里凡构造 ScannedItem 处（含 `scan_videos_subs` 等）删除 `subtitle_path` 赋值行；若原逻辑用 `.ass` 存在与否设 subtitle_path，直接删该赋值（字幕不再入库）。`build_scanned`（scanner 内把自身转 ScannedItem 的地方，line ~125）同步删。

- [ ] **Step 3: library/mod.rs 去列**

改 4 处 SQL 与相关：
- `insert_one_tx` 的 Video 分支：INSERT 列去 `subtitle_path`，VALUES 去对应 `?`（6→5 参，重排编号），ON CONFLICT DO UPDATE 去 `subtitle_path=excluded.subtitle_path,`。params 去 `it.subtitle_path`。
- `update_item`：SET 去 `subtitle_path=?4,`，其余编号前移（category=?1,category_path=?2,title=?3,cover_path=?4,description=?5 WHERE id=?6），params 去 `it.subtitle_path`。
- `create_item` Video 分支：同 insert_one_tx。
- `list_items` Video 分支：SELECT 去 `subtitle_path`，列序前移，`MediaItem{...}` 去 `subtitle_path:` 字段、其余 `r.get(n)` 下标前移（category_path=2,title=3,cover_path=4,description=5）。Comic/Game 分支的 `MediaItem{...}` 去 `subtitle_path: None,`。
- 测试 `sample()` 去 `subtitle_path: None,`。

- [ ] **Step 4: 后端编译（会因 lib.rs/player 仍引用而失败——预期，下个任务修）**

Run: `cd /Users/zhoumo/Documents/Claude/collector/src-tauri && cargo build 2>&1 | grep -E "error\[|subtitle_path" | head -20`
Expected: 仅剩 lib.rs / player/mod.rs 里对 subtitle_path 的引用错误（library 层自身应无错）。记录这些错误点，Task 4 修。

（本任务不单独提交，与 Task 3、4 一起编译通过后提交，避免中间不可编译状态被提交。见 Task 4 Step 末。）

---

### Task 3: schema 加 DROP COLUMN 迁移 + 启动迁移函数

**Files:** Modify `src-tauri/src/db/schema.rs`、`src-tauri/src/lib.rs`（迁移函数 + setup 调用 + 单测）

- [ ] **Step 1: schema 幂等去列**

schema.rs 的 `MIGRATIONS` 末尾追加一条（用子查询判断列存在再 DROP，避免二次启动报错）。SQLite 不支持 `DROP COLUMN IF EXISTS`，故迁移函数用 PRAGMA 检测后再执行 ALTER，schema.rs **不放** DROP（放这里会每次启动都试）。**本步实际不改 schema.rs**——DROP 由 Task3 Step2 的迁移函数按需执行。（保留此步说明：schema.rs 不动。）

- [ ] **Step 2: lib.rs 加迁移函数**

在 lib.rs（`init_from_demo` 附近）新增：
```rust
/// 首启迁移：把旧 hash 字幕(subtitle_path 列)归位到推导明文路径，然后删列。
/// 以「subtitle_path 列是否存在」为幂等闸门；列删掉后再启动为 no-op。
/// 失败条目记日志跳过，不中断启动。
fn migrate_subtitles_to_plain(app_data: &std::path::Path, db: &Db) {
    let conn = db.0.lock().unwrap();
    // 检测列是否存在
    let has_col: bool = conn
        .prepare("SELECT 1 FROM pragma_table_info('media') WHERE name='subtitle_path'")
        .and_then(|mut s| s.exists([]))
        .unwrap_or(false);
    if !has_col {
        return; // 已迁移
    }
    let app_data_str = app_data.to_string_lossy();
    // 备份 sqlite（.bak 已存在则不覆盖）
    let db_path = app_data.join("collector.sqlite");
    let bak = app_data.join("collector.sqlite.bak");
    if !bak.exists() {
        let _ = std::fs::copy(&db_path, &bak);
    }
    // 收集需迁移的行
    let rows: Vec<(String, String, String, String)> = {
        let mut stmt = match conn.prepare(
            "SELECT category, category_path, title, subtitle_path FROM media \
             WHERE subtitle_path IS NOT NULL AND trim(subtitle_path) <> ''") {
            Ok(s) => s, Err(e) => { eprintln!("[migrate] prepare 失败: {e}"); return; }
        };
        let mapped = stmt.query_map([], |r| Ok((
            r.get::<_, String>(0)?, r.get::<_, String>(1)?,
            r.get::<_, String>(2)?, r.get::<_, String>(3)?,
        )));
        match mapped {
            Ok(it) => it.filter_map(|x| x.ok()).collect(),
            Err(e) => { eprintln!("[migrate] query 失败: {e}"); return; }
        }
    };
    let mut moved = 0usize;
    for (category, cpath, title, rel) in &rows {
        let src = crate::library::paths::appdata_to_absolute(rel, &app_data_str);
        let dst = crate::library::paths::subtitle_abs_path(&app_data_str, category, cpath, title);
        if src == dst { continue; }
        if !std::path::Path::new(&src).is_file() { continue; } // 源缺失跳过
        if let Some(parent) = std::path::Path::new(&dst).parent() {
            if std::fs::create_dir_all(parent).is_err() { eprintln!("[migrate] 建目录失败: {dst}"); continue; }
        }
        match std::fs::rename(&src, &dst) {
            Ok(_) => moved += 1,
            Err(e) => eprintln!("[migrate] 移动失败 {src} -> {dst}: {e}"),
        }
    }
    // 删列
    if let Err(e) = conn.execute("ALTER TABLE media DROP COLUMN subtitle_path", []) {
        eprintln!("[migrate] DROP COLUMN 失败: {e}");
    } else {
        eprintln!("[migrate] 完成：归位 {moved} 条字幕，已删除 subtitle_path 列");
    }
}
```

- [ ] **Step 3: setup 里调用迁移**

在 lib.rs setup（:469 附近）`let db = Db::open(...)` 之后、`app.manage(db)` 之前插入：
```rust
            let db = Db::open(&dir.join("collector.sqlite")).expect("open db");
            migrate_subtitles_to_plain(&dir, &db);
            app.manage(db);
```

- [ ] **Step 4: 迁移单测**（lib.rs test 区，用内存库需模拟旧列——因内存库 schema 已无该列，改为在测试内手动建带列的表）:
```rust
    #[test]
    fn migrate_moves_hash_subs_and_drops_column() {
        use std::fs;
        let tmp = tempfile::tempdir().unwrap();
        let app_data = tmp.path();
        fs::create_dir_all(app_data.join("subtitles")).unwrap();
        // 造一个带 subtitle_path 列的库（模拟旧结构）
        let db_path = app_data.join("collector.sqlite");
        {
            let conn = rusqlite::Connection::open(&db_path).unwrap();
            conn.execute_batch(
                "CREATE TABLE media (id INTEGER PRIMARY KEY AUTOINCREMENT, category TEXT, category_path TEXT, title TEXT, description TEXT, subtitle_path TEXT, cover_path TEXT, UNIQUE(category_path,title));").unwrap();
            conn.execute("INSERT INTO media (category,category_path,title,subtitle_path) VALUES ('电影','科幻','星战','subtitles/sub_abc.ass')", []).unwrap();
        }
        // 造旧 hash 字幕文件
        fs::write(app_data.join("subtitles/sub_abc.ass"), "x").unwrap();
        let db = crate::db::Db::open(&db_path).unwrap(); // 注意：Db::open 会跑 MIGRATIONS（CREATE IF NOT EXISTS，不影响已存在表）
        migrate_subtitles_to_plain(app_data, &db);
        // 断言：新路径存在，旧路径消失
        assert!(app_data.join("subtitles/电影/科幻/星战.ass").is_file());
        assert!(!app_data.join("subtitles/sub_abc.ass").exists());
        // 断言：列已删
        let conn = db.0.lock().unwrap();
        let has: bool = conn.prepare("SELECT 1 FROM pragma_table_info('media') WHERE name='subtitle_path'")
            .and_then(|mut s| s.exists([])).unwrap_or(false);
        assert!(!has);
        // .bak 生成
        assert!(app_data.join("collector.sqlite.bak").exists());
    }
```
> 注意：`Db::open` 会执行 `MIGRATIONS`（含 `CREATE TABLE IF NOT EXISTS media (...无 subtitle_path...)`）。因表已存在，IF NOT EXISTS 不重建，旧列保留——迁移函数据此仍能检测到列并迁移。实现时如发现 Db::open 干扰，测试改为直接用 rusqlite Connection 手工建库并跳过 Db::open 的 MIGRATIONS（用一个不跑 migrations 的构造，或直接对 Connection 包 Db）。

（Task 2+3+4 一起编译通过后提交。）

---

### Task 4: 去列的命令层与前端连带修复

**Files:** Modify `src-tauri/src/lib.rs`、`src-tauri/src/player/mod.rs`、`src/lib/ipc.ts`、`src/components/EditDrawer.ts`、`src/components/MoveDialog.ts`

- [ ] **Step 1: player/mod.rs resolve_subtitle 改推导**

把 `resolve_subtitle` 改为不查库、按推导路径判断存在：
```rust
fn resolve_subtitle(app_data: &str, category: &str, category_path: &str, title: &str) -> Option<String> {
    let abs = crate::library::paths::subtitle_abs_path(app_data, category, category_path, title);
    if std::path::Path::new(&abs).is_file() { Some(abs) } else { None }
}
```
`player_open` 里调用处改为 `resolve_subtitle(&app_data_str, &category, &category_path, &title)`（去掉 db 参与、去掉 `?`，返回 Option 直接用）。删除旧 resolve_subtitle 的 3 个 DB 单测，替换为推导版单测：
```rust
    #[test]
    fn resolve_subtitle_by_path() {
        let tmp = tempfile::tempdir().unwrap();
        let app = tmp.path().to_string_lossy().to_string();
        let dir = tmp.path().join("subtitles/电影/科幻");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("星战.ass"), "x").unwrap();
        assert!(resolve_subtitle(&app, "电影", "科幻", "星战").is_some());
        assert!(resolve_subtitle(&app, "电影", "科幻", "不存在").is_none());
    }
```

- [ ] **Step 2: lib.rs 去 subtitle_path**

- `MediaItem` 无 subtitle_path 后，`list_media`（:97-100）删除对 `it.subtitle_path` 的绝对化整段。
- `build_video_item`：去 `subtitle_path` 参数与字段。
- `media_update` / `media_create` 命令：去 `subtitle_path: Option<String>` 参数；调 build_video_item 时去该实参。
- `init_from_demo`：无变化（scanner 已不产 subtitle_path）。

- [ ] **Step 3: ipc.ts 去字段与参数**

- `MediaItem` 接口删 `subtitle_path: string | null;`。
- `mediaUpdate`：去 `subtitlePath` 形参与 invoke 入参（改为 `mediaUpdate(id, category, categoryPath, title, coverPath, description)`）。
- `mediaCreate`：同样去 `subtitlePath`。

- [ ] **Step 4: EditDrawer.ts / MoveDialog.ts 调整**

- EditDrawer：删 `const subtitlePath = ...`（:15）；字幕展示行（:39-40）改为只读显示「字幕：<推导相对路径>」——但前端不易判断文件存在，简化为**移除字幕路径展示行**（YAGNI，字幕由校准工具管理）；两处 `mediaUpdate/mediaCreate` 调用（:106,:109）去掉 subtitlePath 实参。
- MoveDialog：`mediaUpdate` 调用（:112）去掉 `item.subtitle_path,` 实参。

- [ ] **Step 5: 后端编译 + 测试 + 前端类型**

Run:
```bash
cd /Users/zhoumo/Documents/Claude/collector/src-tauri && cargo test --lib 2>&1 | tail -8
cd /Users/zhoumo/Documents/Claude/collector && npx tsc --noEmit && echo "tsc OK"
```
Expected: cargo 全绿（含 Task1/3 新测试）、tsc 无错误。

- [ ] **Step 6: 提交（Task 2+3+4 合并一次，达成可编译状态）**
```bash
cd /Users/zhoumo/Documents/Claude/collector
git add src-tauri/src/library/model.rs src-tauri/src/library/scanner.rs src-tauri/src/library/mod.rs src-tauri/src/db/schema.rs src-tauri/src/lib.rs src-tauri/src/player/mod.rs src/lib/ipc.ts src/components/EditDrawer.ts src/components/MoveDialog.ts
git commit -m "refactor: 去掉 subtitle_path 列，字幕改路径推导 + 启动迁移

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

### Task 5: 校准命令输出改固定 + 输入目录设置

**Files:** Modify `src-tauri/src/normalize/mod.rs`、`src-tauri/src/lib.rs`（注册可能已含）、`src/lib/ipc.ts`

- [ ] **Step 1: normalize_subtitles 输出固定到 app_data/subtitles**

`normalize/mod.rs` 的 `normalize_subtitles` 命令签名改为接收 app + in_dir：
```rust
#[tauri::command(rename_all = "camelCase")]
pub fn normalize_subtitles(app: tauri::AppHandle, in_dir: String) -> AppResult<Vec<SubtitleReport>> {
    let app_data = app.path().app_data_dir()
        .map_err(|e| crate::error::AppError::Other(format!("app_data_dir: {e}")))?;
    let out_dir = app_data.join("subtitles");
    run_subtitle_normalize(std::path::Path::new(&in_dir), &out_dir, &[])
}
```
需要 `use tauri::Manager;`（normalize/mod.rs 顶部加）。

- [ ] **Step 2: 输入目录 settings 命令**

lib.rs 加两个命令（复用 settings）：
```rust
#[tauri::command(rename_all = "camelCase")]
fn get_subtitle_input_dir(db: tauri::State<Db>) -> AppResult<Option<String>> {
    settings::get(&db, "subtitle_input_dir")
}
#[tauri::command(rename_all = "camelCase")]
fn set_subtitle_input_dir(db: tauri::State<Db>, path: String) -> AppResult<()> {
    settings::set(&db, "subtitle_input_dir", &path)
}
```
注册进 invoke_handler。

- [ ] **Step 3: ipc.ts**

- `normalizeSubtitles` 改单参：`normalizeSubtitles: (inDir: string) => invoke<SubReport[]>("normalize_subtitles", { inDir })`。
- 加 `getSubtitleInputDir: () => invoke<string | null>("get_subtitle_input_dir")`、`setSubtitleInputDir: (path) => invoke<void>("set_subtitle_input_dir", { path })`。

- [ ] **Step 4: 编译 + 类型**

Run: `cd /Users/zhoumo/Documents/Claude/collector/src-tauri && cargo build 2>&1 | tail -5; cd .. && npx tsc --noEmit && echo tscOK`
Expected: 均通过。

- [ ] **Step 5: 提交**
```bash
cd /Users/zhoumo/Documents/Claude/collector
git add src-tauri/src/normalize/mod.rs src-tauri/src/lib.rs src/lib/ipc.ts
git commit -m "feat(normalize): 字幕校准输出固定到应用字幕库 + 输入目录可配

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

### Task 6: 工具箱 UI 改名/移位/样式统一/输入可配

**Files:** Modify `src/views/NormalizeView.ts`

- [ ] **Step 1: 卡片改造**

在 `NormalizeView.ts` 的 innerHTML 中：
- 删除原顶部「字幕标准化」`<div class="glass">…</div>` 整块（含 sub-in/sub-out/sub-run）。
- 在「影视海报生成」`.setting-card` **之后**新增「字幕批量校准」卡片，用相同 `.setting-card` 结构：
```html
    <div class="glass setting-card">
      <div class="setting-card-head"><span class="setting-card-title">字幕批量校准</span></div>
      <div class="setting-row">
        <span class="setting-label">输入目录</span>
        <span id="sub-in-p" class="setting-path">docs/subtitles</span>
        <button class="icon-text" id="sub-in">${icon("folder", 15)}<span class="btn-label">选择</span></button>
      </div>
      <div class="setting-row">
        <span class="setting-label">输出</span>
        <span class="setting-path">应用字幕库（自动，按目录结构）</span>
      </div>
      <div class="setting-actions">
        <button class="btn-primary icon-text" id="sub-run">${icon("play", 15)}<span class="btn-label">开始校准</span></button>
      </div>
      <div id="sub-report" style="margin-top:10px"></div>
    </div>
```

- [ ] **Step 2: 逻辑接线**

- 进入视图预填输入目录：`api.getSubtitleInputDir().then(d => { subIn = d || ""; if (d) el.querySelector("#sub-in-p")!.textContent = d; })`。默认显示 `docs/subtitles`（占位文本，subIn 为空时开始按钮提示选择或用默认——决定：subIn 为空时用字符串 `"docs/subtitles"` 作为相对路径传后端）。
- `#sub-in` onclick：`open({directory:true})` 选目录 → 存 subIn、显示、`api.setSubtitleInputDir(subIn)`。
- `#sub-run` onclick：`const dir = subIn || "docs/subtitles";` → `api.normalizeSubtitles(dir)`；报告渲染沿用原逻辑（reports/issues 折叠展示）。
- 删除原 subOut 相关变量与 pick 调用。

- [ ] **Step 3: 类型检查 + 构建**

Run: `cd /Users/zhoumo/Documents/Claude/collector && npx tsc --noEmit && npm run build 2>&1 | tail -6`
Expected: 均成功。

- [ ] **Step 4: 提交**
```bash
cd /Users/zhoumo/Documents/Claude/collector
git add src/views/NormalizeView.ts
git commit -m "feat(toolbox): 字幕标准化→字幕批量校准，移到海报生成下、样式统一、输入可配输出固定

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

### Task 7: 重命名/移动文件夹时字幕目录跟随

**Files:** Modify `src-tauri/src/lib.rs`（rename_folder 加字幕目录 rename；media_update 加单条字幕 rename）

- [ ] **Step 1: rename_folder 加字幕目录跟随**

在 `rename_folder` 命令里（磁盘视频目录 rename 之后、或之前，独立处理），追加字幕目录 rename：
```rust
    // 字幕目录跟随（<app_data>/subtitles/<category>/<old_path> → <new_path>）
    if let Ok(app_data) = app.path().app_data_dir() {
        // 注意：rename_folder 当前签名可能无 app: AppHandle，需要加该参数
        let base = app_data.join("subtitles").join(&category);
        let s_src = base.join(&old_path);
        let s_dst = base.join(&new_path);
        if s_src.is_dir() {
            if let Some(p) = s_dst.parent() { let _ = std::fs::create_dir_all(p); }
            let _ = std::fs::rename(&s_src, &s_dst); // 容错：失败忽略
        }
    }
```
若 `rename_folder` 现签名无 `app: tauri::AppHandle`，加上该参数（tauri 自动注入，前端 invoke 不变）。`use tauri::Manager;` lib.rs 已有。

> old_path/new_path 是分类内相对路径（不含分类名），字幕基目录已含 `<category>`，故用 `base.join(old_path)`——与 subtitle_rel_path 的 `subtitles/<category>/<category_path>` 拼法一致。

- [ ] **Step 2: media_update 单条字幕跟随（移动/改名单条）**

`media_update` 里，若 category_path 或 title 变化，需把该条字幕从旧推导路径 rename 到新。但 media_update 拿不到「旧」category_path/title（只收新值 + id）。**方案**：更新前先查该 id 的旧 category/category_path/title：
```rust
    // 读旧值用于字幕跟随
    let old: Option<(String,String,String)> = {
        let conn = db.0.lock().unwrap();
        conn.query_row("SELECT category,category_path,title FROM media WHERE id=?1",
            rusqlite::params![id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?))).ok()
    };
    // ...（原 update_item 调用保持）
    if let (Some((oc, op, ot)), Ok(app_data)) = (old, app.path().app_data_dir()) {
        let ad = app_data.to_string_lossy();
        let src = library::paths::subtitle_abs_path(&ad, &oc, &op, &ot);
        let dst = library::paths::subtitle_abs_path(&ad, &category, &category_path, &title);
        if src != dst && std::path::Path::new(&src).is_file() {
            if let Some(p) = std::path::Path::new(&dst).parent() { let _ = std::fs::create_dir_all(p); }
            let _ = std::fs::rename(&src, &dst);
        }
    }
```
（media_update 已有 `app: tauri::AppHandle` 与 `db`；读旧值在 update_item 之前，rename 在之后。）

- [ ] **Step 3: 编译 + 测试**

Run: `cd /Users/zhoumo/Documents/Claude/collector/src-tauri && cargo test --lib 2>&1 | tail -6`
Expected: 通过（rename_folder 已有单测不受影响；字幕 rename 为文件系统副作用，不在单测覆盖，真机验证）。

- [ ] **Step 4: 提交**
```bash
cd /Users/zhoumo/Documents/Claude/collector
git add src-tauri/src/lib.rs
git commit -m "feat: 重命名/移动时字幕目录随 category_path 跟随迁移

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

### Task 8: 真机验证

**Files:** 无（手动验收）

- [ ] **Step 1: 启动（触发迁移）**
```bash
cd /Users/zhoumo/Documents/Claude/collector
lsof -ti:1420 | xargs kill -9 2>/dev/null; pkill -f "tauri dev" 2>/dev/null; pkill -f "target/debug/collector" 2>/dev/null; sleep 1
nohup npm run tauri dev > /tmp/collector-dev.log 2>&1 & disown
```

- [ ] **Step 2: 验收迁移结果**
```bash
APP="$HOME/Library/Application Support/com.zhoumo.collector"
ls "$APP/collector.sqlite.bak" && echo "备份OK"
find "$APP/subtitles" -name "*.ass" | grep -E "/电影/|/动漫/|/剧集/" | head   # 明文结构出现
ls "$APP/subtitles" | grep -c "^sub_" || echo "无残留 hash（或仅孤儿）"
sqlite3 "$APP/collector.sqlite" "SELECT 1 FROM pragma_table_info('media') WHERE name='subtitle_path';" | grep -q 1 && echo "列还在(异常)" || echo "列已删OK"
```

- [ ] **Step 3: 应用内逐项**
- 播放有字幕的片 → 字幕正常（走推导路径）。
- 工具箱：卡片在「影视海报生成」下方、标题「字幕批量校准」样式一致；输入目录默认 docs/subtitles、可选、记住；点开始 → 输出到 app_data/subtitles 对应结构、报告正常。
- 重命名一个含字幕的文件夹 → 进该文件夹播放视频字幕仍在（字幕目录已跟随）。
- 移动一条视频到别的文件夹 → 字幕跟随、仍能播放。
- 重启应用 → 不重复迁移（日志无 `[migrate] 完成`）。

- [ ] **Step 4: 关闭**
```bash
lsof -ti:1420 | xargs kill -9 2>/dev/null; pkill -f "tauri dev" 2>/dev/null
```

---

## 自审（对照 spec）

- **spec 覆盖**：路径推导→T1；去 subtitle_path 列→T2(model/scanner/library)+T4(lib/player/前端)；启动迁移+备份+删列→T3；播放推导→T4 Step1；校准输出固定+输入可配→T5；UI 改名/移位/样式统一→T6；重命名/移动字幕跟随→T7；不做完整规则→未纳入（下轮）。全覆盖。
- **占位符**：无 TBD。T3 Step1 明确「schema.rs 不动，DROP 由迁移函数按需执行」，非占位。
- **类型一致**：`subtitle_rel_path/subtitle_abs_path` T1 定义、T3/T4/T7 消费一致；`resolve_subtitle(app_data,category,category_path,title)` T4 定义与 player_open 调用一致；`normalizeSubtitles(inDir)` 前后端一致（T5）；去参后的 `mediaUpdate/mediaCreate` 前后端一致（T4）。

## 风险

- **中间不可编译**：T2 删列后到 T4 修完前后端不可编译——故 T2/T3/T4 合并为一次提交（T4 Step6），不留不可编译的提交。
- **迁移不可逆**：先备份 .bak；rename 非 copy；孤儿 hash 不动。真机 Step2 验证、有 .bak 兜底。
- **DROP COLUMN**：rusqlite 0.31 bundled SQLite 支持。若某环境不支持，退化为建新表迁移（本计划以 DROP 为主）。
- **media_update 读旧值**：移动/改名字幕跟随依赖「更新前查旧 category/path/title」，须在 update_item 之前读。
- **rename_folder 加 app 参数**：若原签名无 AppHandle，加参数（tauri 注入，前端 invoke 不变）——T7 Step1 已注明。
