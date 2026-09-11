use crate::library::model::MediaItem;
use serde::Deserialize;
use std::path::Path;
use walkdir::WalkDir;
use crate::util::junk;

/// 扫描一个视频分类目录。传入的 `category`（电影/动漫/剧集）决定该目录下
/// 所有视频的分类；category_path = category + 目录内相对路径，保留层级用于展示。
/// 每个 .mkv 为一个条目，同名 .ass 作外挂字幕，
/// 同目录 poster.jpg 或同名 .jpg 作封面，同名/同目录 info.txt 作简介。
pub fn scan_videos(root: &Path, category: &str) -> Vec<ScannedItem> {
    let mut items = Vec::new();
    for entry in WalkDir::new(root).into_iter().filter_map(|e| e.ok()) {
        let p = entry.path();
        if junk::is_system_junk_path(p) {
            continue;
        }
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
            category: category.to_string(),
            category_path,
            title: stem,
            cover_path,
            description,
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
        if junk::is_system_junk_path(p) {
            continue;
        }
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
            category: category.to_string(),
            category_path,
            title: stem,
            cover_path,
            description,
        });
    }
    items
}

#[derive(Debug, Clone)]
pub struct ScannedItem {
    pub category: String,
    pub category_path: String,
    pub title: String,
    pub cover_path: Option<String>,
    pub description: Option<String>,
}

impl ScannedItem {
    #[allow(dead_code)]
    pub fn into_item(self, id: i64) -> MediaItem {
        MediaItem {
            id,
            category: self.category,
            category_path: self.category_path,
            title: self.title,
            cover_path: self.cover_path,
            description: self.description,
            playable: false,
            video_path: String::new(),
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
    }

    #[test]
    fn scan_videos_skips_system_junk() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let dir = root.join("科幻");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("星战.mkv"), b"x").unwrap();
        fs::write(dir.join(".DS_Store"), b"junk").unwrap();
        fs::write(dir.join("Thumbs.db"), b"junk").unwrap();
        let items = scan_videos(root, "电影");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].title, "星战");
    }
}

/// 扫描漫画根：含 Vol_XX.zip 的目录 = 一部漫画。
/// title=漫画目录名；category_path=根到该目录父级的相对路径（分类，不含漫画名）。
/// 封面留 None，改由后续 AniList 拓取。
pub fn scan_comics(root: &Path) -> Vec<ScannedItem> {
    let vol_re = regex::Regex::new(r"^Vol_(\d+)\.zip$").unwrap();
    let mut items = Vec::new();
    for entry in WalkDir::new(root).into_iter().filter_map(|e| e.ok()) {
        let dir = entry.path();
        if !dir.is_dir() || junk::is_system_junk_path(dir) {
            continue;
        }
        let mut vols: Vec<u32> = std::fs::read_dir(dir)
            .ok()
            .into_iter()
            .flatten()
            .filter_map(|e| e.ok())
            .filter_map(|e| {
                let p = e.path();
                let name = p.file_name()?.to_str()?.to_string();
                let caps = vol_re.captures(&name)?;
                let no: u32 = caps.get(1)?.as_str().parse().ok()?;
                Some(no)
            })
            .collect();
        if vols.is_empty() {
            continue;
        }
        vols.sort_unstable();
        let title = dir.file_name().unwrap().to_string_lossy().into_owned();
        let rel_parent = dir
            .strip_prefix(root)
            .ok()
            .and_then(|r| r.parent())
            .map(|p| {
                p.components()
                    .map(|c| c.as_os_str().to_string_lossy().into_owned())
                    .collect::<Vec<_>>()
                    .join("/")
            })
            .unwrap_or_default();
        items.push(ScannedItem {
            category: rel_parent.split('/').next().unwrap_or("").to_string(),
            category_path: rel_parent,
            title,
            cover_path: None,
            description: None,
        });
    }
    items
}

#[cfg(test)]
mod comic_scan_tests {
    use super::*;

    /// 在 dir 内写一个空的 Vol 卷 zip（封面已改由 AniList 拓取，zip 内容无关）。
    fn write_vol(dir: &Path, vol_name: &str) {
        std::fs::write(dir.join(vol_name), b"PK").unwrap();
    }

    #[test]
    fn scans_manga_as_item_no_cover() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let manga = root.join("热血").join("灌篮高手");
        std::fs::create_dir_all(&manga).unwrap();
        std::fs::write(manga.join("Vol_01.zip"), b"PK").unwrap();
        let items = scan_comics(root);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].title, "灌篮高手");
        assert_eq!(items[0].category_path, "热血");
        assert!(items[0].cover_path.is_none());
    }

    #[test]
    fn scans_manga_category_and_multi_volumes() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let manga = root.join("热血").join("海贼王");
        std::fs::create_dir_all(&manga).unwrap();
        // 多卷聚合为一部漫画
        write_vol(&manga, "Vol_02.zip");
        write_vol(&manga, "Vol_01.zip");
        let items = scan_comics(root);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].category, "热血");
        assert_eq!(items[0].category_path, "热血");
        assert_eq!(items[0].title, "海贼王");
    }

    #[test]
    fn scan_comics_skips_system_junk() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let manga = root.join("热血").join("龙珠");
        std::fs::create_dir_all(&manga).unwrap();
        write_vol(&manga, "Vol_01.zip");
        // 系统冗余目录不应被当作漫画
        let junk = root.join(".DS_Store");
        std::fs::create_dir_all(&junk).unwrap();
        write_vol(&junk, "Vol_01.zip");
        let items = scan_comics(root);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].title, "龙珠");
    }
}

#[derive(Debug, Deserialize)]
struct GameManifest {
    name: Option<String>,
    description: Option<String>,
}

/// 扫描游戏根：每个含 game.json 的一级子目录为一条目。
/// 可启动性不再入库；exec 路径由 launch_game 运行时读 game.json 现算。
pub fn scan_games(root: &Path, covers_dir: &Path) -> Vec<ScannedItem> {
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
        if junk::is_system_junk_path(&dir) {
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

        let cover_src = dir.join("cover.jpg");
        let cover_path = if cover_src.is_file() {
            std::fs::read(&cover_src)
                .ok()
                .and_then(|b| crate::poster::image_proc::to_cover(&b).ok())
                .and_then(|c| crate::poster::image_proc::save_cover(covers_dir, &c, "game_").ok())
        } else {
            None
        };
        let info = dir.join("info.txt");
        let description = m
            .description
            .or_else(|| std::fs::read_to_string(&info).ok().map(|s| s.trim().to_string()));

        items.push(ScannedItem {
            category: "游戏".into(),
            category_path: dir_name.clone(),
            title: m.name.unwrap_or(dir_name),
            cover_path,
            description,
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
        let covers = tmp.path().join("covers");
        let items = scan_games(tmp.path(), &covers);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].title, "空洞骑士");
    }

    #[test]
    fn scan_games_cover_into_covers_game() {
        use image::{Rgb, RgbImage};
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let g = root.join("空洞骑士");
        std::fs::create_dir_all(&g).unwrap();
        std::fs::write(g.join("game.json"), r#"{"name":"空洞骑士"}"#).unwrap();
        RgbImage::from_pixel(400, 600, Rgb([10, 20, 30]))
            .save(g.join("cover.jpg"))
            .unwrap();
        let covers = tmp.path().join("covers");
        let items = scan_games(root, &covers);
        assert_eq!(items.len(), 1);
        let cp = items[0].cover_path.as_ref().unwrap();
        assert!(
            cp.contains("covers") && cp.contains("game"),
            "cover 应在 covers/game: {cp}"
        );
        assert!(std::path::Path::new(cp).is_file());
    }
}
