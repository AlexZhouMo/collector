# 视频库初始化导入 实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 从 demo/subtitles 的电影/动漫/剧集三分类结构以 .ass 为条目初始化导入视频库（清库、ID 从 1 顺序、cover_path 存展示图），分类「电视剧」改「剧集」，设置页配路径只保存不自动扫描。

**Architecture:** 改造 scanner 以 .ass 为条目（title=字幕名、path 空、subtitle_path=该 .ass）；replace_items 加 sqlite_sequence 重置 + 按 category_path/title 排序使 ID 从 1 有序；新增 init_from_demo command 扫 demo 三目录导入；前后端分类字符串「电视剧」→「剧集」；设置页扫描按钮改为只保存 + 加「初始化示例库」按钮。

**Tech Stack:** Rust（walkdir、rusqlite）、Tauri command、前端 vanilla-ts。

**参考设计：** `docs/superpowers/specs/2026-09-08-video-library-init-import-design.md`

**demo 结构（已探查）：** `demo/subtitles/{电影,动漫,剧集}/子分类/剧名/*.ass`，只有 .ass 无 .mkv。

**当前代码：**
- `library/scanner.rs`：`scan_videos(root, category)` 找 .mkv（要改 .ass）
- `library/mod.rs`：`replace_items(db, kind, items)` = DELETE + insert（要加序列重置 + 排序）
- `lib.rs`：`scan_videos_all` 映射含 `("video_tv","电视剧")`（改剧集）
- `src/views/VideoView.ts`：`cats = ["电影","动漫","电视剧"]`（改剧集）
- `src/views/SettingsView.ts`：VIDEO_CATS 含 `["video_tv","电视剧"]`；扫描按钮自动调 scanVideos（改只保存）

---

## Task 1: replace_items 重置序列 + 排序（ID 从 1 有序）

**Files:** Modify `src-tauri/src/library/mod.rs`

- [ ] **Step 1: 加测试**

在 `library/mod.rs` 的 `#[cfg(test)] mod tests` 里加（sample 已有，构造不同 category_path/title）：
```rust
    #[test]
    fn replace_resets_id_to_one_and_orders() {
        let db = Db::open_in_memory().unwrap();
        // 先插一批占掉 id
        replace_items(&db, MediaKind::Video, &[sample("/a.mkv"), sample("/b.mkv")]).unwrap();
        // 再 replace：id 应重新从 1 开始
        let mut s1 = sample("/x.mkv"); s1.category_path = "电影/z".into(); s1.title = "Z".into();
        let mut s2 = sample("/y.mkv"); s2.category_path = "电影/a".into(); s2.title = "A".into();
        replace_items(&db, MediaKind::Video, &[s1, s2]).unwrap();
        let conn = db.0.lock().unwrap();
        let (min_id, max_id): (i64, i64) = conn
            .query_row("SELECT min(id), max(id) FROM media_item", [], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap();
        assert_eq!(min_id, 1); // 序列重置，从 1 开始
        assert_eq!(max_id, 2);
        // 排序：category_path 升序（电影/a 在 电影/z 前），id=1 是 category_path 小的
        let first_title: String = conn
            .query_row("SELECT title FROM media_item WHERE id=1", [], |r| r.get(0)).unwrap();
        assert_eq!(first_title, "A"); // 电影/a 排在前
    }
```

- [ ] **Step 2: 运行测试确认失败**

Run: `cd src-tauri && cargo test library::tests::replace_resets_id_to_one_and_orders`
Expected: FAIL（当前 id 不重置、不排序）。

- [ ] **Step 3: 实现——加序列重置 + 排序**

把 `replace_items` 改为（DELETE 后加序列重置；插入前排序）：
```rust
pub fn replace_items(db: &Db, kind: MediaKind, items: &[ScannedItem]) -> AppResult<usize> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    // 按 category_path、title 排序，使顺序插入后 id 稳定可预期
    let mut sorted: Vec<&ScannedItem> = items.iter().collect();
    sorted.sort_by(|a, b| {
        a.category_path
            .cmp(&b.category_path)
            .then_with(|| a.title.cmp(&b.title))
    });
    let mut conn = db.0.lock().unwrap();
    let tx = conn.transaction().map_err(|e| AppError::Db(e.to_string()))?;
    tx.execute("DELETE FROM media_item WHERE kind=?1", params![kind.as_str()])
        .map_err(|e| AppError::Db(e.to_string()))?;
    // 重置该表自增序列，使 id 从 1 开始（SQLite AUTOINCREMENT 清表后默认不重置）
    tx.execute(
        "DELETE FROM sqlite_sequence WHERE name='media_item'",
        [],
    )
    .map_err(|e| AppError::Db(e.to_string()))?;
    let mut n = 0;
    for it in &sorted {
        insert_one_tx(&tx, it, now)?;
        n += 1;
    }
    tx.commit().map_err(|e| AppError::Db(e.to_string()))?;
    Ok(n)
}
```
注意：现有代码用 `insert_items_tx(&tx, items, now)` 批量插入。为保证排序后逐条顺序插入，改为循环调用单条插入。若现有只有 `insert_items_tx`（接收 slice），新增一个 `insert_one_tx(tx, item, now)`（把 insert_items_tx 的循环体抽出）或直接让 insert_items_tx 接收已排序的 `&[&ScannedItem]`。**推荐**：把 `insert_items_tx` 改为接收 `items: &[&ScannedItem]`，replace_items 传排序后的 `&sorted`；upsert_items（若还在）相应适配。读实际代码选最小改动方式，保证排序后顺序插入。

- [ ] **Step 4: 运行测试**

Run: `cd src-tauri && cargo test library::` — 全部 PASS（含新测试 + 原有 replace_* 测试；注意原 replace_clears_stale / dedup 测试仍应过）。

- [ ] **Step 5: 提交**

```bash
git add -A && git commit -m "feat: replace_items resets id sequence and inserts sorted"
```

## Task 2: scanner 以 .ass 为条目

**Files:** Modify `src-tauri/src/library/scanner.rs`

- [ ] **Step 1: 加测试**

在 scanner.rs tests 加：
```rust
    #[test]
    fn scan_subs_uses_ass_as_items() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path(); // 视为「剧集」分类目录
        let dir = root.join("日剧").join("怨屋本铺");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("E07.被当做踏脚石的人生.ass"), b"sub").unwrap();
        let items = scan_videos_subs(root, "剧集");
        assert_eq!(items.len(), 1);
        let it = &items[0];
        assert_eq!(it.category, "剧集");
        assert_eq!(it.category_path, "剧集/日剧/怨屋本铺");
        assert_eq!(it.title, "E07.被当做踏脚石的人生");
        assert!(it.subtitle_path.as_deref().unwrap().ends_with("E07.被当做踏脚石的人生.ass"));
        assert_eq!(it.path, ""); // 视频文件待定
    }
```

- [ ] **Step 2: 运行确认失败**

Run: `cd src-tauri && cargo test library::scanner::tests::scan_subs_uses_ass_as_items`
Expected: FAIL（scan_videos_subs 未定义）。

- [ ] **Step 3: 实现 scan_videos_subs**

在 scanner.rs 加（与 scan_videos 结构类似，但找 .ass、path 空、subtitle_path=该 .ass）：
```rust
/// 以 .ass 字幕为条目扫描一个视频分类目录（初始化导入用）。
/// title = 字幕文件名去扩展名；subtitle_path = 该 .ass；path（视频文件）暂空。
/// category_path = category + 目录内相对路径（保留 子分类/剧名 层级）。
pub fn scan_videos_subs(root: &Path, category: &str) -> Vec<ScannedItem> {
    let mut items = Vec::new();
    for entry in WalkDir::new(root).into_iter().filter_map(|e| e.ok()) {
        let p = entry.path();
        if p.extension().and_then(|s| s.to_str()) != Some("ass") {
            continue;
        }
        let rel = p.strip_prefix(root).unwrap_or(p);
        let comps: Vec<String> = rel
            .parent()
            .map(|d| d.components().map(|c| c.as_os_str().to_string_lossy().into_owned()).collect())
            .unwrap_or_default();
        let category_path = if comps.is_empty() {
            category.to_string()
        } else {
            format!("{category}/{}", comps.join("/"))
        };
        let stem = p.file_stem().unwrap().to_string_lossy().into_owned();
        let dir = p.parent().unwrap();

        // 展示图：同目录 poster.jpg 或同名 .jpg（存在才填）
        let poster = dir.join("poster.jpg");
        let named_cover = dir.join(format!("{stem}.jpg"));
        let cover_path = if poster.exists() {
            Some(poster.to_string_lossy().into_owned())
        } else if named_cover.exists() {
            Some(named_cover.to_string_lossy().into_owned())
        } else {
            None
        };
        let info = dir.join("info.txt");
        let description = std::fs::read_to_string(&info).ok().map(|s| s.trim().to_string());

        items.push(ScannedItem {
            kind: MediaKind::Video,
            category: category.to_string(),
            category_path,
            title: stem,
            path: String::new(), // 视频文件待定（后续放同名 .mkv）
            subtitle_path: Some(p.to_string_lossy().into_owned()),
            cover_path,
            description,
            platform_ok: true,
            exec_path: None,
        });
    }
    items
}
```

- [ ] **Step 4: 运行测试**

Run: `cd src-tauri && cargo test library::scanner` — PASS（含原 scan_videos 测试 + 新 subs 测试）。
注意：`path` 现在可为空串——确认 media_item.path 是 UNIQUE，多个空串 path 会冲突！**这是关键问题**：多个 .ass 条目 path 都为 ""，UNIQUE 约束会让第二条起插入失败。**解决**：path 空时用 subtitle_path 作为唯一标识占位（path = subtitle_path，或 path = 一个基于 subtitle 的唯一值）。改为：`path: p.to_string_lossy().into_owned()`（用 .ass 路径作 path 占位，保证唯一；后续放 mkv 时再更新）——即初始化阶段 path 存 .ass 路径。修正上面 `path: String::new()` 为 `path: p.to_string_lossy().into_owned()`，并把测试的 `assert_eq!(it.path, "")` 改为断言 path 是该 .ass 路径。subtitle_path 同样是该 .ass。

- [ ] **Step 5: 提交**

```bash
git add -A && git commit -m "feat: scan_videos_subs imports .ass files as video items"
```

## Task 3: 分类「电视剧」→「剧集」

**Files:** Modify `src-tauri/src/lib.rs`, `src/views/VideoView.ts`, `src/views/SettingsView.ts`

- [ ] **Step 1: 后端 scan_videos_all 映射改剧集**

`src-tauri/src/lib.rs` 的 VIDEO_DIRS：`("video_tv", "电视剧")` → `("video_tv", "剧集")`；注释里"电视剧"改"剧集"。

- [ ] **Step 2: 前端分类字符串改剧集**

- `src/views/VideoView.ts`：`const cats = ["电影", "动漫", "电视剧"];` → `["电影", "动漫", "剧集"]`
- `src/views/SettingsView.ts`：VIDEO_CATS 里 `["video_tv", "电视剧"]` → `["video_tv", "剧集"]`

- [ ] **Step 3: 编译**

Run: `cd src-tauri && cargo build 2>&1 | tail -3` + `npm run build 2>&1 | tail -3` — 通过。

- [ ] **Step 4: 提交**

```bash
git add -A && git commit -m "feat: rename video category 电视剧 to 剧集"
```

## Task 4: init_from_demo command + IPC

**Files:** Modify `src-tauri/src/lib.rs`, `src/lib/ipc.ts`

- [ ] **Step 1: 后端 init_from_demo command**

`src-tauri/src/lib.rs` 加：
```rust
/// 从 demo/subtitles 初始化导入视频库：扫电影/动漫/剧集三目录的 .ass 为条目，
/// 清库（含 id 序列重置）+ 排序顺序插入。demo_root 由前端传入（demo/subtitles 绝对路径）。
#[tauri::command(rename_all = "camelCase")]
fn init_from_demo(db: tauri::State<Db>, demo_root: String) -> AppResult<usize> {
    const CATS: &[(&str, &str)] = &[("电影", "电影"), ("动漫", "动漫"), ("剧集", "剧集")];
    let root = std::path::Path::new(&demo_root);
    let mut all = Vec::new();
    for (dir, category) in CATS {
        let cat_dir = root.join(dir);
        if cat_dir.is_dir() {
            all.extend(library::scanner::scan_videos_subs(&cat_dir, category));
        }
    }
    library::replace_items(&db, MediaKind::Video, &all)
}
```
在 generate_handler! 追加 `init_from_demo`。

- [ ] **Step 2: IPC**

`src/lib/ipc.ts` 的 api 加：
```ts
  initFromDemo: (demoRoot: string) => invoke<number>("init_from_demo", { demoRoot }),
```

- [ ] **Step 3: 编译**

Run: `cd src-tauri && cargo build 2>&1 | tail -3` + `npm run build 2>&1 | tail -3` — 通过。

- [ ] **Step 4: 提交**

```bash
git add -A && git commit -m "feat: init_from_demo command imports demo subtitles library"
```

## Task 5: 设置页「初始化示例库」按钮 + 扫描改只保存

**Files:** Modify `src/views/SettingsView.ts`

- [ ] **Step 1: 视频区加「初始化示例库」按钮 + 扫描改只保存**

在 `src/views/SettingsView.ts`：
- 视频卡片底部「扫描视频库」按钮改为「初始化示例库」，onclick 调 `api.initFromDemo(demoRoot)`——demoRoot 用项目内 `demo/subtitles` 路径。**路径来源**：开发期用一个已知绝对路径。最稳妥：让用户点该按钮时弹目录选择框选 demo/subtitles，或硬编码开发路径 `/Users/zhoumo/Documents/Claude/collector/demo/subtitles`。**推荐**：加一个「选择示例目录」用 dialog open 选目录，再「初始化」——但为简单，本任务用 dialog open 选目录后调 initFromDemo：
```ts
// 视频区「初始化示例库」按钮
const initBtn = el.querySelector<HTMLButtonElement>("#init-demo")!;
initBtn.onclick = async () => {
  const dir = await open({ directory: true });
  if (typeof dir !== "string") return;
  initBtn.disabled = true;
  const label = initBtn.querySelector<HTMLElement>(".btn-label")!;
  const orig = label.textContent;
  label.textContent = "导入中…";
  try {
    const n = await api.initFromDemo(dir);
    alert(`初始化导入完成，${n} 个视频`);
  } catch (e) {
    alert("导入失败：" + e);
  } finally {
    initBtn.disabled = false;
    label.textContent = orig;
  }
};
```
HTML 里把视频区那个 `#scan-videos` 按钮替换为 `#init-demo`（icon refresh、label「初始化示例库」）。
- **视频三分类的「选择目录」按钮改为只保存**：现在选目录后 `api.setRoot` 保存——这本就只保存不扫描（视频扫描是单独的 scan-videos 按钮触发）。所以视频区去掉自动扫描即可（本任务把 scan-videos 换成 init-demo，视频区不再有"扫描"按钮，配路径的选择目录按钮仍只 setRoot 保存）。符合"配路径只保存"。
- 漫画/游戏的扫描按钮：本次不改（设计聚焦视频；漫画/游戏保留扫描）。

- [ ] **Step 2: 编译**

Run: `npm run build 2>&1 | tail -3` — 通过。

- [ ] **Step 3: 提交**

```bash
git add -A && git commit -m "feat: settings 'init demo library' button; video paths save-only"
```

## Task 6: 真机验证

**Files:** 无

- [ ] **Step 1: 重启应用**

```bash
pkill -f "tauri dev"; pkill -f "target/debug/collector"; sleep 1
cd /Users/zhoumo/Documents/Claude/collector && nohup npm run tauri dev > /tmp/collector-dev.log 2>&1 & disown
```

- [ ] **Step 2: 验证清单**

1. 设置页 → 视频区点「初始化示例库」→ 选 `demo/subtitles` → alert「初始化导入完成，N 个视频」。
2. 视频菜单 → 三 tab 显示「电影 / 动漫 / 剧集」（不是电视剧）；各 tab 下按目录层级（子分类/剧名）显示条目，名称=字幕文件名（如 [2007].哈利波特与凤凰社）。
3. 树形/文件夹视图正常展示层级。
4. 数据库 id 从 1 顺序（可用 sqlite3 查 `SELECT id,title FROM media_item ORDER BY id LIMIT 3` 确认 id=1,2,3）。
5. 重复点「初始化示例库」→ 幂等（清库重导，id 仍从 1，无重复累积）。
6. 视频区「选择目录」只保存路径、不自动扫描（不弹扫描完成）。

- [ ] **Step 3: 记录结果**（无代码改动不提交）

---

## 自查

**1. Spec 覆盖：**
- .ass 为条目（title/subtitle_path/path/category_path）→ Task 2 ✓
- 电视剧→剧集（后端映射 + 前端两处）→ Task 3 ✓
- 只导电影/动漫/剧集三类 → Task 4 CATS ✓
- 清库 + ID 从 1 + 排序 → Task 1（序列重置+排序）✓
- cover_path 存展示图 → Task 2（poster/同名 jpg）✓
- 设置页只保存不扫描 + 初始化按钮 → Task 5 ✓
- 初始化触发（设置页按钮，幂等）→ Task 5 ✓

**2. 占位符扫描：** 无 TBD。Task 2 Step 4 明确修正了 path 唯一性问题（path 空串会撞 UNIQUE → 改用 .ass 路径作 path 占位），是关键实现细节非占位。Task 5 的路径来源明确为 dialog 选目录。

**3. 类型一致性：** `scan_videos_subs(root, category)`（Task 2）被 init_from_demo（Task 4）调用，签名一致。`replace_items`（Task 1 改）被 init_from_demo 调用。`init_from_demo` command（rename_all camelCase，参数 demo_root）↔ 前端 `initFromDemo(demoRoot)` 传 `{demoRoot}` 一致。ScannedItem 字段（path 用 .ass 路径、subtitle_path=同 .ass）在 Task 2 定义一致。

**关键修正（自查发现）：** Task 2 的 `path: String::new()` 会因 media_item.path UNIQUE 约束导致多条空串 path 冲突——已在 Task 2 Step 4 修正为 `path = .ass 绝对路径`（占位，保证唯一，后续放 mkv 时更新）。测试断言相应调整为 path=该 .ass 路径。

自查通过。
