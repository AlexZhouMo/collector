# 加速漫画归档 实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 把 `pack_images_to_zip` 卷内图片的解码+JPEG 编码用 rayon 并行化、JPEG 质量固定 85，zip 写入保持串行按页序，签名不变。

**Architecture:** 两阶段——rayon `par_iter` 并行解码+编码为 `(页序号, AppResult<Vec<u8>>)`，再按页序号排序后串行写入单个 zip（首个错误即整卷返回 Err）。`archive_comics`/`pack_comic_dir` 调用方零改动。

**Tech Stack:** Rust、rayon、image（JpegEncoder new_with_quality）、zip。

---

### Task 1: rayon 并行化 pack_images_to_zip + 质量 85

**Files:**
- Modify: `src-tauri/Cargo.toml`（加 `rayon = "1"`）
- Modify: `src-tauri/src/normalize/comic_pack.rs`

- [ ] **Step 1: 加依赖**

在 `src-tauri/Cargo.toml` 的 `[dependencies]` 区，`image = "0.25"` 一行后新增：

```toml
rayon = "1"
```

- [ ] **Step 2: 确认现有测试基线（绿）**

Run: `cd src-tauri && cargo test --lib comic_pack && cargo test --lib comic_archive`
Expected: 全 PASS（改造前基线：pack_images_to_zip_uses_namer、packs_images_into_renamed_jpg_zip、natural_sort_orders_unpadded_numbers、archive_* 均绿）。这些是并行化后必须保持不变的正确性锚点。

- [ ] **Step 3: 并行化改造**

在 `comic_pack.rs` 顶部 import 区新增：

```rust
use rayon::prelude::*;
```

新增质量常量（放在 `use` 之后、`NatChunk` 之前或文件靠上位置）：

```rust
/// 归档打包时统一的 JPEG 编码质量（1-100）。85 在漫画线稿/网点下观感接近无损、体积适中。
const JPEG_QUALITY: u8 = 85;
```

把 `pack_images_to_zip` 函数体（从 `let f = File::create` 到 `Ok(n)`，即空判断与父目录创建之后的部分）替换为两阶段实现：

```rust
pub fn pack_images_to_zip(
    images: &[std::path::PathBuf],
    out_zip: &Path,
    namer: impl Fn(usize) -> String,
) -> AppResult<usize> {
    if images.is_empty() {
        return Err(AppError::Invalid("no images".into()));
    }
    if let Some(parent) = out_zip.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    // 阶段一：并行解码 + JPEG 编码（CPU 密集、各图独立），保留原始页序号。
    let mut encoded: Vec<(usize, AppResult<Vec<u8>>)> = images
        .par_iter()
        .enumerate()
        .map(|(i, path)| (i, encode_one_jpeg(path)))
        .collect();
    // 阶段二：按页序号排序后串行写入单个 zip；首个错误即整卷返回。
    encoded.sort_by_key(|(i, _)| *i);
    let f = File::create(out_zip)?;
    let mut zw = zip::ZipWriter::new(f);
    let opt = SimpleFileOptions::default();
    let mut n = 0;
    for (i, res) in encoded {
        let bytes = res?;
        zw.start_file(namer(i), opt)
            .map_err(|e| AppError::Other(e.to_string()))?;
        zw.write_all(&bytes)?;
        n += 1;
    }
    zw.finish().map_err(|e| AppError::Other(e.to_string()))?;
    Ok(n)
}

/// 解码单张图片并以固定质量编码为 JPEG 字节。
fn encode_one_jpeg(path: &Path) -> AppResult<Vec<u8>> {
    let img = image::open(path).map_err(|e| AppError::Other(e.to_string()))?;
    let rgb = img.to_rgb8();
    let mut buf = std::io::Cursor::new(Vec::new());
    let mut enc = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut buf, JPEG_QUALITY);
    enc.encode_image(&rgb)
        .map_err(|e| AppError::Other(e.to_string()))?;
    Ok(buf.into_inner())
}
```

说明：`encode_image` 需要 `use image::ImageEncoder;` 才能调用——若编译报 `no method named encode_image`，在 import 区加 `use image::ImageEncoder;`。`buf.into_inner()` 取出 `Vec<u8>`（Cursor::into_inner）。

- [ ] **Step 4: 运行测试验证通过（正确性不变）**

Run: `cd src-tauri && cargo test --lib comic_pack && cargo test --lib comic_archive`
Expected: 全 PASS——所有现有测试（页序、命名、内容、失败隔离）在并行化后不变。特别是 `natural_sort_orders_unpadded_numbers`（用像素灰度值验证页序）与 `archive_failure_does_not_abort_batch`（损坏图触发失败）证明并行不乱序、错误仍传播。

- [ ] **Step 5: 提交**

```bash
git add src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/src/normalize/comic_pack.rs
git commit -m "perf(comic): pack_images_to_zip 改 rayon 并行编码，JPEG 质量固定 85

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

### Task 2: 新增并行正确性回归测试

**Files:**
- Modify: `src-tauri/src/normalize/comic_pack.rs`（`mod tests`）

- [ ] **Step 1: 写测试**

在 `comic_pack.rs` 的 `mod tests` 内新增两个测试：

```rust
    #[test]
    fn parallel_encoding_preserves_page_order() {
        // 用像素灰度值给每张图编码源序号，验证并行编码后 zip 内页序严格对应输入顺序。
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("src");
        std::fs::create_dir_all(&dir).unwrap();
        // 输入顺序：v=10,40,70,100,130,160,190,220（8 张，够触发多核并行）
        let vals: Vec<u8> = (0..8).map(|k| 10 + k * 30).collect();
        let imgs: Vec<_> = vals.iter().enumerate().map(|(k, &v)| {
            let p = dir.join(format!("img{k}.png"));
            RgbImage::from_pixel(4, 4, Rgb([v, v, v])).save(&p).unwrap();
            p
        }).collect();
        let out = tmp.path().join("out.zip");
        let n = pack_images_to_zip(&imgs, &out, |i| format!("p_{:03}.jpg", i + 1)).unwrap();
        assert_eq!(n, 8);
        // 依次读回 p_001..p_008，解码取像素灰度，应单调递增（顺序未被并行打乱）
        let mut ar = zip::ZipArchive::new(File::open(&out).unwrap()).unwrap();
        let mut got = Vec::new();
        for k in 1..=8 {
            let mut entry = ar.by_name(&format!("p_{:03}.jpg", k)).unwrap();
            let mut bytes = Vec::new();
            std::io::Read::read_to_end(&mut entry, &mut bytes).unwrap();
            let img = image::load_from_memory(&bytes).unwrap().to_rgb8();
            got.push(img.get_pixel(0, 0)[0] as i32);
        }
        for w in got.windows(2) {
            assert!(w[0] < w[1], "页序被并行打乱: {got:?}");
        }
    }

    #[test]
    fn parallel_propagates_decode_error() {
        // 列表中混入一张损坏"图片"（非图片字节），整卷应返回 Err。
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("src");
        std::fs::create_dir_all(&dir).unwrap();
        let good = dir.join("a.png");
        RgbImage::from_pixel(4, 4, Rgb([10, 20, 30])).save(&good).unwrap();
        let bad = dir.join("b.png");
        std::fs::write(&bad, b"not an image").unwrap();
        let out = tmp.path().join("out.zip");
        let r = pack_images_to_zip(&[good, bad], &out, |i| format!("p_{:03}.jpg", i + 1));
        assert!(r.is_err(), "损坏图片应使整卷返回 Err");
    }
```

（`RgbImage`/`Rgb`/`File` 在该 `mod tests` 现有 `use` 中应已引入——`natural_sort_orders_unpadded_numbers` 用了 `image::{RgbImage, Rgb}`、`File`。若缺，据实补 `use image::{RgbImage, Rgb};`。）

- [ ] **Step 2: 运行测试验证通过**

Run: `cd src-tauri && cargo test --lib comic_pack`
Expected: 全 PASS（含新增两个）。`parallel_encoding_preserves_page_order` 证明并行不乱序；`parallel_propagates_decode_error` 证明错误传播。

- [ ] **Step 3: 全量测试 + 提交**

Run: `cd src-tauri && cargo test --lib 2>&1 | tail -3`
Expected: 全 PASS。

```bash
git add src-tauri/src/normalize/comic_pack.rs
git commit -m "test(comic): 锚定 pack_images_to_zip 并行不乱序与错误传播

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

## 自查

- **规格覆盖**：rayon 并行编码 + 串行写 zip（Task 1 两阶段实现）、JPEG 质量 85（Task 1 `JPEG_QUALITY` 常量 + `new_with_quality`）、签名不变（Task 1 函数签名逐字保留）、错误传播 + 空列表 Err（Task 1 `res?` + 保留空判断）、并行正确性测试（Task 2 两个新测试）、现有测试保留（Task 1 Step 4）——规格各条均有对应任务。
- **占位符扫描**：无 TBD/TODO；每个代码步骤给出完整代码。
- **类型/命名一致性**：`encode_one_jpeg(path: &Path) -> AppResult<Vec<u8>>` 在 Task 1 定义并被 `pack_images_to_zip` 的 par_iter map 调用；`JPEG_QUALITY: u8` 供 `new_with_quality` 用；`pack_images_to_zip` 签名与调用方（archive_comics/pack_comic_dir）一致不变。
- **依赖**：rayon 新增于 Cargo.toml，`use rayon::prelude::*` 提供 `par_iter`；`image::codecs::jpeg::JpegEncoder` + `ImageEncoder` trait 由既有 image 依赖提供。
