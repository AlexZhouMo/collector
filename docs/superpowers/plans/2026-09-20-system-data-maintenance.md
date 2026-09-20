# 工具箱系统数据整理 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 工具箱「漫画自动归档」后新增「系统数据整理」卡片，含「数据库重制」（三表按规则重排+ID从1）与「无效封面清理」（孤立图片删、缺失文件提示）两个按钮。

**Architecture:** 后端新增 `maintenance.rs`（db_reset + clean_covers 两 command + 可测纯逻辑）；前端加 2 个图标、2 个 ipc 封装、NormalizeView 一张卡片（两按钮+结果区+执行态，无进度条）。

**Tech Stack:** Rust (Tauri 2, rusqlite)、TypeScript (Vite)、SQLite。

**验证：** `cd src-tauri && cargo test && cargo build`；`cd .. && npx tsc --noEmit && npm run build`；运行 `npm run tauri:dev`。

**关键事实（修正 spec 一处）：** 三表 media/comic/game **都有 category 列**（comic="漫画"、game="游戏"，值单一）。重插时都要保留 category。排序：media 按 category 自定义序（电影>动漫>剧集）；comic/game 的 category 单一，实际按 category_path→title 排即可（对它们 category 序无差别，可统一用同一比较器，自定义序对"漫画"/"游戏"归入"其他"档，不影响单分类内排序）。三表都 AUTOINCREMENT，ID 从 1 需 `DELETE FROM sqlite_sequence WHERE name='<表>'`。列：id, category, category_path, title, description, cover_path。

---

## Task 1: 后端 maintenance.rs —— db_reset

**Files:**
- Create: `src-tauri/src/maintenance.rs`
- Modify: `src-tauri/src/lib.rs`（`mod maintenance;` + 注册 db_reset）

- [ ] **Step 1: 读现状**

Run: `sed -n '1,30p' src-tauri/src/db/mod.rs; grep -n "open_in_memory\|MIGRATIONS\|run_migrations" src-tauri/src/db/mod.rs`
Expected: 确认 `Db(pub Mutex<Connection>)`、`open_in_memory()`（供单测，会建表）。确认建表在 open 时执行。

- [ ] **Step 2: 写 db_reset 纯逻辑 + 命令**

新建 `src-tauri/src/maintenance.rs`：
```rust
use crate::db::Db;
use crate::error::{AppError, AppResult};
use serde::Serialize;

/// 一条记录（重排用，保留所有字段）。
struct Row { category: String, category_path: String, title: String, description: Option<String>, cover_path: Option<String> }

/// category 自定义序：电影<动漫<剧集<其他。
fn category_rank(c: &str) -> u8 { match c { "电影" => 0, "动漫" => 1, "剧集" => 2, _ => 3 } }

/// 排序比较器：category_rank → category_path → title（均升序）。
fn sort_rows(rows: &mut [Row]) {
    rows.sort_by(|a, b| {
        category_rank(&a.category).cmp(&category_rank(&b.category))
            .then_with(|| a.category_path.cmp(&b.category_path))
            .then_with(|| a.title.cmp(&b.title))
    });
}

#[derive(Serialize)]
pub struct DbResetResult { pub media: usize, pub comic: usize, pub game: usize }

/// 三表在一个事务内：读出→排序→清空+清 sqlite_sequence→按序重插（ID 从 1）。失败回滚。
#[tauri::command]
pub fn db_reset(db: tauri::State<Db>) -> AppResult<DbResetResult> {
    let mut conn = db.0.lock().unwrap();
    let tx = conn.transaction().map_err(|e| AppError::Db(e.to_string()))?;
    let mut counts = [0usize; 3];
    for (i, table) in ["media", "comic", "game"].iter().enumerate() {
        // 读出
        let mut rows: Vec<Row> = {
            let mut stmt = tx.prepare(&format!("SELECT category,category_path,title,description,cover_path FROM {table}"))
                .map_err(|e| AppError::Db(e.to_string()))?;
            let it = stmt.query_map([], |r| Ok(Row {
                category: r.get(0)?, category_path: r.get(1)?, title: r.get(2)?, description: r.get(3)?, cover_path: r.get(4)?,
            })).map_err(|e| AppError::Db(e.to_string()))?;
            it.collect::<Result<Vec<_>,_>>().map_err(|e| AppError::Db(e.to_string()))?
        };
        sort_rows(&mut rows);
        tx.execute(&format!("DELETE FROM {table}"), []).map_err(|e| AppError::Db(e.to_string()))?;
        tx.execute("DELETE FROM sqlite_sequence WHERE name=?1", [table]).map_err(|e| AppError::Db(e.to_string()))?;
        for row in &rows {
            tx.execute(&format!("INSERT INTO {table} (category,category_path,title,description,cover_path) VALUES (?1,?2,?3,?4,?5)"),
                rusqlite::params![row.category, row.category_path, row.title, row.description, row.cover_path])
                .map_err(|e| AppError::Db(e.to_string()))?;
        }
        counts[i] = rows.len();
    }
    tx.commit().map_err(|e| AppError::Db(e.to_string()))?;
    Ok(DbResetResult { media: counts[0], comic: counts[1], game: counts[2] })
}
```
（表名来自硬编码常量数组，非用户输入，format! 拼接无注入。sqlite_sequence 若某表无记录，DELETE 影响 0 行无害。）

- [ ] **Step 3: 注册**

`src-tauri/src/lib.rs`：模块声明区加 `mod maintenance;`；invoke_handler 加 `maintenance::db_reset,`。

- [ ] **Step 4: 单测**

在 maintenance.rs 加 `#[cfg(test)] mod tests`：
- `sort_rows` 测：构造乱序 Row（电影/动漫/剧集混合 + 同 category 下 path/title 乱序）→ sort_rows → 断言顺序符合 电影>动漫>剧集、path 升序、title 升序。
- db_reset 集成测：`Db::open_in_memory()`（建表），插入乱序几条 media（不同 category）+ comic + game，调 db_reset（注意：命令签名用 tauri::State，测试里改为直接调一个提取出的 `fn db_reset_impl(db: &Db)` 更好——把逻辑提到 `db_reset_impl`，command 只包装。这样可单测）。断言：重排后 SELECT id,category,... 顺序正确、id 从 1 连续。
（**重构提示**：把 command 体提到 `pub fn db_reset_impl(db: &Db) -> AppResult<DbResetResult>`，`#[tauri::command] db_reset` 调它，便于单测。）

- [ ] **Step 5: 编译 + 测试**

Run: `cd src-tauri && cargo build 2>&1 | grep -iE "error|warning" || echo clean; cargo test maintenance 2>&1 | tail -5`
Expected: clean + 测试全过。

- [ ] **Step 6: 提交**
```bash
cd /Users/zhoumo/Documents/Claude/collector
git add src-tauri/src/maintenance.rs src-tauri/src/lib.rs
git commit -m "feat(maintenance): db_reset 命令——三表按规则重排+ID从1（事务）

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

## Task 2: 后端 clean_covers

**Files:**
- Modify: `src-tauri/src/maintenance.rs`（加 clean_covers）、`src-tauri/src/lib.rs`（注册）

- [ ] **Step 1: 读封面路径规则**

Run: `grep -n "covers\|cover_path\|app_data_dir\|delete_cover_file" src-tauri/src/lib.rs | head`
Expected: 确认封面存 `<app_data>/covers/<sub>/`，库存相对路径 `covers/<sub>/xxx`；app_data 取法 `app.path().app_data_dir()`。

- [ ] **Step 2: 写 clean_covers**

在 maintenance.rs 加：
```rust
#[derive(Serialize)]
pub struct MissingCover { pub table: String, pub title: String, pub path: String }
#[derive(Serialize)]
pub struct CleanCoversResult { pub deleted_orphans: usize, pub missing: Vec<MissingCover> }

/// 清理封面：孤立图片（文件在、库不引用）直接删；缺失文件（库引用、文件不在）仅记录提示。
#[tauri::command]
pub fn clean_covers(app: tauri::AppHandle, db: tauri::State<Db>) -> AppResult<CleanCoversResult> {
    use tauri::Manager;
    let app_data = app.path().app_data_dir()
        .map_err(|e| AppError::Other(format!("app_data_dir: {e}")))?;
    // 1) 收集库中所有非空 cover_path（去重）→ HashSet<相对路径>
    //    同时记录 (table,title,cover_path) 供缺失提示。
    let conn = db.0.lock().unwrap();
    let mut referenced: std::collections::HashSet<String> = Default::default();
    let mut refs: Vec<(String,String,String)> = vec![]; // (table,title,path)
    for table in ["media","comic","game"] {
        let mut stmt = conn.prepare(&format!("SELECT title,cover_path FROM {table} WHERE cover_path IS NOT NULL AND cover_path != ''"))
            .map_err(|e| AppError::Db(e.to_string()))?;
        let it = stmt.query_map([], |r| Ok((r.get::<_,String>(0)?, r.get::<_,String>(1)?)))
            .map_err(|e| AppError::Db(e.to_string()))?;
        for row in it { let (title,cp) = row.map_err(|e| AppError::Db(e.to_string()))?;
            referenced.insert(cp.clone()); refs.push((table.into(), title, cp)); }
    }
    drop(conn);
    // 2) 缺失：库引用但文件不在（按去重路径检查，避免重复报）
    let mut missing = vec![];
    let mut seen_missing: std::collections::HashSet<String> = Default::default();
    for (table,title,cp) in &refs {
        if !app_data.join(cp).is_file() && seen_missing.insert(cp.clone()) {
            missing.push(MissingCover { table: table.clone(), title: title.clone(), path: cp.clone() });
        }
    }
    // 3) 孤立：遍历 covers/media、covers/comic 下文件，相对路径未被引用 → 删
    let mut deleted = 0usize;
    for sub in ["media","comic"] {
        let dir = app_data.join("covers").join(sub);
        if let Ok(rd) = std::fs::read_dir(&dir) {
            for e in rd.flatten() {
                let p = e.path();
                if !p.is_file() { continue; }
                let rel = format!("covers/{sub}/{}", e.file_name().to_string_lossy());
                if !referenced.contains(&rel) && std::fs::remove_file(&p).is_ok() { deleted += 1; }
            }
        }
    }
    Ok(CleanCoversResult { deleted_orphans: deleted, missing })
}
```
（缺失提示每个不同路径只报一次；孤立按目录内文件相对路径比对去重引用集。）

- [ ] **Step 3: 注册**

lib.rs invoke_handler 加 `maintenance::clean_covers,`。

- [ ] **Step 4: 单测**

同样把逻辑提到 `clean_covers_impl(app_data: &Path, db: &Db)`（不依赖 AppHandle，传 app_data 路径），command 包装。测试：临时目录建 `covers/media/{a.jpg,b.jpg,orphan.jpg}`，Db 插 media 引用 covers/media/a.jpg + covers/media/missing.jpg → 调 impl → 断言 orphan.jpg 被删（deleted=1）、b.jpg 也孤立被删、a.jpg 保留、missing 含 covers/media/missing.jpg。验证共享封面（两条引用同一 a.jpg）不误删 a.jpg。

- [ ] **Step 5: 编译 + 测试**

Run: `cd src-tauri && cargo build 2>&1 | grep -iE "error|warning" || echo clean; cargo test maintenance 2>&1 | tail -5`
Expected: clean + 全过。

- [ ] **Step 6: 提交**
```bash
cd /Users/zhoumo/Documents/Claude/collector
git add src-tauri/src/maintenance.rs src-tauri/src/lib.rs
git commit -m "feat(maintenance): clean_covers 命令——孤立图片删+缺失文件提示（DISTINCT 判定）

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

## Task 3: 前端图标 + ipc

**Files:**
- Modify: `src/lib/icons.ts`（加 database、imageClean）、`src/lib/ipc.ts`（加 dbReset、cleanCovers）

- [ ] **Step 1: 加两个图标**

`src/lib/icons.ts` 加两个内联 SVG（stroke 风格，viewBox/属性同现有如 archive）：
- `database`: 圆柱数据库形（顶部椭圆 + 两条弧线身），如 `<ellipse cx="12" cy="5" rx="8" ry="3"/><path d="M4 5v6c0 1.7 3.6 3 8 3s8-1.3 8-3V5"/><path d="M4 11v6c0 1.7 3.6 3 8 3s8-1.3 8-3v-6"/>`
- `imageClean`: 图片框 + 清理标记（如图片框加对角斜线/扫帚意象），如 `<rect x="3" y="3" width="18" height="18" rx="2"/><circle cx="8.5" cy="8.5" r="1.5"/><path d="M21 15l-5-5L5 21"/><path d="M14 3l7 7"/>`（斜线示意"清理/无效"）。
两个图标要区别于现有 archive/trash/folder。

- [ ] **Step 2: ipc 加封装**

`src/lib/ipc.ts` 加类型 + 方法：
```typescript
export interface DbResetResult { media: number; comic: number; game: number; }
export interface MissingCover { table: string; title: string; path: string; }
export interface CleanCoversResult { deleted_orphans: number; missing: MissingCover[]; }
// api 对象内：
dbReset: () => invoke<DbResetResult>("db_reset"),
cleanCovers: () => invoke<CleanCoversResult>("clean_covers"),
```

- [ ] **Step 3: 类型检查**

Run: `cd /Users/zhoumo/Documents/Claude/collector && npx tsc --noEmit`
Expected: 无输出。

- [ ] **Step 4: 提交**
```bash
git add src/lib/icons.ts src/lib/ipc.ts
git commit -m "feat(maintenance): 前端 database/imageClean 图标 + dbReset/cleanCovers ipc

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

## Task 4: NormalizeView 系统数据整理卡片

**Files:**
- Modify: `src/views/NormalizeView.ts`

- [ ] **Step 1: 读卡片结构与结果渲染**

Run: `sed -n '14,60p' src/views/NormalizeView.ts`
Expected: 确认 innerHTML 里卡片顺序（漫画自动归档卡片位置）、`$` 选择器、结果区渲染模式、import icon。

- [ ] **Step 2: 在漫画归档卡片后加「系统数据整理」卡片**

在漫画自动归档卡片的闭合 `</div>` 之后、模板字符串结束前，加：
```html
<div class="glass setting-card">
  <div class="setting-card-head"><span class="setting-card-title">系统数据整理</span></div>
  <div class="setting-actions">
    <button class="btn-primary icon-text" id="db-reset-run">${icon("database", 15)}<span class="btn-label">数据库重制</span></button>
    <button class="btn-primary icon-text" id="clean-covers-run">${icon("imageClean", 15)}<span class="btn-label">无效封面清理</span></button>
  </div>
  <div id="maint-result" style="margin-top:10px;font-size:12px;color:var(--text-dim);white-space:pre-line"></div>
</div>
```

- [ ] **Step 3: 绑定两个按钮**

在事件绑定区加（用现有 `$` 选择器 + api）：
```typescript
const maintResult = $("#maint-result");
const dbBtn = $<HTMLButtonElement>("#db-reset-run");
dbBtn.onclick = async () => {
  dbBtn.disabled = true;
  const orig = dbBtn.querySelector(".btn-label")!.textContent;
  dbBtn.querySelector(".btn-label")!.textContent = "执行中…";
  try {
    const r = await api.dbReset();
    maintResult.textContent = `数据库重制完成：media ${r.media} 条、comic ${r.comic} 条、game ${r.game} 条，ID 已从 1 重排。`;
  } catch (e) { maintResult.textContent = "数据库重制失败：" + String(e); }
  finally { dbBtn.disabled = false; dbBtn.querySelector(".btn-label")!.textContent = orig; }
};
const coverBtn = $<HTMLButtonElement>("#clean-covers-run");
coverBtn.onclick = async () => {
  coverBtn.disabled = true;
  const orig = coverBtn.querySelector(".btn-label")!.textContent;
  coverBtn.querySelector(".btn-label")!.textContent = "检查中…";
  try {
    const r = await api.cleanCovers();
    let msg = `无效封面清理完成：删除孤立图片 ${r.deleted_orphans} 个。`;
    if (r.missing.length) {
      msg += `\n数据库引用但文件缺失 ${r.missing.length} 个：\n` +
        r.missing.map(m => `· [${m.table}] ${m.title} → ${m.path}`).join("\n");
    } else { msg += "\n未发现缺失文件。"; }
    maintResult.textContent = msg;
  } catch (e) { maintResult.textContent = "无效封面清理失败：" + String(e); }
  finally { coverBtn.disabled = false; coverBtn.querySelector(".btn-label")!.textContent = orig; }
};
```
（确认 NormalizeView 里 api 已 import；btn-label span 选择器与现有一致。）

- [ ] **Step 4: 类型检查 + 构建**

Run: `npx tsc --noEmit && npm run build 2>&1 | tail -1`
Expected: ok + 构建成功。

- [ ] **Step 5: 提交**
```bash
git add src/views/NormalizeView.ts
git commit -m "feat(maintenance): 工具箱系统数据整理卡片（数据库重制+无效封面清理按钮）

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

## Task 5: 端到端验证

- [ ] **Step 1: 全套编译测试**

Run: `cd src-tauri && cargo test 2>&1 | grep "test result:" | head -1; cargo build 2>&1 | grep -iE "error|warning" || echo clean; cd .. && npx tsc --noEmit && npm run build 2>&1 | tail -1`
Expected: 测试全过 + clean + tsc + 构建成功。

- [ ] **Step 2: 真机验证**

`npm run tauri:dev`，工具箱最下方「系统数据整理」卡片：
- 点「数据库重制」→ 结果显示三表条数；进影视/漫画/游戏列表确认顺序（电影>动漫>剧集、字母升序）、ID 重排（可用 sqlite3 查 `SELECT id,title FROM media ORDER BY id LIMIT 5` 确认从 1）。
- 点「无效封面清理」→ 结果显示（当前数据健康，大概率"删除孤立 0、未发现缺失"）。可临时在 covers/media 放一个无引用的 test.jpg 验证它被删。
- 两图标显示正常、区别于其他。

- [ ] **Step 3: 工作树确认**

Run: `git status --short && git log --oneline -6`
Expected: 干净；5 个任务提交在列（Task 5 无代码提交，仅验证）。
