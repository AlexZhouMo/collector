pub mod reader;

use crate::error::AppResult;
use base64::Engine;
use std::path::Path;

#[tauri::command]
pub fn comic_pages(path: String) -> AppResult<Vec<String>> {
    reader::list_pages(Path::new(&path))
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

/// 首图作为封面 data URL。
#[tauri::command]
pub fn comic_cover(path: String) -> AppResult<Option<String>> {
    let pages = reader::list_pages(Path::new(&path))?;
    match pages.first() {
        Some(first) => Ok(Some(comic_page(path, first.clone())?)),
        None => Ok(None),
    }
}
