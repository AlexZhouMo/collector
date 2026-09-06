use crate::error::{AppError, AppResult};
use std::fs::File;
use std::io::Read;
use std::path::Path;
use zip::ZipArchive;

/// 返回 zip 内按文件名排序的图片条目名列表（jpg/jpeg/png）。
pub fn list_pages(zip_path: &Path) -> AppResult<Vec<String>> {
    let file = File::open(zip_path)?;
    let mut ar = ZipArchive::new(file).map_err(|e| AppError::Other(e.to_string()))?;
    let mut names: Vec<String> = (0..ar.len())
        .filter_map(|i| ar.by_index(i).ok().map(|f| f.name().to_string()))
        .filter(|n| {
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use zip::write::SimpleFileOptions;

    fn make_zip(path: &Path) {
        let f = File::create(path).unwrap();
        let mut w = zip::ZipWriter::new(f);
        let opt = SimpleFileOptions::default();
        for name in ["003.jpg", "001.jpg", "002.jpg", "readme.txt"] {
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
    fn reads_entry_bytes() {
        let tmp = tempfile::tempdir().unwrap();
        let zp = tmp.path().join("c.zip");
        make_zip(&zp);
        let bytes = read_entry(&zp, "002.jpg").unwrap();
        assert_eq!(bytes, b"002.jpg");
    }
}
