use crate::error::{AppError, AppResult};
use std::fs::File;
use std::io::Write;
use std::path::Path;
use zip::write::SimpleFileOptions;

/// 把一个图片目录标准化：按文件名排序，转 JPG，重命名为 <prefix>_NNN.jpg，打包为 zip。
/// 清理无关文件（如 Thumbs.db）——只读取图片，不改源目录。
pub fn pack_comic_dir(dir: &Path, prefix: &str, out_zip: &Path) -> AppResult<usize> {
    let mut files: Vec<_> = std::fs::read_dir(dir)?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            let l = p.to_string_lossy().to_lowercase();
            l.ends_with(".jpg") || l.ends_with(".jpeg") || l.ends_with(".png") || l.ends_with(".webp") || l.ends_with(".bmp")
        })
        .collect();
    files.sort();
    if files.is_empty() { return Err(AppError::Invalid("no images".into())); }

    let f = File::create(out_zip)?;
    let mut zw = zip::ZipWriter::new(f);
    let opt = SimpleFileOptions::default();
    let mut n = 0;
    for (i, path) in files.iter().enumerate() {
        let img = image::open(path).map_err(|e| AppError::Other(e.to_string()))?;
        let mut buf = std::io::Cursor::new(Vec::new());
        img.to_rgb8().write_to(&mut buf, image::ImageFormat::Jpeg)
            .map_err(|e| AppError::Other(e.to_string()))?;
        let name = format!("{prefix}_{:03}.jpg", i + 1);
        zw.start_file(name, opt).map_err(|e| AppError::Other(e.to_string()))?;
        zw.write_all(buf.get_ref())?;
        n += 1;
    }
    zw.finish().map_err(|e| AppError::Other(e.to_string()))?;
    Ok(n)
}

#[tauri::command(rename_all = "camelCase")]
pub fn normalize_comic(dir: String, prefix: String, out_zip: String) -> AppResult<usize> {
    pack_comic_dir(Path::new(&dir), &prefix, Path::new(&out_zip))
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
        for name in ["b.png","a.png","Thumbs.db"] {
            if name.ends_with(".png") {
                let img = RgbImage::from_pixel(4, 4, Rgb([10, 20, 30]));
                img.save(dir.join(name)).unwrap();
            } else {
                std::fs::write(dir.join(name), b"junk").unwrap();
            }
        }
        let out = tmp.path().join("out.zip");
        let n = pack_comic_dir(&dir, "海贼王01", &out).unwrap();
        assert_eq!(n, 2); // 仅两张图，Thumbs.db 被忽略
        let mut ar = zip::ZipArchive::new(File::open(&out).unwrap()).unwrap();
        let names: Vec<String> = (0..ar.len()).map(|i| ar.by_index(i).unwrap().name().to_string()).collect();
        assert!(names.contains(&"海贼王01_001.jpg".to_string()));
        assert!(names.contains(&"海贼王01_002.jpg".to_string()));
    }
}
