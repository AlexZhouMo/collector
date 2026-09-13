//! 展示图导入：把用户选的图拷到应用数据目录下的 covers/，返回目标路径。
//! 不引用原图路径（原图移动/删除不影响封面），供 cover_path 存储。
use crate::error::{AppError, AppResult};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::Path;

/// 把 src 图片拷到 covers_dir，目标名用源路径 hash + 原扩展名（稳定、去重）。
/// 返回目标绝对路径字符串。
pub fn import_cover(covers_dir: &Path, src: &str) -> AppResult<String> {
    std::fs::create_dir_all(covers_dir).ok();
    let src_path = Path::new(src);
    if !src_path.is_file() {
        return Err(AppError::Other(format!("cover source not a file: {src}")));
    }
    let ext = src_path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("jpg")
        .to_lowercase();
    let mut h = DefaultHasher::new();
    src.hash(&mut h);
    let dest = covers_dir.join(format!("cover_{:016x}.{ext}", h.finish()));
    std::fs::copy(src_path, &dest)?;
    Ok(dest.to_string_lossy().into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    #[test]
    fn import_copies_into_covers_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("orig.jpg");
        std::fs::File::create(&src).unwrap().write_all(b"img").unwrap();
        let covers = tmp.path().join("covers");
        let dest = import_cover(&covers, src.to_str().unwrap()).unwrap();
        assert!(dest.contains("covers"));
        assert!(dest.ends_with(".jpg"));
        assert!(Path::new(&dest).is_file());
        // 内容拷贝正确
        assert_eq!(std::fs::read(&dest).unwrap(), b"img");
    }
    #[test]
    fn import_rejects_missing_source() {
        let tmp = tempfile::tempdir().unwrap();
        let covers = tmp.path().join("covers");
        assert!(import_cover(&covers, "/no/such.jpg").is_err());
    }
}
