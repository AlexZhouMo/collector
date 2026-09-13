# covers 按素材类型分子目录 实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** covers/ 下分 media/comic/game 子目录，5 处写封面代码写入对应子目录，游戏封面压缩纳入，启动时幂等迁移现有扁平封面并更新 DB。

**Architecture:** 后端。写入点改 covers_dir（media/comic/game）；scan_games 重写为压缩拷贝到 covers/game/、scan_root Game 分支传 covers_dir；新增 migrate_covers_to_subdirs 启动幂等迁移（按前缀移文件 + 改 DB cover_path）。

**Tech Stack:** Rust（std::fs / image / rusqlite）、Tauri。

---

### Task 1: 5 处写入点改子目录 + scan_games 压缩纳入

**Files:**
- Modify: `src-tauri/src/lib.rs`（import_cover / import_cover_cropped / fetch_posters / fetch_manga_covers / scan_root Game 分支）
- Modify: `src-tauri/src/library/scanner.rs`（scan_games）

- [ ] **Step 1: 写失败测试（scan_games 压缩到 covers/game/）**

在 `scanner.rs` 的 `mod game_scan_tests`（或相应测试模块）新增/改：

```rust
    #[test]
    fn scan_games_cover_into_covers_game() {
        use image::{RgbImage, Rgb};
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let g = root.join("空洞骑士");
        std::fs::create_dir_all(&g).unwrap();
        std::fs::write(g.join("game.json"), r#"{"name":"空洞骑士"}"#).unwrap();
        RgbImage::from_pixel(400,600,Rgb([10,20,30])).save(g.join("cover.jpg")).unwrap();
        let covers = tmp.path().join("covers");
        let items = scan_games(root, &covers);
        assert_eq!(items.len(), 1);
        let cp = items[0].cover_path.as_ref().unwrap();
        assert!(cp.contains("covers") && cp.contains("game"), "cover 应在 covers/game: {cp}");
        assert!(std::path::Path::new(cp).is_file());
    }
```

- [ ] **Step 2: 运行验证失败**

Run: `cd src-tauri && cargo test --lib scan_games_cover`
Expected: 编译失败——scan_games 现签名 `(root)` 无 covers_dir。

- [ ] **Step 3: 改 scan_games + scan_root + 4 写入点**

`scanner.rs`：scan_games 签名加 `covers_dir: &Path`；cover 处理改为：

```rust
        let cover_src = dir.join("cover.jpg");
        let cover_path = if cover_src.is_file() {
            std::fs::read(&cover_src).ok()
                .and_then(|b| crate::poster::image_proc::to_cover(&b).ok())
                .and_then(|c| crate::poster::image_proc::save_cover(covers_dir, &c, "game_").ok())
        } else { None };
```

`lib.rs`：
- scan_root Game 分支（:42）：需 covers_dir。scan_root 目前无 app 句柄（漫画修正时去掉了）——给 scan_root 加回 `app: tauri::AppHandle` 参数（tauri 注入，handler 注册不用改），Game 分支：
  ```rust
  MediaKind::Game => {
      let covers = app.path().app_data_dir()
          .map_err(|e| error::AppError::Other(format!("app_data_dir: {e}")))?
          .join("covers").join("game");
      library::scanner::scan_games(std::path::Path::new(&root), &covers)
  }
  ```
  （需 `use tauri::Manager;`，lib.rs 已有。Comic 分支不受影响、Video 仍返回 Err。）
- import_cover（:336）：`app_data.join("covers")` → `app_data.join("covers").join("media")`。
- import_cover_cropped（:357）：同上 `covers/media`。
- fetch_posters（:418-422）：covers_dir 改 `.join("covers").join("media")`。
- fetch_manga_covers（:537-541）：covers_dir 改 `.join("covers").join("comic")`。

注意：save_cover/import_cover 内部 create_dir_all(covers_dir) 会自动建子目录；cover_path 返回绝对路径，lib.rs 命令层已有 appdata_to_relative 转相对（import_cover/cropped 已转；fetch_posters/manga 存的也是 save_cover 返回值——确认它们存库时是否转相对：看现有代码，fetch_posters 用 update_cover_path 存 save_cover 的返回值。若存的是绝对路径需保持与现有一致——本任务只改目录层级，不改相对/绝对策略）。

- [ ] **Step 4: 运行验证通过**

Run: `cd src-tauri && cargo test --lib scanner && cargo build --lib 2>&1 | grep -E "^error" | head`
Expected: 测试 PASS、无 error。原 scan_games 测试（如 scans_game_with_manifest）需改为传 covers_dir。

- [ ] **Step 5: 提交**

```bash
git add src-tauri/src/lib.rs src-tauri/src/library/scanner.rs
git commit -m "feat(covers): 封面写入按类型分 media/comic/game 子目录，游戏封面压缩纳入

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

### Task 2: 启动幂等迁移 migrate_covers_to_subdirs

**Files:**
- Modify: `src-tauri/src/lib.rs`（新增迁移函数 + 启动调用）

- [ ] **Step 1: 写失败测试**

在 `lib.rs` 的 `#[cfg(test)] mod` 内新增（造扁平封面 + DB 记录，跑迁移，断言移到子目录 + DB 更新 + 幂等）：

```rust
    #[test]
    fn migrate_covers_moves_by_prefix_and_updates_db() {
        let tmp = tempfile::tempdir().unwrap();
        let app_data = tmp.path();
        let covers = app_data.join("covers");
        std::fs::create_dir_all(&covers).unwrap();
        // 扁平封面文件
        std::fs::write(covers.join("cover_aaa.jpg"), b"x").unwrap();
        std::fs::write(covers.join("tmdb_bbb.jpg"), b"x").unwrap();
        std::fs::write(covers.join("manga_ccc.jpg"), b"x").unwrap();
        let db = Db::open_in_memory().unwrap();
        {
            let c = db.0.lock().unwrap();
            c.execute("INSERT INTO media(category,category_path,title,cover_path) VALUES('电影','电影','A','covers/cover_aaa.jpg')", []).unwrap();
            c.execute("INSERT INTO media(category,category_path,title,cover_path) VALUES('电影','电影','B','covers/tmdb_bbb.jpg')", []).unwrap();
            c.execute("INSERT INTO comic(category_path,title,cover_path) VALUES('热血','C','covers/manga_ccc.jpg')", []).unwrap();
        }
        migrate_covers_to_subdirs(app_data, &db);
        // 文件移到子目录
        assert!(covers.join("media/cover_aaa.jpg").is_file());
        assert!(covers.join("media/tmdb_bbb.jpg").is_file());
        assert!(covers.join("comic/manga_ccc.jpg").is_file());
        assert!(!covers.join("cover_aaa.jpg").exists());
        // DB 更新
        let c = db.0.lock().unwrap();
        let a: String = c.query_row("SELECT cover_path FROM media WHERE title='A'", [], |r| r.get(0)).unwrap();
        assert_eq!(a, "covers/media/cover_aaa.jpg");
        let cc: String = c.query_row("SELECT cover_path FROM comic WHERE title='C'", [], |r| r.get(0)).unwrap();
        assert_eq!(cc, "covers/comic/manga_ccc.jpg");
        drop(c);
        // 幂等重跑不报错、不重复
        migrate_covers_to_subdirs(app_data, &db);
        assert!(covers.join("media/cover_aaa.jpg").is_file());
    }
```

- [ ] **Step 2: 运行验证失败**

Run: `cd src-tauri && cargo test --lib migrate_covers`
Expected: 编译失败——未定义。

- [ ] **Step 3: 实现迁移函数 + 启动调用**

在 `lib.rs` 加：

```rust
/// 启动幂等迁移：把 covers/ 根下扁平封面按前缀移入子目录（cover_/tmdb_→media，manga_→comic），
/// 并更新 DB cover_path。已在子目录/已迁移的跳过；失败记日志跳过，不中断启动。
fn migrate_covers_to_subdirs(app_data: &std::path::Path, db: &Db) {
    let covers = app_data.join("covers");
    let rd = match std::fs::read_dir(&covers) { Ok(r) => r, Err(_) => return };
    for e in rd.filter_map(|e| e.ok()) {
        let p = e.path();
        if !p.is_file() { continue; } // 跳过 media/comic/game 子目录
        let name = match p.file_name().and_then(|s| s.to_str()) { Some(n) => n.to_string(), None => continue };
        let sub = if name.starts_with("cover_") || name.starts_with("tmdb_") { "media" }
                  else if name.starts_with("manga_") { "comic" }
                  else { continue };
        let dest_dir = covers.join(sub);
        if std::fs::create_dir_all(&dest_dir).is_err() { continue; }
        let dest = dest_dir.join(&name);
        if std::fs::rename(&p, &dest).is_err() { continue; }
        let old_rel = format!("covers/{name}");
        let new_rel = format!("covers/{sub}/{name}");
        if let Ok(conn) = db.0.lock() {
            let _ = conn.execute("UPDATE media SET cover_path=?1 WHERE cover_path=?2", rusqlite::params![new_rel, old_rel]);
            let _ = conn.execute("UPDATE comic SET cover_path=?1 WHERE cover_path=?2", rusqlite::params![new_rel, old_rel]);
        }
    }
}
```

在启动流程（line 638 附近，`migrate_subtitles_to_plain(&dir, &db);` 之后）加 `migrate_covers_to_subdirs(&dir, &db);`。line 757 那处（若是另一初始化路径）同样加。

- [ ] **Step 4: 运行验证通过 + 全量**

Run: `cd src-tauri && cargo test --lib migrate_covers && cargo test --lib 2>&1 | tail -3`
Expected: 全 PASS。

- [ ] **Step 5: 提交**

```bash
git add src-tauri/src/lib.rs
git commit -m "feat(covers): 启动幂等迁移现有扁平封面到 media/comic 子目录并更新 DB

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

## 自查

- **规格覆盖**：5 写入点改子目录（Task 1）、游戏封面压缩纳入 covers/game/（Task 1 scan_games）、启动幂等迁移按前缀 + 改 DB（Task 2）——规格各条均有对应任务。
- **占位符扫描**：无 TBD；每步给出完整代码。Task 1 Step 3 关于 fetch_posters 存相对/绝对的说明是"保持现有策略"的实现提醒，非占位。
- **一致性**：scan_games 新签名 (root, covers_dir) 在 Task 1 定义、scan_root 调用一致；migrate_covers_to_subdirs 前缀规则（cover_/tmdb_→media，manga_→comic）与规格一致；covers/media|comic|game 子目录名贯穿。
- **待实施确认点**：scan_root 加回 app 参数后 handler 注册无需改（tauri 按名注入）；fetch_posters/manga 存库的相对/绝对策略保持现状（只改目录层级）；line 757 是否为独立初始化路径需实施时确认（若是则也加迁移调用）；原 scan_games 测试改传 covers_dir。
