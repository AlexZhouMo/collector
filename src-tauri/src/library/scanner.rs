use crate::library::model::{MediaItem, MediaKind};
use serde::Deserialize;
use std::path::Path;
use walkdir::WalkDir;

/// 扫描一个视频分类目录。传入的 `category`（电影/动漫/剧集）决定该目录下
/// 所有视频的分类；category_path = category + 目录内相对路径，保留层级用于展示。
/// 每个 .mkv 为一个条目，同名 .ass 作外挂字幕，
/// 同目录 poster.jpg 或同名 .jpg 作封面，同名/同目录 info.txt 作简介。
pub fn scan_videos(root: &Path, category: &str) -> Vec<ScannedItem> {
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
        let category_path = if comps.is_empty() {
            category.to_string()
        } else {
            format!("{category}/{}", comps.join("/"))
        };
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
            category: category.to_string(),
            category_path,
            title: stem,
            subtitle_path,
            cover_path,
            description,
            platform_ok: true,
            exec_path: None,
        });
    }
    items
}

/// 以 .ass 字幕为条目扫描一个视频分类目录（初始化导入用）。
/// title=字幕文件名去扩展名；subtitle_path=该 .ass；
/// 去重键为 (kind,category_path,title)，后续放同名 .mkv 后由拼接推导视频路径。
/// category_path=category + 目录内相对路径（保留 子分类/剧名 层级）。
pub fn scan_videos_subs(root: &Path, category: &str) -> Vec<ScannedItem> {
    let mut items = Vec::new();
    for entry in WalkDir::new(root).into_iter().filter_map(|e| e.ok()) {
        let p = entry.path();
        if p.extension().and_then(|s| s.to_str()) != Some("ass") {
            continue;
        }
        let rel = p.strip_prefix(root).unwrap_or(p);
        let comps: Vec<String> = rel
            .parent()
            .map(|d| d.components().map(|c| c.as_os_str().to_string_lossy().into_owned()).collect())
            .unwrap_or_default();
        let category_path = if comps.is_empty() {
            category.to_string()
        } else {
            format!("{category}/{}", comps.join("/"))
        };
        let stem = p.file_stem().unwrap().to_string_lossy().into_owned();
        let dir = p.parent().unwrap();
        let ass_abs = p.to_string_lossy().into_owned();

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
            category: category.to_string(),
            category_path,
            title: stem,
            subtitle_path: Some(ass_abs),
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
            subtitle_path: self.subtitle_path,
            cover_path: self.cover_path,
            description: self.description,
            platform_ok: self.platform_ok,
            exec_path: self.exec_path,
            playable: false,
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
        let root = tmp.path(); // 视为「电影」分类目录
        // 目录内可有子层级：科幻/星球大战/
        let dir = root.join("科幻").join("星球大战");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("星球大战.mkv"), b"x").unwrap();
        fs::write(dir.join("星球大战.ass"), b"sub").unwrap();
        fs::write(dir.join("poster.jpg"), b"img").unwrap();
        fs::write(dir.join("info.txt"), "  一部太空歌剧  ").unwrap();

        let items = scan_videos(root, "电影");
        assert_eq!(items.len(), 1);
        let it = &items[0];
        assert_eq!(it.category, "电影");
        assert_eq!(it.category_path, "电影/科幻/星球大战");
        assert_eq!(it.title, "星球大战");
        assert!(it.subtitle_path.is_some());
        assert!(it.cover_path.is_some());
        assert_eq!(it.description.as_deref(), Some("一部太空歌剧"));
    }

    #[test]
    fn video_directly_in_category_root_has_category_as_path() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        fs::write(root.join("直放.mkv"), b"x").unwrap();
        let items = scan_videos(root, "动漫");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].category, "动漫");
        assert_eq!(items[0].category_path, "动漫");
    }

    #[test]
    fn scan_subs_uses_ass_as_items() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let dir = root.join("日剧").join("怨屋本铺");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("E07.被当做踏脚石的人生.ass"), b"sub").unwrap();
        let items = scan_videos_subs(root, "剧集");
        assert_eq!(items.len(), 1);
        let it = &items[0];
        assert_eq!(it.category, "剧集");
        assert_eq!(it.category_path, "剧集/日剧/怨屋本铺");
        assert_eq!(it.title, "E07.被当做踏脚石的人生");
        assert!(it.subtitle_path.as_deref().unwrap().ends_with("E07.被当做踏脚石的人生.ass"));
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
