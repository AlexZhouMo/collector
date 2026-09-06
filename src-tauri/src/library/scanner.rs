use crate::library::model::{MediaItem, MediaKind};
use std::path::Path;
use walkdir::WalkDir;

/// 扫描视频根目录。分类=相对根的第一级目录名；
/// category_path=相对根的完整目录路径（不含文件名）。
/// 每个 .mkv 为一个条目，同名 .ass 作外挂字幕，
/// 同目录 poster.jpg 或同名 .jpg 作封面，同名/同目录 info.txt 作简介。
pub fn scan_videos(root: &Path) -> Vec<ScannedItem> {
    let mut items = Vec::new();
    for entry in WalkDir::new(root).into_iter().filter_map(|e| e.ok()) {
        let p = entry.path();
        if p.extension().and_then(|s| s.to_str()) != Some("mkv") {
            continue;
        }
        let rel = p.strip_prefix(root).unwrap_or(p);
        let comps: Vec<String> = rel
            .parent()
            .map(|d| d.components().map(|c| c.as_os_str().to_string_lossy().into_owned()).collect())
            .unwrap_or_default();
        let category = comps.first().cloned().unwrap_or_default();
        let category_path = comps.join("/");
        let stem = p.file_stem().unwrap().to_string_lossy().into_owned();
        let dir = p.parent().unwrap();

        let subtitle = dir.join(format!("{stem}.ass"));
        let subtitle_path = subtitle.exists().then(|| subtitle.to_string_lossy().into_owned());

        let poster = dir.join("poster.jpg");
        let named_cover = dir.join(format!("{stem}.jpg"));
        let cover_path = if poster.exists() {
            Some(poster.to_string_lossy().into_owned())
        } else if named_cover.exists() {
            Some(named_cover.to_string_lossy().into_owned())
        } else {
            None
        };

        let info = dir.join("info.txt");
        let description = std::fs::read_to_string(&info).ok().map(|s| s.trim().to_string());

        items.push(ScannedItem {
            kind: MediaKind::Video,
            category,
            category_path,
            title: stem,
            path: p.to_string_lossy().into_owned(),
            subtitle_path,
            cover_path,
            description,
            platform_ok: true,
            exec_path: None,
        });
    }
    items
}

#[derive(Debug, Clone)]
pub struct ScannedItem {
    pub kind: MediaKind,
    pub category: String,
    pub category_path: String,
    pub title: String,
    pub path: String,
    pub subtitle_path: Option<String>,
    pub cover_path: Option<String>,
    pub description: Option<String>,
    pub platform_ok: bool,
    pub exec_path: Option<String>,
}

impl ScannedItem {
    #[allow(dead_code)]
    pub fn into_item(self, id: i64) -> MediaItem {
        MediaItem {
            id,
            kind: self.kind,
            category: self.category,
            category_path: self.category_path,
            title: self.title,
            path: self.path,
            subtitle_path: self.subtitle_path,
            cover_path: self.cover_path,
            description: self.description,
            platform_ok: self.platform_ok,
            exec_path: self.exec_path,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn scans_nested_categories_and_sidecars() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        // 模拟真实多级结构：电影/科幻/星球大战/
        let dir = root.join("电影").join("科幻").join("星球大战");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("星球大战.mkv"), b"x").unwrap();
        fs::write(dir.join("星球大战.ass"), b"sub").unwrap();
        fs::write(dir.join("poster.jpg"), b"img").unwrap();
        fs::write(dir.join("info.txt"), "  一部太空歌剧  ").unwrap();

        let items = scan_videos(root);
        assert_eq!(items.len(), 1);
        let it = &items[0];
        assert_eq!(it.category, "电影");
        assert_eq!(it.category_path, "电影/科幻/星球大战");
        assert_eq!(it.title, "星球大战");
        assert!(it.subtitle_path.is_some());
        assert!(it.cover_path.is_some());
        assert_eq!(it.description.as_deref(), Some("一部太空歌剧"));
    }
}
