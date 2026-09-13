# 加速漫画归档 设计文档

**日期**：2026-09-11
**状态**：已确认

## 背景与动机

漫画自动归档（`archive_comics`）逐卷调用 `pack_images_to_zip`，后者对卷内图片**完全串行**处理：
每张 `image::open`（解码）→ `to_rgb8` → `write_to(Jpeg)`（编码）→ 写入 zip，一张接一张。CPU
密集的解码+编码占绝大部分耗时，却只用单核；zip 写入本身很快，但必须串行（单文件句柄）。

**目标**：把卷内图片的「解码→转 RGB→JPEG 编码」改为用 `rayon` 并行吃满多核；同时把 JPEG
质量从 image 默认（75）改为固定 **85**（漫画线稿/网点下观感更佳）。zip 写入保持串行、页序不变。

## 瓶颈与切分

- **并行**：解码 + 编码（CPU 密集、各图独立）——收益最大。
- **串行**：zip 写入（单文件句柄、必须顺序），但耗时占比小。

因此改造为「卷内图片并行编码 → 按页序串行写 zip」。卷间仍串行（符合选定的"卡内图片并行"
粒度），不改变归档的进度上报与失败隔离。

## 组件改动

### `Cargo.toml`
新增依赖：`rayon = "1"`。

### `comic_pack.rs::pack_images_to_zip`

签名与返回值**不变**：`pub fn pack_images_to_zip(images: &[PathBuf], out_zip: &Path,
namer: impl Fn(usize)->String) -> AppResult<usize>`。调用方（`archive_comics`、`pack_comic_dir`）
零改动。

改造为两阶段：

1. **并行编码阶段**（rayon 默认线程池 = CPU 核数）：
   - `images.par_iter().enumerate()` 对每张：`image::open` → `to_rgb8` →
     用 `image::codecs::jpeg::JpegEncoder::new_with_quality(&mut buf, JPEG_QUALITY)`
     编码为 JPEG 字节。
   - 产出 `Vec<(usize, AppResult<Vec<u8>>)>`（保留原始页序号 i）。
   - `namer` 是 `impl Fn`，在并行闭包内只读调用安全（不在并行段修改共享状态）。

2. **串行写入阶段**：
   - 按页序号 i 升序排序结果。
   - 逐条：若该项为 Err，整卷 `return Err(...)`（首个错误即返回）；否则
     `zw.start_file(namer(i), opt)` + `write_all(&bytes)`。
   - `zw.finish()`，返回写入页数。

### 质量常量
在 `comic_pack.rs` 定义 `const JPEG_QUALITY: u8 = 85;`，替换原隐式默认质量。

## 数据流

`archive_comics`（卷粒度串行、进度按卷上报，不变）
  → 每卷 `pack_images_to_zip`
    → 卷内图片 **rayon 并行编码**（各图独立）
    → 按页序 **串行写单个 zip**

卷间串行、进度条、失败隔离逻辑均不受影响。

## 错误处理

- 并行阶段任一张 `image::open` 或编码失败 → 该项为 Err → 串行阶段遇到首个 Err 时整卷
  `pack_images_to_zip` 返回 Err → `archive_comics` 记该卷 "失败(...)"、继续下一卷（现有行为不变）。
- 空图片列表仍返回 `Err(AppError::Invalid("no images"))`。

## 内存

rayon 默认池并发数 = CPU 核数，内存峰值 ≈ 核数 × 单图解码后 RGB 尺寸 + 编码缓冲。普通漫画
页（几 MB 级）在常见多核机器上峰值可控，不额外限流（YAGNI：不引入并发数上限配置）。

## 测试

- **保留并必须仍全绿**：`pack_images_to_zip_uses_namer`、`packs_images_into_renamed_jpg_zip`、
  `natural_sort_orders_unpadded_numbers`、`archive_generates_vol_zip_and_reports`、
  `archive_skips_existing_zip`、`archive_failure_does_not_abort_batch`——它们验证页序、命名、
  内容正确性与失败隔离，并行化后必须不变（证明并行不乱序、不丢页、错误仍传播）。
- **新增**：`parallel_encoding_preserves_page_order`——多张图（用像素灰度编码源序号，
  复用 natural_sort 测试的手法）并行打包后，zip 内 `NN_001.jpg`/`NN_002.jpg`… 顺序严格对应
  输入顺序，锚定"并行不乱序"。
- **新增**：`parallel_propagates_decode_error`——图片列表中混入一张损坏"图片"（非图片字节），
  断言整卷返回 Err，锚定并行下的错误传播。

## 影响与风险

- **输出变化**：JPEG 质量 75→85，新归档的 `Vol_XX.zip` 内图片字节与体积改变（更清晰、略大）。
  已归档卷因幂等跳过不受影响，只影响新归档。
- 无接口 / DB / 前端改动；`pack_images_to_zip` 签名不变，调用方无需改。
- **YAGNI**：不做并发数上限配置、不做"已是 jpg 则原样拷贝"的跳过转码（保持"统一转 JPEG"的
  既有语义）、不做卷间并行。
