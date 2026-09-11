pub mod reader;

use crate::error::AppResult;
use base64::Engine;
use std::path::Path;

#[tauri::command]
pub fn comic_pages(path: String) -> AppResult<Vec<reader::PageInfo>> {
    reader::list_pages_with_dims(Path::new(&path))
}

/// 返回指定页的 data URL（base64），供 <img> 直接显示。
#[tauri::command]
pub fn comic_page(path: String, entry: String) -> AppResult<String> {
    let bytes = reader::read_entry(Path::new(&path), &entry)?;
    let mime = if entry.to_lowercase().ends_with(".png") {
        "image/png"
    } else {
        "image/jpeg"
    };
    let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
    Ok(format!("data:{mime};base64,{b64}"))
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct VolumeInfo {
    pub vol_no: u32,
    pub label: String,
    pub zip_path: String,
}

/// 列出漫画目录下所有 Vol_XX.zip，按卷号升序。
pub fn list_volumes(manga_dir: &Path) -> Vec<VolumeInfo> {
    let re = regex::Regex::new(r"^Vol_(\d+)\.zip$").unwrap();
    let mut vols: Vec<VolumeInfo> = std::fs::read_dir(manga_dir).ok()
        .into_iter().flatten().filter_map(|e| e.ok())
        .filter_map(|e| {
            let p = e.path();
            let name = p.file_name()?.to_str()?.to_string();
            let caps = re.captures(&name)?;
            let no: u32 = caps.get(1)?.as_str().parse().ok()?;
            Some(VolumeInfo { vol_no: no, label: format!("第 {no:02} 卷"), zip_path: p.to_string_lossy().into_owned() })
        }).collect();
    vols.sort_by_key(|v| v.vol_no);
    vols
}

/// 卷 zip 首图的 data URL（不压缩、不落盘），用作卷列表缩略图。无图返回 None。
pub fn first_page_data_url(zip_path: &Path) -> AppResult<Option<String>> {
    let pages = reader::list_pages(zip_path)?;
    let first = match pages.first() { Some(f) => f.clone(), None => return Ok(None) };
    let bytes = reader::read_entry(zip_path, &first)?;
    let mime = if first.to_lowercase().ends_with(".png") { "image/png" } else { "image/jpeg" };
    let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
    Ok(Some(format!("data:{mime};base64,{b64}")))
}

#[tauri::command(rename_all = "camelCase")]
pub fn comic_volume_cover(zip_path: String) -> AppResult<Option<String>> {
    first_page_data_url(Path::new(&zip_path))
}

/// 命令：按 comic_root + category_path + title 拼漫画目录，列出各卷。
#[tauri::command(rename_all = "camelCase")]
pub fn comic_volumes(
    db: tauri::State<crate::db::Db>,
    category_path: String,
    title: String,
) -> AppResult<Vec<VolumeInfo>> {
    let root = crate::settings::get(&db, "comic_root")?
        .ok_or_else(|| crate::error::AppError::Invalid("comic root not set".into()))?;
    let mut dir = std::path::PathBuf::from(root);
    if !category_path.is_empty() { dir.push(&category_path); }
    dir.push(&title);
    Ok(list_volumes(&dir))
}

#[cfg(test)]
mod vol_tests {
    use super::*;
    #[test]
    fn lists_volumes_sorted() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        for n in ["Vol_02.zip","Vol_01.zip","Vol_10.zip","readme.txt"] {
            std::fs::write(dir.join(n), b"x").unwrap();
        }
        let vols = list_volumes(dir);
        assert_eq!(vols.len(), 3);
        assert_eq!(vols[0].vol_no, 1);
        assert_eq!(vols[0].label, "第 01 卷");
        assert_eq!(vols[1].vol_no, 2);
        assert_eq!(vols[2].vol_no, 10);
        assert_eq!(vols[2].label, "第 10 卷");
    }

    #[test]
    fn volume_cover_returns_first_page() {
        use image::{RgbImage, Rgb};
        let tmp = tempfile::tempdir().unwrap();
        let zp = tmp.path().join("Vol_01.zip");
        let f = std::fs::File::create(&zp).unwrap();
        let mut w = zip::ZipWriter::new(f);
        let opt = zip::write::SimpleFileOptions::default();
        let mut buf = std::io::Cursor::new(Vec::new());
        RgbImage::from_pixel(10,10,Rgb([1,2,3])).write_to(&mut buf, image::ImageFormat::Jpeg).unwrap();
        use std::io::Write;
        w.start_file("001.jpg", opt).unwrap();
        w.write_all(buf.get_ref()).unwrap();
        w.finish().unwrap();
        let url = first_page_data_url(&zp).unwrap();
        assert!(url.unwrap().starts_with("data:image/jpeg;base64,"));
    }
}
