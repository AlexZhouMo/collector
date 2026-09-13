# 漫画自动索引 + 多卷阅读（3D 双页翻书）实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 以每部漫画为条目自动索引（多卷聚合、封面落盘、删 category 列），前端漫画网格 → 卷列表 → 3D 双页翻书阅读器。

**Architecture:** 后端：schema 迁移删 comic.category；scan_comics 重写（含 Vol_XX.zip 的目录=一部漫画，Vol_01 首图压缩存 covers）；新增 comic_volumes 命令；comic_pages 改返回带宽高的 PageInfo。前端：buildSpreads 纯函数按宽高比拼对开；ComicView 读 cover_path、新增 ComicVolumesView、重写 ComicReaderView（CSS 3D rotateY 翻页）。

**Tech Stack:** Rust（rusqlite/zip/image 0.25/regex）、Tauri command、vanilla-ts + CSS 3D。

---

### Task 1: comic 表删 category 列（schema 迁移 + insert/list 调整）

**Files:**
- Modify: `src-tauri/src/db/schema.rs`
- Modify: `src-tauri/src/library/mod.rs`

- [ ] **Step 1: 写失败测试**

在 `src-tauri/src/db/mod.rs` 的 `mod tests` 内新增（模拟旧含 category 表 → 迁移后无 category、唯一键在）：

```rust
    #[test]
    fn migrates_legacy_comic_drops_category() {
        let conn = Connection::open_in_memory().unwrap();
        // 旧结构：含 category 列
        conn.execute_batch(
            "CREATE TABLE comic (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                category TEXT NOT NULL,
                category_path TEXT NOT NULL,
                title TEXT NOT NULL,
                description TEXT,
                cover_path TEXT,
                UNIQUE(category_path,title)
            );",
        ).unwrap();
        conn.execute("INSERT INTO comic(category,category_path,title) VALUES('x','热血','灌篮高手')", []).unwrap();
        for m in schema::MIGRATIONS {
            conn.execute_batch(m).unwrap();
        }
        // 迁移后 comic 无 category 列
        let has_category: bool = conn
            .prepare("SELECT 1 FROM pragma_table_info('comic') WHERE name='category'")
            .unwrap().exists([]).unwrap();
        assert!(!has_category, "comic.category 应已删除");
        // 唯一键仍在：重复 (category_path,title) 冲突
        conn.execute("INSERT INTO comic(category_path,title) VALUES('热血','灌篮高手')", []).unwrap();
        let dup = conn.execute("INSERT INTO comic(category_path,title) VALUES('热血','灌篮高手')", []);
        assert!(dup.is_err(), "UNIQUE(category_path,title) 应拦截重复");
    }
```

- [ ] **Step 2: 运行验证失败**

Run: `cd src-tauri && cargo test --lib migrates_legacy_comic`
Expected: FAIL——当前迁移不删 category 列（旧表 category 仍在）。

- [ ] **Step 3: 加迁移语句**

在 `schema.rs` 的 `MIGRATIONS` 数组**末尾**追加（幂等重建：仅当 comic 仍有 category 列时重建）。因 SQLite 不支持条件 DDL，用"总是建 comic_new（新结构）→ 从 comic 复制公共列 → 删 comic → 改名"，且用 `IF NOT EXISTS`/`IF EXISTS` 保证重复运行安全：

```rust
    // 迁移：comic 表删除 category 列（旧版本建表含 category）。
    // SQLite 无条件 DDL，用标准"新表→拷公共列→换名"。重复运行：若 comic 已是新结构，
    // 下面的 INSERT ... SELECT 仍成立（category_path/title/... 都在），DROP+RENAME 后结构不变。
    "CREATE TABLE IF NOT EXISTS comic_new (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        category_path TEXT NOT NULL,
        title TEXT NOT NULL,
        description TEXT,
        cover_path TEXT,
        UNIQUE(category_path,title)
    );",
    "INSERT INTO comic_new (id,category_path,title,description,cover_path)
       SELECT id,category_path,title,description,cover_path FROM comic;",
    "DROP TABLE comic;",
    "ALTER TABLE comic_new RENAME TO comic;",
    "CREATE UNIQUE INDEX IF NOT EXISTS idx_comic_cat_title ON comic(category_path,title);",
```

> 注意：原 MIGRATIONS 里已有的 `CREATE TABLE IF NOT EXISTS comic (...含 category...)` 保留在前（新库首次建旧结构），迁移语句在其后把它重建为新结构——净效果：新库与旧库最终都得到无 category 的 comic。（可接受：新库多一次建表+重建。）idx_comic_cat_title 若之前迁移已建过，RENAME 后需重建（原索引随旧表 DROP 掉），故重新 CREATE。

在 `library/mod.rs` 把 comic（`MediaKind::Comic`）分支的 insert/create/list SQL 去掉 category 列：
- `insert_one_tx` 与 `create_item` 的 Comic|Game 分支：Comic 不能再写 category。**拆分** Comic 与 Game——Game 保留 category（game 表仍有？确认 game 表结构：schema 里 game 仍含 category）。所以只改 Comic 分支：Comic 单独一个 match 臂，insert 不含 category 列（`INSERT INTO comic (category_path,title,cover_path,description) VALUES(?1,?2,?3,?4)`），ON CONFLICT 更新同理去 category。
- `list_items` 的 Comic 分支：`SELECT id,category_path,title,cover_path,description FROM comic`，构造 MediaItem 时 category 字段填 category_path 的首段或空串（MediaItem 结构仍有 category？看 model.rs——若有则填 category_path.split('/').next()）。

> 实施细节：先看 `library/model.rs` 的 MediaItem 是否含 category 字段。若含，Comic 的 list 用 category_path 首段填充 category 以兼容前端类型；insert 不写 DB category 列。

- [ ] **Step 4: 运行验证通过**

Run: `cd src-tauri && cargo test --lib migrates_legacy_comic && cargo test --lib library`
Expected: 全 PASS。

- [ ] **Step 5: 提交**

```bash
git add src-tauri/src/db/schema.rs src-tauri/src/db/mod.rs src-tauri/src/library/mod.rs
git commit -m "feat(db): comic 表删除 category 列(迁移重建)+ insert/list 适配

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

### Task 2: scan_comics 重写（漫画为条目 + 封面落盘）

**Files:**
- Modify: `src-tauri/src/library/scanner.rs`
- Modify: `src-tauri/src/lib.rs`（scan_root 的 comic 分支需传 covers_dir）

- [ ] **Step 1: 写失败测试**

在 `scanner.rs` 的 `mod comic_scan_tests` 内改写/新增（漫画为条目、category_path 不含漫画名、封面落盘）：

```rust
    #[test]
    fn scans_manga_as_item_with_cover() {
        use image::{RgbImage, Rgb};
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let manga = root.join("热血").join("灌篮高手");
        std::fs::create_dir_all(&manga).unwrap();
        // 造一个含首图的 Vol_01.zip
        let zip_path = manga.join("Vol_01.zip");
        {
            let f = std::fs::File::create(&zip_path).unwrap();
            let mut w = zip::ZipWriter::new(f);
            let opt = zip::write::SimpleFileOptions::default();
            let mut buf = std::io::Cursor::new(Vec::new());
            RgbImage::from_pixel(400, 600, Rgb([10,20,30])).write_to(&mut buf, image::ImageFormat::Jpeg).unwrap();
            use std::io::Write;
            w.start_file("001.jpg", opt).unwrap();
            w.write_all(buf.get_ref()).unwrap();
            w.finish().unwrap();
        }
        let covers = tmp.path().join("covers");
        let items = scan_comics(root, &covers);
        assert_eq!(items.len(), 1);
        let it = &items[0];
        assert_eq!(it.title, "灌篮高手");
        assert_eq!(it.category_path, "热血"); // 不含漫画名
        assert!(it.cover_path.as_ref().unwrap().contains("covers"));
        assert!(std::path::Path::new(it.cover_path.as_ref().unwrap()).is_file());
    }
```

（注意：`scan_comics` 签名将新增 `covers_dir: &Path` 参数。）

- [ ] **Step 2: 运行验证失败**

Run: `cd src-tauri && cargo test --lib scans_manga_as_item`
Expected: 编译失败——scan_comics 旧签名（无 covers_dir）、旧逻辑（zip 为条目）。

- [ ] **Step 3: 重写 scan_comics**

在 `scanner.rs` 顶部加 `use regex::Regex;`（若 regex 未在依赖，Cargo.toml 加 `regex = "1"`——先 grep 确认 `^regex` 是否已有）。重写 `scan_comics`：

```rust
/// 扫描漫画根：含 Vol_XX.zip 的目录 = 一部漫画。
/// title=漫画目录名；category_path=根到该目录父级的相对路径（分类，不含漫画名）。
/// 封面取 Vol_01 首图 → 压缩存 covers_dir → cover_path。
pub fn scan_comics(root: &Path, covers_dir: &Path) -> Vec<ScannedItem> {
    let vol_re = regex::Regex::new(r"^Vol_(\d+)\.zip$").unwrap();
    let mut items = Vec::new();
    for entry in WalkDir::new(root).into_iter().filter_map(|e| e.ok()) {
        let dir = entry.path();
        if !dir.is_dir() || crate::util::junk::is_system_junk_path(dir) {
            continue;
        }
        // 该目录下的 Vol_XX.zip 列表
        let mut vols: Vec<(u32, std::path::PathBuf)> = std::fs::read_dir(dir).ok()
            .into_iter().flatten().filter_map(|e| e.ok())
            .filter_map(|e| {
                let p = e.path();
                let name = p.file_name()?.to_str()?.to_string();
                let caps = vol_re.captures(&name)?;
                let no: u32 = caps.get(1)?.as_str().parse().ok()?;
                Some((no, p))
            }).collect();
        if vols.is_empty() { continue; }
        vols.sort_by_key(|(n, _)| *n);
        let title = dir.file_name().unwrap().to_string_lossy().into_owned();
        let rel_parent = dir.strip_prefix(root).ok()
            .and_then(|r| r.parent())
            .map(|p| p.components().map(|c| c.as_os_str().to_string_lossy().into_owned()).collect::<Vec<_>>().join("/"))
            .unwrap_or_default();
        // 封面：Vol_01（vols[0]）首图 → to_cover → save_cover
        let cover_path = extract_cover(&vols[0].1, covers_dir);
        items.push(ScannedItem {
            category: rel_parent.split('/').next().unwrap_or("").to_string(), // 兼容字段
            category_path: rel_parent,
            title,
            cover_path,
            description: None,
        });
    }
    items
}

/// 从卷 zip 取首图 → 视频封面格式压缩 → 存 covers_dir，返回 cover_path。失败返回 None。
fn extract_cover(zip_path: &Path, covers_dir: &Path) -> Option<String> {
    let pages = crate::comic::reader::list_pages(zip_path).ok()?;
    let first = pages.first()?;
    let bytes = crate::comic::reader::read_entry(zip_path, first).ok()?;
    let cover = crate::poster::image_proc::to_cover(&bytes).ok()?;
    crate::poster::image_proc::save_cover(covers_dir, &cover, "comic_").ok()
}
```

在 `lib.rs` 的 `scan_root` comic 分支：`MediaKind::Comic => { let covers = app_data.join("covers"); library::scanner::scan_comics(Path::new(&root), &covers) }`——需拿到 app_data_dir。看 scan_root 现在是否有 app 句柄；若 scan_root 签名只有 db 无 app，需加 `app: tauri::AppHandle` 参数并在注册处适配，或从 db 存的路径推 covers。**实施**：给 scan_root 加 `app: tauri::AppHandle` 参数（tauri 自动注入），`app.path().app_data_dir()` 取 covers_dir；video/game 分支不受影响。

- [ ] **Step 4: 运行验证通过**

Run: `cd src-tauri && cargo test --lib scans_manga_as_item && cargo test --lib scanner`
Expected: 全 PASS（旧 `scans_zip_comics`/`scan_comics_skips_system_junk` 若与新模型冲突需改写为漫画模型）。

- [ ] **Step 5: 提交**

```bash
git add src-tauri/src/library/scanner.rs src-tauri/src/lib.rs src-tauri/Cargo.toml src-tauri/Cargo.lock
git commit -m "feat(scanner): scan_comics 重写为漫画条目(多卷聚合)+Vol_01首图封面落盘

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

### Task 3: comic_volumes 命令（列卷）

**Files:**
- Modify: `src-tauri/src/comic/mod.rs`
- Modify: `src-tauri/src/lib.rs`（注册）

- [ ] **Step 1: 写失败测试**

在 `comic/mod.rs` 新增 `VolumeInfo` 与纯函数 `list_volumes(manga_dir) -> Vec<VolumeInfo>`，测试：

```rust
#[cfg(test)]
mod vol_tests {
    use super::*;
    #[test]
    fn lists_volumes_sorted() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        for n in ["Vol_02.zip","Vol_01.zip","Vol_10.zip","readme.txt"] {
            std::fs::write(dir.join(n), b"x").unwrap();
        }
        let vols = list_volumes(dir);
        assert_eq!(vols.len(), 3);
        assert_eq!(vols[0].vol_no, 1);
        assert_eq!(vols[0].label, "第 01 卷");
        assert_eq!(vols[1].vol_no, 2);
        assert_eq!(vols[2].vol_no, 10);
        assert_eq!(vols[2].label, "第 10 卷");
    }
}
```

- [ ] **Step 2: 运行验证失败**

Run: `cd src-tauri && cargo test --lib lists_volumes_sorted`
Expected: 编译失败——list_volumes/VolumeInfo 未定义。

- [ ] **Step 3: 实现**

在 `comic/mod.rs` 加：

```rust
#[derive(Debug, Clone, serde::Serialize)]
pub struct VolumeInfo {
    pub vol_no: u32,
    pub label: String,
    pub zip_path: String,
}

/// 列出漫画目录下所有 Vol_XX.zip，按卷号升序。
pub fn list_volumes(manga_dir: &Path) -> Vec<VolumeInfo> {
    let re = regex::Regex::new(r"^Vol_(\d+)\.zip$").unwrap();
    let mut vols: Vec<VolumeInfo> = std::fs::read_dir(manga_dir).ok()
        .into_iter().flatten().filter_map(|e| e.ok())
        .filter_map(|e| {
            let p = e.path();
            let name = p.file_name()?.to_str()?.to_string();
            let caps = re.captures(&name)?;
            let no: u32 = caps.get(1)?.as_str().parse().ok()?;
            Some(VolumeInfo { vol_no: no, label: format!("第 {no:02} 卷"), zip_path: p.to_string_lossy().into_owned() })
        }).collect();
    vols.sort_by_key(|v| v.vol_no);
    vols
}

/// 命令：按 comic_root + category_path + title 拼漫画目录，列出各卷。
#[tauri::command(rename_all = "camelCase")]
pub fn comic_volumes(
    db: tauri::State<crate::db::Db>,
    category_path: String,
    title: String,
) -> AppResult<Vec<VolumeInfo>> {
    let root = crate::settings::get(&db, "comic_root")?
        .ok_or_else(|| crate::error::AppError::Invalid("comic root not set".into()))?;
    let mut dir = std::path::PathBuf::from(root);
    if !category_path.is_empty() { dir.push(&category_path); }
    dir.push(&title);
    Ok(list_volumes(&dir))
}
```

在 `lib.rs` 的 handler 注册 `comic::comic_volumes,`。

- [ ] **Step 4: 运行验证通过**

Run: `cd src-tauri && cargo test --lib lists_volumes_sorted`
Expected: PASS。

- [ ] **Step 5: 提交**

```bash
git add src-tauri/src/comic/mod.rs src-tauri/src/lib.rs
git commit -m "feat(comic): 新增 comic_volumes 命令列出漫画各卷(第XX卷,按卷号排序)

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

### Task 4: comic_pages 返回 PageInfo(宽高) + 移除 comic_cover + ipc

**Files:**
- Modify: `src-tauri/src/comic/reader.rs`（list_pages 返回带宽高）
- Modify: `src-tauri/src/comic/mod.rs`（comic_pages 返回 PageInfo；移除 comic_cover）
- Modify: `src-tauri/src/lib.rs`（注册：去 comic_cover）
- Modify: `src/lib/ipc.ts`

- [ ] **Step 1: 写失败测试**

在 `reader.rs` 的 `mod tests` 新增（list_pages_with_dims 返回宽高）：

```rust
    #[test]
    fn list_pages_with_dims_reads_size() {
        use image::{RgbImage, Rgb};
        let tmp = tempfile::tempdir().unwrap();
        let zp = tmp.path().join("c.zip");
        let f = File::create(&zp).unwrap();
        let mut w = zip::ZipWriter::new(f);
        let opt = SimpleFileOptions::default();
        let mut buf = std::io::Cursor::new(Vec::new());
        RgbImage::from_pixel(120, 200, Rgb([1,2,3])).write_to(&mut buf, image::ImageFormat::Jpeg).unwrap();
        w.start_file("001.jpg", opt).unwrap();
        w.write_all(buf.get_ref()).unwrap();
        w.finish().unwrap();
        let pages = list_pages_with_dims(&zp).unwrap();
        assert_eq!(pages.len(), 1);
        assert_eq!(pages[0].name, "001.jpg");
        assert_eq!((pages[0].w, pages[0].h), (120, 200));
    }
```

- [ ] **Step 2: 运行验证失败**

Run: `cd src-tauri && cargo test --lib list_pages_with_dims`
Expected: 编译失败——未定义。

- [ ] **Step 3: 实现**

在 `reader.rs` 加 `PageInfo` 与 `list_pages_with_dims`（复用 list_pages 取名，再对每条读字节解尺寸）：

```rust
#[derive(Debug, Clone, serde::Serialize)]
pub struct PageInfo { pub name: String, pub w: u32, pub h: u32 }

/// 列页并解每页宽高（解码尺寸而非全图）。
pub fn list_pages_with_dims(zip_path: &Path) -> AppResult<Vec<PageInfo>> {
    let names = list_pages(zip_path)?;
    let mut out = Vec::with_capacity(names.len());
    for name in names {
        let bytes = read_entry(zip_path, &name)?;
        let (w, h) = image::load_from_memory(&bytes)
            .map(|im| (im.width(), im.height()))
            .unwrap_or((0, 0)); // 解析失败按 0,0（前端按竖单页默认处理）
        out.push(PageInfo { name, w, h });
    }
    Ok(out)
}
```

> 说明：为简单起见直接 `load_from_memory` 取尺寸（漫画页 zip 内已解压快）；若后续要优化可换 `image::io::Reader::into_dimensions` 只读头部。本任务用 load_from_memory 保证正确。

在 `comic/mod.rs`：`comic_pages` 改为返回 `Vec<PageInfo>`：

```rust
#[tauri::command]
pub fn comic_pages(path: String) -> AppResult<Vec<reader::PageInfo>> {
    reader::list_pages_with_dims(Path::new(&path))
}
```

删除 `comic_cover` 函数。在 `lib.rs` handler 注册去掉 `comic::comic_cover,`。

在 `src/lib/ipc.ts`：
- 新增 `export interface PageInfo { name: string; w: number; h: number; }` 与 `export interface VolumeInfo { vol_no: number; label: string; zip_path: string; }`。
- `comicPages` 返回类型改 `invoke<PageInfo[]>`。
- 新增 `comicVolumes: (categoryPath, title) => invoke<VolumeInfo[]>("comic_volumes", { categoryPath, title })`。
- 移除 `comicCover`。

- [ ] **Step 4: 运行验证通过 + 编译前端**

Run: `cd src-tauri && cargo test --lib comic && cargo build --lib 2>&1 | grep -E "error" | head`
Expected: 测试全 PASS、无 error。（此时前端 ComicView/ComicReaderView 仍引用旧 comicCover/comicPages 形态，tsc 会报错——留待 Task 6 重写前端；本任务只保证后端 + ipc 自洽。）

- [ ] **Step 5: 提交**

```bash
git add src-tauri/src/comic/reader.rs src-tauri/src/comic/mod.rs src-tauri/src/lib.rs src/lib/ipc.ts
git commit -m "feat(comic): comic_pages 返回 PageInfo(宽高)，新增 comicVolumes ipc，移除 comic_cover

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

### Task 5: 前端 buildSpreads 单双页拼装纯函数

**Files:**
- Create: `src/lib/spreads.ts`
- Create: `src/lib/spreads.test.ts`（若项目有前端测试；否则内联到 spreads.ts 的自测导出并在 Task 6 手工验证）

> 项目当前无前端测试框架（无 vitest 配置）。本任务用**纯函数 + 类型检查**保证：把 buildSpreads 写为纯函数并在 spreads.ts 内用 `if (import.meta.vitest)` 或简单断言难以运行。改为：实现纯函数，逻辑用后端无关的 TS，正确性由 Task 6 集成时人工核对 + 严格类型。**若项目已配置 vitest**（检查 package.json），则写 .test.ts 单测。

- [ ] **Step 1: 检查测试框架**

Run: `grep -E "vitest|jest" package.json`
若有 vitest → 写单测；若无 → 只实现纯函数（Step 3），跳过 Step 2 的运行、正确性靠类型 + Task 6 集成验证。

- [ ] **Step 2: 写测试（仅当有 vitest）**

创建 `src/lib/spreads.test.ts`：

```ts
import { describe, it, expect } from "vitest";
import { buildSpreads } from "./spreads";
const p = (i: number, w: number, h: number) => ({ name: `${i}.jpg`, w, h });
describe("buildSpreads", () => {
  it("封面单页居中", () => {
    const s = buildSpreads([p(0,600,900)]);
    expect(s[0].single).toBe(true);
  });
  it("竖单页从第2页两两配对", () => {
    const s = buildSpreads([p(0,600,900),p(1,600,900),p(2,600,900),p(3,600,900),p(4,600,900)]);
    // 封面单页 + (1,2) + (3,4)
    expect(s.length).toBe(3);
    expect(s[0].single).toBe(true);
    expect(s[1].left?.name).toBe("1.jpg");
    expect(s[1].right?.name).toBe("2.jpg");
  });
  it("宽图单独成对开", () => {
    const s = buildSpreads([p(0,600,900),p(1,1600,900)]);
    expect(s[1].single).toBe(true); // 宽图自身占满
  });
  it("奇数末页落单", () => {
    const s = buildSpreads([p(0,600,900),p(1,600,900),p(2,600,900)]);
    // 封面 + (1,2) → 末页 2 配到 1，无落单；用 4 页测末页落单
    const s2 = buildSpreads([p(0,600,900),p(1,600,900),p(2,600,900),p(3,600,900)]);
    expect(s2[s2.length-1].right).toBeUndefined();
  });
});
```

- [ ] **Step 3: 实现纯函数**

创建 `src/lib/spreads.ts`：

```ts
export interface PageInfo { name: string; w: number; h: number; }
export interface Spread { left?: PageInfo; right?: PageInfo; single: boolean; }

/** 是否横向对开图（宽>高）。宽高为 0（解析失败）按竖单页处理。 */
function isWide(p: PageInfo): boolean {
  return p.w > 0 && p.h > 0 && p.w > p.h;
}

/**
 * 把页序列组装为对开列表（左→右阅读，封面单页居中）：
 * - 第 1 页（封面）单页居中。
 * - 宽图各自成一个 single 对开（本身即对开）。
 * - 连续竖单页从第 2 页起 (2,3)(4,5)… 两两配对；落单末页 single。
 */
export function buildSpreads(pages: PageInfo[]): Spread[] {
  const out: Spread[] = [];
  if (pages.length === 0) return out;
  // 封面单页
  out.push({ right: pages[0], single: true });
  let i = 1;
  while (i < pages.length) {
    const cur = pages[i];
    if (isWide(cur)) {
      out.push({ right: cur, single: true });
      i += 1;
      continue;
    }
    const next = pages[i + 1];
    if (next && !isWide(next)) {
      out.push({ left: cur, right: next, single: false });
      i += 2;
    } else {
      out.push({ left: cur, single: false }); // 末页落单或下一张是宽图
      i += 1;
    }
  }
  return out;
}
```

- [ ] **Step 4: 验证**

若有 vitest：`npx vitest run src/lib/spreads.test.ts`（全 PASS）。
否则：`npx tsc --noEmit`（spreads.ts 无类型错误；正确性 Task 6 集成核对）。

- [ ] **Step 5: 提交**

```bash
git add src/lib/spreads.ts
# 若写了测试：git add src/lib/spreads.test.ts
git commit -m "feat(comic): 单双页拼对开纯函数 buildSpreads(封面单页/宽图对开/竖页配对)

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

### Task 6: 前端漫画三级视图（列表 → 卷列表 → 3D 双页翻书）

**Files:**
- Modify: `src/views/ComicView.ts`
- Create: `src/views/ComicVolumesView.ts`
- Modify: `src/views/ComicReaderView.ts`（重写）
- Modify: 漫画路由处（找到调用 ComicView/ComicReaderView 的地方，通常 `src/main.ts` 或路由文件）
- Modify: `src/styles/theme.css`（3D 翻页样式）

- [ ] **Step 1: ComicView 改造**

`src/views/ComicView.ts`：去掉 comicCover 调用，直接用 `it.cover_path`（转 asset URL——看视频封面怎么显示的，用同样的 `convertFileSrc` 或现有封面显示方式）。点击 `onOpen(it)` 改为进入卷列表。查 ComicView 的调用方（路由）当前 onOpen 做什么，改为导航到 ComicVolumesView。

参考 VideoView 如何用 cover_path 显示封面（`grep -n "cover_path\|convertFileSrc\|asset" src/views/*.ts src/lib/*.ts`），comic 用相同方式。

- [ ] **Step 2: 新增 ComicVolumesView**

创建 `src/views/ComicVolumesView.ts`：

```ts
import { api } from "../lib/ipc";
import type { MediaItem, VolumeInfo } from "../lib/ipc";
import { esc } from "../lib/escape";
import { icon } from "../lib/icons";

export async function ComicVolumesView(
  it: MediaItem,
  onOpenVol: (vol: VolumeInfo, mangaTitle: string) => void,
  onBack: () => void,
): Promise<HTMLElement> {
  const el = document.createElement("div");
  el.className = "view-enter";
  const vols = await api.comicVolumes(it.category_path, it.title);
  el.innerHTML = `
    <div class="reader-bar glass">
      <button class="icon-text back">${icon("arrowLeft", 16)}<span>返回</span></button>
      <span class="title">${esc(it.title)}</span>
      <span style="color:var(--text-dim);font-size:12px">共 ${vols.length} 卷</span>
    </div>
    <div class="poster-grid vol-grid"></div>`;
  el.querySelector<HTMLButtonElement>(".back")!.onclick = onBack;
  const grid = el.querySelector(".vol-grid")!;
  vols.forEach((v) => {
    const card = document.createElement("div");
    card.className = "poster vol-card";
    card.innerHTML = `<div class="poster-img vol-thumb">${icon("book", 28)}</div><div class="poster-title">${esc(v.label)}</div>`;
    card.onclick = () => onOpenVol(v, it.title);
    grid.appendChild(card);
  });
  return el;
}
```

（`book` 图标若 icons.ts 无，用现有相近图标或加一个；先 grep icons.ts。）

- [ ] **Step 3: 重写 ComicReaderView 为 3D 双页翻书**

`src/views/ComicReaderView.ts` 重写：入参改 `(vol: VolumeInfo, mangaTitle: string, onExit)`；`comicPages(vol.zip_path)` 拿 PageInfo[] → `buildSpreads` → 渲染对开翻页。核心 HTML/JS：

```ts
import { api } from "../lib/ipc";
import type { VolumeInfo } from "../lib/ipc";
import { esc } from "../lib/escape";
import { icon } from "../lib/icons";
import { buildSpreads, type Spread } from "../lib/spreads";

export async function ComicReaderView(vol: VolumeInfo, mangaTitle: string, onExit: () => void): Promise<HTMLElement> {
  const el = document.createElement("div");
  el.className = "view-enter comic-reader";
  const pages = await api.comicPages(vol.zip_path);
  const spreads = buildSpreads(pages);
  let idx = 0;
  const cache = new Map<string, string>();
  const load = async (name: string) => {
    if (cache.has(name)) return cache.get(name)!;
    const url = await api.comicPage(vol.zip_path, name);
    cache.set(name, url);
    return url;
  };
  el.innerHTML = `
    <div class="reader-bar glass">
      <button class="icon-text back">${icon("arrowLeft", 16)}<span>返回</span></button>
      <span class="title">${esc(mangaTitle)} · ${esc(vol.label)}</span>
      <span class="pager"></span>
    </div>
    <div class="book-stage"><div class="book" id="book"></div></div>`;
  el.querySelector<HTMLButtonElement>(".back")!.onclick = onExit;
  const book = el.querySelector<HTMLElement>("#book")!;
  const pager = el.querySelector<HTMLElement>(".pager")!;

  const render = async (turningDir: "next" | "prev" | null = null) => {
    const sp = spreads[idx];
    const leftUrl = sp.left ? await load(sp.left.name) : "";
    const rightUrl = sp.right ? await load(sp.right.name) : "";
    book.className = "book" + (sp.single ? " single" : "");
    book.innerHTML =
      (sp.left ? `<div class="leaf left"><img src="${leftUrl}"/></div>` : "") +
      (sp.right ? `<div class="leaf right${turningDir === "next" ? " turn-in" : ""}"><img src="${rightUrl}"/></div>` : "");
    pager.textContent = `${idx + 1} / ${spreads.length}`;
  };
  const go = async (d: number) => {
    const ni = idx + d;
    if (ni < 0 || ni >= spreads.length) return;
    idx = ni;
    await render(d > 0 ? "next" : "prev");
  };
  el.addEventListener("click", (e) => {
    const x = (e as MouseEvent).clientX;
    if (x < window.innerWidth / 2) go(-1); else go(1);
  });
  const onKey = (e: KeyboardEvent) => {
    if (e.key === "ArrowLeft") go(-1);
    else if (e.key === "ArrowRight") go(1);
    else if (e.key === "Escape") onExit();
  };
  window.addEventListener("keydown", onKey);
  // 退出时移除监听：在 onExit 包一层——由路由负责卸载视图时清理；此处在 el 上存引用
  (el as any)._cleanup = () => window.removeEventListener("keydown", onKey);
  await render();
  return el;
}
```

- [ ] **Step 4: 3D 翻页 CSS**

在 `src/styles/theme.css` 追加：

```css
/* 漫画 3D 双页翻书阅读器 */
.comic-reader{height:100%;display:flex;flex-direction:column;gap:10px}
.book-stage{flex:1;min-height:0;display:flex;align-items:center;justify-content:center;background:#0a0d16;border-radius:12px;perspective:2400px;overflow:hidden}
.book{display:flex;align-items:center;justify-content:center;height:calc(100vh - 160px);transform-style:preserve-3d}
.book .leaf{height:100%;background:#000}
.book .leaf img{height:100%;display:block;box-shadow:0 8px 30px rgba(0,0,0,.6)}
.book.single .leaf img{max-width:min(90vw,700px);width:auto}
.book .leaf.right{transform-origin:left center}
.book .leaf.turn-in{animation:leafTurn .42s ease-out}
@keyframes leafTurn{from{transform:rotateY(-95deg)}to{transform:rotateY(0)}}
.vol-thumb{display:flex;align-items:center;justify-content:center;color:var(--accent)}
```

- [ ] **Step 5: 接线路由 + 全量类型检查 + 提交**

找到漫画路由（grep `ComicView\|ComicReaderView` 在 main.ts/router），改为三级导航：ComicView.onOpen(it) → ComicVolumesView(it, onOpenVol, onBack) → onOpenVol(vol,title) → ComicReaderView(vol, title, onExit)。视图切换时若旧视图有 `_cleanup` 则调用（清理 keydown）。

Run: `npx tsc --noEmit`
Expected: 无输出（通过）。修到干净（ComicView/ReaderView 的旧 comicCover/单 zip 形态全部改掉）。

```bash
git add src/views/ComicView.ts src/views/ComicVolumesView.ts src/views/ComicReaderView.ts src/styles/theme.css <路由文件>
git commit -m "feat(comic): 漫画三级视图(列表→卷列表→3D双页翻书阅读器)

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

## 自查

- **规格覆盖**：删 category 列 + 迁移（Task 1）、scan_comics 漫画条目 + 封面落盘（Task 2）、comic_volumes 列卷（Task 3）、comic_pages 带宽高 + 移除 comic_cover + ipc（Task 4）、buildSpreads 单双页拼装（Task 5）、三级视图 + 3D 翻书（Task 6）——规格各条均有对应任务。
- **占位符扫描**：无 TBD/TODO；每步给出完整代码。Task 5 的测试框架分支（有/无 vitest）是运行环境适配，非占位。
- **类型/命名一致性**：`PageInfo{name,w,h}`（reader.rs / ipc.ts / spreads.ts）、`VolumeInfo{vol_no,label,zip_path}`（comic/mod.rs / ipc.ts / views）、`Spread{left,right,single}`（spreads.ts / ReaderView）字段一致；`comic_volumes`/`comic_pages` 命令名与 ipc invoke 一致；scan_comics 新签名 `(root, covers_dir)` 在 Task 2 定义、lib.rs 调用一致。
- **执行依赖**：Task 顺序执行（1→6）；Task 4 后前端 tsc 暂时报错（旧视图未改），Task 6 修复——这是刻意的分阶段，Task 4 只验证后端 + cargo，Task 6 验证 tsc。
- **待实施时确认点**：`MediaItem` 是否含 category 字段（Task 1 Step 3 需看 model.rs）；scan_root 是否有 app 句柄（Task 2 Step 3 需加 app 参数）；前端路由文件位置（Task 6 Step 5 需 grep）；icons.ts 是否有 book 图标（Task 6 Step 2）；package.json 是否有 vitest（Task 5 Step 1）。这些在各任务内用 grep/读文件确认后据实适配。
