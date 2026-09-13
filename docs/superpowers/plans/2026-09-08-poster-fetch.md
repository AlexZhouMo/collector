# 自动抓取海报 实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 设置页手动触发，后端用 TMDB 为所有空封面视频抓取真实海报、统一压成 500×750 JPEG 存入 covers/ 并回填 cover_path，剧集按季共用封面，抓不到的返回失败清单。

**Architecture:** 新增 `src-tauri/src/poster/` 模块（parse 纯解析 / tmdb 网络 / image_proc 图片处理 / mod 编排）。后端命令 `fetch_posters` 后台线程执行 + `poster-progress` 事件推进度；`set_tmdb_key`/`get_tmdb_key` 存 settings。前端设置页加「海报」卡片。

**Tech Stack:** Rust（rusqlite, image 0.25, 新增 ureq 2）+ Tauri 2（AppHandle/Emitter 事件）+ 前端 vanilla-ts。

**设计文档:** `docs/superpowers/specs/2026-09-08-poster-fetch-design.md`

**关键既有形态（供实现参考，勿改）:**
- `settings::set(&db, key, value)` / `settings::get(&db, key) -> AppResult<Option<String>>`（`src-tauri/src/settings.rs`）
- `library::list_items(&db, MediaKind::Video) -> AppResult<Vec<MediaItem>>`（`src-tauri/src/library/mod.rs:106`）
- `MediaItem { id:i64, kind, category:String, category_path:String, title:String, path:String, subtitle_path, cover_path:Option<String>, description, platform_ok, exec_path }`（`src-tauri/src/library/model.rs:33`）
- `Db(pub Mutex<Connection>)`，取连接 `db.0.lock().unwrap()`
- `AppError`/`AppResult` in `src-tauri/src/error.rs`（变体：`Db(String)`、`Invalid(String)`、`Other(String)` — 已在代码中使用）
- lib.rs：`use tauri::Manager;`、`app.path().app_data_dir()`、`app.manage(...)`、`tauri::generate_handler![...]`
- covers 目录：`<app_data>/covers`（见 `import_cover`，lib.rs:213）

---

### Task 1: 新增依赖 ureq + 建立 poster 模块骨架

**Files:**
- Modify: `src-tauri/Cargo.toml`
- Create: `src-tauri/src/poster/mod.rs`
- Modify: `src-tauri/src/lib.rs`（顶部加 `mod poster;`）

- [ ] **Step 1: 加依赖**

在 `src-tauri/Cargo.toml` 的 `[dependencies]` 段（`tiny_http = "0.12"` 下一行）加：

```toml
ureq = { version = "2", features = ["tls"] }
urlencoding = "2"
```

- [ ] **Step 2: 建 poster 模块骨架**

创建 `src-tauri/src/poster/mod.rs`：

```rust
//! 海报自动抓取：TMDB 搜索 → 下载 → 统一压缩 → 回填 cover_path。
pub mod parse;
pub mod tmdb;
pub mod image_proc;
```

- [ ] **Step 3: 注册模块**

在 `src-tauri/src/lib.rs` 顶部模块声明区（`mod player;` 附近）加一行：

```rust
mod poster;
```

- [ ] **Step 4: 建三个子文件占位（避免 mod 引用悬空）**

创建空文件 `src-tauri/src/poster/parse.rs`、`src-tauri/src/poster/tmdb.rs`、`src-tauri/src/poster/image_proc.rs`，各写一行注释：

`parse.rs`:
```rust
//! 从目录结构/标题解析 TMDB 搜索元数据（纯函数）。
```
`tmdb.rs`:
```rust
//! TMDB API 客户端（api.tmdb.org + image.tmdb.org），ureq 同步请求。
```
`image_proc.rs`:
```rust
//! 海报图片处理：解码→居中裁剪缩放 500×750→JPEG q85 编码/存盘。
```

- [ ] **Step 5: 编译验证**

Run: `cd src-tauri && cargo build 2>&1 | tail -15`
Expected: 编译成功（ureq/urlencoding 拉取并通过；poster 模块空但合法）。

- [ ] **Step 6: Commit**

```bash
git add src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/src/poster src-tauri/src/lib.rs
git commit -m "feat(poster): 新增 ureq 依赖与 poster 模块骨架"
```

---

### Task 2: poster/parse.rs — 目录结构解析为搜索元数据

**Files:**
- Modify: `src-tauri/src/poster/parse.rs`

- [ ] **Step 1: 写实现 + 失败测试一起（TDD：先写测试块，运行确认失败，再补实现）**

先只写测试，确认编译失败（函数未定义）。在 `src-tauri/src/poster/parse.rs` 追加：

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn movie_takes_last_segment() {
        let q = parse_query("电影", "电影/科幻/星球大战", "星球大战.国语");
        assert_eq!(q.name, "星球大战");
        assert!(matches!(q.kind, MediaKind::Movie));
        assert_eq!(q.season, None);
    }

    #[test]
    fn tv_normal_season() {
        let q = parse_query("剧集", "剧集/英剧/神探夏洛克/第2季", "S02E01");
        assert_eq!(q.name, "神探夏洛克");
        assert!(matches!(q.kind, MediaKind::Tv));
        assert_eq!(q.season, Some(2));
    }

    #[test]
    fn tv_zero_padded_season() {
        let q = parse_query("剧集", "剧集/美剧/行尸走肉/第07季", "x");
        assert_eq!(q.name, "行尸走肉");
        assert_eq!(q.season, Some(7));
    }

    #[test]
    fn tv_no_season_layer() {
        let q = parse_query("剧集", "剧集/美剧/权力的游戏", "x");
        assert_eq!(q.name, "权力的游戏");
        assert!(matches!(q.kind, MediaKind::Tv));
        assert_eq!(q.season, None);
    }

    #[test]
    fn anime_is_tv() {
        let q = parse_query("动漫", "动漫/热血/海贼王", "x");
        assert_eq!(q.name, "海贼王");
        assert!(matches!(q.kind, MediaKind::Tv));
        assert_eq!(q.season, None);
    }
}
```

- [ ] **Step 2: 运行确认失败**

Run: `cd src-tauri && cargo test poster::parse 2>&1 | tail -15`
Expected: 编译错误（`parse_query`/`MediaKind`/`MediaQuery` 未定义）。

- [ ] **Step 3: 写实现**

在 `src-tauri/src/poster/parse.rs` 文件顶部注释之后、测试块之前插入：

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaKind {
    Movie,
    Tv,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediaQuery {
    pub name: String,
    pub kind: MediaKind,
    pub season: Option<u32>,
}

/// 解析「第N季」「第0N季」中的季号；非季目录返回 None。
fn parse_season(seg: &str) -> Option<u32> {
    let s = seg.strip_prefix('第')?.strip_suffix('季')?;
    s.trim_start_matches('0').parse::<u32>().ok().or_else(|| {
        // 处理 "第0季" 全零的边界（罕见）：解析原串
        s.parse::<u32>().ok()
    })
}

/// 从 category / category_path / title 解析出 TMDB 搜索元数据。
/// - 电影：取 category_path 末段作片名，Movie。
/// - 动漫：取 category_path 末段作名，Tv（动漫按剧搜命中率更高）。
/// - 剧集：若末段是「第N季」，剧名取倒数第二段、season=N；否则末段为剧名、season=None。
pub fn parse_query(category: &str, category_path: &str, title: &str) -> MediaQuery {
    let segs: Vec<&str> = category_path.split('/').filter(|s| !s.is_empty()).collect();
    let last = segs.last().copied().unwrap_or(title);

    match category {
        "剧集" => {
            if let Some(season) = parse_season(last) {
                // 倒数第二段为剧名
                let name = segs
                    .get(segs.len().saturating_sub(2))
                    .copied()
                    .unwrap_or(last)
                    .to_string();
                MediaQuery { name, kind: MediaKind::Tv, season: Some(season) }
            } else {
                MediaQuery { name: last.to_string(), kind: MediaKind::Tv, season: None }
            }
        }
        "动漫" => MediaQuery { name: last.to_string(), kind: MediaKind::Tv, season: None },
        // 电影及兜底
        _ => MediaQuery { name: last.to_string(), kind: MediaKind::Movie, season: None },
    }
}
```

- [ ] **Step 4: 运行确认通过**

Run: `cd src-tauri && cargo test poster::parse 2>&1 | tail -15`
Expected: 5 个测试全部 PASS。

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/poster/parse.rs
git commit -m "feat(poster): parse_query 从目录结构解析搜索名/类型/季号"
```

---

### Task 3: poster/image_proc.rs — 图片统一 500×750 JPEG + 存盘

**Files:**
- Modify: `src-tauri/src/poster/image_proc.rs`

- [ ] **Step 1: 写失败测试**

在 `src-tauri/src/poster/image_proc.rs` 追加：

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use image::{ImageBuffer, Rgb};

    fn png_bytes(w: u32, h: u32) -> Vec<u8> {
        let img = ImageBuffer::from_fn(w, h, |x, _| Rgb([(x % 256) as u8, 100, 150]));
        let mut buf = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgb8(img)
            .write_to(&mut buf, image::ImageFormat::Png)
            .unwrap();
        buf.into_inner()
    }

    #[test]
    fn to_cover_outputs_500x750_jpeg() {
        let out = to_cover(&png_bytes(1000, 1000)).unwrap();
        let decoded = image::load_from_memory(&out).unwrap();
        assert_eq!(decoded.width(), 500);
        assert_eq!(decoded.height(), 750);
        // JPEG 魔数 FF D8
        assert_eq!(&out[0..2], &[0xFF, 0xD8]);
    }

    #[test]
    fn to_cover_wide_image_center_cropped() {
        let out = to_cover(&png_bytes(1000, 400)).unwrap();
        let decoded = image::load_from_memory(&out).unwrap();
        assert_eq!(decoded.width(), 500);
        assert_eq!(decoded.height(), 750);
    }

    #[test]
    fn save_cover_writes_stable_named_file() {
        let tmp = tempfile::tempdir().unwrap();
        let bytes = to_cover(&png_bytes(600, 900)).unwrap();
        let p1 = save_cover(tmp.path(), &bytes).unwrap();
        let p2 = save_cover(tmp.path(), &bytes).unwrap();
        assert_eq!(p1, p2, "相同内容应得同名");
        assert!(p1.contains("tmdb_"));
        assert!(p1.ends_with(".jpg"));
        assert!(std::path::Path::new(&p1).is_file());
    }
}
```

- [ ] **Step 2: 运行确认失败**

Run: `cd src-tauri && cargo test poster::image_proc 2>&1 | tail -15`
Expected: 编译错误（`to_cover`/`save_cover` 未定义）。

- [ ] **Step 3: 写实现**

在 `src-tauri/src/poster/image_proc.rs` 顶部注释后插入：

```rust
use crate::error::{AppError, AppResult};
use image::imageops::FilterType;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::io::Cursor;
use std::path::Path;

const W: u32 = 500;
const H: u32 = 750;
const JPEG_Q: u8 = 85;

/// 解码任意图片字节 → 居中裁剪到 2:3 → 缩放到 500×750 → 编码 JPEG q85。
pub fn to_cover(bytes: &[u8]) -> AppResult<Vec<u8>> {
    let img = image::load_from_memory(bytes)
        .map_err(|e| AppError::Other(format!("decode image: {e}")))?;
    // 居中裁剪到目标宽高比 W:H，再缩放，避免拉伸变形。
    let (iw, ih) = (img.width(), img.height());
    let target_ratio = W as f32 / H as f32;
    let src_ratio = iw as f32 / ih as f32;
    let (cw, ch) = if src_ratio > target_ratio {
        // 源更宽：按高定裁宽
        ((ih as f32 * target_ratio).round() as u32, ih)
    } else {
        // 源更高：按宽定裁高
        (iw, (iw as f32 / target_ratio).round() as u32)
    };
    let x = (iw - cw) / 2;
    let y = (ih - ch) / 2;
    let cropped = img.crop_imm(x, y, cw.max(1), ch.max(1));
    let resized = cropped.resize_exact(W, H, FilterType::Lanczos3);
    let mut out = Cursor::new(Vec::new());
    let rgb = resized.to_rgb8();
    let mut enc = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, JPEG_Q);
    enc.encode(rgb.as_raw(), W, H, image::ExtendedColorType::Rgb8)
        .map_err(|e| AppError::Other(format!("encode jpeg: {e}")))?;
    Ok(out.into_inner())
}

/// 把处理后的封面字节存入 covers_dir，内容 hash 命名 tmdb_<hash>.jpg，返回绝对路径。
pub fn save_cover(covers_dir: &Path, bytes: &[u8]) -> AppResult<String> {
    std::fs::create_dir_all(covers_dir).ok();
    let mut h = DefaultHasher::new();
    bytes.hash(&mut h);
    let dest = covers_dir.join(format!("tmdb_{:016x}.jpg", h.finish()));
    std::fs::write(&dest, bytes).map_err(|e| AppError::Other(format!("write cover: {e}")))?;
    Ok(dest.to_string_lossy().into_owned())
}
```

- [ ] **Step 4: 运行确认通过**

Run: `cd src-tauri && cargo test poster::image_proc 2>&1 | tail -15`
Expected: 3 个测试全部 PASS。

（注：`image` crate 0.25 的 `ExtendedColorType`/`JpegEncoder::encode` API 若签名不符导致编译错，改用 `resized.write_to(&mut out, image::ImageFormat::Jpeg)`——但优先用带质量参数的 JpegEncoder；若报错，实现者调整为能编译通过的等价写法，保持 q85 目标。）

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/poster/image_proc.rs
git commit -m "feat(poster): 图片统一居中裁剪缩放 500x750 JPEG q85 + hash 存盘"
```

---

### Task 4: poster/tmdb.rs — TMDB 搜索/季海报/下载

**Files:**
- Modify: `src-tauri/src/poster/tmdb.rs`

**说明:** 网络层不写自动化单测（依赖外网+Key）。只保证编译通过 + 真机验证。

- [ ] **Step 1: 写实现**

在 `src-tauri/src/poster/tmdb.rs` 顶部注释后写：

```rust
use crate::error::{AppError, AppResult};
use crate::poster::parse::MediaKind;
use std::time::Duration;

const API_BASE: &str = "https://api.tmdb.org/3"; // 主域名被墙，用官方备用域名
const IMG_BASE: &str = "https://image.tmdb.org/t/p/w500";
const UA: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) collector/1.0";

#[derive(Debug, Clone)]
pub struct TmdbHit {
    pub id: u64,
    pub poster_path: Option<String>,
}

fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(10))
        .user_agent(UA)
        .build()
}

/// 搜索电影或剧集，取第一条命中。找不到返回 Ok(None)。
pub fn search(name: &str, kind: MediaKind, api_key: &str) -> AppResult<Option<TmdbHit>> {
    let endpoint = match kind {
        MediaKind::Movie => "search/movie",
        MediaKind::Tv => "search/tv",
    };
    let url = format!(
        "{API_BASE}/{endpoint}?api_key={key}&language=zh-CN&query={q}",
        key = api_key,
        q = urlencoding::encode(name),
    );
    let resp = agent()
        .get(&url)
        .call()
        .map_err(|e| AppError::Other(format!("tmdb search: {e}")))?;
    let json: serde_json::Value = resp
        .into_json()
        .map_err(|e| AppError::Other(format!("tmdb json: {e}")))?;
    let first = json["results"].as_array().and_then(|a| a.first());
    Ok(first.map(|r| TmdbHit {
        id: r["id"].as_u64().unwrap_or(0),
        poster_path: r["poster_path"].as_str().map(|s| s.to_string()),
    }))
}

/// 取剧集某季的海报路径。无海报/无该季返回 Ok(None)。
pub fn season_poster(tv_id: u64, season: u32, api_key: &str) -> AppResult<Option<String>> {
    let url = format!(
        "{API_BASE}/tv/{tv_id}/season/{season}?api_key={api_key}&language=zh-CN"
    );
    let resp = match agent().get(&url).call() {
        Ok(r) => r,
        // 该季不存在等 → 视为无季海报，回退整剧
        Err(_) => return Ok(None),
    };
    let json: serde_json::Value = resp
        .into_json()
        .map_err(|e| AppError::Other(format!("tmdb season json: {e}")))?;
    Ok(json["poster_path"].as_str().map(|s| s.to_string()))
}

/// 从 image.tmdb.org 下载 w500 海报字节。
pub fn download(poster_path: &str) -> AppResult<Vec<u8>> {
    let url = format!("{IMG_BASE}{poster_path}");
    let resp = agent()
        .get(&url)
        .call()
        .map_err(|e| AppError::Other(format!("tmdb download: {e}")))?;
    let mut buf = Vec::new();
    use std::io::Read;
    resp.into_reader()
        .read_to_end(&mut buf)
        .map_err(|e| AppError::Other(format!("read poster: {e}")))?;
    Ok(buf)
}
```

- [ ] **Step 2: 编译验证**

Run: `cd src-tauri && cargo build 2>&1 | tail -15`
Expected: 编译成功。若 `ureq` API（`AgentBuilder`/`into_json`/`into_reader`）签名与 2.x 实际不符，实现者按 ureq 2.x 文档调整为等价写法（保持超时/UA/取 json/读字节的行为）。

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/poster/tmdb.rs
git commit -m "feat(poster): TMDB 客户端 search/season_poster/download（api.tmdb.org）"
```

---

### Task 5: poster/mod.rs — 编排（分组去重 + 回填 + 失败清单）

**Files:**
- Modify: `src-tauri/src/poster/mod.rs`

**设计要点:** 为可测，把「搜索+下载得到封面字节」抽象为闭包 `fetch_cover: Fn(&MediaQuery) -> Result<Vec<u8>, String>`，`fetch_posters` 只做分组/回填/进度/失败收集，单测传假闭包不触网。真实闭包在 lib.rs 命令里组合 tmdb + image_proc 传入。

- [ ] **Step 1: 写失败测试**

替换 `src-tauri/src/poster/mod.rs` 为（保留 pub mod 声明，追加类型+函数+测试）：

```rust
//! 海报自动抓取：TMDB 搜索 → 下载 → 统一压缩 → 回填 cover_path。
pub mod parse;
pub mod tmdb;
pub mod image_proc;

use crate::db::Db;
use crate::error::AppResult;
use crate::library::model::MediaItem;
use parse::{parse_query, MediaKind, MediaQuery};
use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize)]
pub struct FailedItem {
    pub title: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct FetchReport {
    pub ok: usize,
    pub failed: Vec<FailedItem>,
}

/// 分组 key：剧集用「剧名|季」，电影/动漫用「片名|」，使同剧同季合并为一组。
fn group_key(q: &MediaQuery) -> String {
    match q.season {
        Some(s) => format!("{}|{}", q.name, s),
        None => format!("{}|", q.name),
    }
}

/// 只保留 cover_path 为空的视频。
fn needs_cover(it: &MediaItem) -> bool {
    it.cover_path.as_deref().map(|s| s.trim().is_empty()).unwrap_or(true)
}

/// 编排：对所有空封面视频按剧+季分组，每组抓一次封面，组内回填同一 cover_path。
/// `fetch_cover` 返回处理好的封面绝对路径（已下载+压缩+存盘），失败返回 Err(reason)。
/// `progress(done, total, title)` 每处理完一个视频回调一次。
pub fn fetch_posters<FC, PG>(
    db: &Db,
    items: &[MediaItem],
    mut fetch_cover: FC,
    mut progress: PG,
) -> AppResult<FetchReport>
where
    FC: FnMut(&MediaQuery) -> Result<String, String>,
    PG: FnMut(usize, usize, &str),
{
    // 先筛空封面
    let targets: Vec<&MediaItem> = items.iter().filter(|i| needs_cover(i)).collect();
    let total = targets.len();

    // 按 group_key 分组（保留顺序稳定用 BTreeMap by key）
    let mut groups: BTreeMap<String, (MediaQuery, Vec<&MediaItem>)> = BTreeMap::new();
    for it in &targets {
        let q = parse_query(&it.category, &it.category_path, &it.title);
        groups.entry(group_key(&q)).or_insert_with(|| (q.clone(), Vec::new())).1.push(it);
    }

    let mut ok = 0usize;
    let mut failed = Vec::new();
    let mut done = 0usize;

    for (_key, (q, members)) in groups {
        match fetch_cover(&q) {
            Ok(cover_path) => {
                for it in &members {
                    update_cover_path(db, it.id, &cover_path)?;
                    ok += 1;
                    done += 1;
                    progress(done, total, &it.title);
                }
            }
            Err(reason) => {
                for it in &members {
                    failed.push(FailedItem { title: it.title.clone(), reason: reason.clone() });
                    done += 1;
                    progress(done, total, &it.title);
                }
            }
        }
    }

    Ok(FetchReport { ok, failed })
}

/// 回填单个视频的 cover_path。
fn update_cover_path(db: &Db, id: i64, cover_path: &str) -> AppResult<()> {
    let conn = db.0.lock().unwrap();
    conn.execute(
        "UPDATE media_item SET cover_path=?1 WHERE id=?2",
        rusqlite::params![cover_path, id],
    )
    .map_err(|e| crate::error::AppError::Db(e.to_string()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library::model::MediaKind as LibKind;

    fn mk(id: i64, cat: &str, cpath: &str, title: &str, cover: Option<&str>) -> MediaItem {
        MediaItem {
            id,
            kind: LibKind::Video,
            category: cat.into(),
            category_path: cpath.into(),
            title: title.into(),
            path: format!("/p/{id}"),
            subtitle_path: None,
            cover_path: cover.map(|s| s.to_string()),
            description: None,
            platform_ok: true,
            exec_path: None,
        }
    }

    #[test]
    fn groups_same_show_same_season() {
        let db = Db::open_in_memory().unwrap();
        // 先入库这些视频（update 需要行存在）
        {
            let conn = db.0.lock().unwrap();
            for i in 1..=5i64 {
                conn.execute(
                    "INSERT INTO media_item (id,kind,category,category_path,title,path,scanned_at) VALUES (?1,'video','剧集','x','t',?2,0)",
                    rusqlite::params![i, format!("/p/{i}")],
                ).unwrap();
            }
        }
        let items = vec![
            mk(1, "剧集", "剧集/美剧/黑镜/第1季", "黑镜S01E01", None),
            mk(2, "剧集", "剧集/美剧/黑镜/第1季", "黑镜S01E02", None),
            mk(3, "剧集", "剧集/美剧/黑镜/第1季", "黑镜S01E03", None),
            mk(4, "剧集", "剧集/美剧/黑镜/第2季", "黑镜S02E01", None),
            mk(5, "剧集", "剧集/美剧/黑镜/第2季", "黑镜S02E02", None),
        ];
        let mut calls = 0;
        let report = fetch_posters(
            &db,
            &items,
            |_q| { calls += 1; Ok(format!("/covers/c{calls}.jpg")) },
            |_d, _t, _title| {},
        ).unwrap();
        assert_eq!(calls, 2, "两组（第1季/第2季）只应各抓一次");
        assert_eq!(report.ok, 5, "5 个视频全部回填");
        assert!(report.failed.is_empty());
    }

    #[test]
    fn fill_same_cover_for_group_and_skip_existing() {
        let db = Db::open_in_memory().unwrap();
        {
            let conn = db.0.lock().unwrap();
            for i in 1..=3i64 {
                conn.execute(
                    "INSERT INTO media_item (id,kind,category,category_path,title,path,scanned_at) VALUES (?1,'video','电影','x','t',?2,0)",
                    rusqlite::params![i, format!("/p/{i}")],
                ).unwrap();
            }
        }
        let items = vec![
            mk(1, "电影", "电影/科幻/沙丘", "沙丘", None),
            mk(2, "电影", "电影/科幻/沙丘", "沙丘", None), // 同组
            mk(3, "电影", "电影/科幻/降临", "降临", Some("/covers/existing.jpg")), // 已有封面→跳过
        ];
        let report = fetch_posters(
            &db, &items,
            |_q| Ok("/covers/dune.jpg".to_string()),
            |_d, _t, _title| {},
        ).unwrap();
        assert_eq!(report.ok, 2, "沙丘两个视频回填，降临已有封面被跳过");
        // 校验 db 中两个沙丘写了同一路径
        let conn = db.0.lock().unwrap();
        let c1: String = conn.query_row("SELECT cover_path FROM media_item WHERE id=1", [], |r| r.get(0)).unwrap();
        let c2: String = conn.query_row("SELECT cover_path FROM media_item WHERE id=2", [], |r| r.get(0)).unwrap();
        assert_eq!(c1, "/covers/dune.jpg");
        assert_eq!(c2, "/covers/dune.jpg");
    }

    #[test]
    fn failed_group_recorded_not_filled() {
        let db = Db::open_in_memory().unwrap();
        {
            let conn = db.0.lock().unwrap();
            conn.execute("INSERT INTO media_item (id,kind,category,category_path,title,path,scanned_at) VALUES (1,'video','电影','x','冷门片','/p/1',0)", []).unwrap();
        }
        let items = vec![mk(1, "电影", "电影/冷门片", "冷门片", None)];
        let report = fetch_posters(
            &db, &items,
            |_q| Err("搜索无结果".to_string()),
            |_d, _t, _title| {},
        ).unwrap();
        assert_eq!(report.ok, 0);
        assert_eq!(report.failed.len(), 1);
        assert_eq!(report.failed[0].reason, "搜索无结果");
    }
}
```

- [ ] **Step 2: 运行确认失败→通过**

Run: `cd src-tauri && cargo test poster::mod 2>&1 | tail -20`
Expected: 3 个测试全部 PASS（`groups_same_show_same_season`、`fill_same_cover_for_group_and_skip_existing`、`failed_group_recorded_not_filled`）。

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/poster/mod.rs
git commit -m "feat(poster): 编排——按剧+季分组去重、回填同封面、失败清单收集"
```

---

### Task 6: 后端命令 fetch_posters / set_tmdb_key / get_tmdb_key + 注册

**Files:**
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1: 加三个命令**

在 `src-tauri/src/lib.rs` 的 `import_cover` 命令之后（`fn import_cover` 结束的 `}` 之后）插入：

```rust
/// 读/写 TMDB API Key（存 settings 表）。
#[tauri::command]
fn set_tmdb_key(db: tauri::State<Db>, key: String) -> AppResult<()> {
    settings::set(&db, "tmdb_api_key", &key)
}

#[tauri::command]
fn get_tmdb_key(db: tauri::State<Db>) -> AppResult<Option<String>> {
    settings::get(&db, "tmdb_api_key")
}

/// 为所有空封面视频抓取 TMDB 海报，后台线程执行，poster-progress 事件推进度。
/// 返回 { ok, failed:[{title,reason}] }。
#[tauri::command]
async fn fetch_posters(app: tauri::AppHandle) -> AppResult<poster::FetchReport> {
    use tauri::{Emitter, Manager};

    let db_key = {
        let db = app.state::<Db>();
        settings::get(&db, "tmdb_api_key")?
    };
    let api_key = db_key
        .filter(|k| !k.trim().is_empty())
        .ok_or_else(|| error::AppError::Invalid("请先填写 TMDB API Key".into()))?;

    let covers_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| error::AppError::Other(format!("app_data_dir: {e}")))?
        .join("covers");

    // 在阻塞线程里跑（ureq 同步 + sqlite），避免卡 UI。
    let app2 = app.clone();
    let report = tauri::async_runtime::spawn_blocking(move || -> AppResult<poster::FetchReport> {
        let db = app2.state::<Db>();
        let items = library::list_items(&db, MediaKind::Video)?;

        let key = api_key.clone();
        let covers = covers_dir.clone();
        let app3 = app2.clone();

        // fetch_cover 闭包：TMDB 搜索→(季海报)→下载→压缩→存盘，返回封面绝对路径或错误原因。
        let fetch_cover = |q: &poster::parse::MediaQuery| -> Result<String, String> {
            let hit = poster::tmdb::search(&q.name, q.kind, &key)
                .map_err(|e| format!("网络错误: {e}"))?;
            let hit = match hit {
                Some(h) => h,
                None => return Err("搜索无结果".into()),
            };
            // 剧集：优先季海报，回退整剧 poster
            let poster_path = if let (poster::parse::MediaKind::Tv, Some(season)) = (q.kind, q.season) {
                match poster::tmdb::season_poster(hit.id, season, &key) {
                    Ok(Some(p)) => Some(p),
                    _ => hit.poster_path.clone(),
                }
            } else {
                hit.poster_path.clone()
            };
            let poster_path = poster_path.ok_or_else(|| "无海报".to_string())?;
            std::thread::sleep(std::time::Duration::from_millis(250)); // 限速
            let bytes = poster::tmdb::download(&poster_path).map_err(|e| format!("网络错误: {e}"))?;
            let cover = poster::image_proc::to_cover(&bytes).map_err(|e| format!("图片处理失败: {e}"))?;
            let path = poster::image_proc::save_cover(&covers, &cover).map_err(|e| format!("图片处理失败: {e}"))?;
            Ok(path)
        };

        let progress = |done: usize, total: usize, title: &str| {
            let _ = app3.emit("poster-progress", serde_json::json!({
                "done": done, "total": total, "current_title": title
            }));
        };

        poster::fetch_posters(&db, &items, fetch_cover, progress)
    })
    .await
    .map_err(|e| error::AppError::Other(format!("join: {e}")))??;

    Ok(report)
}
```

- [ ] **Step 2: 注册命令**

在 `tauri::generate_handler![` 列表中 `import_cover` 后加逗号并追加：

```rust
            import_cover,
            set_tmdb_key,
            get_tmdb_key,
            fetch_posters
```

（注意：`import_cover` 原本是列表最后一项无逗号，需给它补逗号。）

- [ ] **Step 3: 编译验证**

Run: `cd src-tauri && cargo build 2>&1 | tail -20`
Expected: 编译成功。若 `FetchReport`/`FailedItem` 未实现 `Serialize` 导致命令返回类型报错——它们已在 Task 5 加 `#[derive(Serialize)]`，确认无误；若 `poster::parse::MediaQuery` 可见性不足，确认 parse.rs 中 `MediaQuery`/`MediaKind` 为 `pub`（Task 2 已 pub）。

- [ ] **Step 4: 全后端测试回归**

Run: `cd src-tauri && cargo test 2>&1 | tail -20`
Expected: 所有既有 + poster 测试通过。

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/lib.rs
git commit -m "feat(poster): fetch_posters/set_tmdb_key/get_tmdb_key 命令 + 后台线程 + 进度事件"
```

---

### Task 7: 前端 ipc + 设置页「海报」卡片

**Files:**
- Modify: `src/lib/ipc.ts`
- Modify: `src/views/SettingsView.ts`

- [ ] **Step 1: 加 ipc 方法与类型**

在 `src/lib/ipc.ts` 的 `api` 对象里（`importCover` 之后）加：

```ts
  setTmdbKey: (key: string) => invoke<void>("set_tmdb_key", { key }),
  getTmdbKey: () => invoke<string | null>("get_tmdb_key"),
  fetchPosters: () => invoke<FetchReport>("fetch_posters"),
```

并在文件顶部 `MediaItem` 附近加导出接口：

```ts
export interface FailedItem { title: string; reason: string; }
export interface FetchReport { ok: number; failed: FailedItem[]; }
```

- [ ] **Step 2: 设置页加海报卡片**

在 `src/views/SettingsView.ts` 顶部 import 区确认已 `import { api } from "../lib/ipc";` 与 `import { icon } from "../lib/icons";`（已有）。加 `import { listen } from "@tauri-apps/api/event";` 于文件顶部。

在 `el.innerHTML` 模板中，「视频」卡片 `</div>`（视频 setting-card 结束）之后、`${singleCards}` 之前，插入海报卡片：

```html
    <div class="glass setting-card">
      <div class="setting-card-head"><span class="setting-card-title">海报</span></div>
      <div class="setting-row">
        <span class="setting-label">TMDB Key</span>
        <input id="tmdb-key" class="setting-path" style="flex:1;padding:6px 10px;border-radius:8px;background:var(--glass);border:1px solid var(--border);color:var(--text)" placeholder="填入 TMDB API Key" />
      </div>
      <div class="setting-actions">
        <button class="btn-primary icon-text" id="fetch-posters">${icon("refresh", 15)}<span class="btn-label">抓取缺失海报</span></button>
        <span id="poster-progress" style="color:var(--text-dim);font-size:12px"></span>
      </div>
      <div id="poster-result" style="margin-top:8px;font-size:12px;color:var(--text-dim);max-height:220px;overflow:auto"></div>
    </div>
```

- [ ] **Step 3: 加交互逻辑**

在 `SettingsView` 函数体内（`return el;` 之前）加：

```ts
  // 海报：预填 Key + 保存 + 抓取
  const keyInput = el.querySelector<HTMLInputElement>("#tmdb-key")!;
  api.getTmdbKey().then(k => { if (k) keyInput.value = k; });
  keyInput.onchange = () => { api.setTmdbKey(keyInput.value.trim()); };

  const fetchBtn = el.querySelector<HTMLButtonElement>("#fetch-posters")!;
  const progressEl = el.querySelector<HTMLElement>("#poster-progress")!;
  const resultEl = el.querySelector<HTMLElement>("#poster-result")!;
  let unlisten: (() => void) | null = null;

  fetchBtn.onclick = async () => {
    await api.setTmdbKey(keyInput.value.trim()); // 抓取前先保存 Key
    fetchBtn.disabled = true;
    resultEl.innerHTML = "";
    progressEl.textContent = "准备中…";
    unlisten = await listen<{ done: number; total: number; current_title: string }>(
      "poster-progress",
      (e) => { progressEl.textContent = `抓取中… ${e.payload.done}/${e.payload.total}`; }
    );
    try {
      const report = await api.fetchPosters();
      progressEl.textContent = `完成：成功 ${report.ok}，失败 ${report.failed.length}`;
      if (report.failed.length) {
        resultEl.innerHTML = "<div style='margin-bottom:4px'>失败清单：</div>" +
          report.failed.map(f => `<div>· ${f.title} — ${f.reason}</div>`).join("");
      }
    } catch (err) {
      progressEl.textContent = "抓取失败：" + String(err);
    } finally {
      fetchBtn.disabled = false;
      if (unlisten) { unlisten(); unlisten = null; }
    }
  };
```

- [ ] **Step 4: 类型检查**

Run: `npx tsc --noEmit -p tsconfig.json 2>&1 | head -20`
Expected: 无输出（通过）。

- [ ] **Step 5: Commit**

```bash
git add src/lib/ipc.ts src/views/SettingsView.ts
git commit -m "feat(poster): 前端设置页海报卡片——Key 输入+抓取+进度+失败清单"
```

---

### Task 8: 真机验证与收尾

**Files:** 无（验证）

- [ ] **Step 1: 重启应用**

先杀旧进程再启动：
```bash
pkill -f "tauri dev" 2>/dev/null; pkill -f "target/debug/collector" 2>/dev/null; sleep 1
cd /Users/zhoumo/Documents/Claude/collector && nohup npm run tauri dev > /tmp/collector-dev.log 2>&1 & disown
```

- [ ] **Step 2: 验证清单（人工在应用内操作）**

1. 设置页不填 Key 点「抓取缺失海报」→ 提示「请先填写 TMDB API Key」，不发请求。
2. 填入真实 TMDB Key → 点抓取 → 进度实时 `done/total` 更新 → 结束显示「成功 N / 失败 M」+ 失败清单。
3. 回视频面板刷新 → 原空封面视频显示真实海报，尺寸统一 2:3 不变形。
4. 剧集同一季各集封面一致；不同季不同（若 TMDB 有单季海报）。
5. 再点一次抓取 → 已有封面视频跳过（total 只含仍为空的），只补上次失败的。

- [ ] **Step 3: 若验证通过，最终确认无遗留**

Run: `cd src-tauri && cargo test 2>&1 | tail -5 && cd .. && npx tsc --noEmit 2>&1 | head -5`
Expected: 全绿。

---

## Self-Review

- **Spec 覆盖**：数据源 TMDB(api.tmdb.org)✓(T4)、中文搜索✓(T4)、按季优先回退整剧✓(T6 fetch_cover)、500×750 q85✓(T3)、存 covers/✓(T3)、只补空缺✓(T5 needs_cover)、失败清单✓(T5/T7)、异步进度✓(T6/T7)、Key 存 settings✓(T6)。
- **占位扫描**：无 TBD；网络层无单测是明确决策（非占位）；ureq/image API 若签名不符已给出实现者调整指引（等价行为约束明确）。
- **类型一致性**：`MediaQuery{name,kind,season}`、`MediaKind::{Movie,Tv}`（poster 内，区别于 library 的 MediaKind）、`FetchReport{ok,failed}`、`FailedItem{title,reason}` 全程一致；`fetch_posters(db, items, fetch_cover, progress)` 签名 T5 定义、T6 调用一致；ipc `fetchPosters()->FetchReport` 与后端返回一致。
- **注意点**：poster::parse::MediaKind 与 library::model::MediaKind 同名不同类型——T5/T6 中通过路径限定区分，实现者勿混用。
