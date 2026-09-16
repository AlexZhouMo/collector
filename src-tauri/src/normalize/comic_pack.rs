use crate::error::{AppError, AppResult};
use std::fs::File;
use std::io::Write;
use std::path::Path;
use zip::write::SimpleFileOptions;
use rayon::prelude::*;

/// 归档打包时统一的 JPEG 编码质量（1-100）。85 在漫画线稿/网点下观感接近无损、体积适中。
const JPEG_QUALITY: u8 = 85;

#[derive(PartialEq, Eq, PartialOrd, Ord)]
pub enum NatChunk {
    Str(String),
    Num(u64),
}

/// 自然排序 key：把文件名拆成 数字块/非数字块 交替序列，数字块按数值比较。
/// 保证未补零的数字命名（1.jpg / 2.jpg / 10.jpg）按数值 1<2<10 排序，而非字典序。
pub fn natural_key(name: &str) -> Vec<NatChunk> {
    let mut chunks = Vec::new();
    let mut cur = String::new();
    let mut cur_is_digit = false;
    for c in name.chars() {
        let d = c.is_ascii_digit();
        if cur.is_empty() || d == cur_is_digit {
            cur.push(c);
            cur_is_digit = d;
        } else {
            chunks.push(if cur_is_digit {
                NatChunk::Num(cur.parse().unwrap_or(0))
            } else {
                NatChunk::Str(cur.clone())
            });
            cur = c.to_string();
            cur_is_digit = d;
        }
    }
    if !cur.is_empty() {
        chunks.push(if cur_is_digit {
            NatChunk::Num(cur.parse().unwrap_or(0))
        } else {
            NatChunk::Str(cur)
        });
    }
    chunks
}

/// 把已排序的图片路径列表转 JPEG、按 namer(i) 命名、打包成 out_zip，返回打包页数。
/// namer 接收 0-based 序号，返回 zip 内文件名。图片列表应已按需排序、已排除冗余文件。
pub fn pack_images_to_zip(
    images: &[std::path::PathBuf],
    out_zip: &Path,
    namer: impl Fn(usize) -> String,
    on_image: impl Fn() + Sync,
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
        .map(|(i, path)| {
            let r = encode_one_jpeg(path);
            on_image();
            (i, r)
        })
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

#[cfg(test)]
mod tests {
    use super::*;
    use image::{RgbImage, Rgb};
    #[test]
    fn pack_images_to_zip_uses_namer() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("src");
        std::fs::create_dir_all(&dir).unwrap();
        for name in ["1.png", "2.png"] {
            let img = RgbImage::from_pixel(4, 4, Rgb([10, 20, 30]));
            img.save(dir.join(name)).unwrap();
        }
        let imgs = vec![dir.join("1.png"), dir.join("2.png")];
        let out = tmp.path().join("out.zip");
        let n = pack_images_to_zip(&imgs, &out, |i| format!("01_{:03}.jpg", i + 1), || {}).unwrap();
        assert_eq!(n, 2);
        let mut ar = zip::ZipArchive::new(File::open(&out).unwrap()).unwrap();
        let names: Vec<String> = (0..ar.len()).map(|i| ar.by_index(i).unwrap().name().to_string()).collect();
        assert!(names.contains(&"01_001.jpg".to_string()));
        assert!(names.contains(&"01_002.jpg".to_string()));
    }

    #[test]
    fn on_image_called_once_per_image() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("src");
        std::fs::create_dir_all(&dir).unwrap();
        let imgs: Vec<_> = (0..5).map(|k| {
            let p = dir.join(format!("i{k}.png"));
            RgbImage::from_pixel(4, 4, Rgb([10, 20, 30])).save(&p).unwrap();
            p
        }).collect();
        let out = tmp.path().join("out.zip");
        let cnt = AtomicUsize::new(0);
        let n = pack_images_to_zip(&imgs, &out, |i| format!("p_{:03}.jpg", i + 1),
            || { cnt.fetch_add(1, Ordering::Relaxed); }).unwrap();
        assert_eq!(n, 5);
        assert_eq!(cnt.load(Ordering::Relaxed), 5);
    }

    #[test]
    fn natural_sort_orders_unpadded_numbers() {
        // 字典序会把 "10" 排在 "2" 前；natural_key 应让 1 < 2 < 10。
        let mut names = vec!["10.png", "1.png", "2.png"];
        names.sort_by(|a, b| natural_key(a).cmp(&natural_key(b)));
        assert_eq!(names, vec!["1.png", "2.png", "10.png"]);
    }

    #[test]
    fn parallel_encoding_preserves_page_order() {
        // 用像素灰度值给每张图编码源序号，验证并行编码后 zip 内页序严格对应输入顺序。
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("src");
        std::fs::create_dir_all(&dir).unwrap();
        let vals: Vec<u8> = (0..8).map(|k| 10 + k * 30).collect();
        let imgs: Vec<_> = vals.iter().enumerate().map(|(k, &v)| {
            let p = dir.join(format!("img{k}.png"));
            RgbImage::from_pixel(4, 4, Rgb([v, v, v])).save(&p).unwrap();
            p
        }).collect();
        let out = tmp.path().join("out.zip");
        let n = pack_images_to_zip(&imgs, &out, |i| format!("p_{:03}.jpg", i + 1), || {}).unwrap();
        assert_eq!(n, 8);
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
        let r = pack_images_to_zip(&[good, bad], &out, |i| format!("p_{:03}.jpg", i + 1), || {});
        assert!(r.is_err(), "损坏图片应使整卷返回 Err");
    }
}
