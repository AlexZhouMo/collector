//! 字幕导入：把用户选的字幕拷到应用数据目录下的 subtitles/，返回目标路径。
//! 与封面 cover.rs 同机制：不引用原路径（原字幕移动/删除不影响），供 subtitle_path 相对存储。
use crate::error::{AppError, AppResult};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::Path;

/// 把 src 字幕拷到 subtitles_dir，目标名用源路径 hash + 原扩展名（稳定、去重、防撞名）。
/// 返回目标绝对路径字符串。源不是文件则报错。
pub fn import_subtitle(subtitles_dir: &Path, src: &str) -> AppResult<String> {
    std::fs::create_dir_all(subtitles_dir).ok();
    let src_path = Path::new(src);
    if !src_path.is_file() {
        return Err(AppError::Other(format!("subtitle source not a file: {src}")));
    }
    let ext = src_path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("ass")
        .to_lowercase();
    let mut h = DefaultHasher::new();
    src.hash(&mut h);
    let dest = subtitles_dir.join(format!("sub_{:016x}.{ext}", h.finish()));
    std::fs::copy(src_path, &dest)?;
    Ok(dest.to_string_lossy().into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    #[test]
    fn import_copies_into_subtitles_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("E01.ass");
        std::fs::File::create(&src).unwrap().write_all(b"[Script Info]").unwrap();
        let subs = tmp.path().join("subtitles");
        let dest = import_subtitle(&subs, src.to_str().unwrap()).unwrap();
        assert!(dest.contains("subtitles"));
        assert!(dest.contains("sub_"));
        assert!(dest.ends_with(".ass"));
        assert!(Path::new(&dest).is_file());
        // 相同源路径两次调用得同名（hash 稳定）
        let dest2 = import_subtitle(&subs, src.to_str().unwrap()).unwrap();
        assert_eq!(dest, dest2);
    }
    #[test]
    fn import_rejects_missing_source() {
        let tmp = tempfile::tempdir().unwrap();
        let subs = tmp.path().join("subtitles");
        assert!(import_subtitle(&subs, "/no/such.ass").is_err());
    }
}
