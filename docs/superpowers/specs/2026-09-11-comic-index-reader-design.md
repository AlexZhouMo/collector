# 漫画自动索引 + 多卷阅读（3D 双页翻书）设计文档

**日期**：2026-09-11
**状态**：已确认

## 背景与动机

当前漫画：`scan_comics` 以**每个 zip 为一条目**（旧模型）；comic 表含 `category` 列；封面运行时从 zip 首图取 data URL（不落盘）；阅读器按单个 zip 阅读，有翻页/长条两种模式但无翻书效果。

归档功能已把漫画规整为 `<漫画目录>/Vol_XX.zip`。本功能据此重建漫画的**索引→浏览→阅读**：以每部漫画为条目、多卷聚合、封面落盘、3D 双页翻书阅读。

## 数据层

### comic 表 schema 变更

删除 `category` 列。最终结构：

```sql
CREATE TABLE comic (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    category_path TEXT NOT NULL,  -- 只存分类目录，不含漫画名，如 "热血" 或 "热血/少年"
    title TEXT NOT NULL,          -- 漫画名，如 "灌篮高手"
    description TEXT,
    cover_path TEXT,
    UNIQUE(category_path, title)
);
```

### 迁移

真机已有旧 comic 表（含 category）。因 comic 靠重扫重建、旧数据可丢，迁移用 SQLite 标准重建：
新增迁移语句——若 comic 表含 category 列则重建为新结构（`CREATE TABLE comic_new ... ; DROP TABLE comic; ALTER TABLE comic_new RENAME TO comic;`），随后由扫描重新填充。id 从 1（AUTOINCREMENT，replace_items 已重置 sqlite_sequence）。

## 后端扫描（`library/scanner.rs::scan_comics` 重写）

- 复用归档的卷识别思路：找**含 `Vol_XX.zip` 的目录** = 一部漫画（`Vol_\d+\.zip` 正则匹配，至少一个即算）。
- `title` = 漫画目录名；`category_path` = 从漫画根到该漫画目录**父级**的相对路径（分类，不含漫画名）；漫画目录直接在根下时 category_path 为空串。
- 封面：取该漫画**卷号最小的 zip**（Vol_01）内**首图**字节 → `image_proc::to_cover`（居中裁 2:3 → 500×750 → JPEG q85）→ `image_proc::save_cover(covers_dir, bytes, "comic_")`（内容 hash 命名，落盘）→ cover_path 入库。首卷无图/损坏 → cover_path 留空。
- 入库走 `replace_items(db, Comic, items)`（清空重建、id 从 1）。

`ScannedItem` 已有 category/category_path/title/cover_path/description 字段——comic 的 category 字段填空串或复用（因表已无 category 列，insert 时不写 category）。**需调整** `library/mod.rs` 里 comic 的 insert/list SQL 去掉 category 列。

## 后端阅读命令

### `comic_volumes(category_path, title) -> Vec<VolumeInfo>`

拼出漫画目录（comic_root + category_path + title），列出其下所有 `Vol_XX.zip`，按卷号升序返回：

```rust
pub struct VolumeInfo {
    pub vol_no: u32,       // 卷号
    pub label: String,     // "第 01 卷"
    pub zip_path: String,  // 绝对路径，供 comic_pages/comic_page
}
```

### `comic_pages(zip_path) -> Vec<PageInfo>`（改造）

返回每页名 + 宽高（读图片头部解尺寸，不全解码——用 `image::io::Reader::with_guessed_format().into_dimensions()`）：

```rust
pub struct PageInfo { pub name: String, pub w: u32, pub h: u32 }
```

### `comic_page(zip_path, entry) -> String`（不变）

返回该页 data URL。

### 移除

旧 `comic_cover`（运行时首图）移除，改用入库的 cover_path。

## 前端

### ComicView（改造）
读 cover_path 直接显示（不再运行时 comicCover）；点击进入卷列表 `ComicVolumesView`。

### ComicVolumesView（新增）
调 `comic_volumes(category_path, title)` 列出"第 XX 卷"卡片（可带各卷首图缩略，YAGNI：首版可先纯文字卡 + 卷号）；点某卷进入 `ComicReaderView(zip_path, label)`。顶部返回漫画列表。

### ComicReaderView（重写为 3D 双页翻书）
1. 调 `comic_pages(zip_path)` 拿各页 `{name,w,h}`。
2. 用纯函数 `buildSpreads(pages)` 组装对开序列。
3. 渲染 3D 翻页：左右并排双页对开，CSS `perspective` + `rotateY` 翻页动画（绕书脊）；左右方向键 / 点击左右半屏翻页；顶部返回 + "第 XX 卷 · a-b / total"。
4. 页图用 `comic_page` 懒加载 + 缓存。

### 单双页拼装（前端纯函数 `buildSpreads`，可测）

输入 `PageInfo[]`（按 zip 内顺序）→ 输出 `Spread[]`：

```ts
interface Spread { left?: PageInfo; right?: PageInfo; single: boolean }
```

规则（左→右阅读、封面单页）：
- 第 1 页（封面）→ 单页居中（`{right: p0, single:true}` 或专门 single 字段）。
- 从第 2 页起遍历：宽>高的页 → 单独成一个对开（它本身是对开图，single 展示占满）；连续的竖单页(高≥宽) (2,3)(4,5)… 两两配对为 `{left, right}`；落单的末页 → 单页。
- 输出供阅读器逐"对开"翻页。

## 错误处理

- 漫画根未配置：scan_comics 返回空。
- 卷 zip 损坏/空：comic_volumes 跳过该卷或标记；阅读器该卷无页时提示。
- 封面提取失败：cover_path 空，列表占位。
- 页尺寸解析失败：该页按竖单页(高>宽)默认处理，不崩。

## 测试

### 后端
- `scan_comics`：以漫画为条目（含 Vol_XX.zip 的目录）、title=目录名、category_path 不含漫画名、封面落盘 cover_path 非空。
- `comic_volumes`：列出并按卷号升序、label 格式"第 01 卷"。
- `comic_pages`：返回每页宽高正确。
- schema：迁移后 comic 无 category 列、UNIQUE(category_path,title) 在、id 从 1。

### 前端
- `buildSpreads` 纯函数单测：封面单页；宽图单独对开；竖页 (2,3)(4,5) 配对；奇数末页落单；空列表。

## 影响与风险

- comic 表结构变更 + 迁移（旧数据重扫重建）。
- `scan_comics` 重写、`library/mod.rs` comic insert/list 去 category 列。
- 新增 `comic_volumes` 命令、`comic_pages` 改返回 PageInfo、移除 `comic_cover`。
- 前端 ComicView 改造 + 新增 ComicVolumesView + 重写 ComicReaderView（3D 翻书）；ipc 扩展。
- **YAGNI**：不做右→左（日漫）阅读方向、不做长条模式（本次聚焦双页翻书）、不做阅读进度记忆、卷列表首版可不带缩略图。
