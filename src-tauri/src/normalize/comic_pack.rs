use crate::error::{AppError, AppResult};
use std::fs::File;
use std::io::Write;
use std::path::Path;
use crate::util::junk;
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

/// 把一个图片目录标准化：按文件名排序，转 JPG，重命名为 <prefix>_NNN.jpg，打包为 zip。
/// 清理无关文件（如 Thumbs.db）——只读取图片，不改源目录。
#[allow(dead_code)]
pub fn pack_comic_dir(dir: &Path, prefix: &str, out_zip: &Path) -> AppResult<usize> {
    let mut files: Vec<_> = std::fs::read_dir(dir)?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            if junk::is_system_junk_path(p) {
                return false;
            }
            let l = p.to_string_lossy().to_lowercase();
            l.ends_with(".jpg") || l.ends_with(".jpeg") || l.ends_with(".png") || l.ends_with(".webp") || l.ends_with(".bmp")
        })
        .collect();
    files.sort_by(|a, b| {
        let ka = natural_key(a.file_name().and_then(|s| s.to_str()).unwrap_or(""));
        let kb = natural_key(b.file_name().and_then(|s| s.to_str()).unwrap_or(""));
        ka.cmp(&kb)
    });
    let prefix = prefix.to_string();
    pack_images_to_zip(&files, out_zip, move |i| format!("{prefix}_{:03}.jpg", i + 1))
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{RgbImage, Rgb};
    #[test]
    fn packs_images_into_renamed_jpg_zip() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("src");
        std::fs::create_dir_all(&dir).unwrap();
        for name in ["b.png","a.png","Thumbs.db",".DS_Store"] {
            if name.ends_with(".png") {
                let img = RgbImage::from_pixel(4, 4, Rgb([10, 20, 30]));
                img.save(dir.join(name)).unwrap();
            } else {
                std::fs::write(dir.join(name), b"junk").unwrap();
            }
        }
        let out = tmp.path().join("out.zip");
        let n = pack_comic_dir(&dir, "海贼王01", &out).unwrap();
        assert_eq!(n, 2); // 仅两张图，Thumbs.db 与 .DS_Store 被忽略
        let mut ar = zip::ZipArchive::new(File::open(&out).unwrap()).unwrap();
        let names: Vec<String> = (0..ar.len()).map(|i| ar.by_index(i).unwrap().name().to_string()).collect();
        assert!(names.contains(&"海贼王01_001.jpg".to_string()));
        assert!(names.contains(&"海贼王01_002.jpg".to_string()));
    }

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
        let n = pack_images_to_zip(&imgs, &out, |i| format!("01_{:03}.jpg", i + 1)).unwrap();
        assert_eq!(n, 2);
        let mut ar = zip::ZipArchive::new(File::open(&out).unwrap()).unwrap();
        let names: Vec<String> = (0..ar.len()).map(|i| ar.by_index(i).unwrap().name().to_string()).collect();
        assert!(names.contains(&"01_001.jpg".to_string()));
        assert!(names.contains(&"01_002.jpg".to_string()));
    }

    #[test]
    fn natural_sort_orders_unpadded_numbers() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("src");
        std::fs::create_dir_all(&dir).unwrap();
        // 用像素灰度值给每张图编码源序号：1.png=30, 2.png=60, 10.png=210
        // 字典序会排成 1,10,2 -> 得到 30,210,60；自然序应为 1,2,10 -> 30,60,210
        for (name, v) in [("1.png", 30u8), ("2.png", 60u8), ("10.png", 210u8)] {
            let img = RgbImage::from_pixel(4, 4, Rgb([v, v, v]));
            img.save(dir.join(name)).unwrap();
        }
        let out = tmp.path().join("out.zip");
        let n = pack_comic_dir(&dir, "p", &out).unwrap();
        assert_eq!(n, 3);

        let mut ar = zip::ZipArchive::new(File::open(&out).unwrap()).unwrap();
        // 依次读回 _001/_002/_003，解码取像素值，应为递增（30 < 60 < 210）
        let mut vals = Vec::new();
        for idx in ["p_001.jpg", "p_002.jpg", "p_003.jpg"] {
            let mut entry = ar.by_name(idx).unwrap();
            let mut bytes = Vec::new();
            std::io::Read::read_to_end(&mut entry, &mut bytes).unwrap();
            let img = image::load_from_memory(&bytes).unwrap().to_rgb8();
            vals.push(img.get_pixel(0, 0)[0] as i32);
        }
        // JPEG 有损压缩，用容差判断顺序：30 -> 60 -> 210 单调递增
        assert!(vals[0] < vals[1], "_001 应来自 1.png(30) 而非 10.png(210): {vals:?}");
        assert!(vals[1] < vals[2], "_002 应来自 2.png(60), _003 来自 10.png(210): {vals:?}");
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
        let n = pack_images_to_zip(&imgs, &out, |i| format!("p_{:03}.jpg", i + 1)).unwrap();
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
        let r = pack_images_to_zip(&[good, bad], &out, |i| format!("p_{:03}.jpg", i + 1));
        assert!(r.is_err(), "损坏图片应使整卷返回 Err");
    }
}
