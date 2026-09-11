# 漫画封面 AniList停用识别(A) + 编辑抽屉手动上传(C) 实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A：识别 AniList 停用给准确提示；C：漫画完整编辑抽屉（新增 comic_update/comic_delete，不碰视频字幕逻辑；import_cover 参数化 kind；EditDrawer 加 kind；ComicView 右键菜单）。

**Architecture:** A 在 anilist.rs search_cover 识别 403+disabled 返回明确错误。C 后端新增 comic_update/comic_delete（只改 comic 表）+ import_cover/import_cover_cropped 加 kind 参数选 covers 子目录；前端 EditDrawer 加 kind 参数（漫画文案/隐藏视频路径/调 comic 命令）+ ComicView 传右键菜单。media_update 保持视频专用不动。

**Tech Stack:** Rust（rusqlite/ureq）、vanilla-ts。

---

### Task 1: A — AniList 停用识别

**Files:**
- Modify: `src-tauri/src/poster/anilist.rs`
- Modify: `src-tauri/src/lib.rs`（fetch_manga_covers reason）

- [ ] **Step 1: 写失败测试**

在 anilist.rs 加纯函数 `is_service_disabled(body: &str) -> bool` + 测试：

```rust
    #[test]
    fn detects_service_disabled() {
        assert!(is_service_disabled(r#"{"errors":[{"message":"The AniList API has been temporarily disabled due to severe stability issues.","status":403}]}"#));
        assert!(!is_service_disabled(r#"{"data":{"Media":{"coverImage":{"large":"x"}}}}"#));
    }
```

- [ ] **Step 2: 运行验证失败**

Run: `cd src-tauri && cargo test --lib anilist`
Expected: 编译失败——is_service_disabled 未定义。

- [ ] **Step 3: 实现**

anilist.rs 加：

```rust
/// AniList 响应体是否表示服务被官方临时停用（403 + 特定 message）。
pub fn is_service_disabled(body: &str) -> bool {
    body.contains("temporarily disabled")
}
```

在 search_cover 里，把 send_string 之后的状态/响应体处理改为：先取响应文本，若 status==403 或 is_service_disabled(&text)，返回 `Err(AppError::Other("AniList 服务暂时不可用".into()))`；否则正常解析。（用现有 ureq 用法：response.into_string() 拿 text；403 时 ureq 的 .call()/.send_string() 可能已返回 Err(Status)，需 `.or_else` 捕获 403 状态——参考 tmdb.rs 对状态的处理；若 ureq 对 4xx 直接 Err(ureq::Error::Status(code, resp))，则 match 出 403 分支读 resp.into_string() 判 disabled。据 ureq 实际行为实现，保证 403+disabled → 明确错误。）

在 lib.rs fetch_manga_covers：search_cover 返回 Err 时，若错误信息含"暂时不可用"，FailedItem.reason = "AniList 服务暂时不可用，请稍后重试或手动上传封面"；否则 `网络错误: {e}`（现状）。

- [ ] **Step 4: 运行验证通过**

Run: `cd src-tauri && cargo test --lib anilist && cargo build --lib 2>&1 | grep -E "^error"`
Expected: 测试 PASS、无 error。

- [ ] **Step 5: 提交**

```bash
git add src-tauri/src/poster/anilist.rs src-tauri/src/lib.rs
git commit -m "feat(comic): 识别 AniList 服务停用(403)，给准确提示而非笼统网络错误

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

### Task 2: C 后端 — comic_update/comic_delete + import_cover 参数化 kind

**Files:**
- Modify: `src-tauri/src/lib.rs`
- Modify: `src/lib/ipc.ts`

- [ ] **Step 1: 写失败测试**

在 lib.rs 测试模块加（comic_update 更新 comic 表 title/cover/desc；comic_delete 删除）：

```rust
    #[test]
    fn comic_update_and_delete() {
        let db = Db::open_in_memory().unwrap();
        let id = {
            let c = db.0.lock().unwrap();
            c.execute("INSERT INTO comic(category_path,title,cover_path,description) VALUES('热血','海贼王',NULL,NULL)", []).unwrap();
            c.last_insert_rowid()
        };
        // update
        update_comic_row(&db, id, "热血", "海贼王改", Some("covers/comic/x.jpg"), Some("简介")).unwrap();
        {
            let c = db.0.lock().unwrap();
            let (t, cp): (String, String) = c.query_row("SELECT title,cover_path FROM comic WHERE id=?1", [id], |r| Ok((r.get(0)?, r.get(1)?))).unwrap();
            assert_eq!(t, "海贼王改"); assert_eq!(cp, "covers/comic/x.jpg");
        }
        // delete
        delete_comic_row(&db, id).unwrap();
        let n: i64 = db.0.lock().unwrap().query_row("SELECT count(*) FROM comic WHERE id=?1", [id], |r| r.get(0)).unwrap();
        assert_eq!(n, 0);
    }
```

（用内部函数 update_comic_row/delete_comic_row 便于测试；command 包一层。）

- [ ] **Step 2: 运行验证失败**

Run: `cd src-tauri && cargo test --lib comic_update_and_delete`
Expected: 编译失败——未定义。

- [ ] **Step 3: 实现**

lib.rs 加内部函数 + 命令：

```rust
fn update_comic_row(db: &Db, id: i64, category_path: &str, title: &str, cover_path: Option<&str>, description: Option<&str>) -> AppResult<()> {
    let conn = db.0.lock().unwrap();
    conn.execute("UPDATE comic SET category_path=?1,title=?2,cover_path=?3,description=?4 WHERE id=?5",
        rusqlite::params![category_path, title, cover_path, description, id])
        .map_err(|e| error::AppError::Db(e.to_string()))?;
    Ok(())
}
fn delete_comic_row(db: &Db, id: i64) -> AppResult<()> {
    db.0.lock().unwrap().execute("DELETE FROM comic WHERE id=?1", rusqlite::params![id])
        .map_err(|e| error::AppError::Db(e.to_string()))?;
    Ok(())
}

#[tauri::command(rename_all = "camelCase")]
fn comic_update(app: tauri::AppHandle, db: tauri::State<Db>, id: i64, category_path: String, title: String, cover_path: Option<String>, description: Option<String>) -> AppResult<()> {
    let app_data = app.path().app_data_dir().ok().map(|p| p.to_string_lossy().into_owned()).unwrap_or_default();
    let rel_cover = cover_path.map(|c| library::paths::appdata_to_relative(&c, &app_data));
    update_comic_row(&db, id, &category_path, &title, rel_cover.as_deref(), description.as_deref())
}

#[tauri::command]
fn comic_delete(db: tauri::State<Db>, id: i64) -> AppResult<()> {
    delete_comic_row(&db, id)
}
```

import_cover / import_cover_cropped 加 `kind: String` 参数（camelCase），covers 子目录按 kind：`let sub = if kind == "comic" { "comic" } else { "media" };`，covers_dir = `app_data/covers/<sub>`。

handler 注册 comic_update, comic_delete。ipc.ts：
- 新增 `comicUpdate: (id, categoryPath, title, coverPath, description) => invoke<void>("comic_update", {...})`；`comicDelete: (id) => invoke<void>("comic_delete", { id })`。
- importCover/importCoverCropped 加 kind 参数：`importCover: (srcImage, kind) => invoke("import_cover", { srcImage, kind })`；cropped 同理加 kind。**注意**：这改了现有 importCover 签名——视频调用方（EditDrawer）需同步传 "video"（Task 3 处理）。

- [ ] **Step 4: 运行验证通过**

Run: `cd src-tauri && cargo test --lib comic_update_and_delete && cargo build --lib 2>&1 | grep -E "^error"`
Expected: 测试 PASS、无 error（后端）。tsc 前端因 importCover 签名变化会在 Task 3 修——本任务只保证后端 + ipc 语法。

- [ ] **Step 5: 提交**

```bash
git add src-tauri/src/lib.rs src/lib/ipc.ts
git commit -m "feat(comic): 新增 comic_update/comic_delete 命令 + import_cover 按 kind 存 covers 子目录

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

### Task 3: C 前端 — EditDrawer kind 参数 + ComicView 右键菜单

**Files:**
- Modify: `src/components/EditDrawer.ts`
- Modify: `src/views/ComicView.ts`

- [ ] **Step 1: EditDrawer 加 kind 参数**

`openEditDrawer(item, onSaved, defaultCategory = "电影", kind: "video" | "comic" = "video")`：
- 标题：kind==="comic" → "编辑漫画"/"新增漫画"，否则视频文案。
- 视频路径字段：kind==="comic" 时不渲染（漫画无 video_path）。
- 封面上传：importCover/importCoverCropped 调用传 kind（视频传 "video"、漫画传 "comic"）——修复 Task 2 改的签名。
- 保存：kind==="comic" 走 `api.comicUpdate(item.id, categoryPath, title, coverPath, description)`（漫画只编辑现有，item 必有值；不做 comicCreate）；video 走原 mediaUpdate/mediaCreate。
- 分类/路径：漫画保留 item 原值（category_path 完整路径）。

- [ ] **Step 2: ComicView 传右键菜单**

ComicView：import showContextMenu + openEditDrawer。加 onContext(it,x,y)：显示"编辑"（openEditDrawer(it, refresh, "", "comic")）、"删除"（confirm → api.comicDelete(it.id) → refresh）。refresh = 重新 listMedia+render。把 FolderView/TreeView 调用的 onContext 从 undefined 改为该 onContext。（FolderView 的 .fv-video 已支持 oncontextmenu 调 onContext——确认；enableRename 仍 false。）

- [ ] **Step 3: 类型检查**

Run: `npx tsc --noEmit`
Expected: 无输出（EditDrawer kind、ComicView 右键、importCover 传 kind、comicUpdate/comicDelete 调用都类型正确；视频侧 openEditDrawer 不传 kind 走默认 video，importCover 视频调用处传 "video"）。修到干净。

- [ ] **Step 4: 提交**

```bash
git add src/components/EditDrawer.ts src/views/ComicView.ts
git commit -m "feat(comic): 漫画编辑抽屉(kind=comic,手动上传封面)+漫画卡右键编辑/删除

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

## 自查

- **规格覆盖**：A AniList 停用识别（Task 1）、comic_update/delete + import_cover kind（Task 2）、EditDrawer kind + ComicView 右键（Task 3）——规格各条均有对应任务。微调：C 用新增 comic_update/delete（不参数化 media_update，避免污染视频字幕联动，已与用户确认）。
- **占位符扫描**：无 TBD；每步给出完整代码。Task 1 Step 3 关于 ureq 403 捕获的说明是"据实现行为适配"的提醒，非占位。
- **类型/命名一致性**：comic_update/comic_delete 命令名与 ipc comicUpdate/comicDelete 一致；importCover/cropped 加 kind 在 Task 2 定义、Task 3（EditDrawer 视频/漫画调用）传值；update_comic_row/delete_comic_row 内部函数供 command + 测试复用。
- **视频回归风险**：media_update/media_delete 不动（视频字幕联动保留）；importCover 加 kind 后视频侧传 "video"（Task 3 修 EditDrawer 视频调用）——需回归视频编辑封面上传。
- **待实施确认点**：ureq 对 403 的返回形态（Err(Status) vs Ok）决定 search_cover 里怎么捕获（Task 1 据 tmdb.rs 现有用法适配）；FolderView 的 .fv-video oncontextmenu 是否已调 onContext（Task 3 确认，VideoView 已用同套 onContext）。
