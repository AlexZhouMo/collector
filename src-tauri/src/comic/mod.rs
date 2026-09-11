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
}
