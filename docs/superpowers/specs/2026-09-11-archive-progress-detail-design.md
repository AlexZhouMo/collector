# 归档进度三级细节显示 设计文档

**日期**：2026-09-11
**状态**：已确认

## 背景与动机

漫画自动归档当前进度为"已归档 N/总卷数"（卷粒度），进度条按卷推进——卷少时跳变大、不平滑，
且看不到当前处理到哪部漫画、哪一卷、多少图片。

**目标**：进度条**按图片总数**平滑推进；文本显示**当前正在处理的漫画名 + 卷名 + 图片总进度**
（形如"正在：海贼王 Vol_03｜图片 640/2400"）。

## 进度模型

### 预统计

`archive_comics` 开始时先扫描所有卷，算出**总图片数** `total_images`（各卷图片数之和）、
总卷数、总漫画数（`plan_volumes` 分组后 manga 去重数）。

### 进度回调结构

替换现有 `progress: impl FnMut(usize, usize)`，改为传结构体引用：

```rust
#[derive(Debug, Clone, serde::Serialize)]
pub struct ArchiveProgress {
    pub manga: String,        // 当前处理的漫画名
    pub vol: String,          // 当前卷名，如 "Vol_03"
    pub done_images: usize,   // 已处理图片数（跨卷累计）
    pub total_images: usize,  // 总图片数
    pub total_manga: usize,   // 总漫画数（摘要用）
    pub total_vols: usize,    // 总卷数（摘要用）
}
```

回调签名：`progress: impl FnMut(&ArchiveProgress)`。

### 图片级实时上报

`pack_images_to_zip` 当前并行编码内部不回调。新增每完成一张图的回调参数
`on_image: impl Fn() + Sync`：并行 `map` 内每编码完一张调用一次（用 `AtomicUsize`
在上层累加跨卷已处理图片数）。回调必须 `Sync`（在 rayon 并行闭包内调用）。

## 组件改动

### `comic_pack.rs::pack_images_to_zip`

新增参数 `on_image: impl Fn() + Sync`：

```rust
pub fn pack_images_to_zip(
    images: &[std::path::PathBuf],
    out_zip: &Path,
    namer: impl Fn(usize) -> String,
    on_image: impl Fn() + Sync,
) -> AppResult<usize>
```

在并行编码 `map` 内，每张 `encode_one_jpeg` 完成后调用 `on_image()`。其余（sort_by_key、
串行写 zip、错误传播）不变。调用方 `pack_comic_dir` 传 `|| {}`；两个并行测试
（`pack_images_to_zip_uses_namer`、`parallel_encoding_preserves_page_order`、
`parallel_propagates_decode_error`）相应加 `|| {}`。

### `comic_archive.rs::archive_comics`

- 签名改 `progress: impl FnMut(&ArchiveProgress)`。
- 预扫：`plan_volumes` 后，对每个 plan 数其卷内图片数，求和得 `total_images`；`total_vols`
  = plans.len()；`total_manga` = plans 中去重 manga 数。
- 维护累计 `done_images`（`usize`，主线程侧）。遍历每卷：
  - 进入卷前回调一次（更新 manga/vol，done_images 不变）。
  - **正常卷**：调 `pack_images_to_zip(..., on_image)`，`on_image` 用 `AtomicUsize` 累加，
    并触发 `progress`（携带最新 done_images）。编码完该卷后，把 done_images 对齐到"该卷起点
    + 该卷图片数"（防并行计数与实际张数的细微偏差）。
  - **跳过卷（已存在）**：把该卷图片数一次性加到 done_images，回调一次。
  - **失败卷**：把该卷剩余未计图片数补加到 done_images（进度条不卡），回调一次，记"失败(...)"。
- 结束时保证 `done_images == total_images`。

### command `archive_comics_cmd`

emit payload 扩展为：
```json
{ "manga", "vol", "done_images", "total_images", "total_manga", "total_vols" }
```
**节流在此层做**：闭包捕获 `Instant`/上次计数，仅当"距上次 emit ≥ 80ms 或 done_images 达到
total_images（最后一张强制）"时才 `app.emit`，避免并行编码产生的高频事件淹没 WKWebView。
事件名 `comic-archive-progress` 复用。

### 前端 `NormalizeView.ts`

- listen 新 payload 类型 `{ manga, vol, done_images, total_images, total_manga, total_vols }`。
- 进度条：`pct = total_images ? round(done_images/total_images*100) : 0`。
- 文本：`正在：${manga} ${vol}｜图片 ${done_images}/${total_images}`。
- 完成后进度条置 100%，结果列表逻辑不变（按 manga 分组、成功/跳过/失败三色标）。

## 错误处理

- 单卷失败仍记"失败(...)"、继续下一卷（不变）；失败卷图片数补加到 done_images，进度条不卡。
- 空目录 / 无卷：total_images=0，进度条 0%，文本可显示"无可归档内容"，正常返回空报告。

## 测试

### `comic_archive.rs`

- **预扫总图片数**：多卷各含不同张数，断言 total_images = 各卷之和、total_vols、total_manga 正确。
- **进度单调且末值对齐**：用一个 `Vec<ArchiveProgress>` 收集所有回调，断言 done_images 单调不减、
  末次 == total_images。
- **跳过卷补加**：预置某卷 Vol_01.zip 存在，断言其图片数被一次性计入 done_images。
- **失败卷补加**：某卷含损坏图，断言失败后 done_images 仍推进、最终 == total_images。
- **manga/vol 切换**：多部多卷，断言回调中 manga/vol 随处理位置正确切换。

### `comic_pack.rs`

- 现有测试加 `|| {}` 后保持全绿。
- 新增：`on_image` 被调用次数 == 图片数（用 `AtomicUsize` 在测试中计数）。

## 影响与风险

- `pack_images_to_zip` 与 `archive_comics` 签名变化——调用方（`pack_comic_dir`、command、
  测试）需同步（均为受控内部调用）。
- 进度事件 payload 字段扩展，前端同步；事件名不变。
- 无 DB / 其他接口改动。
- **YAGNI**：不做暂停/取消、不做每部漫画单独进度条、不持久化进度、不显示预估剩余时间。
