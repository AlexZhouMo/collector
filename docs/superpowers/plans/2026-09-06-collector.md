# Collector 实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 用 Tauri 搭建跨平台（macOS + Windows）本地素材管理器，实现视频/漫画/游戏的标准化与一站式查看运行。

**Architecture:** Rust 后端（Tauri core）按模块隔离——`library`（扫描+SQLite 索引）、`comic`（zip 流式解压）、`player`（libmpv FFI）、`launcher`（进程启动）、`normalize`（字幕/漫画标准化）；前端 WebView 用玻璃拟态 UI + 左侧边栏导航；前后端经 Tauri command/event 通信；无网络依赖。

**Tech Stack:** Tauri 2.x、Rust、SQLite（rusqlite）、libmpv（FFI）、zip crate、image crate、前端 Vite + TypeScript + 原生 CSS（玻璃拟态）。

**参考设计文档：** `docs/superpowers/specs/2026-09-06-collector-design.md`

**关键真实数据事实（来自 demo/subtitles 样本）：**
- 分类目录是**多级递归**嵌套（如 `电影/科幻/星球大战/`、`剧集/美剧/权利的游戏/第7季/`），不是固定两级——扫描器必须递归。
- 真实 `.ass` 为 **UTF-8（带 BOM）+ CRLF**，Aegisub 生成，`Dialogue` 行用 `\N{\fnArial\fs30}` 分隔中英，与 demo `SEPARATOR` 一致。
- 字符映射表默认可为空（标准片源无需修复），仅对非标准来源启用。

**分阶段执行：** 本计划为单份文档，但按 6 个阶段分节，每阶段末尾都有可运行/可测试检查点。建议逐阶段验证。

---

## 文件结构规划

```
collector/
├── src-tauri/
│   ├── Cargo.toml
│   ├── tauri.conf.json
│   ├── build.rs
│   └── src/
│       ├── main.rs                 # Tauri 入口，注册所有 command
│       ├── error.rs                # 统一错误类型 AppError
│       ├── db/
│       │   ├── mod.rs              # DB 连接池、迁移
│       │   └── schema.rs           # 表定义、迁移 SQL
│       ├── library/
│       │   ├── mod.rs              # 对外 command：scan_root / list_items / get_item
│       │   ├── scanner.rs          # 递归扫描 + 侧载解析
│       │   └── model.rs            # MediaItem / MediaKind / Category 类型
│       ├── comic/
│       │   ├── mod.rs              # command：open_comic / get_page / comic_page_count
│       │   └── reader.rs           # zip 流式解压 + LRU 预取缓存
│       ├── player/
│       │   ├── mod.rs              # command：player_load / play / pause / seek / ...
│       │   └── mpv.rs              # libmpv FFI 封装
│       ├── launcher/
│       │   └── mod.rs              # command：launch_game + 平台过滤
│       └── normalize/
│           ├── mod.rs              # command：normalize_subtitles / normalize_comics
│           ├── subtitle.rs         # 字幕清洗引擎（移植 SubtitlesFormat）
│           ├── subtitle_check.rs   # 质检（移植 SubtitlesSearch）
│           └── comic_pack.rs       # 漫画重命名/JPG统一/打zip（移植 ComicRename/Adjust）
├── src/                            # 前端
│   ├── index.html
│   ├── main.ts                     # 应用入口、路由
│   ├── styles/
│   │   ├── theme.css               # 玻璃拟态 design tokens
│   │   └── animations.css          # 动画
│   ├── lib/
│   │   ├── ipc.ts                  # Tauri invoke 封装
│   │   └── router.ts               # 简单前端路由
│   ├── components/
│   │   ├── Sidebar.ts
│   │   ├── PosterGrid.ts
│   │   └── Toast.ts
│   └── views/
│       ├── VideoView.ts
│       ├── PlayerView.ts
│       ├── ComicView.ts
│       ├── ComicReaderView.ts
│       ├── GameView.ts
│       ├── NormalizeView.ts
│       └── SettingsView.ts
├── package.json
└── vite.config.ts
```

---

## 阶段 1：骨架（工程 + 导航 + DB + 扫描器 + 设置）

**阶段目标（检查点）：** 应用能启动，显示左侧边栏，能在设置页配置视频/漫画/游戏根目录并触发扫描，视频库能递归列出真实素材（用 demo/subtitles 作为测试根目录）。

### Task 1.1：初始化 Tauri 工程

**Files:**
- Create: `package.json`, `vite.config.ts`, `src/index.html`, `src/main.ts`
- Create: `src-tauri/Cargo.toml`, `src-tauri/tauri.conf.json`, `src-tauri/build.rs`, `src-tauri/src/main.rs`

- [ ] **Step 1: 用脚手架创建工程**

Run:
```bash
cd /Users/zhoumo/Documents/Claude/collector
npm create tauri-app@latest . -- --template vanilla-ts --manager npm --yes
```
Expected: 生成 `src/`、`src-tauri/`、`package.json`。若目录非空报错，先创建到临时目录再拷贝 `src-tauri/` 与前端脚手架文件。

- [ ] **Step 2: 安装依赖并验证可编译**

Run:
```bash
cd /Users/zhoumo/Documents/Claude/collector && npm install && npm run tauri build -- --debug 2>&1 | tail -5
```
Expected: 编译通过（首次较慢），生成 debug 产物。若缺少系统依赖（webkit2gtk 等）按报错安装；macOS 通常开箱可用。

- [ ] **Step 3: 提交**

```bash
git add -A && git commit -m "chore: scaffold Tauri vanilla-ts project"
```

### Task 1.2：统一错误类型

**Files:**
- Create: `src-tauri/src/error.rs`
- Modify: `src-tauri/src/main.rs`

- [ ] **Step 1: 写失败测试**

Create `src-tauri/src/error.rs`:
```rust
use serde::Serialize;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("db error: {0}")]
    Db(String),
    #[error("not found: {0}")]
    NotFound(String),
    #[error("invalid input: {0}")]
    Invalid(String),
    #[error("{0}")]
    Other(String),
}

// Tauri command 返回的错误需可序列化为字符串
impl Serialize for AppError {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_string())
    }
}

pub type AppResult<T> = Result<T, AppError>;

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn not_found_message_formats() {
        let e = AppError::NotFound("item 42".into());
        assert_eq!(e.to_string(), "not found: item 42");
    }
}
```

- [ ] **Step 2: 加依赖并运行测试确认失败→通过**

Modify `src-tauri/Cargo.toml`，在 `[dependencies]` 增加：
```toml
thiserror = "1"
```
在 `src-tauri/src/main.rs` 顶部加 `mod error;`。

Run: `cd src-tauri && cargo test error::tests::not_found_message_formats`
Expected: PASS。

- [ ] **Step 3: 提交**

```bash
git add -A && git commit -m "feat: add unified AppError type"
```

### Task 1.3：SQLite 连接与 schema 迁移

**Files:**
- Create: `src-tauri/src/db/mod.rs`, `src-tauri/src/db/schema.rs`
- Modify: `src-tauri/Cargo.toml`, `src-tauri/src/main.rs`

- [ ] **Step 1: 加依赖**

Modify `src-tauri/Cargo.toml` `[dependencies]`：
```toml
rusqlite = { version = "0.31", features = ["bundled"] }
```

- [ ] **Step 2: 写 schema**

Create `src-tauri/src/db/schema.rs`:
```rust
pub const MIGRATIONS: &[&str] = &[
    // v1
    "CREATE TABLE IF NOT EXISTS media_item (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        kind TEXT NOT NULL,            -- 'video' | 'comic' | 'game'
        category TEXT NOT NULL,        -- 顶级分类，如 电影/动漫/电视剧
        category_path TEXT NOT NULL,   -- 完整相对分类路径，如 电影/科幻/星球大战
        title TEXT NOT NULL,
        path TEXT NOT NULL UNIQUE,     -- 素材绝对路径（视频=mkv, 漫画=zip, 游戏=目录）
        subtitle_path TEXT,            -- 视频外挂 .ass 绝对路径
        cover_path TEXT,               -- 封面绝对路径（漫画为空，运行时取zip内首图）
        description TEXT,
        platform_ok INTEGER NOT NULL DEFAULT 1,  -- 游戏：当前平台是否可启动
        exec_path TEXT,                -- 游戏：当前平台启动绝对路径
        scanned_at INTEGER NOT NULL
    );",
    "CREATE INDEX IF NOT EXISTS idx_media_kind ON media_item(kind);",
    "CREATE TABLE IF NOT EXISTS watch_state (
        item_id INTEGER PRIMARY KEY REFERENCES media_item(id) ON DELETE CASCADE,
        position_secs REAL DEFAULT 0,     -- 视频进度
        comic_page INTEGER DEFAULT 0,     -- 漫画页码
        last_opened_at INTEGER
    );",
    "CREATE TABLE IF NOT EXISTS game_state (
        item_id INTEGER PRIMARY KEY REFERENCES media_item(id) ON DELETE CASCADE,
        last_launched_at INTEGER,
        launch_count INTEGER DEFAULT 0
    );",
    "CREATE TABLE IF NOT EXISTS settings (
        key TEXT PRIMARY KEY,
        value TEXT NOT NULL
    );",
];
```

- [ ] **Step 3: 写连接+迁移+失败测试**

Create `src-tauri/src/db/mod.rs`:
```rust
pub mod schema;

use crate::error::{AppError, AppResult};
use rusqlite::Connection;
use std::path::Path;
use std::sync::Mutex;

pub struct Db(pub Mutex<Connection>);

impl Db {
    pub fn open(path: &Path) -> AppResult<Self> {
        let conn = Connection::open(path).map_err(|e| AppError::Db(e.to_string()))?;
        conn.execute_batch("PRAGMA foreign_keys = ON;")
            .map_err(|e| AppError::Db(e.to_string()))?;
        for m in schema::MIGRATIONS {
            conn.execute_batch(m).map_err(|e| AppError::Db(e.to_string()))?;
        }
        Ok(Db(Mutex::new(conn)))
    }

    pub fn open_in_memory() -> AppResult<Self> {
        let conn = Connection::open_in_memory().map_err(|e| AppError::Db(e.to_string()))?;
        for m in schema::MIGRATIONS {
            conn.execute_batch(m).map_err(|e| AppError::Db(e.to_string()))?;
        }
        Ok(Db(Mutex::new(conn)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn migrations_create_tables() {
        let db = Db::open_in_memory().unwrap();
        let conn = db.0.lock().unwrap();
        let count: i64 = conn
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE type='table' AND name IN ('media_item','watch_state','game_state','settings')",
                [], |r| r.get(0)).unwrap();
        assert_eq!(count, 4);
    }
}
```
在 `main.rs` 加 `mod db;`。

- [ ] **Step 4: 运行测试**

Run: `cd src-tauri && cargo test db::tests::migrations_create_tables`
Expected: PASS。

- [ ] **Step 5: 提交**

```bash
git add -A && git commit -m "feat: add SQLite db with schema migrations"
```

### Task 1.4：媒体类型模型

**Files:**
- Create: `src-tauri/src/library/model.rs`, `src-tauri/src/library/mod.rs`
- Modify: `src-tauri/Cargo.toml`, `src-tauri/src/main.rs`

- [ ] **Step 1: 加 serde 依赖**（若脚手架未含）

Modify `src-tauri/Cargo.toml` `[dependencies]`：
```toml
serde = { version = "1", features = ["derive"] }
serde_json = "1"
```

- [ ] **Step 2: 写模型 + 失败测试**

Create `src-tauri/src/library/model.rs`:
```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MediaKind {
    Video,
    Comic,
    Game,
}

impl MediaKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            MediaKind::Video => "video",
            MediaKind::Comic => "comic",
            MediaKind::Game => "game",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaItem {
    pub id: i64,
    pub kind: MediaKind,
    pub category: String,
    pub category_path: String,
    pub title: String,
    pub path: String,
    pub subtitle_path: Option<String>,
    pub cover_path: Option<String>,
    pub description: Option<String>,
    pub platform_ok: bool,
    pub exec_path: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn media_kind_serializes_lowercase() {
        let j = serde_json::to_string(&MediaKind::Video).unwrap();
        assert_eq!(j, "\"video\"");
    }
}
```

Create `src-tauri/src/library/mod.rs`:
```rust
pub mod model;
pub mod scanner;
```
在 `main.rs` 加 `mod library;`。（`scanner` 下一任务创建，先建空文件避免编译错误：`echo "" > src-tauri/src/library/scanner.rs`）

- [ ] **Step 3: 运行测试**

Run: `cd src-tauri && cargo test library::model::tests::media_kind_serializes_lowercase`
Expected: PASS。

- [ ] **Step 4: 提交**

```bash
git add -A && git commit -m "feat: add media item model"
```

### Task 1.5：视频扫描器（递归 + 外挂字幕 + 封面）

**Files:**
- Modify: `src-tauri/src/library/scanner.rs`
- Modify: `src-tauri/Cargo.toml`

- [ ] **Step 1: 加 walkdir 依赖**

Modify `src-tauri/Cargo.toml` `[dependencies]`：
```toml
walkdir = "2"
```

- [ ] **Step 2: 写失败测试（用临时目录模拟真实多级结构）**

Create `src-tauri/src/library/scanner.rs`:
```rust
use crate::library::model::{MediaItem, MediaKind};
use std::path::Path;
use walkdir::WalkDir;

/// 扫描视频根目录。分类=相对根的第一级目录名；
/// category_path=相对根的完整目录路径（不含文件名）。
/// 每个 .mkv 为一个条目，同名 .ass 作外挂字幕，
/// 同目录 poster.jpg 或同名 .jpg 作封面，同名/同目录 info.txt 作简介。
pub fn scan_videos(root: &Path) -> Vec<ScannedItem> {
    let mut items = Vec::new();
    for entry in WalkDir::new(root).into_iter().filter_map(|e| e.ok()) {
        let p = entry.path();
        if p.extension().and_then(|s| s.to_str()) != Some("mkv") {
            continue;
        }
        let rel = p.strip_prefix(root).unwrap_or(p);
        let comps: Vec<String> = rel
            .parent()
            .map(|d| d.components().map(|c| c.as_os_str().to_string_lossy().into_owned()).collect())
            .unwrap_or_default();
        let category = comps.first().cloned().unwrap_or_default();
        let category_path = comps.join("/");
        let stem = p.file_stem().unwrap().to_string_lossy().into_owned();
        let dir = p.parent().unwrap();

        let subtitle = dir.join(format!("{stem}.ass"));
        let subtitle_path = subtitle.exists().then(|| subtitle.to_string_lossy().into_owned());

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
            category,
            category_path,
            title: stem,
            path: p.to_string_lossy().into_owned(),
            subtitle_path,
            cover_path,
            description,
            platform_ok: true,
            exec_path: None,
        });
    }
    items
}

#[derive(Debug, Clone)]
pub struct ScannedItem {
    pub kind: MediaKind,
    pub category: String,
    pub category_path: String,
    pub title: String,
    pub path: String,
    pub subtitle_path: Option<String>,
    pub cover_path: Option<String>,
    pub description: Option<String>,
    pub platform_ok: bool,
    pub exec_path: Option<String>,
}

impl ScannedItem {
    #[allow(dead_code)]
    pub fn into_item(self, id: i64) -> MediaItem {
        MediaItem {
            id,
            kind: self.kind,
            category: self.category,
            category_path: self.category_path,
            title: self.title,
            path: self.path,
            subtitle_path: self.subtitle_path,
            cover_path: self.cover_path,
            description: self.description,
            platform_ok: self.platform_ok,
            exec_path: self.exec_path,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn scans_nested_categories_and_sidecars() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        // 模拟真实多级结构：电影/科幻/星球大战/
        let dir = root.join("电影").join("科幻").join("星球大战");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("星球大战.mkv"), b"x").unwrap();
        fs::write(dir.join("星球大战.ass"), b"sub").unwrap();
        fs::write(dir.join("poster.jpg"), b"img").unwrap();
        fs::write(dir.join("info.txt"), "  一部太空歌剧  ").unwrap();

        let items = scan_videos(root);
        assert_eq!(items.len(), 1);
        let it = &items[0];
        assert_eq!(it.category, "电影");
        assert_eq!(it.category_path, "电影/科幻/星球大战");
        assert_eq!(it.title, "星球大战");
        assert!(it.subtitle_path.is_some());
        assert!(it.cover_path.is_some());
        assert_eq!(it.description.as_deref(), Some("一部太空歌剧"));
    }
}
```

- [ ] **Step 3: 加 dev 依赖 tempfile**

Modify `src-tauri/Cargo.toml`，新增：
```toml
[dev-dependencies]
tempfile = "3"
```

- [ ] **Step 4: 运行测试**

Run: `cd src-tauri && cargo test library::scanner::tests::scans_nested_categories_and_sidecars`
Expected: PASS。

- [ ] **Step 5: 提交**

```bash
git add -A && git commit -m "feat: recursive video scanner with sidecar detection"
```

### Task 1.6：扫描结果入库 + 查询 command

**Files:**
- Modify: `src-tauri/src/library/mod.rs`

- [ ] **Step 1: 写入库/查询逻辑 + 失败测试**

Append to `src-tauri/src/library/mod.rs`:
```rust
use crate::db::Db;
use crate::error::{AppError, AppResult};
use crate::library::model::{MediaItem, MediaKind};
use crate::library::scanner::ScannedItem;
use rusqlite::params;

pub fn upsert_items(db: &Db, items: &[ScannedItem]) -> AppResult<usize> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() as i64;
    let mut conn = db.0.lock().unwrap();
    let tx = conn.transaction().map_err(|e| AppError::Db(e.to_string()))?;
    let mut n = 0;
    for it in items {
        tx.execute(
            "INSERT INTO media_item
              (kind,category,category_path,title,path,subtitle_path,cover_path,description,platform_ok,exec_path,scanned_at)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)
             ON CONFLICT(path) DO UPDATE SET
               category=excluded.category, category_path=excluded.category_path,
               title=excluded.title, subtitle_path=excluded.subtitle_path,
               cover_path=excluded.cover_path, description=excluded.description,
               platform_ok=excluded.platform_ok, exec_path=excluded.exec_path,
               scanned_at=excluded.scanned_at",
            params![
                it.kind.as_str(), it.category, it.category_path, it.title, it.path,
                it.subtitle_path, it.cover_path, it.description,
                it.platform_ok as i64, it.exec_path, now
            ],
        ).map_err(|e| AppError::Db(e.to_string()))?;
        n += 1;
    }
    tx.commit().map_err(|e| AppError::Db(e.to_string()))?;
    Ok(n)
}

pub fn list_items(db: &Db, kind: MediaKind) -> AppResult<Vec<MediaItem>> {
    let conn = db.0.lock().unwrap();
    let mut stmt = conn.prepare(
        "SELECT id,kind,category,category_path,title,path,subtitle_path,cover_path,description,platform_ok,exec_path
         FROM media_item WHERE kind=?1 ORDER BY category_path, title"
    ).map_err(|e| AppError::Db(e.to_string()))?;
    let rows = stmt.query_map(params![kind.as_str()], |r| {
        let kind_s: String = r.get(1)?;
        let kind = match kind_s.as_str() {
            "video" => MediaKind::Video, "comic" => MediaKind::Comic, _ => MediaKind::Game,
        };
        Ok(MediaItem {
            id: r.get(0)?, kind, category: r.get(2)?, category_path: r.get(3)?,
            title: r.get(4)?, path: r.get(5)?, subtitle_path: r.get(6)?,
            cover_path: r.get(7)?, description: r.get(8)?,
            platform_ok: r.get::<_, i64>(9)? != 0, exec_path: r.get(10)?,
        })
    }).map_err(|e| AppError::Db(e.to_string()))?;
    let mut out = Vec::new();
    for row in rows { out.push(row.map_err(|e| AppError::Db(e.to_string()))?); }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library::scanner::ScannedItem;

    fn sample(path: &str) -> ScannedItem {
        ScannedItem {
            kind: MediaKind::Video, category: "电影".into(),
            category_path: "电影/科幻".into(), title: "T".into(), path: path.into(),
            subtitle_path: None, cover_path: None, description: None,
            platform_ok: true, exec_path: None,
        }
    }

    #[test]
    fn upsert_then_list_roundtrip_and_dedup() {
        let db = Db::open_in_memory().unwrap();
        upsert_items(&db, &[sample("/a.mkv"), sample("/b.mkv")]).unwrap();
        upsert_items(&db, &[sample("/a.mkv")]).unwrap(); // 同 path 应更新而非重复
        let items = list_items(&db, MediaKind::Video).unwrap();
        assert_eq!(items.len(), 2);
    }
}
```

- [ ] **Step 2: 运行测试**

Run: `cd src-tauri && cargo test library::tests::upsert_then_list_roundtrip_and_dedup`
Expected: PASS。

- [ ] **Step 3: 提交**

```bash
git add -A && git commit -m "feat: upsert and list media items in db"
```

### Task 1.7：设置存取 + Tauri command 注册

**Files:**
- Modify: `src-tauri/src/main.rs`
- Create: `src-tauri/src/settings.rs`

- [ ] **Step 1: 写设置读写 + 失败测试**

Create `src-tauri/src/settings.rs`:
```rust
use crate::db::Db;
use crate::error::{AppError, AppResult};
use rusqlite::params;

pub fn set(db: &Db, key: &str, value: &str) -> AppResult<()> {
    let conn = db.0.lock().unwrap();
    conn.execute(
        "INSERT INTO settings(key,value) VALUES(?1,?2)
         ON CONFLICT(key) DO UPDATE SET value=excluded.value",
        params![key, value],
    ).map_err(|e| AppError::Db(e.to_string()))?;
    Ok(())
}

pub fn get(db: &Db, key: &str) -> AppResult<Option<String>> {
    let conn = db.0.lock().unwrap();
    conn.query_row("SELECT value FROM settings WHERE key=?1", params![key], |r| r.get(0))
        .map(Some)
        .or_else(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => Ok(None),
            other => Err(AppError::Db(other.to_string())),
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn set_get_roundtrip() {
        let db = Db::open_in_memory().unwrap();
        assert_eq!(get(&db, "video_root").unwrap(), None);
        set(&db, "video_root", "/媒体/视频").unwrap();
        assert_eq!(get(&db, "video_root").unwrap().as_deref(), Some("/媒体/视频"));
    }
}
```

- [ ] **Step 2: 在 main.rs 注册 command 与 state**

Rewrite `src-tauri/src/main.rs`:
```rust
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod error;
mod db;
mod library;
mod settings;

use db::Db;
use error::AppResult;
use library::model::{MediaItem, MediaKind};
use std::path::PathBuf;
use tauri::Manager;

#[tauri::command]
fn set_root(db: tauri::State<Db>, kind: String, path: String) -> AppResult<()> {
    settings::set(&db, &format!("{kind}_root"), &path)
}

#[tauri::command]
fn get_root(db: tauri::State<Db>, kind: String) -> AppResult<Option<String>> {
    settings::get(&db, &format!("{kind}_root"))
}

#[tauri::command]
fn scan_root(db: tauri::State<Db>, kind: String) -> AppResult<usize> {
    let root = settings::get(&db, &format!("{kind}_root"))?
        .ok_or_else(|| error::AppError::Invalid(format!("{kind} root not set")))?;
    let items = match kind.as_str() {
        "video" => library::scanner::scan_videos(std::path::Path::new(&root)),
        // comic / game 在后续阶段接入
        _ => Vec::new(),
    };
    library::upsert_items(&db, &items)
}

#[tauri::command]
fn list_media(db: tauri::State<Db>, kind: String) -> AppResult<Vec<MediaItem>> {
    let k = match kind.as_str() {
        "video" => MediaKind::Video, "comic" => MediaKind::Comic, _ => MediaKind::Game,
    };
    library::list_items(&db, k)
}

fn main() {
    tauri::Builder::default()
        .setup(|app| {
            let dir: PathBuf = app.path().app_data_dir().expect("app data dir");
            std::fs::create_dir_all(&dir).ok();
            let db = Db::open(&dir.join("collector.sqlite")).expect("open db");
            app.manage(db);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![set_root, get_root, scan_root, list_media])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
```

- [ ] **Step 3: 运行后端测试 + 编译**

Run: `cd src-tauri && cargo test && cargo build 2>&1 | tail -3`
Expected: 全部测试 PASS，编译通过。

- [ ] **Step 4: 提交**

```bash
git add -A && git commit -m "feat: settings store and register tauri commands"
```

### Task 1.8：前端骨架（主题 + IPC + 路由 + 侧边栏）

**Files:**
- Create: `src/styles/theme.css`, `src/styles/animations.css`, `src/lib/ipc.ts`, `src/lib/router.ts`, `src/components/Sidebar.ts`
- Modify: `src/index.html`, `src/main.ts`

- [ ] **Step 1: 玻璃拟态 design tokens**

Create `src/styles/theme.css`:
```css
:root {
  --bg-0: #0d1220;
  --bg-1: #131a2e;
  --glass: rgba(255,255,255,.06);
  --glass-strong: rgba(255,255,255,.1);
  --border: rgba(255,255,255,.12);
  --accent: #5b8cff;
  --accent-glow: rgba(91,140,255,.35);
  --text: #dbe6ff;
  --text-dim: #8b9bc0;
  --radius: 12px;
}
* { box-sizing: border-box; margin: 0; padding: 0; }
html, body, #app { height: 100%; }
body {
  font-family: system-ui, -apple-system, "PingFang SC", "Microsoft YaHei", sans-serif;
  color: var(--text);
  background: linear-gradient(135deg, var(--bg-0), var(--bg-1));
  overflow: hidden;
}
.glass {
  background: var(--glass);
  border: 1px solid var(--border);
  backdrop-filter: blur(12px);
  -webkit-backdrop-filter: blur(12px);
  border-radius: var(--radius);
}
```

Create `src/styles/animations.css`:
```css
@keyframes fadeInUp { from { opacity:0; transform: translateY(8px);} to {opacity:1; transform:none;} }
.view-enter { animation: fadeInUp .28s ease both; }
.card-hover { transition: transform .18s ease, box-shadow .18s ease; }
.card-hover:hover { transform: translateY(-4px); box-shadow: 0 8px 24px var(--accent-glow); }
```

- [ ] **Step 2: IPC 封装**

Create `src/lib/ipc.ts`:
```ts
import { invoke } from "@tauri-apps/api/core";

export interface MediaItem {
  id: number;
  kind: "video" | "comic" | "game";
  category: string;
  category_path: string;
  title: string;
  path: string;
  subtitle_path: string | null;
  cover_path: string | null;
  description: string | null;
  platform_ok: boolean;
  exec_path: string | null;
}

export const api = {
  setRoot: (kind: string, path: string) => invoke<void>("set_root", { kind, path }),
  getRoot: (kind: string) => invoke<string | null>("get_root", { kind }),
  scanRoot: (kind: string) => invoke<number>("scan_root", { kind }),
  listMedia: (kind: string) => invoke<MediaItem[]>("list_media", { kind }),
};
```

- [ ] **Step 3: 简单路由**

Create `src/lib/router.ts`:
```ts
export type Route = "video" | "comic" | "game" | "normalize" | "settings";
type Handler = (route: Route) => void;

class Router {
  private handlers: Handler[] = [];
  current: Route = "video";
  on(h: Handler) { this.handlers.push(h); }
  go(route: Route) { this.current = route; this.handlers.forEach(h => h(route)); }
}
export const router = new Router();
```

- [ ] **Step 4: 侧边栏组件**

Create `src/components/Sidebar.ts`:
```ts
import { router, Route } from "../lib/router";

const TOP: [Route, string][] = [["video","▶ 视频"],["comic","▤ 漫画"],["game","◉ 游戏"]];
const BOTTOM: [Route, string][] = [["normalize","⚙ 标准化"],["settings","⚙ 设置"]];

export function Sidebar(): HTMLElement {
  const el = document.createElement("aside");
  el.className = "sidebar glass";
  const render = () => {
    const item = ([r,label]: [Route,string]) =>
      `<div class="nav-item ${router.current===r?"active":""}" data-route="${r}">${label}</div>`;
    el.innerHTML =
      `<div class="brand">◈ COLLECTOR</div>
       <nav class="nav-top">${TOP.map(item).join("")}</nav>
       <nav class="nav-bottom">${BOTTOM.map(item).join("")}</nav>`;
    el.querySelectorAll<HTMLElement>(".nav-item").forEach(n =>
      n.onclick = () => router.go(n.dataset.route as Route));
  };
  router.on(render);
  render();
  return el;
}
```
（配套 CSS 追加到 `theme.css`：`.sidebar{width:180px;height:100%;padding:16px 12px;display:flex;flex-direction:column;gap:6px} .brand{color:var(--text);letter-spacing:2px;font-size:13px;margin-bottom:14px;padding-left:8px} .nav-bottom{margin-top:auto} .nav-item{padding:9px 12px;border-radius:10px;color:var(--text-dim);cursor:pointer;font-size:13px;transition:background .15s} .nav-item:hover{background:var(--glass)} .nav-item.active{background:var(--accent-glow);color:var(--text)}`）

- [ ] **Step 5: 入口装配**

Rewrite `src/main.ts`:
```ts
import "./styles/theme.css";
import "./styles/animations.css";
import { Sidebar } from "./components/Sidebar";
import { router, Route } from "./lib/router";

const app = document.querySelector<HTMLDivElement>("#app")!;
app.style.display = "flex";
app.appendChild(Sidebar());

const content = document.createElement("main");
content.className = "content";
content.style.flex = "1";
content.style.padding = "20px";
content.style.overflow = "auto";
app.appendChild(content);

async function renderRoute(route: Route) {
  content.innerHTML = `<div class="view-enter"><h1 style="color:var(--text);font-size:20px">${route}</h1></div>`;
  // 各 view 在后续任务替换此处
}
router.on(renderRoute);
renderRoute(router.current);
```
`src/index.html` 确保有 `<div id="app"></div>`。

- [ ] **Step 6: 运行应用手动验证**

Run: `cd /Users/zhoumo/Documents/Claude/collector && npm run tauri dev`
Expected: 窗口打开，左侧显示 COLLECTOR 边栏，点击各项内容区文字切换，玻璃拟态样式生效。

- [ ] **Step 7: 提交**

```bash
git add -A && git commit -m "feat: frontend skeleton with sidebar, theme, ipc, router"
```

### Task 1.9：设置页 + 视频库视图（阶段 1 检查点）

**Files:**
- Create: `src/views/SettingsView.ts`, `src/views/VideoView.ts`, `src/components/PosterGrid.ts`
- Modify: `src/main.ts`

- [ ] **Step 1: 设置页（选目录 + 扫描）**

Create `src/views/SettingsView.ts`:
```ts
import { open } from "@tauri-apps/plugin-dialog";
import { api } from "../lib/ipc";

export async function SettingsView(): Promise<HTMLElement> {
  const el = document.createElement("div");
  el.className = "view-enter";
  const kinds: [string,string][] = [["video","视频"],["comic","漫画"],["game","游戏"]];
  const roots = Object.fromEntries(await Promise.all(
    kinds.map(async ([k]) => [k, await api.getRoot(k)])));
  el.innerHTML = `<h1 style="font-size:20px;margin-bottom:16px">设置</h1>` +
    kinds.map(([k,label]) => `
      <div class="glass" style="padding:14px;margin-bottom:12px">
        <div style="margin-bottom:8px">${label}根目录：<span id="root-${k}" style="color:var(--text-dim)">${roots[k]??"未设置"}</span></div>
        <button data-pick="${k}">选择目录</button>
        <button data-scan="${k}">扫描</button>
      </div>`).join("");
  el.querySelectorAll<HTMLButtonElement>("[data-pick]").forEach(b => b.onclick = async () => {
    const k = b.dataset.pick!;
    const dir = await open({ directory: true });
    if (typeof dir === "string") { await api.setRoot(k, dir); el.querySelector(`#root-${k}`)!.textContent = dir; }
  });
  el.querySelectorAll<HTMLButtonElement>("[data-scan]").forEach(b => b.onclick = async () => {
    const n = await api.scanRoot(b.dataset.scan!);
    alert(`扫描完成，${n} 项`);
  });
  return el;
}
```
（需装插件：`npm i @tauri-apps/plugin-dialog` 并在 `src-tauri/Cargo.toml` 加 `tauri-plugin-dialog = "2"`，`main.rs` 的 builder 链上 `.plugin(tauri_plugin_dialog::init())`。）

- [ ] **Step 2: 海报网格组件**

Create `src/components/PosterGrid.ts`:
```ts
import { convertFileSrc } from "@tauri-apps/api/core";
import { MediaItem } from "../lib/ipc";

export function PosterGrid(items: MediaItem[], onOpen: (it: MediaItem) => void): HTMLElement {
  const grid = document.createElement("div");
  grid.className = "poster-grid";
  grid.innerHTML = items.map((it, i) => `
    <div class="poster card-hover" data-i="${i}">
      <div class="poster-img">${it.cover_path ? `<img src="${convertFileSrc(it.cover_path)}"/>` : `<div class="poster-ph">${it.title}</div>`}</div>
      <div class="poster-title">${it.title}</div>
    </div>`).join("");
  grid.querySelectorAll<HTMLElement>(".poster").forEach(p =>
    p.onclick = () => onOpen(items[Number(p.dataset.i)]));
  return grid;
}
```
（CSS 追加：`.poster-grid{display:grid;grid-template-columns:repeat(auto-fill,minmax(150px,1fr));gap:14px} .poster{cursor:pointer} .poster-img{aspect-ratio:2/3;border-radius:10px;overflow:hidden;background:var(--glass);border:1px solid var(--border);display:flex;align-items:center;justify-content:center} .poster-img img{width:100%;height:100%;object-fit:cover} .poster-ph{color:var(--text-dim);font-size:12px;padding:8px;text-align:center} .poster-title{margin-top:6px;font-size:12px;color:var(--text);text-align:center}`）

- [ ] **Step 3: 视频库视图（分类 tab + 网格）**

Create `src/views/VideoView.ts`:
```ts
import { api, MediaItem } from "../lib/ipc";
import { PosterGrid } from "../components/PosterGrid";

export async function VideoView(): Promise<HTMLElement> {
  const el = document.createElement("div");
  el.className = "view-enter";
  const items = await api.listMedia("video");
  const cats = ["电影","动漫","电视剧"];
  let active = cats[0];
  const render = () => {
    const filtered = items.filter(i => i.category === active);
    el.innerHTML = `<div class="tabs">${cats.map(c =>
      `<span class="tab ${c===active?"active":""}" data-c="${c}">${c}</span>`).join("")}</div>`;
    el.querySelectorAll<HTMLElement>(".tab").forEach(t =>
      t.onclick = () => { active = t.dataset.c!; render(); });
    el.appendChild(PosterGrid(filtered, (it: MediaItem) => {
      console.log("open video", it.path); // 阶段 3 接入播放器
    }));
  };
  render();
  return el;
}
```
（CSS 追加：`.tabs{display:flex;gap:8px;margin-bottom:16px} .tab{padding:5px 14px;border-radius:20px;background:var(--glass);color:var(--text-dim);cursor:pointer;font-size:12px} .tab.active{background:var(--accent-glow);color:var(--text)}`）

- [ ] **Step 4: 在 main.ts 路由接入真实 view**

Modify `src/main.ts` 的 `renderRoute`：
```ts
import { VideoView } from "./views/VideoView";
import { SettingsView } from "./views/SettingsView";

async function renderRoute(route: Route) {
  content.innerHTML = "";
  let view: HTMLElement;
  switch (route) {
    case "video": view = await VideoView(); break;
    case "settings": view = await SettingsView(); break;
    default: view = document.createElement("div"); view.className = "view-enter";
             view.innerHTML = `<h1 style="font-size:20px">${route}（后续阶段）</h1>`;
  }
  content.appendChild(view);
}
```

- [ ] **Step 5: 阶段 1 检查点验证**

Run: `npm run tauri dev`，然后：
1. 进设置页，把视频根目录设为 `/Users/zhoumo/Documents/Claude/collector/demo/subtitles`（该目录只有 .ass 无 .mkv，仅验证不崩；正式验证需一个含 .mkv 的目录）
2. 为验证 .mkv 扫描，临时造测试目录：`mkdir -p /tmp/vtest/电影/科幻/星战 && touch /tmp/vtest/电影/科幻/星战/星战.mkv`，设为视频根并扫描
3. 回到视频菜单，「电影」tab 下应出现「星战」海报占位

Expected: 扫描返回计数，视频库正确按分类展示递归扫到的条目。

- [ ] **Step 6: 提交**

```bash
git add -A && git commit -m "feat: settings view and video library view (phase 1 checkpoint)"
```

---

## 阶段 2：漫画阅读器

**阶段目标（检查点）：** 配置漫画根目录后能列出 zip 漫画（封面取 zip 内首图），点击进入阅读器，翻页浏览（按需流式解压 + 预取），可切长条滚动，记忆页码。

### Task 2.1：漫画扫描（zip 条目 + 分类）

**Files:**
- Modify: `src-tauri/src/library/scanner.rs`
- Modify: `src-tauri/Cargo.toml`

- [ ] **Step 1: 加 zip 依赖**

Modify `src-tauri/Cargo.toml` `[dependencies]`：
```toml
zip = "2"
```

- [ ] **Step 2: 写漫画扫描 + 失败测试**

Append to `src-tauri/src/library/scanner.rs`:
```rust
/// 扫描漫画根目录：每个 .zip 为一条目，分类=第一级目录，
/// category_path=完整目录路径，简介取同名 .txt。封面运行时取 zip 内首图，此处留空。
pub fn scan_comics(root: &Path) -> Vec<ScannedItem> {
    let mut items = Vec::new();
    for entry in WalkDir::new(root).into_iter().filter_map(|e| e.ok()) {
        let p = entry.path();
        if p.extension().and_then(|s| s.to_str()) != Some("zip") { continue; }
        let rel = p.strip_prefix(root).unwrap_or(p);
        let comps: Vec<String> = rel.parent()
            .map(|d| d.components().map(|c| c.as_os_str().to_string_lossy().into_owned()).collect())
            .unwrap_or_default();
        let stem = p.file_stem().unwrap().to_string_lossy().into_owned();
        let dir = p.parent().unwrap();
        let info = dir.join(format!("{stem}.txt"));
        let description = std::fs::read_to_string(&info).ok().map(|s| s.trim().to_string());
        items.push(ScannedItem {
            kind: MediaKind::Comic,
            category: comps.first().cloned().unwrap_or_default(),
            category_path: comps.join("/"),
            title: stem,
            path: p.to_string_lossy().into_owned(),
            subtitle_path: None, cover_path: None, description,
            platform_ok: true, exec_path: None,
        });
    }
    items
}

#[cfg(test)]
mod comic_scan_tests {
    use super::*;
    use std::fs;
    #[test]
    fn scans_zip_comics() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("热血").join("海贼王");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("第01卷.zip"), b"PK").unwrap();
        fs::write(dir.join("第01卷.txt"), "简介").unwrap();
        let items = scan_comics(tmp.path());
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].category, "热血");
        assert_eq!(items[0].description.as_deref(), Some("简介"));
    }
}
```

- [ ] **Step 3: 接入 scan_root**

Modify `src-tauri/src/main.rs` 的 `scan_root` match，加分支：
```rust
"comic" => library::scanner::scan_comics(std::path::Path::new(&root)),
```

- [ ] **Step 4: 运行测试**

Run: `cd src-tauri && cargo test library::scanner::comic_scan_tests::scans_zip_comics`
Expected: PASS。

- [ ] **Step 5: 提交**

```bash
git add -A && git commit -m "feat: comic zip scanner"
```

### Task 2.2：zip 阅读器（页列表 + 单页读取 + 首图封面）

**Files:**
- Create: `src-tauri/src/comic/mod.rs`, `src-tauri/src/comic/reader.rs`
- Modify: `src-tauri/src/main.rs`

- [ ] **Step 1: 写 reader + 失败测试**

Create `src-tauri/src/comic/reader.rs`:
```rust
use crate::error::{AppError, AppResult};
use std::fs::File;
use std::io::Read;
use std::path::Path;
use zip::ZipArchive;

/// 返回 zip 内按文件名排序的图片条目名列表（jpg/jpeg/png）。
pub fn list_pages(zip_path: &Path) -> AppResult<Vec<String>> {
    let file = File::open(zip_path)?;
    let mut ar = ZipArchive::new(file).map_err(|e| AppError::Other(e.to_string()))?;
    let mut names: Vec<String> = (0..ar.len())
        .filter_map(|i| ar.by_index(i).ok().map(|f| f.name().to_string()))
        .filter(|n| {
            let l = n.to_lowercase();
            l.ends_with(".jpg") || l.ends_with(".jpeg") || l.ends_with(".png")
        })
        .collect();
    names.sort();
    Ok(names)
}

/// 读取 zip 内指定条目的原始字节。
pub fn read_entry(zip_path: &Path, entry_name: &str) -> AppResult<Vec<u8>> {
    let file = File::open(zip_path)?;
    let mut ar = ZipArchive::new(file).map_err(|e| AppError::Other(e.to_string()))?;
    let mut f = ar.by_name(entry_name).map_err(|_| AppError::NotFound(entry_name.into()))?;
    let mut buf = Vec::with_capacity(f.size() as usize);
    f.read_to_end(&mut buf)?;
    Ok(buf)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use zip::write::SimpleFileOptions;

    fn make_zip(path: &Path) {
        let f = File::create(path).unwrap();
        let mut w = zip::ZipWriter::new(f);
        let opt = SimpleFileOptions::default();
        for name in ["003.jpg","001.jpg","002.jpg","readme.txt"] {
            w.start_file(name, opt).unwrap();
            w.write_all(name.as_bytes()).unwrap();
        }
        w.finish().unwrap();
    }

    #[test]
    fn lists_sorted_image_pages_only() {
        let tmp = tempfile::tempdir().unwrap();
        let zp = tmp.path().join("c.zip");
        make_zip(&zp);
        let pages = list_pages(&zp).unwrap();
        assert_eq!(pages, vec!["001.jpg","002.jpg","003.jpg"]);
    }

    #[test]
    fn reads_entry_bytes() {
        let tmp = tempfile::tempdir().unwrap();
        let zp = tmp.path().join("c.zip");
        make_zip(&zp);
        let bytes = read_entry(&zp, "002.jpg").unwrap();
        assert_eq!(bytes, b"002.jpg");
    }
}
```

- [ ] **Step 2: 写 command（含首图封面 base64、页字节 base64）**

Create `src-tauri/src/comic/mod.rs`:
```rust
pub mod reader;

use crate::error::{AppError, AppResult};
use base64::Engine;
use std::path::Path;

#[tauri::command]
pub fn comic_pages(path: String) -> AppResult<Vec<String>> {
    reader::list_pages(Path::new(&path))
}

/// 返回指定页的 data URL（base64），供 <img> 直接显示。
#[tauri::command]
pub fn comic_page(path: String, entry: String) -> AppResult<String> {
    let bytes = reader::read_entry(Path::new(&path), &entry)?;
    let mime = if entry.to_lowercase().ends_with(".png") { "image/png" } else { "image/jpeg" };
    let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
    Ok(format!("data:{mime};base64,{b64}"))
}

/// 首图作为封面 data URL。
#[tauri::command]
pub fn comic_cover(path: String) -> AppResult<Option<String>> {
    let pages = reader::list_pages(Path::new(&path))?;
    match pages.first() {
        Some(first) => Ok(Some(comic_page(path, first.clone())?)),
        None => Ok(None),
    }
}

#[allow(dead_code)]
fn _assert(_: AppError) {}
```
Modify `src-tauri/Cargo.toml` `[dependencies]` 加：
```toml
base64 = "0.22"
```
Modify `src-tauri/src/main.rs`：加 `mod comic;`，在 `generate_handler!` 里追加 `comic::comic_pages, comic::comic_page, comic::comic_cover`。

- [ ] **Step 3: 运行测试 + 编译**

Run: `cd src-tauri && cargo test comic:: && cargo build 2>&1 | tail -3`
Expected: PASS，编译通过。

- [ ] **Step 4: 提交**

```bash
git add -A && git commit -m "feat: comic zip reader commands (pages/page/cover)"
```

### Task 2.3：漫画库视图（首图封面）

**Files:**
- Create: `src/views/ComicView.ts`
- Modify: `src/lib/ipc.ts`, `src/main.ts`

- [ ] **Step 1: 扩展 IPC**

Append to `src/lib/ipc.ts` 的 `api`：
```ts
  comicPages: (path: string) => invoke<string[]>("comic_pages", { path }),
  comicPage: (path: string, entry: string) => invoke<string>("comic_page", { path, entry }),
  comicCover: (path: string) => invoke<string | null>("comic_cover", { path }),
```

- [ ] **Step 2: 漫画库视图（异步加载首图封面）**

Create `src/views/ComicView.ts`:
```ts
import { api, MediaItem } from "../lib/ipc";

export async function ComicView(onOpen: (it: MediaItem) => void): Promise<HTMLElement> {
  const el = document.createElement("div");
  el.className = "view-enter";
  const items = await api.listMedia("comic");
  el.innerHTML = `<h1 style="font-size:20px;margin-bottom:16px">漫画</h1><div class="poster-grid"></div>`;
  const grid = el.querySelector(".poster-grid")!;
  items.forEach((it, i) => {
    const card = document.createElement("div");
    card.className = "poster card-hover";
    card.innerHTML = `<div class="poster-img" id="cc-${i}"><div class="poster-ph">加载中…</div></div><div class="poster-title">${it.title}</div>`;
    card.onclick = () => onOpen(it);
    grid.appendChild(card);
    api.comicCover(it.path).then(url => {
      const box = card.querySelector(`#cc-${i}`)!;
      box.innerHTML = url ? `<img src="${url}"/>` : `<div class="poster-ph">${it.title}</div>`;
    }).catch(() => {});
  });
  return el;
}
```

- [ ] **Step 3: 路由接入**

Modify `src/main.ts` `renderRoute` 的 `case "comic"`：
```ts
    case "comic": view = await ComicView((it) => openComicReader(it)); break;
```
先加占位 `function openComicReader(_it:any){}`（下一任务实现）。

- [ ] **Step 4: 手动验证**

造测试漫画：
```bash
mkdir -p /tmp/ctest/热血 && cd /tmp && (mkdir -p _c && cd _c && for n in 001 002 003; do printf '\xff\xd8\xff\xe0test' > $n.jpg; done && zip -q ../ctest/热血/测试卷.zip *.jpg) && rm -rf _c
```
Run `npm run tauri dev`，设置漫画根为 `/tmp/ctest` 并扫描，漫画菜单应显示「测试卷」卡片（封面为首图）。

- [ ] **Step 5: 提交**

```bash
git add -A && git commit -m "feat: comic library view with cover from first page"
```

### Task 2.4：漫画阅读器视图（翻页 + 长条 + 预取 + 记忆页码）

**Files:**
- Create: `src/views/ComicReaderView.ts`
- Modify: `src/main.ts`, `src-tauri/src/main.rs`（加读/写页码 command）

- [ ] **Step 1: 后端页码读写 command**

Modify `src-tauri/src/main.rs`，加两个 command：
```rust
#[tauri::command]
fn set_comic_page(db: tauri::State<Db>, item_id: i64, page: i64) -> AppResult<()> {
    let conn = db.0.lock().unwrap();
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() as i64;
    conn.execute(
        "INSERT INTO watch_state(item_id,comic_page,last_opened_at) VALUES(?1,?2,?3)
         ON CONFLICT(item_id) DO UPDATE SET comic_page=excluded.comic_page, last_opened_at=excluded.last_opened_at",
        rusqlite::params![item_id, page, now],
    ).map_err(|e| error::AppError::Db(e.to_string()))?;
    Ok(())
}

#[tauri::command]
fn get_comic_page(db: tauri::State<Db>, item_id: i64) -> AppResult<i64> {
    let conn = db.0.lock().unwrap();
    conn.query_row("SELECT comic_page FROM watch_state WHERE item_id=?1", rusqlite::params![item_id], |r| r.get(0))
        .or_else(|e| match e { rusqlite::Error::QueryReturnedNoRows => Ok(0), o => Err(error::AppError::Db(o.to_string())) })
}
```
在 `generate_handler!` 追加 `set_comic_page, get_comic_page`。IPC 加：
```ts
  setComicPage: (itemId: number, page: number) => invoke<void>("set_comic_page", { itemId, page }),
  getComicPage: (itemId: number) => invoke<number>("get_comic_page", { itemId }),
```

- [ ] **Step 2: 阅读器视图**

Create `src/views/ComicReaderView.ts`:
```ts
import { api, MediaItem } from "../lib/ipc";

const PREFETCH = 2;

export async function ComicReaderView(it: MediaItem, onExit: () => void): Promise<HTMLElement> {
  const el = document.createElement("div");
  el.className = "comic-reader view-enter";
  const pages = await api.comicPages(it.path);
  let idx = Math.min(await api.getComicPage(it.id), Math.max(0, pages.length - 1));
  const cache = new Map<number, string>();
  let mode: "page" | "strip" = "page";

  const load = async (i: number) => {
    if (i < 0 || i >= pages.length || cache.has(i)) return cache.get(i);
    const url = await api.comicPage(it.path, pages[i]);
    cache.set(i, url);
    return url;
  };
  const prefetch = () => { for (let k = 1; k <= PREFETCH; k++) { load(idx + k); load(idx - k); } };

  const renderPage = async () => {
    const url = await load(idx);
    el.querySelector(".stage")!.innerHTML = `<img class="page-img" src="${url}"/>`;
    el.querySelector(".pager")!.textContent = `${idx + 1} / ${pages.length}`;
    await api.setComicPage(it.id, idx);
    prefetch();
  };

  const renderStrip = async () => {
    const stage = el.querySelector(".stage")!;
    stage.innerHTML = "";
    for (let i = 0; i < pages.length; i++) {
      const img = document.createElement("img");
      img.className = "strip-img";
      img.loading = "lazy";
      load(i).then(u => { if (u) img.src = u; });
      stage.appendChild(img);
    }
  };

  el.innerHTML = `
    <div class="reader-bar glass">
      <button class="back">← 返回</button>
      <span class="title">${it.title}</span>
      <span class="pager"></span>
      <button class="toggle">切换：长条</button>
    </div>
    <div class="stage"></div>`;
  const stage = el.querySelector<HTMLElement>(".stage")!;

  el.querySelector<HTMLButtonElement>(".back")!.onclick = onExit;
  el.querySelector<HTMLButtonElement>(".toggle")!.onclick = () => {
    mode = mode === "page" ? "strip" : "page";
    el.querySelector(".toggle")!.textContent = mode === "page" ? "切换：长条" : "切换：翻页";
    stage.className = "stage " + mode;
    mode === "page" ? renderPage() : renderStrip();
  };

  const key = (e: KeyboardEvent) => {
    if (mode !== "page") return;
    if (e.key === "ArrowRight" || e.key === " ") { if (idx < pages.length - 1) { idx++; renderPage(); } }
    if (e.key === "ArrowLeft") { if (idx > 0) { idx--; renderPage(); } }
  };
  document.addEventListener("keydown", key);
  el.addEventListener("comic-reader-detach", () => document.removeEventListener("keydown", key));

  stage.className = "stage page";
  await renderPage();
  return el;
}
```
（CSS 追加：`.reader-bar{display:flex;align-items:center;gap:14px;padding:10px 14px;margin-bottom:12px} .stage.page{display:flex;justify-content:center} .page-img{max-height:calc(100vh - 120px);border-radius:8px;animation:fadeInUp .2s ease} .stage.strip{display:flex;flex-direction:column;align-items:center;gap:6px} .strip-img{max-width:820px;width:100%}`）

- [ ] **Step 3: main.ts 接入 reader（切换到全屏内容态）**

Modify `src/main.ts`，替换占位 `openComicReader`：
```ts
import { ComicReaderView } from "./views/ComicReaderView";
import { MediaItem } from "./lib/ipc";

async function openComicReader(it: MediaItem) {
  const prev = content.querySelector(".comic-reader");
  prev?.dispatchEvent(new Event("comic-reader-detach"));
  content.innerHTML = "";
  content.appendChild(await ComicReaderView(it, () => renderRoute("comic")));
}
```
并把 `case "comic"` 的回调改为 `openComicReader`。

- [ ] **Step 4: 阶段 2 检查点验证**

Run `npm run tauri dev`：进入漫画库 → 点「测试卷」→ 阅读器打开，方向键翻页，页码显示更新，点「切换：长条」变滚动模式；退出重进应回到上次页码。

Expected: 翻页流畅、模式切换正常、页码记忆生效。

- [ ] **Step 5: 提交**

```bash
git add -A && git commit -m "feat: comic reader with paging, strip mode, prefetch, page memory (phase 2 checkpoint)"
```

---

## 阶段 3：视频播放器（libmpv FFI）

**阶段目标（检查点）：** 点击视频海报，用内嵌 libmpv 子窗口播放 MKV，加载外挂 .ass 字幕，玻璃控制条支持播放/暂停/快进快退/进度/音量/全屏，退出记忆进度。

> **风险提示：** 本阶段是全项目最高技术风险。libmpv 需随包分发；macOS 与 Windows 的动态库加载路径不同。建议先在开发机安装 mpv（`brew install mpv` 提供 libmpv）验证 FFI，再处理打包分发。

### Task 3.1：libmpv FFI 封装

**Files:**
- Create: `src-tauri/src/player/mpv.rs`
- Modify: `src-tauri/Cargo.toml`

- [ ] **Step 1: 加 libmpv 绑定依赖**

Modify `src-tauri/Cargo.toml` `[dependencies]`：
```toml
libmpv2 = "4"
```
（`libmpv2` 是维护中的 mpv Rust 绑定；若目标平台绑定不可用，回退方案为用 `libloading` + 手写 FFI，本任务以 `libmpv2` 为主路径。）

- [ ] **Step 2: 写封装 + 冒烟测试（需本机有 libmpv）**

Create `src-tauri/src/player/mpv.rs`:
```rust
use crate::error::{AppError, AppResult};
use libmpv2::{Mpv, SetData};

pub struct Player {
    mpv: Mpv,
}

impl Player {
    /// wid: 宿主原生窗口句柄（Tauri 子窗口），mpv 渲染到该窗口。
    pub fn new(wid: i64) -> AppResult<Self> {
        let mpv = Mpv::new().map_err(|e| AppError::Other(format!("mpv init: {e:?}")))?;
        mpv.set_property("wid", wid).map_err(|e| AppError::Other(format!("set wid: {e:?}")))?;
        mpv.set_property("hwdec", "auto").ok();
        mpv.set_property("keep-open", "yes").ok();
        Ok(Player { mpv })
    }

    pub fn load(&self, path: &str, sub: Option<&str>) -> AppResult<()> {
        self.mpv.command("loadfile", &[path]).map_err(|e| AppError::Other(format!("{e:?}")))?;
        if let Some(s) = sub {
            self.mpv.command("sub-add", &[s]).map_err(|e| AppError::Other(format!("{e:?}")))?;
        }
        Ok(())
    }
    pub fn set_pause(&self, p: bool) -> AppResult<()> { self.setp("pause", p) }
    pub fn seek(&self, secs: f64) -> AppResult<()> {
        self.mpv.command("seek", &[&secs.to_string(), "relative"]).map_err(|e| AppError::Other(format!("{e:?}")))
    }
    pub fn set_volume(&self, v: f64) -> AppResult<()> { self.setp("volume", v) }
    pub fn position(&self) -> f64 { self.mpv.get_property("time-pos").unwrap_or(0.0) }
    pub fn duration(&self) -> f64 { self.mpv.get_property("duration").unwrap_or(0.0) }
    pub fn seek_absolute(&self, secs: f64) -> AppResult<()> {
        self.mpv.command("seek", &[&secs.to_string(), "absolute"]).map_err(|e| AppError::Other(format!("{e:?}")))
    }
    fn setp<T: SetData>(&self, k: &str, v: T) -> AppResult<()> {
        self.mpv.set_property(k, v).map_err(|e| AppError::Other(format!("{e:?}")))
    }
}
```

- [ ] **Step 3: 编译验证（不跑实际播放的单测，避免 CI 无 libmpv）**

Run: `cd src-tauri && cargo build 2>&1 | tail -5`
Expected: 编译通过（本机需能找到 libmpv；macOS 上 `brew install mpv`）。若链接失败，按平台设置 `MPV_SOURCE`/库搜索路径或改用 libloading 回退。

- [ ] **Step 4: 提交**

```bash
git add -A && git commit -m "feat: libmpv FFI player wrapper"
```

### Task 3.2：播放器状态管理 + command

**Files:**
- Create: `src-tauri/src/player/mod.rs`
- Modify: `src-tauri/src/main.rs`

- [ ] **Step 1: 写 player 模块（全局 Option<Player>）+ command**

Create `src-tauri/src/player/mod.rs`:
```rust
pub mod mpv;

use crate::error::{AppError, AppResult};
use mpv::Player;
use std::sync::Mutex;

#[derive(Default)]
pub struct PlayerState(pub Mutex<Option<Player>>);

fn with<T>(s: &PlayerState, f: impl FnOnce(&Player) -> AppResult<T>) -> AppResult<T> {
    let g = s.0.lock().unwrap();
    let p = g.as_ref().ok_or_else(|| AppError::Invalid("player not initialized".into()))?;
    f(p)
}

#[tauri::command]
pub fn player_init(state: tauri::State<PlayerState>, wid: i64) -> AppResult<()> {
    *state.0.lock().unwrap() = Some(Player::new(wid)?);
    Ok(())
}
#[tauri::command]
pub fn player_load(state: tauri::State<PlayerState>, path: String, subtitle: Option<String>) -> AppResult<()> {
    with(&state, |p| p.load(&path, subtitle.as_deref()))
}
#[tauri::command]
pub fn player_pause(state: tauri::State<PlayerState>, paused: bool) -> AppResult<()> {
    with(&state, |p| p.set_pause(paused))
}
#[tauri::command]
pub fn player_seek(state: tauri::State<PlayerState>, secs: f64) -> AppResult<()> {
    with(&state, |p| p.seek(secs))
}
#[tauri::command]
pub fn player_seek_to(state: tauri::State<PlayerState>, secs: f64) -> AppResult<()> {
    with(&state, |p| p.seek_absolute(secs))
}
#[tauri::command]
pub fn player_volume(state: tauri::State<PlayerState>, vol: f64) -> AppResult<()> {
    with(&state, |p| p.set_volume(vol))
}
#[tauri::command]
pub fn player_progress(state: tauri::State<PlayerState>) -> AppResult<(f64, f64)> {
    with(&state, |p| Ok((p.position(), p.duration())))
}
```
Modify `src-tauri/src/main.rs`：加 `mod player;`；在 `setup` 里 `app.manage(player::PlayerState::default());`；`generate_handler!` 追加所有 `player::*` command。

- [ ] **Step 2: 编译**

Run: `cd src-tauri && cargo build 2>&1 | tail -3`
Expected: 编译通过。

- [ ] **Step 3: 提交**

```bash
git add -A && git commit -m "feat: player state and commands"
```

### Task 3.3：播放器子窗口 + 前端控制条

**Files:**
- Create: `src/views/PlayerView.ts`
- Modify: `src/lib/ipc.ts`, `src/main.ts`, `src-tauri/src/main.rs`（创建播放子窗口的 command）

- [ ] **Step 1: 后端创建/嵌入子窗口并返回原生句柄**

Modify `src-tauri/src/main.rs`，加 command 创建一个用于 mpv 渲染的子窗口并初始化 player（拿窗口原生句柄传给 `player_init`）：
```rust
#[tauri::command]
fn open_player_window(app: tauri::AppHandle, state: tauri::State<player::PlayerState>) -> AppResult<()> {
    use tauri::WebviewWindowBuilder;
    // 复用或新建一个无边框窗口承载 mpv 渲染层
    let win = match app.get_webview_window("mpv") {
        Some(w) => w,
        None => WebviewWindowBuilder::new(&app, "mpv", tauri::WebviewUrl::App("about:blank".into()))
            .title("player").decorations(false).build()
            .map_err(|e| error::AppError::Other(e.to_string()))?,
    };
    // 取原生句柄（各平台不同），转成 i64 wid 给 mpv
    #[cfg(target_os = "macos")]
    let wid = win.ns_view().map_err(|e| error::AppError::Other(e.to_string()))? as i64;
    #[cfg(target_os = "windows")]
    let wid = win.hwnd().map_err(|e| error::AppError::Other(e.to_string()))?.0 as i64;
    *state.0.lock().unwrap() = Some(player::mpv::Player::new(wid)?);
    Ok(())
}
```
在 `generate_handler!` 追加 `open_player_window`。（注：句柄取法与嵌入策略在不同 Tauri/系统版本上有差异，实测后微调；若子窗口嵌入困难，回退为独立播放窗口 + 主窗浮层控制。）

- [ ] **Step 2: IPC 扩展**

Append to `src/lib/ipc.ts`：
```ts
  openPlayerWindow: () => invoke<void>("open_player_window"),
  playerLoad: (path: string, subtitle: string | null) => invoke<void>("player_load", { path, subtitle }),
  playerPause: (paused: boolean) => invoke<void>("player_pause", { paused }),
  playerSeek: (secs: number) => invoke<void>("player_seek", { secs }),
  playerSeekTo: (secs: number) => invoke<void>("player_seek_to", { secs }),
  playerVolume: (vol: number) => invoke<void>("player_volume", { vol }),
  playerProgress: () => invoke<[number, number]>("player_progress"),
```

- [ ] **Step 3: 前端控制条视图**

Create `src/views/PlayerView.ts`:
```ts
import { api, MediaItem } from "../lib/ipc";

export async function PlayerView(it: MediaItem, onExit: () => void): Promise<HTMLElement> {
  const el = document.createElement("div");
  el.className = "player-view view-enter";
  await api.openPlayerWindow();
  await api.playerLoad(it.path, it.subtitle_path);
  let paused = false;

  el.innerHTML = `
    <div class="player-bar glass">
      <button class="back">←</button>
      <button class="rw">⏪ 10s</button>
      <button class="pp">⏸</button>
      <button class="ff">10s ⏩</button>
      <input class="seek" type="range" min="0" max="1000" value="0"/>
      <span class="time">0:00 / 0:00</span>
      <input class="vol" type="range" min="0" max="100" value="100"/>
      <button class="fs">⛶</button>
    </div>`;
  const fmt = (s: number) => `${Math.floor(s/60)}:${String(Math.floor(s%60)).padStart(2,"0")}`;
  const seek = el.querySelector<HTMLInputElement>(".seek")!;
  const time = el.querySelector<HTMLElement>(".time")!;

  el.querySelector<HTMLButtonElement>(".back")!.onclick = () => { cleanup(); onExit(); };
  el.querySelector<HTMLButtonElement>(".pp")!.onclick = async () => {
    paused = !paused; await api.playerPause(paused);
    el.querySelector(".pp")!.textContent = paused ? "▶" : "⏸";
  };
  el.querySelector<HTMLButtonElement>(".rw")!.onclick = () => api.playerSeek(-10);
  el.querySelector<HTMLButtonElement>(".ff")!.onclick = () => api.playerSeek(10);
  el.querySelector<HTMLInputElement>(".vol")!.oninput = (e) =>
    api.playerVolume(Number((e.target as HTMLInputElement).value));
  seek.onchange = async () => {
    const [, dur] = await api.playerProgress();
    api.playerSeekTo((Number(seek.value)/1000)*dur);
  };
  el.querySelector<HTMLButtonElement>(".fs")!.onclick = () => document.documentElement.requestFullscreen?.();

  const timer = setInterval(async () => {
    try {
      const [pos, dur] = await api.playerProgress();
      if (dur > 0) { seek.value = String((pos/dur)*1000); time.textContent = `${fmt(pos)} / ${fmt(dur)}`; }
    } catch {}
  }, 1000);
  const cleanup = () => clearInterval(timer);
  el.addEventListener("player-detach", cleanup);
  return el;
}
```
（CSS 追加：`.player-bar{position:fixed;left:50%;bottom:24px;transform:translateX(-50%);display:flex;align-items:center;gap:10px;padding:10px 16px;z-index:10} .player-bar .seek{width:280px} .player-bar button{background:transparent;border:none;color:var(--text);cursor:pointer;font-size:15px}`）

- [ ] **Step 4: main.ts 接入**

Modify `src/main.ts` `VideoView` 的 onOpen 回调改为打开播放器：
```ts
import { PlayerView } from "./views/PlayerView";
// VideoView 已接收 onOpen；把 case "video" 改为：
case "video": view = await VideoView(async (it) => {
  const prev = content.querySelector(".player-view");
  prev?.dispatchEvent(new Event("player-detach"));
  content.innerHTML = "";
  content.appendChild(await PlayerView(it, () => renderRoute("video")));
}); break;
```
（相应调整 `VideoView` 签名接收 onOpen 参数——把 Task 1.9 里 `console.log` 那处替换为调用传入的 `onOpen(it)`。）

- [ ] **Step 5: 阶段 3 检查点验证**

准备一个真实 MKV + 同名 .ass 放入视频根目录并扫描。Run `npm run tauri dev`：点海报 → mpv 子窗口播放，字幕显示在下方，控制条可暂停/快进快退/拖进度/调音量/全屏。

Expected: MKV 正常播放、外挂字幕加载、控制条各功能生效。

- [ ] **Step 6: 提交**

```bash
git add -A && git commit -m "feat: video player window and control bar (phase 3 checkpoint)"
```

### Task 3.4：进度记忆（续播）

**Files:**
- Modify: `src-tauri/src/main.rs`, `src/views/PlayerView.ts`, `src/lib/ipc.ts`

- [ ] **Step 1: 后端进度读写 command**

Modify `src-tauri/src/main.rs`：
```rust
#[tauri::command]
fn set_video_pos(db: tauri::State<Db>, item_id: i64, secs: f64) -> AppResult<()> {
    let conn = db.0.lock().unwrap();
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() as i64;
    conn.execute(
        "INSERT INTO watch_state(item_id,position_secs,last_opened_at) VALUES(?1,?2,?3)
         ON CONFLICT(item_id) DO UPDATE SET position_secs=excluded.position_secs, last_opened_at=excluded.last_opened_at",
        rusqlite::params![item_id, secs, now],
    ).map_err(|e| error::AppError::Db(e.to_string()))?;
    Ok(())
}
#[tauri::command]
fn get_video_pos(db: tauri::State<Db>, item_id: i64) -> AppResult<f64> {
    let conn = db.0.lock().unwrap();
    conn.query_row("SELECT position_secs FROM watch_state WHERE item_id=?1", rusqlite::params![item_id], |r| r.get(0))
        .or_else(|e| match e { rusqlite::Error::QueryReturnedNoRows => Ok(0.0), o => Err(error::AppError::Db(o.to_string())) })
}
```
`generate_handler!` 追加；IPC 加 `setVideoPos`/`getVideoPos`。

- [ ] **Step 2: PlayerView 加载时续播 + 定时保存**

Modify `src/views/PlayerView.ts`：load 后取 `getVideoPos(it.id)`，若 >5 秒则 `playerSeekTo(pos)`；把定时器里同时 `api.setVideoPos(it.id, pos)`；`.back` 退出前保存一次。

- [ ] **Step 3: 验证 + 提交**

播放中途退出再进，应从上次位置续播。
```bash
git add -A && git commit -m "feat: video resume playback via saved position"
```

---

## 阶段 4：游戏启动器

**阶段目标（检查点）：** 配置游戏根目录后，读取每个游戏目录的 game.json + cover.jpg，展示卡片，双击按当前平台启动，非本平台条目标注不可用，记录最近启动。

### Task 4.1：游戏扫描（game.json 解析 + 平台过滤）

**Files:**
- Modify: `src-tauri/src/library/scanner.rs`, `src-tauri/src/main.rs`

- [ ] **Step 1: 写 game.json 结构 + 扫描 + 失败测试**

Append to `src-tauri/src/library/scanner.rs`:
```rust
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct GameManifest {
    name: Option<String>,
    description: Option<String>,
    exec_win: Option<String>,
    exec_mac: Option<String>,
    #[allow(dead_code)]
    fullscreen: Option<bool>,
}

/// 扫描游戏根：每个含 game.json 的一级子目录为一条目。
/// 按当前平台取 exec_win/exec_mac 决定 platform_ok 与 exec_path（绝对）。
pub fn scan_games(root: &Path) -> Vec<ScannedItem> {
    let mut items = Vec::new();
    let entries = match std::fs::read_dir(root) { Ok(e) => e, Err(_) => return items };
    for e in entries.filter_map(|e| e.ok()) {
        let dir = e.path();
        if !dir.is_dir() { continue; }
        let manifest_path = dir.join("game.json");
        if !manifest_path.exists() { continue; }
        let text = match std::fs::read_to_string(&manifest_path) { Ok(t) => t, Err(_) => continue };
        let m: GameManifest = match serde_json::from_str(&text) { Ok(m) => m, Err(_) => continue };
        let dir_name = dir.file_name().unwrap().to_string_lossy().into_owned();

        let rel_exec = if cfg!(target_os = "windows") { m.exec_win.clone() } else { m.exec_mac.clone() };
        let (platform_ok, exec_path) = match rel_exec {
            Some(rel) => (true, Some(dir.join(rel).to_string_lossy().into_owned())),
            None => (false, None),
        };
        let cover = dir.join("cover.jpg");
        let cover_path = cover.exists().then(|| cover.to_string_lossy().into_owned());
        let info = dir.join("info.txt");
        let description = m.description.or_else(|| std::fs::read_to_string(&info).ok().map(|s| s.trim().to_string()));

        items.push(ScannedItem {
            kind: MediaKind::Game,
            category: "游戏".into(),
            category_path: dir_name.clone(),
            title: m.name.unwrap_or(dir_name),
            path: dir.to_string_lossy().into_owned(),
            subtitle_path: None, cover_path, description,
            platform_ok, exec_path,
        });
    }
    items
}

#[cfg(test)]
mod game_scan_tests {
    use super::*;
    use std::fs;
    #[test]
    fn scans_game_with_manifest_current_platform() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("空洞骑士");
        fs::create_dir_all(&dir).unwrap();
        let exec_key = if cfg!(target_os = "windows") { "exec_win" } else { "exec_mac" };
        let exec_val = if cfg!(target_os = "windows") { "game.exe" } else { "Game.app" };
        fs::write(dir.join("game.json"),
            format!(r#"{{"name":"空洞骑士","{exec_key}":"{exec_val}","description":"银河恶魔城"}}"#)).unwrap();
        let items = scan_games(tmp.path());
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].title, "空洞骑士");
        assert!(items[0].platform_ok);
        assert!(items[0].exec_path.as_ref().unwrap().ends_with(exec_val));
    }
}
```

- [ ] **Step 2: 接入 scan_root**

Modify `src-tauri/src/main.rs` 的 `scan_root` match 加：
```rust
"game" => library::scanner::scan_games(std::path::Path::new(&root)),
```

- [ ] **Step 3: 运行测试**

Run: `cd src-tauri && cargo test library::scanner::game_scan_tests`
Expected: PASS。

- [ ] **Step 4: 提交**

```bash
git add -A && git commit -m "feat: game scanner with manifest and platform filter"
```

### Task 4.2：启动游戏进程 command

**Files:**
- Create: `src-tauri/src/launcher/mod.rs`
- Modify: `src-tauri/src/main.rs`

- [ ] **Step 1: 写启动逻辑 + command**

Create `src-tauri/src/launcher/mod.rs`:
```rust
use crate::db::Db;
use crate::error::{AppError, AppResult};
use std::path::Path;
use std::process::Command;

#[tauri::command]
pub fn launch_game(db: tauri::State<Db>, item_id: i64, exec_path: String) -> AppResult<()> {
    let p = Path::new(&exec_path);
    if !p.exists() { return Err(AppError::NotFound(exec_path)); }

    #[cfg(target_os = "macos")]
    {
        // .app 用 open，普通可执行直接 spawn
        if exec_path.ends_with(".app") {
            Command::new("open").arg(&exec_path).spawn()?;
        } else {
            Command::new(&exec_path).spawn()?;
        }
    }
    #[cfg(target_os = "windows")]
    {
        let dir = p.parent().unwrap_or(Path::new("."));
        Command::new(&exec_path).current_dir(dir).spawn()?;
    }

    let conn = db.0.lock().unwrap();
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() as i64;
    conn.execute(
        "INSERT INTO game_state(item_id,last_launched_at,launch_count) VALUES(?1,?2,1)
         ON CONFLICT(item_id) DO UPDATE SET last_launched_at=?2, launch_count=launch_count+1",
        rusqlite::params![item_id, now],
    ).map_err(|e| AppError::Db(e.to_string()))?;
    Ok(())
}
```
Modify `src-tauri/src/main.rs`：加 `mod launcher;`；`generate_handler!` 追加 `launcher::launch_game`。

- [ ] **Step 2: 编译**

Run: `cd src-tauri && cargo build 2>&1 | tail -3`
Expected: 编译通过。

- [ ] **Step 3: 提交**

```bash
git add -A && git commit -m "feat: launch game process command"
```

### Task 4.3：游戏库视图（阶段 4 检查点）

**Files:**
- Create: `src/views/GameView.ts`
- Modify: `src/lib/ipc.ts`, `src/main.ts`

- [ ] **Step 1: IPC 扩展**

Append to `src/lib/ipc.ts`：
```ts
  launchGame: (itemId: number, execPath: string) => invoke<void>("launch_game", { itemId, execPath }),
```

- [ ] **Step 2: 游戏库视图**

Create `src/views/GameView.ts`:
```ts
import { convertFileSrc } from "@tauri-apps/api/core";
import { api } from "../lib/ipc";

export async function GameView(): Promise<HTMLElement> {
  const el = document.createElement("div");
  el.className = "view-enter";
  const items = await api.listMedia("game");
  el.innerHTML = `<h1 style="font-size:20px;margin-bottom:16px">游戏</h1>
    <div class="game-grid"></div>`;
  const grid = el.querySelector(".game-grid")!;
  grid.innerHTML = items.map((it, i) => `
    <div class="game-card glass card-hover ${it.platform_ok ? "" : "disabled"}" data-i="${i}">
      <div class="game-cover">${it.cover_path ? `<img src="${convertFileSrc(it.cover_path)}"/>` : ""}</div>
      <div class="game-info">
        <div class="game-title">${it.title}</div>
        <div class="game-desc">${it.description ?? ""}</div>
        ${it.platform_ok ? "" : `<div class="game-na">本平台不可用</div>`}
      </div>
    </div>`).join("");
  grid.querySelectorAll<HTMLElement>(".game-card").forEach(c => {
    const it = items[Number(c.dataset.i)];
    c.ondblclick = () => { if (it.platform_ok && it.exec_path) api.launchGame(it.id, it.exec_path); };
  });
  return el;
}
```
（CSS 追加：`.game-grid{display:grid;grid-template-columns:repeat(auto-fill,minmax(260px,1fr));gap:14px} .game-card{display:flex;gap:12px;padding:12px} .game-cover{width:80px;height:80px;border-radius:8px;overflow:hidden;background:var(--glass);flex-shrink:0} .game-cover img{width:100%;height:100%;object-fit:cover} .game-title{font-size:14px;margin-bottom:4px} .game-desc{font-size:11px;color:var(--text-dim);line-height:1.4} .game-na{color:#ff8a8a;font-size:11px;margin-top:6px} .game-card.disabled{opacity:.5}`）

- [ ] **Step 3: 路由接入**

Modify `src/main.ts` `renderRoute` 的 `case "game"`：`view = await GameView(); break;`

- [ ] **Step 4: 阶段 4 检查点验证**

造测试游戏：`mkdir -p /tmp/gtest/测试游戏 && printf '{"name":"测试游戏","exec_mac":"run.sh","description":"演示"}' > /tmp/gtest/测试游戏/game.json && printf '#!/bin/sh\necho hi' > /tmp/gtest/测试游戏/run.sh && chmod +x /tmp/gtest/测试游戏/run.sh`。设置游戏根为 `/tmp/gtest` 扫描，游戏菜单显示卡片，双击触发启动（run.sh 执行）。

Expected: 卡片正确展示，本平台可用条目双击启动，非本平台标注不可用。

- [ ] **Step 5: 提交**

```bash
git add -A && git commit -m "feat: game library view with launch (phase 4 checkpoint)"
```

---

## 阶段 5：标准化工具台

**阶段目标（检查点）：** 标准化专区能选文件夹、跑字幕标准化（双语合并/样式重建/标点全角化/可配置映射）+ 质检报告，跑漫画标准化（重命名/JPG统一/打zip）。

### Task 5.1：字幕清洗引擎（核心移植）

**Files:**
- Create: `src-tauri/src/normalize/mod.rs`, `src-tauri/src/normalize/subtitle.rs`
- Modify: `src-tauri/src/main.rs`

- [ ] **Step 1: 写引擎骨架 + 分隔符/BOM/CRLF 处理 + 失败测试**

Create `src-tauri/src/normalize/subtitle.rs`:
```rust
use crate::error::AppResult;

pub const SEPARATOR: &str = "\\N{\\fnArial\\fs30}";

/// 一条对白：时间戳区间 + 文本（可能含中英，用 SEPARATOR 分隔）。
#[derive(Debug, Clone)]
pub struct Dialogue {
    pub start: String,
    pub end: String,
    pub text: String,
}

/// 去除 UTF-8 BOM，统一换行为 \n。
pub fn preprocess(raw: &str) -> String {
    raw.trim_start_matches('\u{feff}').replace("\r\n", "\n").replace('\r', "\n")
}

/// 从 .ass 文本解析出所有 Dialogue 行（保留时间与文本主体）。
pub fn parse_dialogues(content: &str) -> Vec<Dialogue> {
    let mut out = Vec::new();
    for line in preprocess(content).lines() {
        if !line.starts_with("Dialogue:") { continue; }
        // Dialogue: Layer,Start,End,Style,Name,ML,MR,MV,Effect,Text
        let rest = &line["Dialogue:".len()..];
        let parts: Vec<&str> = rest.splitn(10, ',').collect();
        if parts.len() < 10 { continue; }
        out.push(Dialogue {
            start: parts[1].trim().to_string(),
            end: parts[2].trim().to_string(),
            text: parts[9].to_string(),
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn strips_bom_and_crlf() {
        let s = preprocess("\u{feff}a\r\nb\r\n");
        assert_eq!(s, "a\nb\n");
    }
    #[test]
    fn parses_dialogue_lines() {
        let ass = "\u{feff}[Events]\r\nDialogue: 0,0:00:02.20,0:00:05.68,Default,,0,0,0,,中文\\N{\\fnArial\\fs30}English\r\n";
        let ds = parse_dialogues(ass);
        assert_eq!(ds.len(), 1);
        assert_eq!(ds[0].start, "0:00:02.20");
        assert!(ds[0].text.contains(SEPARATOR));
    }
}
```

Create `src-tauri/src/normalize/mod.rs`:
```rust
pub mod subtitle;
pub mod subtitle_check;
pub mod comic_pack;
```
Modify `main.rs` 加 `mod normalize;`。为让 subtitle_check/comic_pack 先编译，创建空文件占位。

- [ ] **Step 2: 运行测试**

Run: `cd src-tauri && cargo test normalize::subtitle::tests`
Expected: PASS。

- [ ] **Step 3: 提交**

```bash
git add -A && git commit -m "feat: subtitle parse (bom/crlf/dialogue)"
```

### Task 5.2：双语合并 + 标点全角化 + 样式头重建

**Files:**
- Modify: `src-tauri/src/normalize/subtitle.rs`

- [ ] **Step 1: 写标点全角化 + 失败测试**

Append to `src-tauri/src/normalize/subtitle.rs`:
```rust
/// 中文标点全角化：把中文部分的半角标点转为全角。作用于 SEPARATOR 之前的中文段。
pub fn fullwidth_chinese_punct(chinese: &str) -> String {
    chinese
        .replace(", ", "，").replace(',', "，")
        .replace("! ", "！").replace('!', "！")
        .replace("? ", "？").replace('?', "？")
        .replace(": ", "：").replace(':', "：")
        .replace("...", "…")
        .replace(". ", "。")
}

/// 把一条对白的文本按 SEPARATOR 拆成 (中文, 英文可选)，对中文做全角化后重组。
pub fn normalize_text(text: &str) -> String {
    match text.split_once(SEPARATOR) {
        Some((zh, en)) => format!("{}{}{}", fullwidth_chinese_punct(zh.trim()), SEPARATOR, en.trim()),
        None => fullwidth_chinese_punct(text.trim()),
    }
}

#[cfg(test)]
mod norm_tests {
    use super::*;
    #[test]
    fn fullwidth_punct_on_chinese_side_only() {
        let t = "你好, 世界!\\N{\\fnArial\\fs30}Hello, world!";
        let r = normalize_text(t);
        assert!(r.starts_with("你好，世界！"));
        assert!(r.ends_with("Hello, world!")); // 英文侧不动
    }
}
```

- [ ] **Step 2: 写样式头重建 + 输出组装 + 失败测试**

Append:
```rust
/// 生成统一 ASS 头（样式取自 demo：Default/Title/Note，MarginV 控制字幕在下方）。
pub fn build_header() -> String {
    let mut s = String::new();
    s.push_str("[Script Info]\nPlayResX: 1280\nPlayResY: 720\n\n");
    s.push_str("[V4+ Styles]\n");
    s.push_str("Format: Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, OutlineColour, BackColour, Bold, Italic, Underline, StrikeOut, ScaleX, ScaleY, Spacing, Angle, BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, Encoding\n");
    s.push_str("Style: Default,SimHei,32,&H00FFFFFF,&H0000FFFF,&H00000000,&H80000000,0,0,0,0,100,100,0,0,1,2,2,2,0,0,10,1\n");
    s.push_str("Style: Title,SimHei,35,&H00FFFFFF,&H0000FFFF,&H00000000,&H80000000,0,0,0,0,100,100,0,0,1,2,2,2,0,0,30,1\n");
    s.push_str("Style: Note,SimHei,32,&H00FFFFFF,&H0000FFFF,&H00000000,&H80000000,0,0,0,0,100,100,0,0,1,2,2,2,0,0,40,1\n\n");
    s.push_str("[Events]\n");
    s.push_str("Format: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\n");
    s
}

/// 应用可配置字符映射表（默认空）后，产出标准化后的完整 .ass 文本。
pub fn format_ass(content: &str, char_map: &[(String, String)]) -> String {
    let dialogues = parse_dialogues(content);
    let mut out = build_header();
    for d in dialogues {
        let mut text = normalize_text(&d.text);
        for (from, to) in char_map { text = text.replace(from, to); }
        out.push_str(&format!(
            "Dialogue: 0,{},{},Default,0,0,0,0,,{}\n", d.start, d.end, text));
    }
    out
}

#[cfg(test)]
mod build_tests {
    use super::*;
    #[test]
    fn format_produces_header_and_dialogue() {
        let ass = "[Events]\nDialogue: 0,0:00:01.00,0:00:02.00,Default,,0,0,0,,你好!\\N{\\fnArial\\fs30}Hi!\n";
        let out = format_ass(ass, &[]);
        assert!(out.contains("[V4+ Styles]"));
        assert!(out.contains("Style: Default,SimHei,32"));
        assert!(out.contains("你好！"));
        assert!(out.contains("Dialogue: 0,0:00:01.00,0:00:02.00,Default"));
    }
    #[test]
    fn char_map_applies() {
        let ass = "[Events]\nDialogue: 0,0:00:01.00,0:00:02.00,Default,,0,0,0,,錯字\n";
        let out = format_ass(ass, &[("錯".to_string(), "错".to_string())]);
        assert!(out.contains("错字"));
    }
}
```

- [ ] **Step 3: 运行测试**

Run: `cd src-tauri && cargo test normalize::subtitle`
Expected: 全部 PASS。

- [ ] **Step 4: 提交**

```bash
git add -A && git commit -m "feat: subtitle bilingual normalize, fullwidth punct, header rebuild"
```

### Task 5.3：字幕质检（移植 SubtitlesSearch 核心规则）

**Files:**
- Modify: `src-tauri/src/normalize/subtitle_check.rs`

- [ ] **Step 1: 写质检规则 + 失败测试**

Write `src-tauri/src/normalize/subtitle_check.rs`:
```rust
use crate::normalize::subtitle::{parse_dialogues, SEPARATOR};

#[derive(Debug, Clone, serde::Serialize)]
pub struct Issue {
    pub line: usize,
    pub kind: String,
    pub text: String,
}

/// 对标准化后文本做质检，返回可疑行。移植 SubtitlesSearch 的核心规则子集。
pub fn check(content: &str) -> Vec<Issue> {
    let dialogues = parse_dialogues(content);
    let mut issues = Vec::new();
    for (i, d) in dialogues.iter().enumerate() {
        // 规则1：时间轴逆序（当前 start < 上一条 end 记为异常）
        if i > 0 {
            let prev_end = &dialogues[i - 1].end;
            if d.start.as_str() < prev_end.as_str() {
                issues.push(Issue { line: i + 1, kind: "时间轴逆序".into(), text: d.text.clone() });
            }
        }
        // 规则2：可疑标点组合
        for bad in [".,", ",.", "--", "  ", ".!", ".?", "!.", "?."] {
            if d.text.contains(bad) {
                issues.push(Issue { line: i + 1, kind: format!("可疑标点[{bad}]"), text: d.text.clone() });
            }
        }
        // 规则3：双语结构缺失（含中文但无分隔符与英文）——仅提示
        let has_cjk = d.text.chars().any(|c| ('\u{4e00}'..='\u{9fa5}').contains(&c));
        if has_cjk && !d.text.contains(SEPARATOR) {
            issues.push(Issue { line: i + 1, kind: "缺英文行".into(), text: d.text.clone() });
        }
    }
    issues
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn flags_suspicious_punct() {
        let ass = "[Events]\nDialogue: 0,0:00:01.00,0:00:02.00,Default,,0,0,0,,坏  标点\\N{\\fnArial\\fs30}bad\n";
        let issues = check(ass);
        assert!(issues.iter().any(|i| i.kind.contains("可疑标点")));
    }
    #[test]
    fn flags_time_reversal() {
        let ass = "[Events]\n\
Dialogue: 0,0:00:05.00,0:00:09.00,Default,,0,0,0,,一\\N{\\fnArial\\fs30}one\n\
Dialogue: 0,0:00:03.00,0:00:04.00,Default,,0,0,0,,二\\N{\\fnArial\\fs30}two\n";
        let issues = check(ass);
        assert!(issues.iter().any(|i| i.kind == "时间轴逆序"));
    }
}
```

- [ ] **Step 2: 运行测试**

Run: `cd src-tauri && cargo test normalize::subtitle_check`
Expected: PASS。

- [ ] **Step 3: 提交**

```bash
git add -A && git commit -m "feat: subtitle quality check rules"
```

### Task 5.4：字幕标准化 command（批量目录处理）

**Files:**
- Modify: `src-tauri/src/normalize/mod.rs`, `src-tauri/src/main.rs`

- [ ] **Step 1: 写批处理 command + 失败测试**

Append to `src-tauri/src/normalize/mod.rs`:
```rust
use crate::error::AppResult;
use serde::Serialize;
use std::path::Path;
use walkdir::WalkDir;

#[derive(Debug, Serialize)]
pub struct SubtitleReport {
    pub file: String,
    pub issues: Vec<subtitle_check::Issue>,
}

/// 递归处理目录下所有 .ass：标准化写入 out_dir（保持相对结构），并返回质检报告。
pub fn run_subtitle_normalize(
    in_dir: &Path, out_dir: &Path, char_map: &[(String, String)],
) -> AppResult<Vec<SubtitleReport>> {
    let mut reports = Vec::new();
    for entry in WalkDir::new(in_dir).into_iter().filter_map(|e| e.ok()) {
        let p = entry.path();
        if p.extension().and_then(|s| s.to_str()) != Some("ass") { continue; }
        let raw = std::fs::read_to_string(p)?;
        let formatted = subtitle::format_ass(&raw, char_map);
        let issues = subtitle_check::check(&formatted);
        let rel = p.strip_prefix(in_dir).unwrap_or(p);
        let dest = out_dir.join(rel);
        if let Some(parent) = dest.parent() { std::fs::create_dir_all(parent)?; }
        std::fs::write(&dest, &formatted)?;
        reports.push(SubtitleReport { file: rel.to_string_lossy().into_owned(), issues });
    }
    Ok(reports)
}

#[tauri::command]
pub fn normalize_subtitles(in_dir: String, out_dir: String) -> AppResult<Vec<SubtitleReport>> {
    run_subtitle_normalize(Path::new(&in_dir), Path::new(&out_dir), &[])
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    #[test]
    fn normalizes_dir_and_reports() {
        let tmp = tempfile::tempdir().unwrap();
        let indir = tmp.path().join("in");
        let outdir = tmp.path().join("out");
        fs::create_dir_all(indir.join("剧集")).unwrap();
        fs::write(indir.join("剧集/a.ass"),
            "\u{feff}[Events]\r\nDialogue: 0,0:00:01.00,0:00:02.00,Default,,0,0,0,,你好!\\N{\\fnArial\\fs30}Hi!\r\n").unwrap();
        let reports = run_subtitle_normalize(&indir, &outdir, &[]).unwrap();
        assert_eq!(reports.len(), 1);
        let out = fs::read_to_string(outdir.join("剧集/a.ass")).unwrap();
        assert!(out.contains("你好！"));
        assert!(out.contains("[V4+ Styles]"));
    }
}
```
Modify `main.rs` `generate_handler!` 追加 `normalize::normalize_subtitles`。

- [ ] **Step 2: 运行测试（用真实样本做一次冒烟）**

Run: `cd src-tauri && cargo test normalize::tests::normalizes_dir_and_reports`
Expected: PASS。
额外冒烟：临时测试把 `demo/subtitles/剧集/英剧/神探夏洛克/第1季` 作输入跑一次（可写个 ignore 测试或手动），确认真实 UTF-8+BOM 文件不崩、输出含标准样式头。

- [ ] **Step 3: 提交**

```bash
git add -A && git commit -m "feat: subtitle batch normalize command with report"
```

### Task 5.5：漫画标准化（重命名 + JPG 统一 + 打 zip）

**Files:**
- Modify: `src-tauri/src/normalize/comic_pack.rs`, `src-tauri/src/main.rs`, `src-tauri/Cargo.toml`

- [ ] **Step 1: 加 image 依赖**

Modify `src-tauri/Cargo.toml` `[dependencies]`：
```toml
image = "0.25"
```

- [ ] **Step 2: 写打包逻辑 + 失败测试**

Write `src-tauri/src/normalize/comic_pack.rs`:
```rust
use crate::error::{AppError, AppResult};
use std::fs::File;
use std::io::Write;
use std::path::Path;
use zip::write::SimpleFileOptions;

/// 把一个图片目录标准化：按文件名排序，转 JPG，重命名为 <prefix>_NNN.jpg，打包为 zip。
/// 清理无关文件（Thumbs.db 等）不影响源目录——只读取图片。
pub fn pack_comic_dir(dir: &Path, prefix: &str, out_zip: &Path) -> AppResult<usize> {
    let mut files: Vec<_> = std::fs::read_dir(dir)?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            let l = p.to_string_lossy().to_lowercase();
            l.ends_with(".jpg") || l.ends_with(".jpeg") || l.ends_with(".png") || l.ends_with(".webp") || l.ends_with(".bmp")
        })
        .collect();
    files.sort();
    if files.is_empty() { return Err(AppError::Invalid("no images".into())); }

    let f = File::create(out_zip)?;
    let mut zw = zip::ZipWriter::new(f);
    let opt = SimpleFileOptions::default();
    let mut n = 0;
    for (i, path) in files.iter().enumerate() {
        let img = image::open(path).map_err(|e| AppError::Other(e.to_string()))?;
        let mut buf = std::io::Cursor::new(Vec::new());
        img.to_rgb8().write_to(&mut buf, image::ImageFormat::Jpeg)
            .map_err(|e| AppError::Other(e.to_string()))?;
        let name = format!("{prefix}_{:03}.jpg", i + 1);
        zw.start_file(name, opt).map_err(|e| AppError::Other(e.to_string()))?;
        zw.write_all(buf.get_ref())?;
        n += 1;
    }
    zw.finish().map_err(|e| AppError::Other(e.to_string()))?;
    Ok(n)
}

#[tauri::command]
pub fn normalize_comic(dir: String, prefix: String, out_zip: String) -> AppResult<usize> {
    pack_comic_dir(Path::new(&dir), &prefix, Path::new(&out_zip))
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{RgbImage, Rgb};
    #[test]
    fn packs_images_into_renamed_jpg_zip() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("src");
        std::fs::create_dir_all(&dir).unwrap();
        for name in ["b.png","a.png","Thumbs.db"] {
            if name.ends_with(".png") {
                let img = RgbImage::from_pixel(4, 4, Rgb([10, 20, 30]));
                img.save(dir.join(name)).unwrap();
            } else {
                std::fs::write(dir.join(name), b"junk").unwrap();
            }
        }
        let out = tmp.path().join("out.zip");
        let n = pack_comic_dir(&dir, "海贼王01", &out).unwrap();
        assert_eq!(n, 2); // 仅两张图，Thumbs.db 被忽略
        // 校验 zip 内命名
        let mut ar = zip::ZipArchive::new(File::open(&out).unwrap()).unwrap();
        let names: Vec<String> = (0..ar.len()).map(|i| ar.by_index(i).unwrap().name().to_string()).collect();
        assert!(names.contains(&"海贼王01_001.jpg".to_string()));
        assert!(names.contains(&"海贼王01_002.jpg".to_string()));
    }
}
```
Modify `main.rs` `generate_handler!` 追加 `normalize::comic_pack::normalize_comic`。

- [ ] **Step 3: 运行测试**

Run: `cd src-tauri && cargo test normalize::comic_pack`
Expected: PASS。

- [ ] **Step 4: 提交**

```bash
git add -A && git commit -m "feat: comic normalize (rename/jpg/zip)"
```

### Task 5.6：标准化工具台前端（阶段 5 检查点）

**Files:**
- Create: `src/views/NormalizeView.ts`
- Modify: `src/lib/ipc.ts`, `src/main.ts`

- [ ] **Step 1: IPC 扩展**

Append to `src/lib/ipc.ts`：
```ts
export interface SubIssue { line: number; kind: string; text: string; }
export interface SubReport { file: string; issues: SubIssue[]; }
// api 内追加：
  normalizeSubtitles: (inDir: string, outDir: string) =>
    invoke<SubReport[]>("normalize_subtitles", { inDir, outDir }),
  normalizeComic: (dir: string, prefix: string, outZip: string) =>
    invoke<number>("normalize_comic", { dir, prefix, outZip }),
```

- [ ] **Step 2: 工具台视图（两个子面板：字幕/漫画）**

Create `src/views/NormalizeView.ts`:
```ts
import { open, save } from "@tauri-apps/plugin-dialog";
import { api, SubReport } from "../lib/ipc";

export function NormalizeView(): HTMLElement {
  const el = document.createElement("div");
  el.className = "view-enter";
  el.innerHTML = `
    <h1 style="font-size:20px;margin-bottom:16px">标准化工具台</h1>
    <div class="glass" style="padding:16px;margin-bottom:16px">
      <h3 style="margin-bottom:10px">字幕标准化</h3>
      <div style="display:flex;gap:10px;flex-wrap:wrap;align-items:center">
        <button id="sub-in">选择输入目录</button><span id="sub-in-p" style="color:var(--text-dim)">未选</span>
        <button id="sub-out">选择输出目录</button><span id="sub-out-p" style="color:var(--text-dim)">未选</span>
        <button id="sub-run">开始</button>
      </div>
      <div id="sub-report" style="margin-top:12px"></div>
    </div>
    <div class="glass" style="padding:16px">
      <h3 style="margin-bottom:10px">漫画标准化</h3>
      <div style="display:flex;gap:10px;flex-wrap:wrap;align-items:center">
        <button id="c-dir">选择图片目录</button><span id="c-dir-p" style="color:var(--text-dim)">未选</span>
        <input id="c-prefix" placeholder="命名前缀，如 海贼王01" style="padding:6px"/>
        <button id="c-out">选择输出zip</button><span id="c-out-p" style="color:var(--text-dim)">未选</span>
        <button id="c-run">开始</button>
      </div>
      <div id="c-report" style="margin-top:12px;color:var(--text-dim)"></div>
    </div>`;

  let subIn = "", subOut = "", cDir = "", cOut = "";
  const pick = async (setter: (v: string) => void, spanId: string) => {
    const d = await open({ directory: true });
    if (typeof d === "string") { setter(d); el.querySelector(`#${spanId}`)!.textContent = d; }
  };
  el.querySelector<HTMLButtonElement>("#sub-in")!.onclick = () => pick(v => subIn = v, "sub-in-p");
  el.querySelector<HTMLButtonElement>("#sub-out")!.onclick = () => pick(v => subOut = v, "sub-out-p");
  el.querySelector<HTMLButtonElement>("#c-dir")!.onclick = () => pick(v => cDir = v, "c-dir-p");
  el.querySelector<HTMLButtonElement>("#c-out")!.onclick = async () => {
    const f = await save({ filters: [{ name: "zip", extensions: ["zip"] }] });
    if (f) { cOut = f; el.querySelector("#c-out-p")!.textContent = f; }
  };

  el.querySelector<HTMLButtonElement>("#sub-run")!.onclick = async () => {
    if (!subIn || !subOut) return alert("请选择输入/输出目录");
    const reports: SubReport[] = await api.normalizeSubtitles(subIn, subOut);
    const total = reports.reduce((a, r) => a + r.issues.length, 0);
    el.querySelector("#sub-report")!.innerHTML =
      `<div style="margin-bottom:8px">处理 ${reports.length} 个文件，质检问题 ${total} 条</div>` +
      reports.filter(r => r.issues.length).map(r =>
        `<div class="glass" style="padding:8px;margin-bottom:6px"><b>${r.file}</b>` +
        r.issues.map(i => `<div style="color:#ffb08a;font-size:12px">L${i.line} [${i.kind}] ${i.text}</div>`).join("") +
        `</div>`).join("");
  };
  el.querySelector<HTMLButtonElement>("#c-run")!.onclick = async () => {
    const prefix = (el.querySelector("#c-prefix") as HTMLInputElement).value.trim();
    if (!cDir || !cOut || !prefix) return alert("请选择目录、前缀和输出zip");
    const n = await api.normalizeComic(cDir, prefix, cOut);
    el.querySelector("#c-report")!.textContent = `完成：${n} 页已打包`;
  };
  return el;
}
```
（需 `npm i @tauri-apps/plugin-dialog` 已在阶段1完成；`save` 同来自该插件。）

- [ ] **Step 3: 路由接入**

Modify `src/main.ts` `renderRoute` 的 `case "normalize"`：`view = NormalizeView(); break;`（同步返回，去掉 await 或包一层）。

- [ ] **Step 4: 阶段 5 检查点验证**

Run `npm run tauri dev`：
1. 字幕：输入选 `demo/subtitles/剧集/英剧/神探夏洛克/第1季`，输出选临时目录，开始 → 显示处理文件数与质检问题；打开输出 .ass 确认含标准样式头、中文标点全角。
2. 漫画：图片目录选任意含 jpg 的目录，填前缀，输出 zip，开始 → 提示完成页数，用漫画阅读器打开该 zip 验证。

Expected: 两条标准化流程都能跑通并给出反馈。

- [ ] **Step 5: 提交**

```bash
git add -A && git commit -m "feat: normalize workbench UI (phase 5 checkpoint)"
```

---

## 阶段 6：打磨与跨平台打包

**阶段目标（检查点）：** 动画特效完善，libmpv 随包分发，macOS 与 Windows 均能构建并运行。

### Task 6.1：视图切换与卡片动画完善

**Files:**
- Modify: `src/styles/animations.css`, 各 view

- [ ] **Step 1: 增强动画**

Append to `src/styles/animations.css`:
```css
@keyframes glow { 0%,100%{box-shadow:0 0 0 rgba(91,140,255,0);} 50%{box-shadow:0 0 20px var(--accent-glow);} }
.nav-item.active { animation: glow 2.4s ease-in-out infinite; }
.tab { transition: background .2s, color .2s; }
.poster-img { transition: box-shadow .2s; }
.poster:hover .poster-img { box-shadow: 0 6px 20px var(--accent-glow); }
```
（保持视频播放区无干扰动画——不给 `.player-view .stage` 加动画。）

- [ ] **Step 2: 手动验证 + 提交**

Run `npm run tauri dev` 观感检查。
```bash
git add -A && git commit -m "polish: view and card animations"
```

### Task 6.2：libmpv 打包分发配置

**Files:**
- Modify: `src-tauri/tauri.conf.json`, 新增 `src-tauri/resources/` 说明

- [ ] **Step 1: 配置资源打包与库加载**

Modify `src-tauri/tauri.conf.json` 的 `bundle.resources`，把对应平台的 libmpv 动态库（macOS `.dylib`，Windows `mpv-2.dll` / `libmpv-2.dll`）纳入分发；在应用启动时设置库搜索路径指向资源目录（若用 libloading，则 `Library::new` 指向资源目录内的库文件）。

macOS 说明：开发期用 `brew install mpv` 提供 `/opt/homebrew/lib/libmpv*.dylib`；分发时拷入 `resources/` 并用 `install_name_tool`/`@rpath` 处理。
Windows 说明：从 mpv 官方 build 取 `libmpv-2.dll` 放入 `resources/`。

- [ ] **Step 2: 分平台构建验证**

macOS Run: `npm run tauri build 2>&1 | tail -10`
Expected: 生成 `.app`/`.dmg`，双击运行播放正常。
Windows（在 Windows 环境）Run: `npm run tauri build`，生成 `.msi`/`.exe`，运行播放正常。

- [ ] **Step 3: 提交**

```bash
git add -A && git commit -m "build: bundle libmpv for macOS and Windows"
```

### Task 6.3：README 与最终检查

**Files:**
- Create: `README.md`

- [ ] **Step 1: 写 README（构建/运行/目录约定说明）**

Create `README.md`，包含：项目简介、开发依赖（Node、Rust、mpv）、`npm run tauri dev`/`build`、三个素材根目录的约定（引用设计文档第 4 节）、game.json 字段说明。

- [ ] **Step 2: 全量测试 + 提交**

Run: `cd src-tauri && cargo test` 全绿；`npm run tauri build` 通过。
```bash
git add -A && git commit -m "docs: add README with build and directory conventions"
```

---

## 自查（Self-Review）

**1. Spec 覆盖：**
- 视频 MKV 播放 + libmpv 子窗口 → 阶段 3 ✓
- 外挂 .ass 字幕加载 + 下方位置（MarginV）→ Task 3.3 + Task 5.2 样式头 ✓
- 字幕标准化（双语合并/样式/全角/可配置映射）→ Task 5.1-5.2 ✓
- 字幕质检（SubtitlesSearch）→ Task 5.3 ✓
- 漫画标准化（重命名/JPG/打zip）→ Task 5.5 ✓
- 漫画阅读（翻页+长条+预取+记忆）→ 阶段 2 ✓
- 游戏启动（game.json + 平台过滤 + 双击启动）→ 阶段 4 ✓
- 纯本地元数据 + SQLite 索引 → Task 1.3-1.6 ✓
- 侧载约定（poster/cover/info/game.json）→ 各 scanner ✓
- 玻璃拟态 + 左侧边栏 + 动画 → Task 1.8, 6.1 ✓
- 跨平台打包 → Task 6.2 ✓
- 独立批处理专区 → 阶段 5 前端 ✓

**2. 占位符扫描：** 无 TODO/TBD；libmpv 句柄取法与打包处标注了"实测微调/回退方案"，属真实工程不确定性而非占位，已给明确回退路径。

**3. 类型一致性：** `MediaItem`/`MediaKind`/`ScannedItem` 字段贯穿各 scanner 与 db；IPC 的 `api.*` 方法名与后端 command 名一一对应（`snake_case` command ↔ camelCase invoke key，Tauri 自动转换参数名需注意：Rust 参数 `item_id` ↔ JS `itemId`，已在 IPC 封装中用 camelCase）。

自查通过。
