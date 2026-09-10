# 漫画自动归档 设计文档

**日期**：2026-09-11
**状态**：已确认

## 背景与动机

工具箱现有「漫画标准化」面板：让用户**手动**选一个图片目录、填命名前缀、选输出 zip 路径，
调用 `normalize_comic(dir, prefix, out_zip)` 打成一个 zip。它一次只处理一卷、参数全靠手填，
与「影视海报生成」「字幕批量校准」那种"一键处理配置好的根目录 + 进度条 + 结果列表"的体验
不一致。

**目标**：把该面板升级为「漫画自动归档」——一键遍历**设置里配置的漫画根目录**，自动识别每部
漫画的各卷，把每卷图片转 JPG、按规则重命名、打包为 `Vol_XX.zip` 写回漫画目录，并用与另外两个
工具一致的进度条 + 结果列表展示每卷的成功 / 跳过 / 失败。

## 归档逻辑

### 卷的识别（自底向上）

遍历漫画根目录，找出所有**直接包含图片文件的叶子目录**，每个这样的目录为**一卷**。
图片指扩展名 `.jpg/.jpeg/.png/.webp/.bmp`（大小写不敏感）；系统冗余文件（`.DS_Store` 等）
经 `util::junk` 排除，不计入图片。

### 一部漫画的判定

一卷（图片目录）的**父目录**即为一部漫画。据此把所有卷按父目录分组，每组是一部漫画。

- 若图片目录直接位于漫画根下（父目录 = 根），则该目录自身即一部单卷漫画，`Vol_01.zip` 生成在
  漫画根下。

### 卷号编号

- 每部漫画（每个父目录分组）内**独立**从 01 开始编号。
- 组内各卷目录按目录名**自然序**（复用 `natural_key`）排序，依次编号 1、2、3……
- 卷号位数按"这部漫画的总卷数"统一决定：总卷数 < 100 → 2 位（`01`、`02`）；总卷数 ≥ 100 →
  该组全部 3 位（`001`、`002`）。同组不混用位数。

### 页号

- 页号 YYY 固定 3 位（`001` 起）。页数超过 999 极罕见，不特殊处理（`format!("{:03}")`
  自然溢出为 4 位），符合 YAGNI。

### 单卷处理

对每一卷：

1. 列出卷内图片（排除系统冗余文件），按 `natural_key` 自然序排序。
2. 逐张 `image::open` → 转 RGB → 编码为 JPEG。
3. 命名 `{XX}_{YYY}.jpg`（如 `01_001.jpg`；卷 ≥100 组为 `001_001.jpg`）。
4. 打包为 `Vol_{XX}.zip`，写到**父目录（漫画目录）下**，与原图片目录并列。原图片目录保留不动。

### 幂等（跳过已存在）

若目标 `Vol_XX.zip` 已存在，**跳过**该卷、不重新生成，并在结果列表中记录一条**"跳过(已存在)"**
状态（显式可见，不静默忽略）。重跑归档安全。

## 组件

### 后端

**共享核心提取**：`normalize/comic_pack.rs` 现有 `pack_comic_dir` 内"图片列表 → 转 JPEG →
命名 → 打 zip"的逻辑，提取为可复用函数，供新归档模块调用，避免重复。`natural_key` 亦复用。

**新增 `src-tauri/src/normalize/comic_archive.rs`**：

```rust
/// 一卷的归档结果。
pub struct ArchiveReport {
    pub manga: String,   // 漫画名（父目录名；单卷时为该目录名）
    pub vol: String,     // 卷 zip 名，如 "Vol_01"
    pub status: String,  // "成功" / "跳过(已存在)" / "失败(<原因>)"
    pub pages: usize,    // 成功时的页数；跳过/失败为 0
}

/// 遍历漫画根，自动识别并归档所有卷。progress(done, total) 逐卷上报。
pub fn archive_comics(
    root: &Path,
    mut progress: impl FnMut(usize, usize),
) -> Vec<ArchiveReport>;
```

**新增 async command**（`normalize/comic_archive.rs` 内）：

```rust
#[tauri::command]
pub async fn archive_comics(app, db) -> AppResult<Vec<ArchiveReport>>
```

- 从 settings 读 `comic_root`（`get_root("comic")`）；未配置则返回错误。
- `spawn_blocking` 执行 `archive_comics`，通过 `app.emit("comic-archive-progress", {done, total})`
  上报进度（与 `subtitle-progress` 同构）。

**旧 command 处理**：`normalize_comic` command 前端不再调用，移除该 command 与 `NatChunk`/
`natural_key`/打包核心之外不再使用的部分；保留被复用的核心。

### 前端（`src/views/NormalizeView.ts`）

将「漫画标准化」面板重构为「漫画自动归档」，**移动到「字幕批量校准」面板下面**：

- 用 `.setting-card` + `.setting-card-title`，标题「漫画自动归档」，字体与另外两个一致。
- 一行只读文本显示当前漫画目录：调 `getRoot("comic")`；已配置显示路径，未配置显示
  "请先在设置中配置漫画根目录"且「开始归档」按钮禁用。
- 「开始归档」按钮：`.btn-primary.icon-text` + play 图标，风格与另外两个一致。
- 进度条三件套：`#comic-progress`（默认隐藏）+ 进度槽 + `#comic-progress-bar` +
  `#comic-progress-text`，样式、双 rAF、百分比逻辑与字幕校准 / 海报生成完全一致。
- 结果列表：复用 `.sub-summary` / `.sub-file` / `.sub-issue` 等样式结构，展示每部漫画分组、
  每卷一行，状态用色标区分：成功（绿）/ 跳过(已存在)（灰或蓝）/ 失败（橙红）。跳过的卷**必须
  在列表中显式体现**。
- 归档完成后自动 `scanRoot("comic")` 重扫漫画库，使新生成的 zip 立即出现在漫画视图。

### ipc 层（`src/lib/ipc.ts`）

- 新增 `archiveComics: () => invoke<ArchiveReport[]>("archive_comics")`。
- 新增 `ArchiveReport` 接口。
- 移除不再使用的 `normalizeComic`。

## 错误处理

- 单卷失败（图片损坏、转码失败、写盘失败）：只把该卷记为 `status="失败(<原因>)"` 并继续下一卷，
  不中断整批（与字幕校准跳过读取失败同风格）。
- 漫画根未配置：command 直接返回错误，前端 alert 提示并保持按钮可用。
- 目标 zip 已存在：跳过并记"跳过(已存在)"（见幂等）。

## 测试

### `comic_archive.rs` 单元测试

- **多部多卷分组编号**：根下两部漫画，各含多个卷目录，断言各自从 01 独立编号、zip 名正确、
  写在各自父目录下。
- **卷号 3 位**：构造一部含 ≥100 卷的漫画（可用 100 个空图片目录的轻量替身，或直接测编号
  函数），断言该组全部用 3 位。
- **单卷（图片直接在根下）**：漫画根下直接一个图片目录，断言生成 `Vol_01.zip` 于根下、
  manga 名为该目录名。
- **卷内自然序**：卷内 `1.png/2.png/10.png`，断言打包后为 `01_001/01_002/01_003` 且顺序
  对应 1<2<10（复用 comic_pack 的像素灰度编码验证法）。
- **排除系统冗余**：卷内混入 `.DS_Store`、`Thumbs.db`，断言不计入页数、不进 zip。
- **幂等跳过**：目标 `Vol_01.zip` 预先存在，断言该卷 report.status 含"跳过"、不覆盖原 zip。
- **单卷失败不中断**：一卷含损坏"图片"（非图片字节），断言该卷 status 为"失败"、其他卷仍
  正常归档。

### 复用核心回归

保留并适配 `comic_pack.rs` 现有测试（`packs_images_into_renamed_jpg_zip`、
`natural_sort_orders_unpadded_numbers`），确保提取的共享函数不回归。

## 影响与风险

- **交互变化**：面板从"手选单目录/前缀/输出"变为"一键归档配置好的漫画根"，与另两个工具统一。
  旧 `normalize_comic` 前端入口移除。
- **写盘副作用**：在漫画目录下新建 `Vol_XX.zip`（不动原图）；幂等跳过保证重跑安全。
- **YAGNI**：不做卷号/页号位数的可配置、不做删除原图选项、不做增量"只归档新卷"（幂等跳过已
  覆盖重跑场景）、页数超 999 不特殊处理。
