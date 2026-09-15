# 代码整洁清理（第 1 档）实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 消除代码库中确凿的重复、死代码，并统一少数命名离群项，行为零变化。

**Architecture:** 全部为等价重构——重命名标识符、删除未用代码、抽取/合并等价函数。不改 IPC 边界字段、不改数据库、不改运行时逻辑。前端靠 `tsc --noEmit` 兜底，后端靠 `cargo build` + `cargo test --lib` 兜底。

**Tech Stack:** TypeScript（vanilla-ts）、Rust（rusqlite / image）、Tauri 2。

---

## Task 1: 前端清理（F1 + F2 + F3）

**Files:**
- Modify: `src/lib/spreads.ts`（顶部 PageInfo 定义）
- Modify: `src/lib/ipc.ts`（删除 ArchiveProgress）
- Modify: `src/components/SubtitleRenderer.ts`（cjkFontUrl → CJK_FONT_URL）

- [ ] **Step 1: F1 — spreads.ts 复用 ipc 的 PageInfo**

`src/lib/spreads.ts` 顶部当前为：

```typescript
export interface PageInfo { name: string; w: number; h: number; }
export interface Spread { left?: PageInfo; right?: PageInfo; single: boolean; }
```

改为（删除本地 PageInfo，改从 ipc import；Spread 保留）：

```typescript
import type { PageInfo } from "./ipc";

export type { PageInfo };
export interface Spread { left?: PageInfo; right?: PageInfo; single: boolean; }
```

说明：`export type { PageInfo }` 保留 `spreads.ts` 对外仍可导出 `PageInfo`（以防其它文件从 spreads 导入它），避免破坏现有 import。

- [ ] **Step 2: 确认谁从 spreads 导入 PageInfo，验证不破坏**

Run: `grep -rn "PageInfo" src/ | grep -v "ipc.ts"`
Expected: 查看 `spreads.ts` 及任何从 `./spreads`/`../lib/spreads` 导入 `PageInfo` 的地方。因 Step 1 用 `export type { PageInfo }` 重新导出，这些导入仍有效。

- [ ] **Step 3: F2 — 删除未使用的 ArchiveProgress 类型**

在 `src/lib/ipc.ts` 找到并删除整行：

```typescript
export interface ArchiveProgress { manga: string; vol: string; done_images: number; total_images: number; total_manga: number; total_vols: number; }
```

先验证确实无人使用：

Run: `grep -rn "ArchiveProgress" src/`
Expected: 仅 `ipc.ts` 该声明一处。若有其它引用，停止并报告（说明它其实被用到，不该删）。

- [ ] **Step 4: F3 — cjkFontUrl → CJK_FONT_URL**

在 `src/components/SubtitleRenderer.ts` 找到 `cjkFontUrl` 的声明（约第 5 行）与所有使用处，全部重命名为 `CJK_FONT_URL`。

Run 先定位: `grep -n "cjkFontUrl" src/components/SubtitleRenderer.ts`
逐处改名。

- [ ] **Step 5: 类型检查 + 确认无残留旧名**

Run: `cd /Users/zhoumo/Documents/Claude/collector && npx tsc --noEmit 2>&1 | tail -10 && echo "---" && grep -rn "cjkFontUrl\|ArchiveProgress" src/ || echo "无残留"`
Expected: tsc 无错误；grep 无 `cjkFontUrl`、无 `ArchiveProgress` 残留。

- [ ] **Step 6: 提交**

```bash
git add src/lib/spreads.ts src/lib/ipc.ts src/components/SubtitleRenderer.ts
git commit -m "refactor(fe): 复用 PageInfo、删未用类型、常量命名统一 CJK_FONT_URL"
```

---

## Task 2: 后端死代码删除（B1）

**Files:**
- Modify: `src-tauri/src/library/scanner.rs`（删除 into_item）
- Modify: `src-tauri/src/normalize/comic_pack.rs`（删除 pack_comic_dir）

- [ ] **Step 1: 确认两函数确实无人调用**

Run: `cd /Users/zhoumo/Documents/Claude/collector && grep -rn "into_item\|pack_comic_dir" src-tauri/src/`
Expected: 各自仅在定义处出现（`scanner.rs` 的 `into_item`、`comic_pack.rs` 的 `pack_comic_dir`）。若有其它调用处，停止并报告。

- [ ] **Step 2: 删除 `scanner.rs::into_item`**

在 `src-tauri/src/library/scanner.rs` 删除以 `#[allow(dead_code)]` 标注的 `pub fn into_item(self, id: i64) -> MediaItem` 整个函数（含其上的 `#[allow(dead_code)]` 属性行与文档注释）。

- [ ] **Step 3: 删除 `comic_pack.rs::pack_comic_dir`**

在 `src-tauri/src/normalize/comic_pack.rs` 删除以 `#[allow(dead_code)]` 标注的 `pub fn pack_comic_dir(...)` 整个函数（含属性行与文档注释）。

- [ ] **Step 4: 编译，清理因删除产生的未用 import**

Run: `cd /Users/zhoumo/Documents/Claude/collector/src-tauri && cargo build 2>&1 | tail -20`
Expected: 编译通过。若出现 `unused import` 警告（因删函数后某 import 不再需要），删除对应 import 后重新 build 至无警告。

- [ ] **Step 5: 测试**

Run: `cd /Users/zhoumo/Documents/Claude/collector/src-tauri && cargo test --lib 2>&1 | tail -15`
Expected: 全部通过（删除的是无测试依赖的死函数）。

- [ ] **Step 6: 提交**

```bash
git add src-tauri/src/library/scanner.rs src-tauri/src/normalize/comic_pack.rs
git commit -m "refactor(be): 删除死函数 into_item、pack_comic_dir"
```

---

## Task 3: 后端常量重命名（B2 + B3）

**Files:**
- Modify: `src-tauri/src/poster/image_proc.rs`（JPEG_Q → JPEG_QUALITY，W/H → COVER_WIDTH/COVER_HEIGHT）
- Modify: `src-tauri/src/poster/tmdb.rs`（UA → USER_AGENT）

- [ ] **Step 1: image_proc.rs — 常量声明改名**

在 `src-tauri/src/poster/image_proc.rs` 顶部（约 10-12 行）：

```rust
const W: u32 = 500;
const H: u32 = 750;
const JPEG_Q: u8 = 85;
```

改为：

```rust
const COVER_WIDTH: u32 = 500;
const COVER_HEIGHT: u32 = 750;
const JPEG_QUALITY: u8 = 85;
```

- [ ] **Step 2: image_proc.rs — 更新所有引用**

Run 定位所有引用: `grep -n "\bW\b\|\bH\b\|JPEG_Q\b" src-tauri/src/poster/image_proc.rs`
把每处对常量 `W`/`H`/`JPEG_Q` 的引用改为 `COVER_WIDTH`/`COVER_HEIGHT`/`JPEG_QUALITY`。
**注意**：只改对这三个模块常量的引用；不要误改局部变量或结构体字段（如 `img.width()`、局部 `w`/`h`、`cw`/`ch` 等小写标识符）。改完再次 grep 确认无独立的 `W`/`H`/`JPEG_Q` 常量引用残留。

- [ ] **Step 3: tmdb.rs — UA → USER_AGENT**

在 `src-tauri/src/poster/tmdb.rs`：

```rust
const UA: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) collector/1.0";
```

改为：

```rust
const USER_AGENT: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) collector/1.0";
```

并把 `.user_agent(UA)`（约第 80 行）改为 `.user_agent(USER_AGENT)`。

Run 确认: `grep -n "\bUA\b" src-tauri/src/poster/tmdb.rs`
Expected: 改完后无 `UA` 残留。

- [ ] **Step 4: 编译**

Run: `cd /Users/zhoumo/Documents/Claude/collector/src-tauri && cargo build 2>&1 | tail -15`
Expected: 编译通过，无未定义常量错误、无未用常量警告。

- [ ] **Step 5: 提交**

```bash
git add src-tauri/src/poster/image_proc.rs src-tauri/src/poster/tmdb.rs
git commit -m "refactor(be): 常量改名 COVER_WIDTH/HEIGHT、JPEG_QUALITY、USER_AGENT"
```

---

## Task 4: 后端抽取 cover_subdir helper（B4）

**Files:**
- Modify: `src-tauri/src/lib.rs`（import_cover / import_cover_cropped）

- [ ] **Step 1: 新增 helper 函数**

在 `src-tauri/src/lib.rs` 中 `import_cover` 函数定义之前，新增：

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

- [ ] **Step 2: 替换 import_cover 中的重复 match**

在 `import_cover` 中，把：

```rust
    let sub = match kind.as_str() { "comic" => "comic", "game" => "game", _ => "media" };
```

改为：

```rust
    let sub = cover_subdir(&kind);
```

- [ ] **Step 3: 替换 import_cover_cropped 中的重复 match**

在 `import_cover_cropped` 中，把同样的一行：

```rust
    let sub = match kind.as_str() { "comic" => "comic", "game" => "game", _ => "media" };
```

改为：

```rust
    let sub = cover_subdir(&kind);
```

- [ ] **Step 4: 确认无其它同款重复残留 + 编译**

Run: `cd /Users/zhoumo/Documents/Claude/collector && grep -n '"comic" => "comic"' src-tauri/src/lib.rs && echo "---build---" && (cd src-tauri && cargo build 2>&1 | tail -12)`
Expected: grep 只应命中 helper 内那一行（两处调用点已不再有该 match）；编译通过。

- [ ] **Step 5: 提交**

```bash
git add src-tauri/src/lib.rs
git commit -m "refactor(be): 抽取 cover_subdir helper，消除封面子目录重复 match"
```

---

## Task 5: 后端合并 comic/game CRUD 孪生函数（B5）

**Files:**
- Modify: `src-tauri/src/lib.rs`（update/delete comic/game row + 四个命令）

- [ ] **Step 1: 新增两个参数化表名的函数**

在 `src-tauri/src/lib.rs` 中（放在原 `update_comic_row` 所在位置附近即可），新增：

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

- [ ] **Step 2: 删除四个孪生函数**

删除 `update_comic_row`、`delete_comic_row`、`update_game_row`、`delete_game_row` 四个函数（含其文档注释）。

- [ ] **Step 3: 更新四个命令调用点**

- `comic_update` 中把 `update_comic_row(&db, id, &category_path, &title, rel_cover.as_deref(), description.as_deref())` 改为 `update_media_kind_row(&db, "comic", id, &category_path, &title, rel_cover.as_deref(), description.as_deref())`。
- `comic_delete` 中把 `delete_comic_row(&db, id)` 改为 `delete_media_kind_row(&db, "comic", id)`。
- `game_update` 中把 `update_game_row(...)` 改为 `update_media_kind_row(&db, "game", ...)`（其余参数不变）。
- `game_delete` 中把 `delete_game_row(&db, id)` 改为 `delete_media_kind_row(&db, "game", id)`。

- [ ] **Step 4: 确认旧函数已无引用 + 编译 + 测试**

Run: `cd /Users/zhoumo/Documents/Claude/collector && grep -n "update_comic_row\|delete_comic_row\|update_game_row\|delete_game_row" src-tauri/src/lib.rs || echo "旧函数已清空"`
Expected: 无残留（旧函数名不再出现）。

Run: `cd /Users/zhoumo/Documents/Claude/collector/src-tauri && cargo build 2>&1 | tail -12 && cargo test --lib 2>&1 | tail -15`
Expected: 编译通过；`cargo test --lib` 全绿。

- [ ] **Step 5: 提交**

```bash
git add src-tauri/src/lib.rs
git commit -m "refactor(be): 合并 comic/game CRUD 孪生函数为参数化表名"
```

---

## Task 6: 整体验证

**Files:** 无（验证任务）

- [ ] **Step 1: 全量构建与测试**

Run: `cd /Users/zhoumo/Documents/Claude/collector && npx tsc --noEmit && (cd src-tauri && cargo build 2>&1 | tail -5 && cargo test --lib 2>&1 | tail -10)`
Expected: tsc 无错误；cargo build 完成；cargo test --lib 全绿。

- [ ] **Step 2: 手动验证清单（在运行的应用中确认，行为应与清理前完全一致）**

- 漫画阅读器正常打开、翻页（涉及 F1 的 PageInfo 复用）。
- 封面导入/裁剪导入正常，封面存入正确子目录（涉及 B4 的 cover_subdir）。
- 漫画、游戏条目的编辑保存与删除正常（涉及 B5 的 CRUD 合并）。
- 字幕渲染正常（涉及 F3 的常量改名）。
- 海报抓取功能正常（涉及 B3 的 image_proc/tmdb 常量改名）。

- [ ] **Step 3: 前序任务已各自提交，本任务无额外提交**

---

## 自审记录

- **Spec 覆盖**：F1→Task1 Step1；F2→Task1 Step3；F3→Task1 Step4；B1→Task2；B2→Task3 Step1-2；B3→Task3；B4→Task4；B5→Task5；验证→Task6。全覆盖。
- **无占位符**：每步含确切文件位置、确切改动代码/命令、预期结果。
- **类型/命名一致**：新常量名 `COVER_WIDTH`/`COVER_HEIGHT`/`JPEG_QUALITY`/`USER_AGENT`/`CJK_FONT_URL` 在计划中前后一致；新函数 `cover_subdir`/`update_media_kind_row`/`delete_media_kind_row` 定义与调用点签名一致。
- **风险控制**：F1 用 `export type` 重导出以防破坏现有 import；删除类/函数前均先 grep 验证无引用；常量改名步骤特别提示不要误伤同名小写局部变量。
