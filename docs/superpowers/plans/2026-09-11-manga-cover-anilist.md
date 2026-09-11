# 卷封面 + 漫画封面(AniList) 实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 卷封面用 zip 首图作卷列表缩略图（运行时、不落盘）；漫画封面从 AniList 批量拓取、压缩存 covers/。

**Architecture:** scan_comics 去封面落盘；新增 comic_volume_cover 命令（zip 首图 data URL）；新增 poster/anilist.rs（GraphQL 搜 MANGA 封面）+ fetch_manga_covers 命令（仿 fetch_posters）；前端工具箱加"漫画封面拓取"按钮、ComicVolumesView 用卷缩略图。

**Tech Stack:** Rust（ureq/serde_json/image/base64）、Tauri command、vanilla-ts。

---

### Task 1: scan_comics 去封面落盘

**Files:**
- Modify: `src-tauri/src/library/scanner.rs`
- Modify: `src-tauri/src/lib.rs`（scan_root Comic 分支）

- [ ] **Step 1: 改测试**

`scanner.rs` 的 `scans_manga_as_item_with_cover` 与 `scans_manga_category_and_multi_volumes`（Task 2 曾建）现断言 cover_path 非空/落盘——改为断言 **cover_path 为 None**（扫描不再生成封面）。改名/改断言为：

```rust
    #[test]
    fn scans_manga_as_item_no_cover() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let manga = root.join("热血").join("灌篮高手");
        std::fs::create_dir_all(&manga).unwrap();
        std::fs::write(manga.join("Vol_01.zip"), b"PK").unwrap(); // 内容无关，扫描不再读图
        let items = scan_comics(root);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].title, "灌篮高手");
        assert_eq!(items[0].category_path, "热血");
        assert!(items[0].cover_path.is_none());
    }
```

保留多卷聚合断言的那个测试，但调用改 `scan_comics(root)`、去掉 cover 断言（或断言 None）。junk 测试同样改 `scan_comics(root)`。

- [ ] **Step 2: 运行验证失败**

Run: `cd src-tauri && cargo test --lib scanner`
Expected: 编译失败（scan_comics 现签名带 covers_dir）。

- [ ] **Step 3: 改 scan_comics**

`scan_comics` 签名去掉 `covers_dir`：`pub fn scan_comics(root: &Path) -> Vec<ScannedItem>`。删除 `extract_cover` 函数及其调用，`cover_path: None`。其余（漫画识别、title/category_path 派生）不变。

`lib.rs` scan_root 的 Comic 分支改回 `library::scanner::scan_comics(std::path::Path::new(&root))`（不再取 covers）。若 scan_root 的 `app` 参数此时只被 Comic 分支用过、去掉后无其它用途导致 unused，则一并去掉 app 参数（看是否还有别处用 app）；若拿不准就保留 app 参数加 `_` 前缀或 `#[allow(unused)]`——优先干净：能去则去。

- [ ] **Step 4: 运行验证通过**

Run: `cd src-tauri && cargo test --lib scanner && cargo build --lib 2>&1 | grep -E "^error" | head`
Expected: 测试 PASS、无 error。

- [ ] **Step 5: 提交**

```bash
git add src-tauri/src/library/scanner.rs src-tauri/src/lib.rs
git commit -m "refactor(scanner): scan_comics 扫描时不再生成漫画封面(改由 AniList 拓取)

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

### Task 2: comic_volume_cover 命令 + 卷列表缩略图

**Files:**
- Modify: `src-tauri/src/comic/mod.rs`
- Modify: `src-tauri/src/lib.rs`（注册）
- Modify: `src/lib/ipc.ts`
- Modify: `src/views/ComicVolumesView.ts`

- [ ] **Step 1: 写失败测试**

`comic/mod.rs` 新增纯函数 `first_page_data_url(zip_path) -> AppResult<Option<String>>` 与命令 `comic_volume_cover`。测试：

```rust
    #[test]
    fn volume_cover_returns_first_page() {
        use image::{RgbImage, Rgb};
        let tmp = tempfile::tempdir().unwrap();
        let zp = tmp.path().join("Vol_01.zip");
        let f = std::fs::File::create(&zp).unwrap();
        let mut w = zip::ZipWriter::new(f);
        let opt = zip::write::SimpleFileOptions::default();
        let mut buf = std::io::Cursor::new(Vec::new());
        RgbImage::from_pixel(10,10,Rgb([1,2,3])).write_to(&mut buf, image::ImageFormat::Jpeg).unwrap();
        use std::io::Write;
        w.start_file("001.jpg", opt).unwrap();
        w.write_all(buf.get_ref()).unwrap();
        w.finish().unwrap();
        let url = first_page_data_url(&zp).unwrap();
        assert!(url.unwrap().starts_with("data:image/jpeg;base64,"));
    }
```

（放在 comic/mod.rs 的 test 模块，需 use super::*。）

- [ ] **Step 2: 运行验证失败**

Run: `cd src-tauri && cargo test --lib volume_cover_returns_first_page`
Expected: 编译失败（未定义）。

- [ ] **Step 3: 实现**

`comic/mod.rs` 加（复用 reader::list_pages + read_entry + base64，逻辑同旧 comic_cover 但独立）：

```rust
/// 卷 zip 首图的 data URL（不压缩、不落盘），用作卷列表缩略图。无图返回 None。
pub fn first_page_data_url(zip_path: &Path) -> AppResult<Option<String>> {
    let pages = reader::list_pages(zip_path)?;
    let first = match pages.first() { Some(f) => f.clone(), None => return Ok(None) };
    let bytes = reader::read_entry(zip_path, &first)?;
    let mime = if first.to_lowercase().ends_with(".png") { "image/png" } else { "image/jpeg" };
    let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
    Ok(Some(format!("data:{mime};base64,{b64}")))
}

#[tauri::command(rename_all = "camelCase")]
pub fn comic_volume_cover(zip_path: String) -> AppResult<Option<String>> {
    first_page_data_url(Path::new(&zip_path))
}
```

`lib.rs` handler 加 `comic::comic_volume_cover,`。

`ipc.ts` 加 `comicVolumeCover: (zipPath: string) => invoke<string|null>("comic_volume_cover", { zipPath })`。

`ComicVolumesView.ts`：卷卡片渲染后，对每张卡 `api.comicVolumeCover(v.zip_path).then(url => { 有则 <img>、无则保留 book 图标 })`（仿 ComicView 旧封面懒加载）。`.vol-thumb` 里放 img。

- [ ] **Step 4: 验证**

Run: `cd src-tauri && cargo test --lib comic && cd .. && npx tsc --noEmit`
Expected: 测试 PASS、tsc 通过。

- [ ] **Step 5: 提交**

```bash
git add src-tauri/src/comic/mod.rs src-tauri/src/lib.rs src/lib/ipc.ts src/views/ComicVolumesView.ts
git commit -m "feat(comic): 新增 comic_volume_cover(zip首图)，卷列表显示卷缩略图

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

### Task 3: AniList 漫画封面模块 + fetch_manga_covers 命令

**Files:**
- Create: `src-tauri/src/poster/anilist.rs`
- Modify: `src-tauri/src/poster/mod.rs`（加 `pub mod anilist;`；确认 FailedItem/FetchReport/update_cover_path 可见性）
- Modify: `src-tauri/src/lib.rs`（fetch_manga_covers 命令 + 注册）

- [ ] **Step 0: 探明（先做）**

`grep -n "pub struct FailedItem\|pub struct FetchReport\|fn update_cover_path\|fn needs_cover\|fn agent" src-tauri/src/poster/mod.rs src-tauri/src/poster/tmdb.rs`：确认 FailedItem/FetchReport 字段与可见性、update_cover_path 是否 pub、ureq agent 怎么建。读 `fetch_posters`（lib.rs 约 410-480）完整结构以仿写。FailedItem 字段（category/category_path/title/reason/suggest_name/suggest_note）——漫画填 category_path/title，reason 填未命中原因，suggest_* 填空/None。

- [ ] **Step 1: 写失败测试**

创建 `src-tauri/src/poster/anilist.rs`，先写纯函数 `parse_cover_url` + 测试：

```rust
use crate::error::{AppError, AppResult};

/// 从 AniList GraphQL 响应提取封面 URL：data.Media.coverImage.extraLarge，回退 large。
pub fn parse_cover_url(json: &serde_json::Value) -> Option<String> {
    let ci = json.get("data")?.get("Media")?.get("coverImage")?;
    ci.get("extraLarge").and_then(|v| v.as_str())
        .or_else(|| ci.get("large").and_then(|v| v.as_str()))
        .map(|s| s.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_extra_large_then_large() {
        let j: serde_json::Value = serde_json::from_str(
            r#"{"data":{"Media":{"coverImage":{"extraLarge":"https://x/xl.jpg","large":"https://x/l.jpg"}}}}"#).unwrap();
        assert_eq!(parse_cover_url(&j).as_deref(), Some("https://x/xl.jpg"));
        let j2: serde_json::Value = serde_json::from_str(
            r#"{"data":{"Media":{"coverImage":{"large":"https://x/l.jpg"}}}}"#).unwrap();
        assert_eq!(parse_cover_url(&j2).as_deref(), Some("https://x/l.jpg"));
        let j3: serde_json::Value = serde_json::from_str(r#"{"data":{"Media":null}}"#).unwrap();
        assert_eq!(parse_cover_url(&j3), None);
    }
}
```

在 `poster/mod.rs` 加 `pub mod anilist;`。

- [ ] **Step 2: 运行验证失败/通过**

Run: `cd src-tauri && cargo test --lib parses_extra_large`
Expected: 编译通过 + 测试 PASS（纯函数直接可过——这是锚点；先确认 parse 正确）。

- [ ] **Step 3: 加 search_cover + download + fetch_manga_covers**

`anilist.rs` 加网络函数（仿 tmdb.rs 的 agent 风格）：

```rust
fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout(std::time::Duration::from_secs(15))
        .build()
}

/// 按名搜 AniList MANGA 封面 URL。无需 API key。
pub fn search_cover(name: &str) -> AppResult<Option<String>> {
    let query = "query($q:String){ Media(search:$q, type:MANGA){ coverImage{ extraLarge large } } }";
    let body = serde_json::json!({ "query": query, "variables": { "q": name } });
    let resp = agent()
        .post("https://graphql.anilist.co")
        .set("Content-Type", "application/json")
        .set("Accept", "application/json")
        .send_json(body)
        .map_err(|e| AppError::Other(format!("anilist search: {e}")))?;
    let json: serde_json::Value = resp.into_json()
        .map_err(|e| AppError::Other(format!("anilist json: {e}")))?;
    Ok(parse_cover_url(&json))
}

/// 下载封面字节。
pub fn download(url: &str) -> AppResult<Vec<u8>> {
    let resp = agent().get(url).call()
        .map_err(|e| AppError::Other(format!("anilist download: {e}")))?;
    let mut buf = Vec::new();
    std::io::Read::read_to_end(&mut resp.into_reader(), &mut buf)
        .map_err(|e| AppError::Other(format!("anilist read: {e}")))?;
    Ok(buf)
}
```

在 `lib.rs` 加命令（仿 fetch_posters，遍历 comic 无 cover 的、search→download→to_cover→save_cover("manga_")→update_cover_path，未命中记 FailedItem，emit "manga-cover-progress"）：

```rust
#[tauri::command]
async fn fetch_manga_covers(app: tauri::AppHandle) -> AppResult<poster::FetchReport> {
    use tauri::{Emitter, Manager};
    let covers_dir = app.path().app_data_dir()
        .map_err(|e| error::AppError::Other(format!("app_data_dir: {e}")))?
        .join("covers");
    let app2 = app.clone();
    let report = tauri::async_runtime::spawn_blocking(move || -> AppResult<poster::FetchReport> {
        let db = app2.state::<Db>();
        let items = library::list_items(&db, MediaKind::Comic)?;
        let todo: Vec<_> = items.into_iter()
            .filter(|it| it.cover_path.as_ref().map(|c| c.is_empty()).unwrap_or(true))
            .collect();
        let total = todo.len();
        let mut ok = 0usize;
        let mut failed = Vec::new();
        for (i, it) in todo.iter().enumerate() {
            let _ = app2.emit("manga-cover-progress",
                serde_json::json!({ "done": i, "total": total, "current_title": it.title }));
            match poster::anilist::search_cover(&it.title) {
                Ok(Some(url)) => match poster::anilist::download(&url)
                    .and_then(|b| poster::image_proc::to_cover(&b))
                    .and_then(|c| poster::image_proc::save_cover(&covers_dir, &c, "manga_")) {
                    Ok(path) => { poster::update_cover_path(&db, it.id, &path)?; ok += 1; }
                    Err(e) => failed.push(poster::FailedItem {
                        category: it.category.clone(), category_path: it.category_path.clone(),
                        title: it.title.clone(), reason: format!("下载/处理失败: {e}"),
                        suggest_name: None, suggest_note: String::new() }),
                },
                Ok(None) => failed.push(poster::FailedItem {
                    category: it.category.clone(), category_path: it.category_path.clone(),
                    title: it.title.clone(), reason: "AniList 未命中".into(),
                    suggest_name: None, suggest_note: "请手动上传封面".into() }),
                Err(e) => failed.push(poster::FailedItem {
                    category: it.category.clone(), category_path: it.category_path.clone(),
                    title: it.title.clone(), reason: format!("网络错误: {e}"),
                    suggest_name: None, suggest_note: String::new() }),
            }
        }
        let _ = app2.emit("manga-cover-progress",
            serde_json::json!({ "done": total, "total": total, "current_title": "" }));
        Ok(poster::FetchReport { ok, failed })
    }).await.map_err(|e| error::AppError::Other(format!("join: {e}")))??;
    Ok(report)
}
```

> 按 Step 0 探明的 FailedItem/FetchReport 实际字段调整构造；`poster::update_cover_path` 若非 pub 则改 pub。handler 注册 `fetch_manga_covers,`。

- [ ] **Step 4: 验证**

Run: `cd src-tauri && cargo test --lib anilist && cargo build --lib 2>&1 | grep -E "^error" | head`
Expected: parse 测试 PASS、无 error。

- [ ] **Step 5: 提交**

```bash
git add src-tauri/src/poster/anilist.rs src-tauri/src/poster/mod.rs src-tauri/src/lib.rs
git commit -m "feat(poster): AniList 漫画封面模块 + fetch_manga_covers 批量拓取命令

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

### Task 4: 前端工具箱"漫画封面拓取"按钮 + ipc

**Files:**
- Modify: `src/lib/ipc.ts`
- Modify: `src/views/NormalizeView.ts`

- [ ] **Step 1: ipc**

`ipc.ts` 加 `fetchMangaCovers: () => invoke<FetchReport>("fetch_manga_covers")`（复用现有 FetchReport 类型）。

- [ ] **Step 2: NormalizeView 加卡片**

在 `NormalizeView` 的 innerHTML 里（"影视海报生成"卡之后或"漫画自动归档"卡附近）加"漫画封面拓取"`setting-card`：说明文字 + `.btn-primary.icon-text#manga-cover-run` + 进度条三件套（`#manga-cover-progress`/`-bar`/`-text`，复用同款内联样式）+ 结果区 `#manga-cover-result`。

绑定 onclick（仿海报 fetchBtn 逻辑）：禁用按钮、显示进度条、listen `manga-cover-progress` `{done,total,current_title}` 更新条与文本（"拓取中… done/total"）、双 rAF、`await api.fetchMangaCovers()`、完成置 100% + 显示"成功 N，未命中 M"、未命中列表（复用 renderPosterTable 或简列表展示 failed 的 title+reason）、finally 恢复。

- [ ] **Step 3: 类型检查**

Run: `npx tsc --noEmit`
Expected: 无输出。

- [ ] **Step 4: 提交**

```bash
git add src/lib/ipc.ts src/views/NormalizeView.ts
git commit -m "feat(normalize): 工具箱新增「漫画封面拓取」(AniList,进度条+未命中列表)

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

## 自查

- **规格覆盖**：scan_comics 去封面落盘（Task 1）、卷缩略图 comic_volume_cover（Task 2）、AniList 模块 + fetch_manga_covers（Task 3）、工具箱按钮（Task 4）——规格各条均有对应任务。
- **占位符扫描**：无 TBD/TODO；每步给出完整代码。Task 3 Step 0 探明后按实际字段微调 FailedItem 构造是合理的实现适配。
- **类型/命名一致性**：`comic_volume_cover`/`comicVolumeCover`（zipPath）、`fetch_manga_covers`/`fetchMangaCovers`、事件名 `manga-cover-progress`（后端 emit / 前端 listen）、`parse_cover_url`/`search_cover`/`download`（anilist.rs）在各任务间一致；复用 `poster::FetchReport`/`FailedItem`/`update_cover_path`/`image_proc::{to_cover,save_cover}`。
- **待实施确认点**：poster FailedItem/FetchReport 字段与 update_cover_path 可见性（Task 3 Step 0 grep）；scan_root 去 covers 后 app 参数是否还需要（Task 1 Step 3）；ureq send_json/into_json API 与项目 ureq 版本是否匹配（Task 3 Step 3，按 tmdb.rs 现有用法对齐）。
