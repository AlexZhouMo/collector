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

/// 解码任意图片字节 → 居中裁剪到 2:3 → 缩放到 500×750 → 编码 JPEG q85。
pub fn to_cover(bytes: &[u8]) -> AppResult<Vec<u8>> {
    let img = image::load_from_memory(bytes)
        .map_err(|e| AppError::Other(format!("decode image: {e}")))?;
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
