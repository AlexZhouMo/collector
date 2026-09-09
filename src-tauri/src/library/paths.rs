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
}
