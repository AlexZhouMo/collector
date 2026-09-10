//! 路径相对化/绝对化转换。数据库存相对，读取拼绝对。
use std::path::Path;

/// 分类名 → settings 中的 video root key。
pub fn video_root_key(category: &str) -> &'static str {
    match category {
        "电影" => "video_movie_root",
        "动漫" => "video_anime_root",
        "剧集" => "video_tv_root",
        _ => "video_movie_root",
    }
}

/// 绝对视频路径 → 分类根下相对（去掉 root 前缀）。root 为空或不匹配则原样返回。
pub fn video_to_relative(abs: &str, root: &str) -> String {
    if root.is_empty() { return abs.to_string(); }
    match Path::new(abs).strip_prefix(root) {
        Ok(rel) => rel.to_string_lossy().replace('\\', "/"),
        Err(_) => abs.to_string(),
    }
}

/// 相对视频路径 + root → 绝对。root 为空或 rel 已是绝对则原样返回 rel。
pub fn video_to_absolute(rel: &str, root: &str) -> String {
    if root.is_empty() || rel.is_empty() { return rel.to_string(); }
    if Path::new(rel).is_absolute() { return rel.to_string(); }
    format!("{}/{}", root.trim_end_matches('/'), rel)
}

/// 由分类根 + 分类内相对目录 + 标题拼出视频绝对 mkv 路径。
/// root 为空返回空串（未配置根，无法定位）。category_path 为空则省略该段。
/// 例：video_abs_path("/媒体/电影","动作/古墓丽影","[2001].古墓丽影") = "/媒体/电影/动作/古墓丽影/[2001].古墓丽影.mkv"
pub fn video_abs_path(root: &str, category_path: &str, title: &str) -> String {
    if root.is_empty() {
        return String::new();
    }
    let root = root.trim_end_matches('/');
    if category_path.is_empty() {
        format!("{root}/{title}.mkv")
    } else {
        format!("{root}/{category_path}/{title}.mkv")
    }
}

/// 绝对 app_data 内路径 → 相对 app_data（保留 covers/ 或 subtitles/ 段）。不匹配原样返回。
pub fn appdata_to_relative(abs: &str, app_data: &str) -> String {
    match Path::new(abs).strip_prefix(app_data) {
        Ok(rel) => rel.to_string_lossy().replace('\\', "/"),
        Err(_) => abs.to_string(),
    }
}

/// 相对 app_data 路径 + app_data → 绝对。空则空；已是绝对原样返回。
pub fn appdata_to_absolute(rel: &str, app_data: &str) -> String {
    if rel.is_empty() { return String::new(); }
    if Path::new(rel).is_absolute() { return rel.to_string(); }
    format!("{}/{}", app_data.trim_end_matches('/'), rel)
}

/// 字幕在 app_data 下的相对路径：subtitles/<category>/<category_path>/<title>.ass
/// category_path 为空则省略中段。播放推导、校准输出、迁移归位三处共用。
pub fn subtitle_rel_path(category: &str, category_path: &str, title: &str) -> String {
    if category_path.is_empty() {
        format!("subtitles/{category}/{title}.ass")
    } else {
        format!("subtitles/{category}/{category_path}/{title}.ass")
    }
}

/// 字幕绝对路径：<app_data>/subtitles/...
pub fn subtitle_abs_path(app_data: &str, category: &str, category_path: &str, title: &str) -> String {
    format!("{}/{}", app_data.trim_end_matches('/'),
        subtitle_rel_path(category, category_path, title))
}

/// 去掉 category_path 首段分类名：电影/科幻/星战 → 科幻/星战；仅分类名 → 空串。
pub fn strip_category(category_path: &str) -> String {
    let mut it = category_path.splitn(2, '/');
    it.next(); // 丢弃首段
    it.next().unwrap_or("").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn video_rel_abs_roundtrip() {
        let abs = "/media/电影根/科幻/星战/x.mkv";
        let rel = video_to_relative(abs, "/media/电影根");
        assert_eq!(rel, "科幻/星战/x.mkv");
        assert_eq!(video_to_absolute(&rel, "/media/电影根"), abs);
    }
    #[test]
    fn video_empty_root_passthrough() {
        assert_eq!(video_to_relative("/a/b.mkv", ""), "/a/b.mkv");
        assert_eq!(video_to_absolute("科幻/x.mkv", ""), "科幻/x.mkv");
        // 绝对路径传给 to_absolute 原样返回（迁移前幂等）
        assert_eq!(video_to_absolute("/abs/x.mkv", "/media/根"), "/abs/x.mkv");
    }
    #[test]
    fn appdata_rel_abs() {
        assert_eq!(appdata_to_relative("/app/covers/x.jpg", "/app"), "covers/x.jpg");
        assert_eq!(appdata_to_absolute("covers/x.jpg", "/app"), "/app/covers/x.jpg");
        assert_eq!(appdata_to_absolute("", "/app"), "");
        // 绝对原样（幂等）
        assert_eq!(appdata_to_absolute("/abs/covers/x.jpg", "/app"), "/abs/covers/x.jpg");
    }
    #[test]
    fn strip_category_works() {
        assert_eq!(strip_category("电影/科幻/星战"), "科幻/星战");
        assert_eq!(strip_category("电影"), "");
    }
    #[test]
    fn video_abs_path_joins() {
        assert_eq!(
            video_abs_path("/媒体/电影", "动作/古墓丽影", "[2001].古墓丽影"),
            "/媒体/电影/动作/古墓丽影/[2001].古墓丽影.mkv"
        );
        // category_path 空 → 省略该段
        assert_eq!(video_abs_path("/媒体/电影", "", "沙丘"), "/媒体/电影/沙丘.mkv");
        // root 空 → 空串
        assert_eq!(video_abs_path("", "动作", "x"), "");
        // root 尾部斜杠归一
        assert_eq!(video_abs_path("/媒体/电影/", "科幻", "星战"), "/媒体/电影/科幻/星战.mkv");
    }
    #[test]
    fn subtitle_paths() {
        assert_eq!(subtitle_rel_path("电影", "动作/古墓丽影", "[2001].古墓丽影"),
            "subtitles/电影/动作/古墓丽影/[2001].古墓丽影.ass");
        assert_eq!(subtitle_rel_path("电影", "", "沙丘"), "subtitles/电影/沙丘.ass");
        assert_eq!(subtitle_abs_path("/app", "电影", "科幻", "星战"),
            "/app/subtitles/电影/科幻/星战.ass");
    }
}
