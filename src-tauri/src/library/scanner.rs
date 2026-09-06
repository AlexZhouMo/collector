use crate::library::model::{MediaItem, MediaKind};
use serde::Deserialize;
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

/// 扫描漫画根目录：每个 .zip 为一条目，分类=第一级目录，
/// category_path=完整目录路径，简介取同名 .txt。封面运行时取 zip 内首图，此处留空。
pub fn scan_comics(root: &Path) -> Vec<ScannedItem> {
    let mut items = Vec::new();
    for entry in WalkDir::new(root).into_iter().filter_map(|e| e.ok()) {
        let p = entry.path();
        if p.extension().and_then(|s| s.to_str()) != Some("zip") {
            continue;
        }
        let rel = p.strip_prefix(root).unwrap_or(p);
        let comps: Vec<String> = rel
            .parent()
            .map(|d| d.components().map(|c| c.as_os_str().to_string_lossy().into_owned()).collect())
            .unwrap_or_default();
        let stem = p.file_stem().unwrap().to_string_lossy().into_owned();
        let dir = p.parent().unwrap();
        let info = dir.join(format!("{stem}.txt"));
        let description = std::fs::read_to_string(&info).ok().map(|s| s.trim().to_string());
        items.push(ScannedItem {
            kind: MediaKind::Comic,
            category: comps.first().cloned().unwrap_or_default(),
            category_path: comps.join("/"),
            title: stem,
            path: p.to_string_lossy().into_owned(),
            subtitle_path: None,
            cover_path: None,
            description,
            platform_ok: true,
            exec_path: None,
        });
    }
    items
}

#[cfg(test)]
mod comic_scan_tests {
    use super::*;
    use std::fs;
    #[test]
    fn scans_zip_comics() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("热血").join("海贼王");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("第01卷.zip"), b"PK").unwrap();
        fs::write(dir.join("第01卷.txt"), "简介").unwrap();
        let items = scan_comics(tmp.path());
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].category, "热血");
        assert_eq!(items[0].description.as_deref(), Some("简介"));
    }
}

#[derive(Debug, Deserialize)]
struct GameManifest {
    name: Option<String>,
    description: Option<String>,
    exec_win: Option<String>,
    exec_mac: Option<String>,
    #[allow(dead_code)]
    fullscreen: Option<bool>,
}

/// 扫描游戏根：每个含 game.json 的一级子目录为一条目。
/// 按当前平台取 exec_win/exec_mac 决定 platform_ok 与 exec_path（绝对）。
pub fn scan_games(root: &Path) -> Vec<ScannedItem> {
    let mut items = Vec::new();
    let entries = match std::fs::read_dir(root) {
        Ok(e) => e,
        Err(_) => return items,
    };
    for e in entries.filter_map(|e| e.ok()) {
        let dir = e.path();
        if !dir.is_dir() {
            continue;
        }
        let manifest_path = dir.join("game.json");
        if !manifest_path.exists() {
            continue;
        }
        let text = match std::fs::read_to_string(&manifest_path) {
            Ok(t) => t,
            Err(_) => continue,
        };
        let m: GameManifest = match serde_json::from_str(&text) {
            Ok(m) => m,
            Err(_) => continue,
        };
        let dir_name = dir.file_name().unwrap().to_string_lossy().into_owned();

        let rel_exec = if cfg!(target_os = "windows") {
            m.exec_win.clone()
        } else {
            m.exec_mac.clone()
        };
        let (platform_ok, exec_path) = match rel_exec {
            Some(rel) => (true, Some(dir.join(rel).to_string_lossy().into_owned())),
            None => (false, None),
        };
        let cover = dir.join("cover.jpg");
        let cover_path = cover.exists().then(|| cover.to_string_lossy().into_owned());
        let info = dir.join("info.txt");
        let description = m
            .description
            .or_else(|| std::fs::read_to_string(&info).ok().map(|s| s.trim().to_string()));

        items.push(ScannedItem {
            kind: MediaKind::Game,
            category: "游戏".into(),
            category_path: dir_name.clone(),
            title: m.name.unwrap_or(dir_name),
            path: dir.to_string_lossy().into_owned(),
            subtitle_path: None,
            cover_path,
            description,
            platform_ok,
            exec_path,
        });
    }
    items
}

#[cfg(test)]
mod game_scan_tests {
    use super::*;
    use std::fs;
    #[test]
    fn scans_game_with_manifest_current_platform() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("空洞骑士");
        fs::create_dir_all(&dir).unwrap();
        let exec_key = if cfg!(target_os = "windows") { "exec_win" } else { "exec_mac" };
        let exec_val = if cfg!(target_os = "windows") { "game.exe" } else { "Game.app" };
        fs::write(
            dir.join("game.json"),
            format!(r#"{{"name":"空洞骑士","{exec_key}":"{exec_val}","description":"银河恶魔城"}}"#),
        )
        .unwrap();
        let items = scan_games(tmp.path());
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].title, "空洞骑士");
        assert!(items[0].platform_ok);
        assert!(items[0].exec_path.as_ref().unwrap().ends_with(exec_val));
    }
}
