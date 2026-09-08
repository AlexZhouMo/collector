//! 海报图片处理：解码→居中裁剪缩放 500×750→JPEG q85 编码/存盘。

use crate::error::{AppError, AppResult};
use image::imageops::FilterType;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::io::Cursor;
use std::path::Path;

const W: u32 = 500;
const H: u32 = 750;
const JPEG_Q: u8 = 85;

/// 把 DynamicImage 居中裁剪到 2:3 → 缩放到 500×750 → 编码 JPEG q85。
/// to_cover 与 crop_to_cover 共用此函数，保证输出格式完全一致。
fn finalize(img: image::DynamicImage) -> AppResult<Vec<u8>> {
    let (iw, ih) = (img.width(), img.height());
    let target_ratio = W as f32 / H as f32;
    let src_ratio = iw as f32 / ih as f32;
    let (cw, ch) = if src_ratio > target_ratio {
        ((ih as f32 * target_ratio).round() as u32, ih)
    } else {
        (iw, (iw as f32 / target_ratio).round() as u32)
    };
    let x = (iw.saturating_sub(cw)) / 2;
    let y = (ih.saturating_sub(ch)) / 2;
    let cropped = img.crop_imm(x, y, cw.max(1), ch.max(1));
    let resized = cropped.resize_exact(W, H, FilterType::Lanczos3);
    let rgb = resized.to_rgb8();
    let mut out = Cursor::new(Vec::new());
    let mut enc = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, JPEG_Q);
    enc.encode(rgb.as_raw(), W, H, image::ExtendedColorType::Rgb8)
        .map_err(|e| AppError::Other(format!("encode jpeg: {e}")))?;
    Ok(out.into_inner())
}

/// 解码任意图片字节 → 居中裁剪到 2:3 → 缩放到 500×750 → 编码 JPEG q85。
pub fn to_cover(bytes: &[u8]) -> AppResult<Vec<u8>> {
    let img = image::load_from_memory(bytes)
        .map_err(|e| AppError::Other(format!("decode image: {e}")))?;
    finalize(img)
}

/// 按裁剪矩形 (x,y,w,h) 从原图裁出区域，再走与 to_cover 完全相同的
/// 缩放 500×750 + JPEG q85（共用 finalize），格式与自动抓取的海报一致。
/// 坐标越界由 crop_imm 安全截断到图像边界。
pub fn crop_to_cover(bytes: &[u8], x: u32, y: u32, w: u32, h: u32) -> AppResult<Vec<u8>> {
    let img = image::load_from_memory(bytes)
        .map_err(|e| AppError::Other(format!("decode image: {e}")))?;
    let cropped = img.crop_imm(x, y, w.max(1), h.max(1));
    finalize(cropped)
}

/// 把封面字节存入 covers_dir，内容 hash 命名 tmdb_<hash>.jpg，返回绝对路径。
pub fn save_cover(covers_dir: &Path, bytes: &[u8]) -> AppResult<String> {
    std::fs::create_dir_all(covers_dir).ok();
    let mut h = DefaultHasher::new();
    bytes.hash(&mut h);
    let dest = covers_dir.join(format!("tmdb_{:016x}.jpg", h.finish()));
    std::fs::write(&dest, bytes).map_err(|e| AppError::Other(format!("write cover: {e}")))?;
    Ok(dest.to_string_lossy().into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{ImageBuffer, Rgb};

    fn png_bytes(w: u32, h: u32) -> Vec<u8> {
        let img = ImageBuffer::from_fn(w, h, |x, _| Rgb([(x % 256) as u8, 100, 150]));
        let mut buf = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgb8(img)
            .write_to(&mut buf, image::ImageFormat::Png)
            .unwrap();
        buf.into_inner()
    }

    #[test]
    fn to_cover_outputs_500x750_jpeg() {
        let out = to_cover(&png_bytes(1000, 1000)).unwrap();
        let decoded = image::load_from_memory(&out).unwrap();
        assert_eq!(decoded.width(), 500);
        assert_eq!(decoded.height(), 750);
        assert_eq!(&out[0..2], &[0xFF, 0xD8]);
    }

    #[test]
    fn to_cover_wide_image_center_cropped() {
        let out = to_cover(&png_bytes(1000, 400)).unwrap();
        let decoded = image::load_from_memory(&out).unwrap();
        assert_eq!(decoded.width(), 500);
        assert_eq!(decoded.height(), 750);
    }

    #[test]
    fn crop_to_cover_outputs_500x750_jpeg() {
        let out = crop_to_cover(&png_bytes(1000, 1000), 100, 100, 600, 900).unwrap();
        let decoded = image::load_from_memory(&out).unwrap();
        assert_eq!(decoded.width(), 500);
        assert_eq!(decoded.height(), 750);
        assert_eq!(&out[0..2], &[0xFF, 0xD8]);
    }

    #[test]
    fn crop_to_cover_full_image_matches_to_cover_spec() {
        // 裁全图 (0,0,w,h) 与 to_cover 输出规格一致（均 500×750 JPEG）
        let src = png_bytes(800, 1200);
        let a = to_cover(&src).unwrap();
        let b = crop_to_cover(&src, 0, 0, 800, 1200).unwrap();
        let da = image::load_from_memory(&a).unwrap();
        let db = image::load_from_memory(&b).unwrap();
        assert_eq!((da.width(), da.height()), (500, 750));
        assert_eq!((db.width(), db.height()), (500, 750));
    }

    #[test]
    fn save_cover_writes_stable_named_file() {
        let tmp = tempfile::tempdir().unwrap();
        let bytes = to_cover(&png_bytes(600, 900)).unwrap();
        let p1 = save_cover(tmp.path(), &bytes).unwrap();
        let p2 = save_cover(tmp.path(), &bytes).unwrap();
        assert_eq!(p1, p2, "相同内容应得同名");
        assert!(p1.contains("tmdb_"));
        assert!(p1.ends_with(".jpg"));
        assert!(std::path::Path::new(&p1).is_file());
    }
}
