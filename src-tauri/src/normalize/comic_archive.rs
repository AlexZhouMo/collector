use crate::normalize::comic_pack::natural_key;
use crate::util::junk;
use std::path::{Path, PathBuf};

/// 一卷的归档结果。
#[derive(Debug, Clone, serde::Serialize)]
pub struct ArchiveReport {
    pub manga: String,
    pub vol: String,
    pub status: String,
    pub pages: usize,
}

/// 一个待归档卷：源图片目录 + 其漫画（父目录）+ 组内卷号 + 卷号位数。
#[derive(Debug, Clone, PartialEq)]
pub struct VolumePlan {
    pub dir: PathBuf,
    pub manga_dir: PathBuf,
    pub manga: String,
    pub index: usize,
    pub width: usize,
}

const IMG_EXTS: &[&str] = &["jpg", "jpeg", "png", "webp", "bmp"];

/// 目录是否直接包含至少一张图片（排除系统冗余文件）。
fn dir_has_images(dir: &Path) -> bool {
    let rd = match std::fs::read_dir(dir) {
        Ok(r) => r,
        Err(_) => return false,
    };
    for e in rd.filter_map(|e| e.ok()) {
        let p = e.path();
        if !p.is_file() || junk::is_system_junk_path(&p) {
            continue;
        }
        if let Some(ext) = p.extension().and_then(|s| s.to_str()) {
            if IMG_EXTS.contains(&ext.to_lowercase().as_str()) {
                return true;
            }
        }
    }
    false
}

/// 递归找出所有"直接含图片"的目录（含 root 自身若它直接含图片）。
pub fn find_volume_dirs(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        if dir_has_images(dir) {
            out.push(dir.to_path_buf());
        }
        if let Ok(rd) = std::fs::read_dir(dir) {
            let mut subs: Vec<PathBuf> = rd
                .filter_map(|e| e.ok().map(|e| e.path()))
                .filter(|p| p.is_dir() && !junk::is_system_junk_path(p))
                .collect();
            subs.sort();
            for s in subs {
                walk(&s, out);
            }
        }
    }
    walk(root, &mut out);
    out
}

/// 把卷目录按父目录分组、组内按目录名自然序编号、按组卷数决定位数，产出计划。
pub fn plan_volumes(vol_dirs: &[PathBuf]) -> Vec<VolumePlan> {
    use std::collections::BTreeMap;
    let mut groups: BTreeMap<PathBuf, Vec<PathBuf>> = BTreeMap::new();
    for d in vol_dirs {
        let parent = d.parent().map(|p| p.to_path_buf()).unwrap_or_else(|| d.clone());
        groups.entry(parent).or_default().push(d.clone());
    }
    let mut plans = Vec::new();
    for (manga_dir, mut dirs) in groups {
        dirs.sort_by(|a, b| {
            let ka = natural_key(a.file_name().and_then(|s| s.to_str()).unwrap_or(""));
            let kb = natural_key(b.file_name().and_then(|s| s.to_str()).unwrap_or(""));
            ka.cmp(&kb)
        });
        let width = if dirs.len() >= 100 { 3 } else { 2 };
        let manga = manga_dir
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_string();
        for (i, dir) in dirs.into_iter().enumerate() {
            plans.push(VolumePlan {
                dir,
                manga_dir: manga_dir.clone(),
                manga: manga.clone(),
                index: i + 1,
                width,
            });
        }
    }
    plans
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn touch_img(dir: &Path, name: &str) {
        fs::create_dir_all(dir).unwrap();
        use image::{RgbImage, Rgb};
        RgbImage::from_pixel(2, 2, Rgb([1, 2, 3])).save(dir.join(name)).unwrap();
    }

    #[test]
    fn find_leaf_image_dirs() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        touch_img(&root.join("海贼王/第01卷"), "a.png");
        touch_img(&root.join("海贼王/第02卷"), "a.png");
        touch_img(&root.join("火影/单卷"), "a.png");
        let mut dirs = find_volume_dirs(root);
        dirs.sort();
        assert_eq!(dirs.len(), 3);
    }

    #[test]
    fn group_and_number_per_manga() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        touch_img(&root.join("海贼王/第01卷"), "a.png");
        touch_img(&root.join("海贼王/第02卷"), "a.png");
        touch_img(&root.join("火影/vol1"), "a.png");
        let dirs = find_volume_dirs(root);
        let plans = plan_volumes(&dirs);
        let op: Vec<_> = plans.iter().filter(|p| p.manga == "海贼王").collect();
        assert_eq!(op.len(), 2);
        assert_eq!(op[0].index, 1);
        assert_eq!(op[1].index, 2);
        assert_eq!(op[0].width, 2);
        let np: Vec<_> = plans.iter().filter(|p| p.manga == "火影").collect();
        assert_eq!(np.len(), 1);
        assert_eq!(np[0].index, 1);
    }

    #[test]
    fn width_is_3_when_group_has_100_plus() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("长篇");
        for i in 1..=100 {
            touch_img(&root.join(format!("{i:04}")), "a.png");
        }
        let dirs = find_volume_dirs(tmp.path());
        let plans = plan_volumes(&dirs);
        assert!(plans.iter().all(|p| p.width == 3));
        assert_eq!(plans.len(), 100);
    }
}
