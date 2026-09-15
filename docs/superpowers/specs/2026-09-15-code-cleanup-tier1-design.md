# 代码整洁清理（第 1 档）· 设计

日期：2026-09-15
分支：`feat/ui-home-icons-fullscreen-layout`

## 背景与目标

对全代码库做了命名规范审计（前端 30 个 .ts、后端 32 个 .rs + 数据库 schema）。结论：命名已高度规范——前端函数 camelCase / 视图工厂 PascalCase / 类型 PascalCase、IPC 参数统一 camelCase、返回值字段统一 snake_case、数据库全 snake_case 且列名与结构体字段一一对应，均无违规。

因此本次**只做低风险的「真正不一致 + 死代码 + 重复逻辑」清理**（审计的第 1 档），不动 IPC 边界的驼峰/蛇形架构（第 2 档）、不做清晰度重命名与大文件拆分（第 3 档）。目标：消除确凿的重复与死代码，统一少数命名离群项，行为零变化。

## 约束

- **行为零变化**：所有改动是重命名、删除未用代码、抽取等价 helper、合并等价函数。不改任何运行时逻辑、不改 IPC 边界字段名、不改数据库。
- **验证**：`npx tsc --noEmit` 通过；`cargo build` 通过；`cargo test --lib` 全绿；应用启动无报错。

## 涉及文件

- `src/lib/spreads.ts`、`src/lib/ipc.ts`、`src/components/SubtitleRenderer.ts`（前端）
- `src-tauri/src/library/scanner.rs`、`src-tauri/src/normalize/comic_pack.rs`、`src-tauri/src/poster/image_proc.rs`、`src-tauri/src/poster/tmdb.rs`、`src-tauri/src/lib.rs`（后端）

---

## 前端清理

### F1 · 消除重复的 `PageInfo` 类型
`PageInfo`（`{name; w; h}`）在 `ipc.ts:14` 和 `spreads.ts:1` 各定义一遍。
**改法**：`spreads.ts` 删除本地定义，改为 `import type { PageInfo } from "./ipc";`。`ipc.ts` 保留为唯一定义源。
**验收**：tsc 通过，`spreads.ts` 不再有本地 `PageInfo` 声明；`buildSpreads` 签名不变。

### F2 · 删除未使用的 `ArchiveProgress` 类型
`ipc.ts:23` 的 `export interface ArchiveProgress` 全项目无 import（漫画进度在 `normalizeStore.ts` 内联匿名解构）。
**改法**：删除该 interface。
**验收**：tsc 通过，grep 确认无残留引用。

### F3 · 常量命名统一 `cjkFontUrl` → `CJK_FONT_URL`
`SubtitleRenderer.ts:5` 的模块级不可变字符串 `cjkFontUrl` 与同文件邻居 `WORKER_URL`/`LEGACY_WORKER_URL` 同类却用了 camelCase。
**改法**：重命名为 `CJK_FONT_URL`，更新文件内所有引用。
**验收**：tsc 通过，grep 确认无 `cjkFontUrl` 残留。

---

## 后端清理

### B1 · 删除死函数
- `library/scanner.rs:68` 的 `#[allow(dead_code)] pub fn into_item`。
- `normalize/comic_pack.rs:103` 的 `#[allow(dead_code)] pub fn pack_comic_dir`（已被归档管线取代）。
**改法**：删除这两个函数及其 `#[allow(dead_code)]`。若删除后 `comic_pack.rs` 出现未用 import，一并清理。
**验收**：`cargo build` 通过、无未用 import 警告；`cargo test --lib` 全绿。

### B2 · JPEG 质量常量命名统一（不合并模块）
`poster/image_proc.rs:12` 的 `JPEG_Q` 与 `normalize/comic_pack.rs:10` 的 `JPEG_QUALITY` 同值 85 但语义分属不同模块（海报封面质量 vs 漫画打包质量）。二者非真重复，是巧合同值。
**改法**：仅把 `image_proc.rs` 的 `JPEG_Q` 重命名为 `JPEG_QUALITY`（消除「同概念两名字」），更新其在该文件内的引用。**不**抽取为跨模块共享常量——保持两模块独立（YAGNI：将来可独立调整各自质量）。
**验收**：`cargo build` 通过；两模块各自保留独立 `JPEG_QUALITY` 常量。

### B3 · 过度缩写常量重命名
- `poster/image_proc.rs:10-11` 的 `W`/`H` → `COVER_WIDTH`/`COVER_HEIGHT`，更新文件内所有引用（如 `finalize` 中的 `W as f32 / H as f32` 等）。
- `poster/tmdb.rs:9` 的 `UA` → `USER_AGENT`，更新 `.user_agent(UA)` 引用。
**验收**：`cargo build` 通过；grep 确认无 `W`/`H`/`UA` 旧常量残留（注意只改这三个常量标识符，不误伤同名局部变量如结构体字段 `w`/`h`）。

### B4 · 抽取 `kind → 封面子目录` helper
`lib.rs` 的 `import_cover`（458 行附近）和 `import_cover_cropped`（481 行附近）都有逐字重复的：
```rust
let sub = match kind.as_str() { "comic" => "comic", "game" => "game", _ => "media" };
```
**改法**：抽取为文件内 helper：
```rust
/// kind → 封面存放子目录（comic/game 各自子目录，其余归 media）。
fn cover_subdir(kind: &str) -> &'static str {
    match kind {
        "comic" => "comic",
        "game" => "game",
        _ => "media",
    }
}
```
两处调用改为 `let sub = cover_subdir(&kind);`。
**验收**：`cargo build` 通过；两命令行为不变（封面仍存入同一子目录）。

### B5 · 合并 comic/game 的 CRUD 孪生函数
`lib.rs` 的 `update_comic_row`/`update_game_row` 仅 SQL 表名不同（`comic` vs `game`），`delete_comic_row`/`delete_game_row` 同理。表名来自受控字符串常量，非用户输入，无注入风险（与既有 `rename_folder_in_db` 参数化表名的做法一致）。
**改法**：合并为两个参数化表名的函数：
```rust
/// 更新 comic/game 表单条记录（表名受控，非用户输入）。
fn update_media_kind_row(db: &Db, table: &str, id: i64, category_path: &str, title: &str, cover_path: Option<&str>, description: Option<&str>) -> AppResult<()> {
    let conn = db.0.lock().unwrap();
    conn.execute(
        &format!("UPDATE {table} SET category_path=?1,title=?2,cover_path=?3,description=?4 WHERE id=?5"),
        rusqlite::params![category_path, title, cover_path, description, id],
    ).map_err(|e| error::AppError::Db(e.to_string()))?;
    Ok(())
}

/// 删除 comic/game 表单条记录（表名受控，非用户输入）。
fn delete_media_kind_row(db: &Db, table: &str, id: i64) -> AppResult<()> {
    db.0.lock().unwrap()
        .execute(&format!("DELETE FROM {table} WHERE id=?1"), rusqlite::params![id])
        .map_err(|e| error::AppError::Db(e.to_string()))?;
    Ok(())
}
```
`comic_update`/`comic_delete`/`game_update`/`game_delete` 四个命令改为调用 `update_media_kind_row(&db, "comic"/"game", ...)` 与 `delete_media_kind_row(&db, "comic"/"game", ...)`。删除原来的 4 个孪生函数。
**验收**：`cargo build` 通过；四个命令的 SQL 与原来完全等价（表名分别为 comic/game）；`cargo test --lib` 全绿。

> 命名说明：新函数用 `update_media_kind_row` / `delete_media_kind_row` 以区别于已有的 `media` 表专用逻辑（`media_update` 命令走 `media` 表，不经这两个函数）——避免与 `media` 表混淆。若实现时发现更贴切的名字（如 `update_row_in`/`delete_row_in`），可采用，只需保持自解释。

---

## 测试策略

- 后端：现有 `cargo test --lib` 全部保持通过（B1 删函数后确认无测试依赖它们；B5 合并后 CRUD 行为等价）。不新增测试——本次全为等价重构，现有测试即回归保障。
- 前端：无单测框架，靠 `tsc --noEmit` + 应用启动验证。
- 集成：应用 `npm run tauri dev` 启动无报错；漫画阅读（涉及 F1 的 `PageInfo`）、封面导入（B4）、漫画/游戏编辑删除（B5）功能手动确认可用。

## 非目标（本档不做）

- 不统一 IPC 边界的驼峰/蛇形（第 2 档）。
- 不重命名 `go`/`fmt`/`dur`/`fl`/`ni` 等清晰度项、不改宽参数列表为选项对象（第 3 档）。
- 不拆分 `lib.rs`（第 3 档）。
- 不合并 ComicView/GameView/VideoView 的重复脚手架（第 3 档，属较大重构）。
- 不动数据库 schema（含 comic/game 的常量 `category` 列）。
