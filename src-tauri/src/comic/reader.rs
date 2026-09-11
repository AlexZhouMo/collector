use crate::error::{AppError, AppResult};
use std::fs::File;
use std::io::Read;
use std::path::Path;
use zip::ZipArchive;

use crate::util::junk;

/// 返回 zip 内按文件名排序的图片条目名列表（jpg/jpeg/png）。
pub fn list_pages(zip_path: &Path) -> AppResult<Vec<String>> {
    let file = File::open(zip_path)?;
    let mut ar = ZipArchive::new(file).map_err(|e| AppError::Other(e.to_string()))?;
    let mut names: Vec<String> = (0..ar.len())
        .filter_map(|i| ar.by_index(i).ok().map(|f| f.name().to_string()))
        .filter(|n| {
            let base = n.rsplit('/').next().unwrap_or(n);
            if junk::is_system_junk(base) {
                return false;
            }
            let l = n.to_lowercase();
            l.ends_with(".jpg") || l.ends_with(".jpeg") || l.ends_with(".png")
        })
        .collect();
    names.sort();
    Ok(names)
}

/// 读取 zip 内指定条目的原始字节。
pub fn read_entry(zip_path: &Path, entry_name: &str) -> AppResult<Vec<u8>> {
    let file = File::open(zip_path)?;
    let mut ar = ZipArchive::new(file).map_err(|e| AppError::Other(e.to_string()))?;
    let mut f = ar
        .by_name(entry_name)
        .map_err(|_| AppError::NotFound(entry_name.into()))?;
    let mut buf = Vec::with_capacity(f.size() as usize);
    f.read_to_end(&mut buf)?;
    Ok(buf)
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct PageInfo { pub name: String, pub w: u32, pub h: u32 }

/// 列页并解每页宽高。解析失败按 (0,0)（前端按竖单页默认处理）。
pub fn list_pages_with_dims(zip_path: &Path) -> AppResult<Vec<PageInfo>> {
    let names = list_pages(zip_path)?;
    let mut out = Vec::with_capacity(names.len());
    for name in names {
        let bytes = read_entry(zip_path, &name)?;
        let (w, h) = image::load_from_memory(&bytes)
            .map(|im| (im.width(), im.height()))
            .unwrap_or((0, 0));
        out.push(PageInfo { name, w, h });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use zip::write::SimpleFileOptions;

    fn make_zip(path: &Path) {
        let f = File::create(path).unwrap();
        let mut w = zip::ZipWriter::new(f);
        let opt = SimpleFileOptions::default();
        for name in ["003.jpg", "001.jpg", "002.jpg", "readme.txt", "__MACOSX/._001.jpg", ".DS_Store"] {
            w.start_file(name, opt).unwrap();
            w.write_all(name.as_bytes()).unwrap();
        }
        w.finish().unwrap();
    }

    #[test]
    fn lists_sorted_image_pages_only() {
        let tmp = tempfile::tempdir().unwrap();
        let zp = tmp.path().join("c.zip");
        make_zip(&zp);
        let pages = list_pages(&zp).unwrap();
        assert_eq!(pages, vec!["001.jpg", "002.jpg", "003.jpg"]);
    }

    #[test]
    fn list_pages_with_dims_reads_size() {
        use image::{RgbImage, Rgb};
        let tmp = tempfile::tempdir().unwrap();
        let zp = tmp.path().join("c.zip");
        let f = File::create(&zp).unwrap();
        let mut w = zip::ZipWriter::new(f);
        let opt = SimpleFileOptions::default();
        let mut buf = std::io::Cursor::new(Vec::new());
        RgbImage::from_pixel(120, 200, Rgb([1,2,3])).write_to(&mut buf, image::ImageFormat::Jpeg).unwrap();
        w.start_file("001.jpg", opt).unwrap();
        w.write_all(buf.get_ref()).unwrap();
        w.finish().unwrap();
        let pages = list_pages_with_dims(&zp).unwrap();
        assert_eq!(pages.len(), 1);
        assert_eq!(pages[0].name, "001.jpg");
        assert_eq!((pages[0].w, pages[0].h), (120, 200));
    }

    #[test]
    fn reads_entry_bytes() {
        let tmp = tempfile::tempdir().unwrap();
        let zp = tmp.path().join("c.zip");
        make_zip(&zp);
        let bytes = read_entry(&zp, "002.jpg").unwrap();
        assert_eq!(bytes, b"002.jpg");
    }
}
