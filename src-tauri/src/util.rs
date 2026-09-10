//! 通用工具。

pub mod junk {
    use std::path::Path;

    /// 系统/工具自动生成的冗余文件名清单（比较时统一转小写）。
    const JUNK_NAMES: &[&str] = &[
        ".ds_store",       // macOS Finder
        "thumbs.db",       // Windows 缩略图缓存
        "desktop.ini",     // Windows 文件夹配置
        ".spotlight-v100", // macOS 索引
        ".trashes",        // macOS 废纸篓
        ".fseventsd",      // macOS 文件系统事件
    ];

    /// 判断一个文件/目录名是否为系统冗余文件：
    /// 命中 JUNK_NAMES（大小写不敏感），或以 `._` 开头（macOS AppleDouble 派生文件）。
    pub fn is_system_junk(name: &str) -> bool {
        if name.is_empty() {
            return false;
        }
        if name.starts_with("._") {
            return true;
        }
        let lower = name.to_lowercase();
        JUNK_NAMES.contains(&lower.as_str())
    }

    /// 便捷版：取路径末段文件名判定；无末段（如根路径）返回 false。
    pub fn is_system_junk_path(path: &Path) -> bool {
        path.file_name()
            .and_then(|s| s.to_str())
            .map(is_system_junk)
            .unwrap_or(false)
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use std::path::Path;

        #[test]
        fn matches_known_junk_names() {
            assert!(is_system_junk(".DS_Store"));
            assert!(is_system_junk("Thumbs.db"));
            assert!(is_system_junk("Desktop.ini"));
            assert!(is_system_junk(".Spotlight-V100"));
            assert!(is_system_junk(".Trashes"));
            assert!(is_system_junk(".fseventsd"));
        }

        #[test]
        fn case_insensitive() {
            assert!(is_system_junk("thumbs.DB"));
            assert!(is_system_junk(".ds_store"));
            assert!(is_system_junk("THUMBS.DB"));
            assert!(is_system_junk("DESKTOP.INI"));
        }

        #[test]
        fn apple_double_prefix() {
            assert!(is_system_junk("._星战.mkv"));
            assert!(is_system_junk("._foo"));
            assert!(is_system_junk("._"));
        }

        #[test]
        fn normal_files_not_flagged() {
            assert!(!is_system_junk("01.jpg"));
            assert!(!is_system_junk("星战.ass"));
            assert!(!is_system_junk("poster.jpg"));
            assert!(!is_system_junk("game.json"));
            assert!(!is_system_junk("info.txt"));
            assert!(!is_system_junk(".gitignore"));
        }

        #[test]
        fn empty_name_not_junk() {
            assert!(!is_system_junk(""));
        }

        #[test]
        fn path_helper() {
            assert!(is_system_junk_path(Path::new("/a/b/.DS_Store")));
            assert!(is_system_junk_path(Path::new("/a/b/._x.mkv")));
            assert!(!is_system_junk_path(Path::new("/a/b/星战.mkv")));
            assert!(!is_system_junk_path(Path::new("/")));
        }
    }
}
